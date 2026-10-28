//! Password login guarding every collection's write and private routes
//! (`Routes::write`, `Routes::private`) and the private media folders
//! (shipment PDFs, calendar photos: personal data); every page too in
//! private mode ([`AuthConfig::public_reads`] false, the deployed app).
//!
//! The users and their passwords come from the environment
//! ([`AuthConfig::from_env`]): `APP_PASSWORD` (user `owner`) and
//! `APP_PASSWORD_<NAME>` (user `<name>`, e.g. a friend's). The login form
//! asks only for the password: it says who logs in, so two users can't share
//! one. Without any, login is disabled and the server only accepts to listen
//! on a loopback address (see `main`).
//!
//! What a user may change ([`Changes`]): the owner everything; another user
//! only the books, unless `APP_ACCESS_<NAME>` says `all` or lists the
//! collections (`books,pokemon`). Every logged-in user reads everything.
//! [`require_access`] checks it on each collection's write routes.
//!
//! A session is a cookie `<user>.<expiry>.<signature>`: the expiry as a unix
//! timestamp, signed with HMAC-SHA256 together with the user and their
//! current password, by a random key generated once and kept in the
//! `app_secret` table: restarts don't log out, changing a password does.
//! `SameSite=Strict` keeps other sites from posting forms with it (CSRF),
//! `Secure` (served over HTTPS) keeps it off plain HTTP.
//!
//! Wrong passwords: after [`MAX_FAILURES`] from one client, its next tries
//! are refused for [`LOCKOUT`], the right password too.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use askama::Template;
use axum::extract::{ConnectInfo, Query, Request, State};
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
use crate::web::error::AppResult;
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

const COOKIE: &str = "session";
const SESSION_DAYS: i64 = 30;

/// Wrong passwords allowed from one client before it must wait.
pub const MAX_FAILURES: u32 = 5;
/// How long a client that made too many wrong guesses waits.
pub const LOCKOUT: Duration = Duration::from_secs(15 * 60);

/// The collections, by the name their `Routes::collection` and
/// `APP_ACCESS_<NAME>` use, and how a page names them.
pub const COLLECTIONS: [(&str, &str); 3] = [
    ("yugioh", "the Yu-Gi-Oh! cards"),
    ("books", "the books"),
    ("pokemon", "the Pokémon cards"),
];

/// What a user may change. Reading is open to every logged-in user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Changes {
    Everything,
    /// These collections only ([`COLLECTIONS`] names).
    Only(Vec<String>),
}

impl Changes {
    /// A user's rights when `APP_ACCESS_<NAME>` doesn't say: the books.
    pub fn books() -> Changes {
        Changes::Only(vec!["books".to_string()])
    }

    pub fn allows(&self, collection: &str) -> bool {
        match self {
            Changes::Everything => true,
            Changes::Only(collections) => collections.iter().any(|c| c == collection),
        }
    }

    /// `all`, or collection names separated by commas.
    fn parse(value: &str) -> Result<Changes, String> {
        if value.trim().eq_ignore_ascii_case("all") {
            return Ok(Changes::Everything);
        }

        let mut collections = Vec::new();
        for name in value.split(',').map(|n| n.trim().to_lowercase()) {
            if !COLLECTIONS.iter().any(|(known, _)| *known == name) {
                let known: Vec<_> = COLLECTIONS.iter().map(|(n, _)| *n).collect();
                return Err(format!(
                    "{name:?} is no collection: all, or some of {}",
                    known.join(", ")
                ));
            }
            if !collections.contains(&name) {
                collections.push(name);
            }
        }
        Ok(Changes::Only(collections))
    }

    /// "the books and the Pokémon cards", for the pages.
    pub fn describe(&self) -> String {
        match self {
            Changes::Everything => "everything".to_string(),
            Changes::Only(collections) => {
                let labels: Vec<_> = collections.iter().map(|c| label(c)).collect();
                match labels.split_last() {
                    None => "nothing".to_string(),
                    Some((last, [])) => last.to_string(),
                    Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
                }
            }
        }
    }
}

/// How a page names a collection.
fn label(collection: &str) -> &str {
    COLLECTIONS
        .iter()
        .find(|(name, _)| *name == collection)
        .map_or(collection, |(_, label)| label)
}

/// One person who may log in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub name: String,
    pub password: String,
    pub changes: Changes,
}

impl User {
    pub fn new(name: &str, password: &str, changes: Changes) -> User {
        User {
            name: name.to_string(),
            password: password.to_string(),
            changes,
        }
    }
}

/// Who may log in, and how.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// None: login disabled.
    pub users: Vec<User>,
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
    /// One user, `owner`, with this password (none: login disabled).
    pub fn password(password: Option<&str>) -> AuthConfig {
        AuthConfig {
            users: password
                .filter(|p| !p.is_empty())
                .map(|p| vec![User::new("owner", p, Changes::Everything)])
                .unwrap_or_default(),
            ..AuthConfig::default()
        }
    }

    /// From the environment: `APP_PASSWORD`, `APP_PASSWORD_<NAME>`,
    /// `APP_ACCESS_<NAME>`, `SECURE_COOKIES`, `PUBLIC_READS`.
    pub fn from_env() -> Result<AuthConfig, String> {
        AuthConfig::from_vars(std::env::vars())
    }

    fn from_vars(vars: impl IntoIterator<Item = (String, String)>) -> Result<AuthConfig, String> {
        let mut config = AuthConfig::default();
        let mut access = Vec::new();
        let flag = |value: &str| matches!(value.trim(), "1" | "true" | "yes");
        let user_name = |var: &str, user: &str| {
            let user = user.to_lowercase();
            if user.is_empty() || !user.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
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
                config
                    .users
                    .push(User::new("owner", &value, Changes::Everything));
            } else if let Some(user) = name.strip_prefix("APP_PASSWORD_") {
                let user = user_name(&name, user)?;
                config
                    .users
                    .push(User::new(&user, &value, Changes::books()));
            } else if let Some(user) = name.strip_prefix("APP_ACCESS_") {
                let changes = Changes::parse(&value).map_err(|e| format!("{name}: {e}"))?;
                access.push((user_name(&name, user)?, changes));
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
            if config.users[..i]
                .iter()
                .any(|other| other.password == user.password)
            {
                return Err(
                    "two users have the same password: the password says who logs in".into(),
                );
            }
        }
        for (name, changes) in access {
            if name == "owner" {
                return Err("APP_ACCESS_OWNER: the owner can change everything".into());
            }
            let Some(user) = config.users.iter_mut().find(|u| u.name == name) else {
                return Err(format!(
                    "APP_ACCESS_{}: no APP_PASSWORD_{0} for this user",
                    name.to_uppercase()
                ));
            };
            user.changes = changes;
        }
        Ok(config)
    }
}

#[derive(Clone)]
pub struct Auth {
    /// None: login disabled.
    users: Arc<Vec<User>>,
    key: [u8; 32],
    secure_cookies: bool,
    public_reads: bool,
    /// Wrong passwords by client: how many, the latest when.
    failures: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}

impl Auth {
    /// Load the session key, creating it on first use.
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

        Ok(Auth {
            users: Arc::new(config.users),
            key,
            secure_cookies: config.secure_cookies,
            public_reads: config.public_reads,
            failures: Arc::default(),
        })
    }

    pub fn enabled(&self) -> bool {
        !self.users.is_empty()
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

    /// The user whose password this is. Every password is compared, in
    /// constant time (their MACs), so the time says nothing.
    fn user_with_password(&self, input: &str) -> Option<&str> {
        let input = self.mac(input.as_bytes()).finalize().into_bytes();
        let mut found = None;
        for user in self.users.iter() {
            if self
                .mac(user.password.as_bytes())
                .verify_slice(&input)
                .is_ok()
            {
                found = Some(user.name.as_str());
            }
        }
        found
    }

    /// The MAC of a session: the user, the expiry and the user's password.
    fn session_mac(&self, user: &str, expiry: &str) -> Option<HmacSha256> {
        let password = &self.users.iter().find(|u| u.name == user)?.password;
        Some(self.mac(format!("{user}.{expiry}.{password}").as_bytes()))
    }

    /// What `user` may change (everything with the login disabled: the
    /// user `local`).
    pub fn changes(&self, user: &str) -> Changes {
        if !self.enabled() {
            return Changes::Everything;
        }
        self.users
            .iter()
            .find(|u| u.name == user)
            .map_or(Changes::Only(Vec::new()), |u| u.changes.clone())
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
    collection: &'static str,
    allowed: String,
}

/// Middleware on a collection's write routes, inside [`require_login`]:
/// let through the users who may change `collection` (an empty name: the
/// users who may change everything). Otherwise a 403, in JSON for the APIs.
pub async fn require_access(
    State((state, collection)): State<(AppState, &'static str)>,
    request: Request,
    next: Next,
) -> Response {
    let user = state.auth.user(request.headers()).unwrap_or_default();
    let changes = state.auth.changes(&user);
    let allowed = if collection.is_empty() {
        changes == Changes::Everything
    } else {
        changes.allows(collection)
    };
    if allowed {
        return next.run(request).await;
    }

    tracing::warn!(
        user,
        path = request.uri().path(),
        "change refused: not allowed"
    );
    let collection = if collection.is_empty() {
        "this"
    } else {
        label(collection)
    };
    if is_api(request.uri().path()) {
        let error = format!("Your login can't change {collection}.");
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": error })),
        )
            .into_response();
    }
    let page = ForbiddenPage {
        collection,
        allowed: changes.describe(),
    };
    (StatusCode::FORBIDDEN, page).into_response()
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginPage {
    enabled: bool,
    /// The logged-in user and what they may change.
    who: Option<(String, String)>,
    next: String,
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
            let changes = state.auth.changes(&user).describe();
            (user, changes)
        });
    LoginPage {
        enabled: state.auth.enabled(),
        who,
        next: safe_next(param.next.as_deref()),
        error: None,
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
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
            error: Some(error),
        };
        Ok((status, page).into_response())
    };

    if state.auth.locked_out(&client) {
        tracing::warn!(client, "login refused: too many wrong passwords");
        return refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many wrong passwords: try again in 15 minutes.",
        );
    }

    let now = chrono::Utc::now().timestamp();
    let Some(cookie) = state
        .auth
        .user_with_password(&form.password)
        .and_then(|user| Some((user, state.auth.session_cookie(user, now)?)))
    else {
        state.auth.failed(&client);
        // slow down guessing
        tokio::time::sleep(Duration::from_secs(1)).await;
        tracing::warn!(client, "failed login attempt");
        return refuse(StatusCode::UNAUTHORIZED, "Wrong password.");
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

    fn auth_with(users: &[(&str, &str)]) -> Auth {
        Auth {
            users: Arc::new(
                users
                    .iter()
                    .map(|(u, p)| User::new(u, p, Changes::Everything))
                    .collect(),
            ),
            key: [7; 32],
            secure_cookies: false,
            public_reads: true,
            failures: Arc::default(),
        }
    }

    fn auth() -> Auth {
        auth_with(&[("owner", "secret"), ("friend", "other")])
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
        assert_eq!(auth.session_user(&format!("owner.{rest}"), 1000), None);
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
        let new_password = auth_with(&[("owner", "secret"), ("friend", "changed")]);
        assert_eq!(new_password.session_user(value, 1000), None);
        assert!(auth.session_cookie("nobody", 1000).is_none());
    }

    #[test]
    fn the_password_says_who_logs_in() {
        assert_eq!(auth().user_with_password("secret"), Some("owner"));
        assert_eq!(auth().user_with_password("other"), Some("friend"));
        assert_eq!(auth().user_with_password("Secret"), None);
        assert_eq!(auth().user_with_password(""), None);
        assert_eq!(auth_with(&[]).user_with_password(""), None);
    }

    #[test]
    fn the_cookie_is_secure_over_https() {
        let auth = Auth {
            secure_cookies: true,
            ..auth()
        };
        let cookie = auth.session_cookie("owner", 1000).unwrap();
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
            [
                User::new("friend", "b", Changes::books()),
                User::new("owner", "a", Changes::Everything)
            ]
        );
        assert!(config.secure_cookies && !config.public_reads);
        assert!(vars(&[]).unwrap().public_reads);
        assert!(vars(&[("APP_PASSWORD", "a"), ("APP_PASSWORD_FRIEND", "a")]).is_err());
        assert!(vars(&[("APP_PASSWORD", "a"), ("APP_PASSWORD_OWNER", "b")]).is_err());
        assert!(vars(&[("APP_PASSWORD_A-B", "a")]).is_err());

        // what a user may change: the books unless APP_ACCESS_<NAME> says
        let changes = |pairs: &[(&str, &str)]| {
            let config = vars(pairs)?;
            Ok::<_, String>(
                config
                    .users
                    .iter()
                    .find(|u| u.name == "friend")
                    .unwrap()
                    .changes
                    .clone(),
            )
        };
        let friend = ("APP_PASSWORD_FRIEND", "b");
        assert_eq!(
            changes(&[friend, ("APP_ACCESS_FRIEND", "all")]),
            Ok(Changes::Everything)
        );
        assert_eq!(
            changes(&[friend, ("APP_ACCESS_FRIEND", "ALL ")]),
            Ok(Changes::Everything)
        );
        assert_eq!(
            changes(&[friend, ("APP_ACCESS_FRIEND", "Books, pokemon,books")]),
            Ok(Changes::Only(vec!["books".into(), "pokemon".into()]))
        );
        assert_eq!(
            changes(&[("APP_ACCESS_FRIEND", "yugioh"), friend]),
            Ok(Changes::Only(vec!["yugioh".into()]))
        );
        // an empty value: the default
        assert_eq!(
            changes(&[friend, ("APP_ACCESS_FRIEND", "")]),
            Ok(Changes::books())
        );
        assert!(changes(&[friend, ("APP_ACCESS_FRIEND", "cards")]).is_err());
        assert!(vars(&[("APP_PASSWORD", "a"), ("APP_ACCESS_OWNER", "books")]).is_err());
        assert!(vars(&[("APP_PASSWORD", "a"), ("APP_ACCESS_NOBODY", "all")]).is_err());
    }

    #[test]
    fn changes_allow_their_collections() {
        assert!(Changes::Everything.allows("yugioh"));
        assert!(Changes::books().allows("books"));
        assert!(!Changes::books().allows("pokemon"));
        assert!(!Changes::books().allows(""));

        assert_eq!(Changes::Everything.describe(), "everything");
        assert_eq!(Changes::books().describe(), "the books");
        let all = Changes::Only(vec!["yugioh".into(), "books".into(), "pokemon".into()]);
        assert_eq!(
            all.describe(),
            "the Yu-Gi-Oh! cards, the books and the Pokémon cards"
        );
        assert_eq!(Changes::Only(Vec::new()).describe(), "nothing");

        let auth = auth_with(&[("owner", "secret")]);
        assert_eq!(auth.changes("owner"), Changes::Everything);
        assert_eq!(auth.changes("nobody"), Changes::Only(Vec::new()));
        assert_eq!(auth_with(&[]).changes("local"), Changes::Everything);
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
