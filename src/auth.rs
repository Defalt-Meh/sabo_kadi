//! Two audiences: the public *user view* and the *admin view*.
//!
//! Source texts and the verbatim spreadsheet rows (`/sources`) are only served
//! to administrators. Everything else (appointments, places, persons, flows,
//! stats, CSV export) is the same for both.
//!
//! * `KADI_ATLAS__ADMIN_PASSWORD` unset → no login exists and everybody gets
//!   the admin view (local development, private deployments).
//! * set → visitors get the user view; `GET /admin/login` asks for the
//!   password (browser Basic-auth dialog, any user name) and sets an HttpOnly
//!   session cookie; `GET /admin/logout` clears it.
//!
//! The cookie holds HMAC-SHA256(password, fixed label), so changing the
//! password logs everybody out. Serve the site over HTTPS (Render does), or
//! the password crosses the network in clear text.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;

use crate::app::AppState;

pub const COOKIE_NAME: &str = "kadi_admin";
const COOKIE_LABEL: &[u8] = b"kadi-atlas admin session v1";
const COOKIE_MAX_AGE_SECS: u32 = 30 * 24 * 3600;
/// Slows down password guessing.
const FAILED_LOGIN_DELAY: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
}

/// Login configuration shared through [`AppState`].
#[derive(Clone, Default)]
pub struct Auth {
    /// Expected cookie value; `None` when no admin password is configured.
    token: Option<Arc<String>>,
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth")
            .field("enabled", &self.enabled())
            .finish()
    }
}

impl Auth {
    pub fn new(admin_password: Option<&str>) -> Self {
        Self {
            token: admin_password.map(|p| Arc::new(session_token(p))),
        }
    }

    /// Whether an admin password is configured (i.e. a user view exists).
    pub fn enabled(&self) -> bool {
        self.token.is_some()
    }

    pub fn role(&self, headers: &HeaderMap) -> Role {
        let Some(expected) = &self.token else {
            return Role::Admin;
        };
        let presented = headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .find(|(name, _)| *name == COOKIE_NAME)
            .map(|(_, value)| value);
        match presented {
            Some(value) if constant_time_eq(value.as_bytes(), expected.as_bytes()) => Role::Admin,
            _ => Role::User,
        }
    }

    fn password_matches(&self, password: &str) -> bool {
        match &self.token {
            Some(expected) => {
                constant_time_eq(session_token(password).as_bytes(), expected.as_bytes())
            }
            None => false,
        }
    }
}

impl FromRequestParts<AppState> for Role {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(state.auth.role(&parts.headers))
    }
}

fn session_token(password: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(password.as_bytes()).expect("HMAC accepts any key length");
    mac.update(COOKIE_LABEL);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Password from an `Authorization: Basic …` header (the user name is ignored).
fn basic_password(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = value
        .strip_prefix("Basic ")
        .or_else(|| value.strip_prefix("basic "))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (_, password) = text.split_once(':')?;
    Some(password.to_string())
}

fn behind_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

fn redirect_home(cookie: Option<String>) -> Response {
    let mut response = (StatusCode::SEE_OTHER, [(header::LOCATION, "/")]).into_response();
    if let Some(cookie) = cookie.and_then(|c| HeaderValue::from_str(&c).ok()) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// `GET /admin/login`
pub async fn login(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let auth = &state.auth;
    if !auth.enabled() {
        return redirect_home(None);
    }

    match basic_password(&headers) {
        Some(password) if auth.password_matches(&password) => {
            let token = auth
                .token
                .as_deref()
                .map(String::as_str)
                .unwrap_or_default();
            let secure = if behind_https(&headers) {
                "; Secure"
            } else {
                ""
            };
            redirect_home(Some(format!(
                "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={COOKIE_MAX_AGE_SECS}{secure}"
            )))
        }
        attempt => {
            if attempt.is_some() {
                tokio::time::sleep(FAILED_LOGIN_DELAY).await;
            }
            (
                StatusCode::UNAUTHORIZED,
                [
                    (
                        header::WWW_AUTHENTICATE,
                        "Basic realm=\"Kadi Atlasi yonetici\", charset=\"UTF-8\"",
                    ),
                    (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
                ],
                "Yönetici parolası gerekli. / Admin password required.",
            )
                .into_response()
        }
    }
}

/// `GET /admin/logout`
pub async fn logout() -> Response {
    redirect_home(Some(format!(
        "{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"
    )))
}

#[derive(Serialize)]
pub struct Session {
    role: Role,
    /// Whether a login exists (an admin password is configured).
    login_enabled: bool,
}

/// `GET /api/v1/session`
pub async fn session(State(state): State<AppState>, role: Role) -> Json<Session> {
    Json(Session {
        role,
        login_enabled: state.auth.enabled(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn open_mode_is_admin_for_everyone() {
        let auth = Auth::new(None);
        assert!(!auth.enabled());
        assert_eq!(auth.role(&HeaderMap::new()), Role::Admin);
    }

    #[test]
    fn cookie_decides_the_role() {
        let auth = Auth::new(Some("gizli"));
        assert_eq!(auth.role(&HeaderMap::new()), Role::User);

        let token = session_token("gizli");
        let ok = headers(&[("cookie", &format!("a=1; {COOKIE_NAME}={token}; b=2"))]);
        assert_eq!(auth.role(&ok), Role::Admin);

        let wrong = headers(&[("cookie", &format!("{COOKIE_NAME}={}", session_token("x")))]);
        assert_eq!(auth.role(&wrong), Role::User);
    }

    #[test]
    fn basic_auth_parsing() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("admin:pa:ss");
        let h = headers(&[("authorization", &format!("Basic {encoded}"))]);
        assert_eq!(basic_password(&h).as_deref(), Some("pa:ss"));
        assert!(Auth::new(Some("pa:ss")).password_matches("pa:ss"));
        assert!(!Auth::new(Some("pa:ss")).password_matches("pass"));
    }
}
