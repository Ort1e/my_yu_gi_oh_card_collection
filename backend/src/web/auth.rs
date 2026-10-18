//! Single-password login guarding every write route (`routes::write_router`)
//! and the shipment PDFs (personal data).
//!
//! The password comes from `APP_PASSWORD`; without it login is disabled and
//! the server only accepts to listen on a loopback address (see `main`). A
//! session is a cookie `<expiry>.<signature>`: the expiry as a unix timestamp,
//! signed with HMAC-SHA256 by a random key generated once and kept in the
//! `app_secret` table, so restarts don't log out. `SameSite=Strict` keeps
//! other sites from posting forms with it (CSRF).

use askama::Template;
use axum::extract::{Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::store::db::Pool;
use crate::web::error::AppResult;
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

const COOKIE: &str = "session";
const SESSION_DAYS: i64 = 30;

#[derive(Clone)]
pub struct Auth {
    /// `None`: login disabled.
    password: Option<String>,
    key: [u8; 32],
}

impl Auth {
    /// Load the session key, creating it on first use.
    pub async fn load(pool: &Pool, password: Option<String>) -> sqlx::Result<Auth> {
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
            password: password.filter(|p| !p.is_empty()),
            key,
        })
    }

    pub fn enabled(&self) -> bool {
        self.password.is_some()
    }

    fn mac(&self, data: &[u8]) -> HmacSha256 {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC takes any key size");
        mac.update(data);
        mac
    }

    /// Constant-time comparison (of the MACs of both strings).
    fn password_matches(&self, input: &str) -> bool {
        let Some(password) = &self.password else {
            return false;
        };

        let expected = self.mac(password.as_bytes()).finalize().into_bytes();
        self.mac(input.as_bytes()).verify_slice(&expected).is_ok()
    }

    fn session_cookie(&self, now: i64) -> String {
        let expiry = (now + SESSION_DAYS * 24 * 3600).to_string();
        let signature = URL_SAFE_NO_PAD.encode(self.mac(expiry.as_bytes()).finalize().into_bytes());
        format!(
            "{COOKIE}={expiry}.{signature}; Path=/; Max-Age={}; HttpOnly; SameSite=Strict",
            SESSION_DAYS * 24 * 3600
        )
    }

    fn valid_session(&self, value: &str, now: i64) -> bool {
        let Some((expiry, signature)) = value.split_once('.') else {
            return false;
        };
        let Ok(signature) = URL_SAFE_NO_PAD.decode(signature) else {
            return false;
        };

        expiry.parse::<i64>().is_ok_and(|e| e > now)
            && self.mac(expiry.as_bytes()).verify_slice(&signature).is_ok()
    }

    /// Whether the request may write: login disabled, or a valid session.
    pub fn allows(&self, headers: &HeaderMap) -> bool {
        if !self.enabled() {
            return true;
        }

        let now = chrono::Utc::now().timestamp();
        session_values(headers).any(|value| self.valid_session(value, now))
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

/// Middleware: let logged-in requests through. Otherwise the API answers
/// 401 and pages redirect to the login form, which comes back here (or, for
/// a form post, to the page the form was on).
pub async fn require_login(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if state.auth.allows(request.headers()) {
        return next.run(request).await;
    }

    let uri = request.uri();
    // the JSON APIs: the deck builder's and the books' ISBN lookup
    if uri.path().starts_with("/api/") || uri.path().starts_with("/books/api/") {
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

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginPage {
    enabled: bool,
    logged_in: bool,
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
    LoginPage {
        enabled: state.auth.enabled(),
        logged_in: state.auth.enabled() && state.auth.allows(&headers),
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
    Form(form): Form<LoginForm>,
) -> AppResult<Response> {
    let next = safe_next(form.next.as_deref());

    if !state.auth.password_matches(&form.password) {
        // slow down guessing
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        tracing::warn!("failed login attempt");

        let page = LoginPage {
            enabled: state.auth.enabled(),
            logged_in: false,
            next,
            error: Some("Wrong password."),
        };
        return Ok((StatusCode::UNAUTHORIZED, page).into_response());
    }

    let cookie = state.auth.session_cookie(chrono::Utc::now().timestamp());
    tracing::info!("logged in");
    Ok(([(header::SET_COOKIE, cookie)], Redirect::to(&next)).into_response())
}

pub async fn logout() -> impl IntoResponse {
    let cookie = format!("{COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict");
    tracing::info!("logged out");
    ([(header::SET_COOKIE, cookie)], Redirect::to("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> Auth {
        Auth {
            password: Some("secret".into()),
            key: [7; 32],
        }
    }

    #[test]
    fn sessions_are_signed_and_expire() {
        let auth = auth();
        let cookie = auth.session_cookie(1000);
        let value = cookie
            .strip_prefix("session=")
            .and_then(|c| c.split(';').next())
            .unwrap();
        assert!(auth.valid_session(value, 1000));
        assert!(!auth.valid_session(value, 1000 + SESSION_DAYS * 24 * 3600));

        let (expiry, signature) = value.split_once('.').unwrap();
        let forged = format!("{}.{signature}", expiry.parse::<i64>().unwrap() + 1);
        assert!(!auth.valid_session(&forged, 1000));
        assert!(!auth.valid_session("garbage", 1000));

        let other = Auth {
            key: [8; 32],
            ..auth
        };
        assert!(!other.valid_session(value, 1000));
    }

    #[test]
    fn password_check() {
        assert!(auth().password_matches("secret"));
        assert!(!auth().password_matches("Secret"));
        assert!(!auth().password_matches(""));
        let disabled = Auth {
            password: None,
            key: [0; 32],
        };
        assert!(!disabled.password_matches(""));
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
