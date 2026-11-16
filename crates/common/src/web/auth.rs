//! Login guarding every collection's write and private routes
//! (`Routes::write`, `Routes::private`, `Routes::login_only`) and the private
//! media folders (shipment PDFs, calendar photos: personal data); every page
//! too in private mode ([`AuthConfig::public_reads`] false, the deployed app).
//!
//! The users, their password hashes and their permissions live in the
//! database (`app_user`, `app_permission`, `app_user_permission`), managed at
//! `/users` ([`crate::web::users`]). [`Auth`] keeps them in memory, so the
//! middlewares stay synchronous; [`Auth::reload`] runs after each change.
//! The environment ([`AuthConfig::from_env`]) only **seeds** an empty table
//! on the first start: `APP_PASSWORD` → the user `admin` (every permission),
//! `APP_PASSWORD_<NAME>` → `<name>`, who changes the books unless
//! `APP_ACCESS_<NAME>` says `all` or lists the collections
//! (`books,pokemon`). Without any, and with no user in the database, login is
//! disabled and the server only accepts to listen on a loopback address (see
//! `main`). `backend set-password` is the way back in.
//!
//! What a user may change ([`Permissions`]): the admin everything; another
//! user the permissions granted, `yugioh`, `books`, `pokemon` (a collection)
//! or `pokemon_wishlist:<id>` (one wishlist). Every logged-in user reads
//! everything. [`require_access`] checks a collection on its write routes;
//! the routes of `Routes::login_only` check their own permission with
//! [`CurrentUser::require`].
//!
//! The login asks for a user name and a password (argon2 hashed). An unknown
//! name is checked against a dummy hash, so the time says nothing.
//!
//! A session is a cookie `<user>.<expiry>.<signature>`: the expiry as a unix
//! timestamp, signed with HMAC-SHA256 together with the user and their
//! current password hash, by a random key generated once and kept in the
//! `app_secret` table: restarts don't log out, changing a password does.
//! `SameSite=Strict` keeps other sites from posting forms with it (CSRF),
//! `Secure` (served over HTTPS) keeps it off plain HTTP.
//!
//! Wrong passwords: after [`MAX_FAILURES`] from one client, its next tries
//! are refused for [`LOCKOUT`], the right password too.

use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use askama::Template;
use axum::async_trait;
use axum::extract::{ConnectInfo, FromRequestParts, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use tracing::Instrument;

use crate::db::Pool;
use crate::web::error::{AppError, AppResult};
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

const COOKIE: &str = "session";
const SESSION_DAYS: i64 = 30;

/// Wrong passwords allowed from one client before it must wait.
pub const MAX_FAILURES: u32 = 5;
/// How long a client that made too many wrong guesses waits.
pub const LOCKOUT: Duration = Duration::from_secs(15 * 60);

/// The collections' permission keys, by the name `Routes::collection` and
/// `APP_ACCESS_<NAME>` use.
pub const COLLECTIONS: [&str; 3] = ["yugioh", "books", "pokemon"];

/// A user name: lower case letters, digits and `_`, 32 at most (the session
/// cookie splits on dots).
pub fn valid_user_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// An argon2 PHC string for `password`.
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| format!("cannot hash the password: {e}"))
}

/// Whether `password` is the one `hash` was made from.
pub fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

/// A hash nobody knows the password of: an unknown user costs the same time
/// as a wrong password.
fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password(&format!("{:032x}", rand::random::<u128>())).expect("a hash"))
}

/// What a user may change. Reading is open to every logged-in user.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Permissions {
    pub admin: bool,
    /// The granted `app_permission` keys.
    pub keys: BTreeSet<String>,
}

impl Permissions {
    pub fn admin() -> Permissions {
        Permissions {
            admin: true,
            keys: BTreeSet::new(),
        }
    }

    /// Whether `key` is granted: the admin is allowed everything. An empty
    /// key means "admin only".
    pub fn allows(&self, key: &str) -> bool {
        self.admin || (!key.is_empty() && self.keys.contains(key))
    }
}

/// One person who may log in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub password_hash: String,
    pub admin: bool,
    pub permissions: BTreeSet<String>,
}

/// A user the environment seeds an empty database with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedUser {
    pub name: String,
    pub password: String,
    pub admin: bool,
    /// `app_permission` keys (collections).
    pub permissions: Vec<String>,
}

impl SeedUser {
    fn admin(password: &str) -> SeedUser {
        SeedUser {
            name: "admin".into(),
            password: password.into(),
            admin: true,
            permissions: Vec::new(),
        }
    }

    pub fn new(name: &str, password: &str, permissions: &[&str]) -> SeedUser {
        SeedUser {
            name: name.into(),
            password: password.into(),
            admin: false,
            permissions: permissions.iter().map(|p| p.to_string()).collect(),
        }
    }

    /// The default rights of `APP_PASSWORD_<NAME>`: the books.
    pub fn books(name: &str, password: &str) -> SeedUser {
        SeedUser::new(name, password, &["books"])
    }
}

/// How to start: who to create when the database has no user, and the
/// cookie and read settings.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Seed users (only used while `app_user` is empty).
    pub users: Vec<SeedUser>,
    /// The session cookie only over HTTPS (`SECURE_COOKIES=1`).
    pub secure_cookies: bool,
    /// The pages that only read are open to all (`PUBLIC_READS=0`: every
    /// page needs the login).
    pub public_reads: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig {
            users: Vec::new(),
            secure_cookies: false,
            public_reads: true,
        }
    }
}

impl AuthConfig {
    /// One user, `admin`, with this password (none: login disabled).
    pub fn password(password: Option<&str>) -> AuthConfig {
        AuthConfig {
            users: password
                .filter(|p| !p.is_empty())
                .map(|p| vec![SeedUser::admin(p)])
                .unwrap_or_default(),
            ..AuthConfig::default()
        }
    }

    /// From the environment: `APP_PASSWORD`, `APP_PASSWORD_<NAME>`,
    /// `APP_ACCESS_<NAME>`, `SECURE_COOKIES`, `PUBLIC_READS`.
    pub fn from_env() -> Result<AuthConfig, String> {
        AuthConfig::from_vars(std::env::vars())
    }

    /// `all`, or collection names separated by commas.
    fn parse_access(value: &str) -> Result<Vec<String>, String> {
        if value.trim().eq_ignore_ascii_case("all") {
            return Ok(COLLECTIONS.iter().map(|c| c.to_string()).collect());
        }

        let mut collections = Vec::new();
        for name in value.split(',').map(|n| n.trim().to_lowercase()) {
            if !COLLECTIONS.contains(&name.as_str()) {
                return Err(format!(
                    "{name:?} is no collection: all, or some of {}",
                    COLLECTIONS.join(", ")
                ));
            }
            if !collections.contains(&name) {
                collections.push(name);
            }
        }
        Ok(collections)
    }

    fn from_vars(vars: impl IntoIterator<Item = (String, String)>) -> Result<AuthConfig, String> {
        let mut config = AuthConfig::default();
        let mut access = Vec::new();
        let flag = |value: &str| matches!(value.trim(), "1" | "true" | "yes");
        let user_name = |var: &str, user: &str| {
            let user = user.to_lowercase();
            if !valid_user_name(&user) {
                return Err(format!("{var}: a user name is letters, digits and _"));
            }
            Ok(user)
        };
        for (name, value) in vars {
            if name == "SECURE_COOKIES" {
                config.secure_cookies = flag(&value);
            } else if name == "PUBLIC_READS" {
                config.public_reads = flag(&value);
            } else if value.is_empty() {
                continue;
            } else if name == "APP_PASSWORD" {
                config.users.push(SeedUser::admin(&value));
            } else if let Some(user) = name.strip_prefix("APP_PASSWORD_") {
                let user = user_name(&name, user)?;
                config.users.push(SeedUser::books(&user, &value));
            } else if let Some(user) = name.strip_prefix("APP_ACCESS_") {
                let collections =
                    AuthConfig::parse_access(&value).map_err(|e| format!("{name}: {e}"))?;
                access.push((user_name(&name, user)?, collections));
            }
        }
        config.users.sort_by(|a, b| a.name.cmp(&b.name));

        for (i, user) in config.users.iter().enumerate() {
            if config.users[..i]
                .iter()
                .any(|other| other.name == user.name)
            {
                return Err(format!("user {} is set twice", user.name));
            }
        }
        for (name, collections) in access {
            if name == "admin" {
                return Err("APP_ACCESS_ADMIN: the admin can change everything".into());
            }
            let Some(user) = config.users.iter_mut().find(|u| u.name == name) else {
                return Err(format!(
                    "APP_ACCESS_{}: no APP_PASSWORD_{0} for this user",
                    name.to_uppercase()
                ));
            };
            user.permissions = collections;
        }
        Ok(config)
    }
}

/// What the database says, in memory.
#[derive(Default)]
struct Directory {
    users: Vec<User>,
    /// Every grantable permission, in display order: `(key, label)`.
    permissions: Vec<(String, String)>,
}

#[derive(Clone)]
pub struct Auth {
    directory: Arc<RwLock<Directory>>,
    key: [u8; 32],
    secure_cookies: bool,
    public_reads: bool,
    /// Wrong passwords by client: how many, the latest when.
    failures: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}

impl Auth {
    /// Load the session key (created on first use) and the users. An empty
    /// `app_user` table is seeded from the config.
    pub async fn load(pool: &Pool, config: AuthConfig) -> sqlx::Result<Auth> {
        sqlx::query("INSERT OR IGNORE INTO app_secret (name, value) VALUES ('session_key', ?)")
            .bind(rand::random::<[u8; 32]>().to_vec())
            .execute(pool)
            .await?;
        let stored: Vec<u8> =
            sqlx::query_scalar("SELECT value FROM app_secret WHERE name = 'session_key'")
                .fetch_one(pool)
                .await?;
        let key = stored
            .try_into()
            .map_err(|_| sqlx::Error::Decode("session_key must be 32 bytes".into()))?;

        let auth = Auth {
            directory: Arc::default(),
            key,
            secure_cookies: config.secure_cookies,
            public_reads: config.public_reads,
            failures: Arc::default(),
        };

        let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM app_user")
            .fetch_one(pool)
            .await?;
        if existing > 0 && !config.users.is_empty() {
            tracing::warn!(
                "APP_PASSWORD[_<NAME>] only seed an empty user table: ignored, \
                 the users are managed at /users"
            );
        } else if existing == 0 {
            for seed in config.users {
                let password = seed.password.clone();
                let hash = tokio::task::spawn_blocking(move || hash_password(&password))
                    .await
                    .map_err(|e| sqlx::Error::Protocol(e.to_string()))?
                    .map_err(sqlx::Error::Protocol)?;
                let mut keys = seed.permissions;
                if keys.iter().any(|k| k == "pokemon") {
                    // the wishlists that exist: what `pokemon` meant before them
                    let lists: Vec<String> = sqlx::query_scalar(
                        "SELECT key FROM app_permission WHERE key LIKE 'pokemon_wishlist:%'",
                    )
                    .fetch_all(pool)
                    .await?;
                    keys.extend(lists);
                }
                crate::web::users::create(pool, &seed.name, &hash, seed.admin, &keys).await?;
            }
        }
        auth.reload(pool).await?;
        Ok(auth)
    }

    /// Read the users and permissions again, after a change.
    pub async fn reload(&self, pool: &Pool) -> sqlx::Result<()> {
        let rows: Vec<(i64, String, String, bool)> =
            sqlx::query_as("SELECT id, name, password_hash, is_admin FROM app_user ORDER BY name")
                .fetch_all(pool)
                .await?;
        let grants: Vec<(i64, String)> =
            sqlx::query_as("SELECT user_id, permission FROM app_user_permission")
                .fetch_all(pool)
                .await?;
        let permissions: Vec<(String, String)> =
            sqlx::query_as("SELECT key, label FROM app_permission ORDER BY position, key")
                .fetch_all(pool)
                .await?;

        let users = rows
            .into_iter()
            .map(|(id, name, password_hash, admin)| User {
                id,
                name,
                password_hash,
                admin,
                permissions: grants
                    .iter()
                    .filter(|(user_id, _)| *user_id == id)
                    .map(|(_, key)| key.clone())
                    .collect(),
            })
            .collect();
        *self.directory.write().expect("not poisoned") = Directory { users, permissions };
        Ok(())
    }

    fn directory(&self) -> std::sync::RwLockReadGuard<'_, Directory> {
        self.directory.read().expect("not poisoned")
    }

    pub fn enabled(&self) -> bool {
        !self.directory().users.is_empty()
    }

    /// Whether the pages that only read are open to all.
    pub fn public_reads(&self) -> bool {
        self.public_reads || !self.enabled()
    }

    fn mac(&self, data: &[u8]) -> HmacSha256 {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC takes any key size");
        mac.update(data);
        mac
    }

    /// The hash to check a login against, and the user's name when there is
    /// one (else a hash nobody has the password of).
    fn login_target(&self, name: &str) -> (Option<String>, String) {
        let name = name.trim().to_lowercase();
        match self.directory().users.iter().find(|u| u.name == name) {
            Some(user) => (Some(user.name.clone()), user.password_hash.clone()),
            None => (None, dummy_hash().to_string()),
        }
    }

    /// The user this name and password log in (slow: argon2).
    async fn authenticate(&self, name: &str, password: &str) -> Option<String> {
        let (user, hash) = self.login_target(name);
        let password = password.to_string();
        let right = tokio::task::spawn_blocking(move || verify_password(&hash, &password))
            .await
            .unwrap_or(false);
        user.filter(|_| right)
    }

    /// The MAC of a session: the user, the expiry and the user's password hash.
    fn session_mac(&self, user: &str, expiry: &str) -> Option<HmacSha256> {
        let directory = self.directory();
        let hash = &directory
            .users
            .iter()
            .find(|u| u.name == user)?
            .password_hash;
        Some(self.mac(format!("{user}.{expiry}.{hash}").as_bytes()))
    }

    /// What `user` may change (everything with the login disabled: the
    /// user `local`).
    pub fn permissions(&self, user: &str) -> Permissions {
        if !self.enabled() {
            return Permissions::admin();
        }
        self.directory()
            .users
            .iter()
            .find(|u| u.name == user)
            .map(|u| Permissions {
                admin: u.admin,
                keys: u.permissions.clone(),
            })
            .unwrap_or_default()
    }

    /// How a page names a permission (after "change"): "the books".
    pub fn label(&self, key: &str) -> String {
        self.directory()
            .permissions
            .iter()
            .find(|(k, _)| k == key)
            .map_or_else(|| key.to_string(), |(_, label)| label.clone())
    }

    /// "the books and the Pokémon cards", for the pages.
    pub fn describe(&self, permissions: &Permissions) -> String {
        if permissions.admin {
            return "everything".to_string();
        }
        let labels: Vec<_> = {
            let directory = self.directory();
            directory
                .permissions
                .iter()
                .filter(|(key, _)| permissions.keys.contains(key))
                .map(|(_, label)| label.clone())
                .collect()
        };
        match labels.split_last() {
            None => "nothing".to_string(),
            Some((last, [])) => last.clone(),
            Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        }
    }

    fn cookie_attributes(&self, max_age: i64) -> String {
        let secure = if self.secure_cookies { "; Secure" } else { "" };
        format!("Path=/; Max-Age={max_age}; HttpOnly; SameSite=Strict{secure}")
    }

    fn session_cookie(&self, user: &str, now: i64) -> Option<String> {
        let expiry = (now + SESSION_DAYS * 24 * 3600).to_string();
        let mac = self.session_mac(user, &expiry)?;
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Some(format!(
            "{COOKIE}={user}.{expiry}.{signature}; {}",
            self.cookie_attributes(SESSION_DAYS * 24 * 3600)
        ))
    }

    /// The user of a valid, unexpired session.
    fn session_user<'a>(&self, value: &'a str, now: i64) -> Option<&'a str> {
        let mut parts = value.splitn(3, '.');
        let (user, expiry, signature) = (parts.next()?, parts.next()?, parts.next()?);
        let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
        let valid = expiry.parse::<i64>().is_ok_and(|e| e > now)
            && self
                .session_mac(user, expiry)?
                .verify_slice(&signature)
                .is_ok();
        valid.then_some(user)
    }

    /// The logged-in user of a request (`"local"` with the login disabled).
    pub fn user(&self, headers: &HeaderMap) -> Option<String> {
        if !self.enabled() {
            return Some("local".to_string());
        }

        let now = chrono::Utc::now().timestamp();
        session_values(headers)
            .find_map(|value| self.session_user(value, now))
            .map(str::to_string)
    }

    /// Whether the request may write: login disabled, or a valid session.
    pub fn allows(&self, headers: &HeaderMap) -> bool {
        self.user(headers).is_some()
    }

    /// A new session cookie for `user`, after their name or password changed.
    pub(crate) fn fresh_cookie(&self, user: &str) -> Option<String> {
        self.session_cookie(user, chrono::Utc::now().timestamp())
    }

    /// The users, for the users page.
    pub(crate) fn users(&self) -> Vec<User> {
        self.directory().users.clone()
    }

    /// Every grantable permission: `(key, label)`.
    pub(crate) fn grantable(&self) -> Vec<(String, String)> {
        self.directory().permissions.clone()
    }

    /// Whether `client` made too many wrong guesses lately; forgets the old
    /// ones on the way.
    fn locked_out(&self, client: &str) -> bool {
        let mut failures = self.failures.lock().expect("not poisoned");
        failures.retain(|_, (_, last)| last.elapsed() < LOCKOUT);
        failures
            .get(client)
            .is_some_and(|(count, _)| *count >= MAX_FAILURES)
    }

    fn failed(&self, client: &str) {
        let mut failures = self.failures.lock().expect("not poisoned");
        let entry = failures
            .entry(client.to_string())
            .or_insert((0, Instant::now()));
        entry.0 += 1;
        entry.1 = Instant::now();
    }

    fn succeeded(&self, client: &str) {
        self.failures.lock().expect("not poisoned").remove(client);
    }
}

/// The logged-in user, in a handler of `Routes::login_only` (or any page that
/// shows more to those who may change it, as `Option<CurrentUser>`): checks
/// its own permission with [`CurrentUser::require`].
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub name: String,
    pub permissions: Permissions,
    auth: Auth,
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Auth")
    }
}

impl CurrentUser {
    pub fn allows(&self, key: &str) -> bool {
        self.permissions.allows(key)
    }

    pub fn is_admin(&self) -> bool {
        self.permissions.admin
    }

    /// A 403 unless this permission is granted.
    pub fn require(&self, key: &str) -> AppResult<()> {
        if self.allows(key) {
            return Ok(());
        }
        tracing::warn!(user = self.name, key, "change refused: not allowed");
        Err(AppError::Forbidden {
            what: self.auth.label(key),
            allowed: self.auth.describe(&self.permissions),
        })
    }

    /// A 403 unless the user is an admin.
    pub fn require_admin(&self) -> AppResult<()> {
        if self.is_admin() {
            return Ok(());
        }
        tracing::warn!(user = self.name, "change refused: admin only");
        Err(AppError::Forbidden {
            what: "this".to_string(),
            allowed: self.auth.describe(&self.permissions),
        })
    }
}

#[async_trait]
impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let name = state
            .auth
            .user(&parts.headers)
            .ok_or(AppError::Unauthorized)?;
        Ok(CurrentUser {
            permissions: state.auth.permissions(&name),
            name,
            auth: state.auth.clone(),
        })
    }
}

/// Who is asking, for the limit on wrong passwords: the address of the
/// connection, or, behind a proxy on this machine (Tailscale's), the first
/// address it forwards.
fn client(connection: Option<SocketAddr>, headers: &HeaderMap) -> String {
    let forwarded = || {
        headers
            .get("x-forwarded-for")?
            .to_str()
            .ok()?
            .split(',')
            .next()
            .map(|a| a.trim().to_string())
    };
    match connection {
        Some(addr) if addr.ip().is_loopback() => {
            forwarded().unwrap_or_else(|| addr.ip().to_string())
        }
        Some(addr) => addr.ip().to_string(),
        None => forwarded().unwrap_or_else(|| "unknown".to_string()),
    }
}

fn session_values(headers: &HeaderMap) -> impl Iterator<Item = &str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(';'))
        .filter_map(|c| c.trim().strip_prefix(COOKIE)?.strip_prefix('='))
}

/// A local path to go back to after login; anything else becomes `/`.
fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") && !n.contains('\\') => n.to_string(),
        _ => "/".to_string(),
    }
}

/// Middleware: let logged-in requests through (their logs name the user).
/// Otherwise the API answers 401 and pages redirect to the login form, which
/// comes back here (or, for a form post, to the page the form was on).
pub async fn require_login(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(user) = state.auth.user(request.headers()) {
        return next
            .run(request)
            .instrument(tracing::info_span!("user", name = %user))
            .await;
    }

    let uri = request.uri();
    if is_api(uri.path()) {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "Log in to make changes." })),
        )
            .into_response();
    }

    let back = if request.method() == Method::GET {
        Some(
            uri.path_and_query()
                .map_or(uri.path(), |p| p.as_str())
                .to_string(),
        )
    } else {
        request
            .headers()
            .get(header::REFERER)
            .and_then(|r| url::Url::parse(r.to_str().ok()?).ok())
            .map(|r| match r.query() {
                Some(q) => format!("{}?{q}", r.path()),
                None => r.path().to_string(),
            })
    };
    let next = safe_next(back.as_deref());
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("next", &next)
        .finish();
    Redirect::to(&format!("/login?{query}")).into_response()
}

/// The JSON APIs: the deck builder's, the books' ISBN lookup, the Pokémon
/// finder. They answer errors in JSON.
fn is_api(path: &str) -> bool {
    ["/api/", "/books/api/", "/pokemon/api/"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

#[derive(Template)]
#[template(path = "forbidden.html")]
pub struct ForbiddenPage {
    pub what: String,
    pub allowed: String,
}

/// Middleware on a collection's write routes, inside [`require_login`]:
/// let through the users who may change `collection` (an empty name: the
/// admins). Otherwise a 403, in JSON for the APIs.
pub async fn require_access(
    State((state, collection)): State<(AppState, &'static str)>,
    request: Request,
    next: Next,
) -> Response {
    let user = state.auth.user(request.headers()).unwrap_or_default();
    let permissions = state.auth.permissions(&user);
    if permissions.allows(collection) {
        return next.run(request).await;
    }

    tracing::warn!(
        user,
        path = request.uri().path(),
        "change refused: not allowed"
    );
    let what = if collection.is_empty() {
        "this".to_string()
    } else {
        state.auth.label(collection)
    };
    if is_api(request.uri().path()) {
        let error = format!("Your login can't change {what}.");
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": error })),
        )
            .into_response();
    }
    let page = ForbiddenPage {
        what,
        allowed: state.auth.describe(&permissions),
    };
    (StatusCode::FORBIDDEN, page).into_response()
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginPage {
    enabled: bool,
    /// The logged-in user, what they may change, whether they are an admin.
    who: Option<(String, String, bool)>,
    next: String,
    name: String,
    error: Option<&'static str>,
}

#[derive(Debug, Deserialize)]
pub struct NextParam {
    next: Option<String>,
}

pub async fn page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(param): Query<NextParam>,
) -> LoginPage {
    let who = state
        .auth
        .user(&headers)
        .filter(|_| state.auth.enabled())
        .map(|user| {
            let permissions = state.auth.permissions(&user);
            let changes = state.auth.describe(&permissions);
            (user, changes, permissions.admin)
        });
    LoginPage {
        enabled: state.auth.enabled(),
        who,
        next: safe_next(param.next.as_deref()),
        name: String::new(),
        error: None,
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    #[serde(default)]
    name: String,
    password: String,
    next: Option<String>,
}

pub async fn login(
    State(state): State<AppState>,
    connection: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> AppResult<Response> {
    let next = safe_next(form.next.as_deref());
    let client = client(connection.map(|c| c.0), &headers);
    let refuse = |status, error| {
        let page = LoginPage {
            enabled: state.auth.enabled(),
            who: None,
            next: next.clone(),
            name: form.name.clone(),
            error: Some(error),
        };
        Ok((status, page).into_response())
    };

    if state.auth.locked_out(&client) {
        tracing::warn!(client, "login refused: too many wrong passwords");
        return refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many wrong attempts: try again in 15 minutes.",
        );
    }

    let now = chrono::Utc::now().timestamp();
    let user = state.auth.authenticate(&form.name, &form.password).await;
    let Some(cookie) = user
        .as_deref()
        .and_then(|user| Some((user, state.auth.session_cookie(user, now)?)))
    else {
        state.auth.failed(&client);
        // slow down guessing
        tokio::time::sleep(Duration::from_secs(1)).await;
        tracing::warn!(client, "failed login attempt");
        return refuse(StatusCode::UNAUTHORIZED, "Wrong user name or password.");
    };

    state.auth.succeeded(&client);
    let (user, cookie) = cookie;
    tracing::info!(user, client, "logged in");
    Ok(([(header::SET_COOKIE, cookie)], Redirect::to(&next)).into_response())
}

pub async fn logout(State(state): State<AppState>) -> impl IntoResponse {
    let cookie = format!("{COOKIE}=; {}", state.auth.cookie_attributes(0));
    tracing::info!("logged out");
    ([(header::SET_COOKIE, cookie)], Redirect::to("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Users `(name, password)`, hashed cheaply: the tests only compare.
    fn auth_with(users: &[(&str, &str)]) -> Auth {
        let auth = Auth {
            directory: Arc::default(),
            key: [7; 32],
            secure_cookies: false,
            public_reads: true,
            failures: Arc::default(),
        };
        auth.directory.write().unwrap().users = users
            .iter()
            .enumerate()
            .map(|(i, (name, password))| User {
                id: i as i64,
                name: name.to_string(),
                password_hash: hash_password(password).unwrap(),
                admin: true,
                permissions: BTreeSet::new(),
            })
            .collect();
        auth
    }

    fn auth() -> Auth {
        auth_with(&[("admin", "secret"), ("friend", "other")])
    }

    fn cookie_value(cookie: &str) -> &str {
        cookie
            .strip_prefix("session=")
            .and_then(|c| c.split(';').next())
            .unwrap()
    }

    #[test]
    fn sessions_are_signed_and_expire() {
        let auth = auth();
        let cookie = auth.session_cookie("friend", 1000).unwrap();
        let value = cookie_value(&cookie);
        assert_eq!(auth.session_user(value, 1000), Some("friend"));
        assert_eq!(
            auth.session_user(value, 1000 + SESSION_DAYS * 24 * 3600),
            None
        );

        // another user's name, another expiry: the signature no longer fits
        let (_, rest) = value.split_once('.').unwrap();
        assert_eq!(auth.session_user(&format!("admin.{rest}"), 1000), None);
        let (expiry, signature) = rest.split_once('.').unwrap();
        let forged = format!("friend.{}.{signature}", expiry.parse::<i64>().unwrap() + 1);
        assert_eq!(auth.session_user(&forged, 1000), None);
        assert_eq!(auth.session_user("garbage", 1000), None);

        let other_key = Auth {
            key: [8; 32],
            ..auth.clone()
        };
        assert_eq!(other_key.session_user(value, 1000), None);
        // a new password logs the user out
        let new_password = auth_with(&[("admin", "secret"), ("friend", "changed")]);
        assert_eq!(new_password.session_user(value, 1000), None);
        assert!(auth.session_cookie("nobody", 1000).is_none());
    }

    #[tokio::test]
    async fn a_login_needs_the_name_and_the_password() {
        let auth = auth();
        assert_eq!(
            auth.authenticate("admin", "secret").await.as_deref(),
            Some("admin")
        );
        assert_eq!(
            auth.authenticate(" Friend ", "other").await.as_deref(),
            Some("friend")
        );
        // two users can share a password, the name says who logs in
        assert_eq!(auth.authenticate("admin", "other").await, None);
        assert_eq!(auth.authenticate("admin", "Secret").await, None);
        assert_eq!(auth.authenticate("admin", "").await, None);
        assert_eq!(auth.authenticate("nobody", "secret").await, None);
        assert_eq!(auth_with(&[]).authenticate("admin", "").await, None);
    }

    #[test]
    fn passwords_are_hashed_with_a_salt() {
        let hash = hash_password("secret").unwrap();
        assert!(hash.starts_with("$argon2"), "{hash}");
        assert_ne!(hash, hash_password("secret").unwrap());
        assert!(verify_password(&hash, "secret"));
        assert!(!verify_password(&hash, "Secret"));
        assert!(!verify_password("not a hash", "secret"));
    }

    #[test]
    fn the_cookie_is_secure_over_https() {
        let auth = Auth {
            secure_cookies: true,
            ..auth()
        };
        let cookie = auth.session_cookie("admin", 1000).unwrap();
        assert!(
            cookie.ends_with("HttpOnly; SameSite=Strict; Secure"),
            "{cookie}"
        );
    }

    #[test]
    fn too_many_wrong_passwords_lock_the_client_out() {
        let auth = auth();
        for _ in 0..MAX_FAILURES - 1 {
            auth.failed("1.2.3.4");
        }
        assert!(!auth.locked_out("1.2.3.4"));
        auth.failed("1.2.3.4");
        assert!(auth.locked_out("1.2.3.4"));
        assert!(!auth.locked_out("5.6.7.8"));
        auth.succeeded("1.2.3.4");
        assert!(!auth.locked_out("1.2.3.4"));
    }

    #[test]
    fn the_client_is_the_proxys_first_address() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "100.64.0.7, 127.0.0.1".parse().unwrap());
        let local: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let remote: SocketAddr = "192.168.1.20:5000".parse().unwrap();
        assert_eq!(client(Some(local), &headers), "100.64.0.7");
        // only a proxy on this machine is believed
        assert_eq!(client(Some(remote), &headers), "192.168.1.20");
        assert_eq!(client(Some(local), &HeaderMap::new()), "127.0.0.1");
    }

    #[test]
    fn users_come_from_the_environment() {
        let vars = |pairs: &[(&str, &str)]| {
            AuthConfig::from_vars(
                pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<Vec<_>>(),
            )
        };
        let config = vars(&[
            ("APP_PASSWORD", "a"),
            ("APP_PASSWORD_FRIEND", "b"),
            ("APP_PASSWORD_EMPTY", ""),
            ("SECURE_COOKIES", "1"),
            ("PUBLIC_READS", "0"),
            ("HOME", "/x"),
        ])
        .unwrap();
        assert_eq!(
            config.users,
            [SeedUser::admin("a"), SeedUser::books("friend", "b")]
        );
        assert!(config.secure_cookies && !config.public_reads);
        assert!(vars(&[]).unwrap().public_reads);
        // the same password is fine now: the name says who logs in
        assert!(vars(&[("APP_PASSWORD", "a"), ("APP_PASSWORD_FRIEND", "a")]).is_ok());
        assert!(vars(&[("APP_PASSWORD", "a"), ("APP_PASSWORD_ADMIN", "b")]).is_err());
        assert!(vars(&[("APP_PASSWORD_A-B", "a")]).is_err());

        // what a user may change: the books unless APP_ACCESS_<NAME> says
        let granted = |pairs: &[(&str, &str)]| {
            let config = vars(pairs)?;
            Ok::<_, String>(
                config
                    .users
                    .iter()
                    .find(|u| u.name == "friend")
                    .unwrap()
                    .permissions
                    .clone(),
            )
        };
        let friend = ("APP_PASSWORD_FRIEND", "b");
        let all: Vec<String> = COLLECTIONS.iter().map(|c| c.to_string()).collect();
        assert_eq!(
            granted(&[friend, ("APP_ACCESS_FRIEND", "all")]),
            Ok(all.clone())
        );
        assert_eq!(granted(&[friend, ("APP_ACCESS_FRIEND", "ALL ")]), Ok(all));
        assert_eq!(
            granted(&[friend, ("APP_ACCESS_FRIEND", "Books, pokemon,books")]),
            Ok(vec!["books".to_string(), "pokemon".to_string()])
        );
        assert_eq!(
            granted(&[("APP_ACCESS_FRIEND", "yugioh"), friend]),
            Ok(vec!["yugioh".to_string()])
        );
        // an empty value: the default
        assert_eq!(
            granted(&[friend, ("APP_ACCESS_FRIEND", "")]),
            Ok(vec!["books".to_string()])
        );
        assert!(granted(&[friend, ("APP_ACCESS_FRIEND", "cards")]).is_err());
        assert!(vars(&[("APP_PASSWORD", "a"), ("APP_ACCESS_ADMIN", "books")]).is_err());
        assert!(vars(&[("APP_PASSWORD", "a"), ("APP_ACCESS_NOBODY", "all")]).is_err());
    }

    #[test]
    fn permissions_allow_what_is_granted() {
        let admin = Permissions::admin();
        assert!(admin.allows("yugioh") && admin.allows(""));
        let books = Permissions {
            admin: false,
            keys: BTreeSet::from(["books".to_string()]),
        };
        assert!(books.allows("books"));
        assert!(!books.allows("pokemon"));
        assert!(!books.allows(""));

        let auth = auth_with(&[("admin", "secret")]);
        auth.directory.write().unwrap().permissions = vec![
            ("yugioh".into(), "the Yu-Gi-Oh! cards".into()),
            ("books".into(), "the books".into()),
            ("pokemon".into(), "the Pokémon cards".into()),
        ];
        assert_eq!(auth.describe(&admin), "everything");
        assert_eq!(auth.describe(&books), "the books");
        let three = Permissions {
            admin: false,
            keys: COLLECTIONS.iter().map(|c| c.to_string()).collect(),
        };
        assert_eq!(
            auth.describe(&three),
            "the Yu-Gi-Oh! cards, the books and the Pokémon cards"
        );
        assert_eq!(auth.describe(&Permissions::default()), "nothing");

        assert!(auth.permissions("admin").admin);
        assert_eq!(auth.permissions("nobody"), Permissions::default());
        assert!(auth_with(&[]).permissions("local").admin);
    }

    #[test]
    fn next_stays_local() {
        assert_eq!(safe_next(Some("/lots/1/edit?x=1")), "/lots/1/edit?x=1");
        assert_eq!(safe_next(Some("//evil.example")), "/");
        assert_eq!(safe_next(Some("https://evil.example")), "/");
        assert_eq!(safe_next(Some("/\\evil.example")), "/");
        assert_eq!(safe_next(None), "/");
    }
}
