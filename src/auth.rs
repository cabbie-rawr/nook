//! Supabase Auth integration: signup/login/logout go through Supabase's
//! GoTrue REST API (`/auth/v1/...`) rather than our own password hashing and
//! sessions table. The access token Supabase issues is a JWT; we verify it
//! locally on every request (HS256, using the project's JWT secret) instead
//! of calling out to Supabase each time, and hold it — plus a refresh token,
//! for silent renewal once it expires — in two httponly cookies.
//!
//! `CurrentUser` / `OptionalCurrentUser` are unchanged in shape from before:
//! every protected handler still just asks for one and gets a `models::User`.

use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tower_cookies::cookie::time::Duration as CookieDuration;
use tower_cookies::{Cookie, Cookies};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{Profile, User};
use crate::state::AppState;

pub const ACCESS_COOKIE: &str = "sb_access_token";
pub const REFRESH_COOKIE: &str = "sb_refresh_token";
const REFRESH_COOKIE_DAYS: i64 = 30;

/// Holds the PKCE code_verifier between "redirect to the provider" and "the
/// provider redirects back" — a few minutes at most, so a short cookie life
/// is plenty and limits how long a stale value could be replayed.
pub const OAUTH_VERIFIER_COOKIE: &str = "sb_oauth_verifier";

fn cookies_are_secure() -> bool {
    std::env::var("NOOK_ENV").map(|v| v == "production").unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Supabase Auth REST calls
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SupabaseSession {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub expires_in: Option<i64>,
    pub user: SupabaseUser,
}

#[derive(Debug, Deserialize)]
pub struct SupabaseUser {
    pub id: Uuid,
    #[serde(default)]
    pub email: Option<String>,
}

/// Supabase's error bodies aren't perfectly consistent across endpoints/
/// versions — try the field names it's known to use, in order, before
/// falling back to a generic message.
#[derive(Debug, Deserialize, Default)]
struct SupabaseErrorBody {
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

async fn supabase_error(resp: reqwest::Response) -> AppError {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let parsed: SupabaseErrorBody = serde_json::from_str(&body).unwrap_or_default();
    let message = parsed
        .msg
        .or(parsed.error_description)
        .or(parsed.message)
        .or(parsed.error)
        .unwrap_or_else(|| format!("Authentication request failed ({status})."));
    tracing::warn!(%status, body, "supabase auth error");
    AppError::Auth(message)
}

pub async fn sign_up(
    state: &AppState,
    email: &str,
    password: &str,
    display_name: &str,
    mode: &str,
) -> Result<SupabaseSession, AppError> {
    let resp = state
        .http
        .post(format!("{}/auth/v1/signup", state.supabase_url))
        .header("apikey", &state.supabase_anon_key)
        .json(&json!({
            "email": email,
            "password": password,
            "data": { "display_name": display_name, "mode": mode },
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(supabase_error(resp).await);
    }

    let text = resp.text().await?;
    // If the project has "Confirm email" turned on, signup succeeds but
    // returns a user with no session (they must click the emailed link
    // first) — surface that as a clear message rather than a decode panic.
    serde_json::from_str::<SupabaseSession>(&text).map_err(|_| {
        AppError::Auth("Account created — check your email to confirm it before logging in.".into())
    })
}

pub async fn sign_in(state: &AppState, email: &str, password: &str) -> Result<SupabaseSession, AppError> {
    let resp = state
        .http
        .post(format!("{}/auth/v1/token?grant_type=password", state.supabase_url))
        .header("apikey", &state.supabase_anon_key)
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(supabase_error(resp).await);
    }
    Ok(resp.json().await?)
}

async fn refresh_session(state: &AppState, refresh_token: &str) -> Result<SupabaseSession, AppError> {
    let resp = state
        .http
        .post(format!("{}/auth/v1/token?grant_type=refresh_token", state.supabase_url))
        .header("apikey", &state.supabase_anon_key)
        .json(&json!({ "refresh_token": refresh_token }))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(supabase_error(resp).await);
    }
    Ok(resp.json().await?)
}

/// Best-effort — revokes the refresh token server-side. Cookie clearing
/// (which is what actually logs the browser out) happens regardless of
/// whether this call succeeds.
pub async fn sign_out(state: &AppState, access_token: &str) {
    let result = state
        .http
        .post(format!("{}/auth/v1/logout", state.supabase_url))
        .header("apikey", &state.supabase_anon_key)
        .bearer_auth(access_token)
        .send()
        .await;
    if let Err(err) = result {
        tracing::warn!(?err, "supabase logout request failed (cookies are cleared regardless)");
    }
}

// ---------------------------------------------------------------------------
// OAuth (Google / GitHub via Supabase Auth) — authorization-code + PKCE.
// No client secret ever lives in this app; Google's and GitHub's are
// configured directly in the Supabase dashboard (Authentication > Providers).
// This app only needs Supabase's own anon key, same as password auth.
// ---------------------------------------------------------------------------

/// Providers this app offers on the login/signup buttons. Supabase supports
/// many more, but only accept what we actually render a button for — an
/// unrecognized value goes straight into a URL query param otherwise.
pub fn is_supported_oauth_provider(provider: &str) -> bool {
    matches!(provider, "google" | "github")
}

/// A random RFC 7636 code_verifier (43 base64url chars from 32 random
/// bytes) and its S256 code_challenge.
pub fn generate_pkce_pair() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// The URL to send the browser to in order to start the provider's login —
/// Supabase handles the provider-specific dance and redirects back to
/// `redirect_to` with either `?code=...` or `?error=...`.
pub fn oauth_authorize_url(state: &AppState, provider: &str, code_challenge: &str, redirect_to: &str) -> String {
    format!(
        "{}/auth/v1/authorize?provider={}&code_challenge={}&code_challenge_method=s256&redirect_to={}",
        state.supabase_url,
        urlencoding_component(provider),
        urlencoding_component(code_challenge),
        urlencoding_component(redirect_to),
    )
}

/// Minimal percent-encoding for the handful of query values we build URLs
/// with ourselves — avoids pulling in a whole URL-building crate for it.
fn urlencoding_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Exchanges the `code` Supabase's redirect handed back, plus the
/// code_verifier this app generated before the redirect, for a real session
/// — same shape (and same cookie-setting path) as password sign-in.
pub async fn exchange_oauth_code(state: &AppState, code: &str, code_verifier: &str) -> Result<SupabaseSession, AppError> {
    let resp = state
        .http
        .post(format!("{}/auth/v1/token?grant_type=pkce", state.supabase_url))
        .header("apikey", &state.supabase_anon_key)
        .json(&json!({ "auth_code": code, "code_verifier": code_verifier }))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(supabase_error(resp).await);
    }
    Ok(resp.json().await?)
}

pub fn set_oauth_verifier_cookie(cookies: &Cookies, verifier: &str) {
    let mut cookie = Cookie::new(OAUTH_VERIFIER_COOKIE, verifier.to_string());
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_secure(cookies_are_secure());
    cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    cookie.set_max_age(CookieDuration::minutes(10));
    cookies.add(cookie);
}

pub fn take_oauth_verifier_cookie(cookies: &Cookies) -> Option<String> {
    let value = cookies.get(OAUTH_VERIFIER_COOKIE).map(|c| c.value().to_string());
    let mut removal = Cookie::new(OAUTH_VERIFIER_COOKIE, "");
    removal.set_path("/");
    cookies.remove(removal);
    value
}

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

pub fn set_session_cookies(cookies: &Cookies, session: &SupabaseSession) {
    let mut access = Cookie::new(ACCESS_COOKIE, session.access_token.clone());
    access.set_path("/");
    access.set_http_only(true);
    access.set_secure(cookies_are_secure());
    access.set_same_site(tower_cookies::cookie::SameSite::Lax);
    access.set_max_age(CookieDuration::seconds(session.expires_in.unwrap_or(3600)));
    cookies.add(access);

    let mut refresh = Cookie::new(REFRESH_COOKIE, session.refresh_token.clone());
    refresh.set_path("/");
    refresh.set_http_only(true);
    refresh.set_secure(cookies_are_secure());
    refresh.set_same_site(tower_cookies::cookie::SameSite::Lax);
    refresh.set_max_age(CookieDuration::days(REFRESH_COOKIE_DAYS));
    cookies.add(refresh);
}

pub fn clear_session_cookies(cookies: &Cookies) {
    for name in [ACCESS_COOKIE, REFRESH_COOKIE] {
        let mut removal = Cookie::new(name, "");
        removal.set_path("/");
        cookies.remove(removal);
    }
}

// ---------------------------------------------------------------------------
// JWT verification
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    #[serde(default)]
    email: Option<String>,
    exp: usize,
}

pub fn build_decoding_key(jwt_secret: &str) -> DecodingKey {
    DecodingKey::from_secret(jwt_secret.as_bytes())
}

fn verify_access_token(token: &str, key: &DecodingKey) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_aud = false; // Supabase sets aud="authenticated"; we only need `sub`/`email`.
    Ok(decode::<Claims>(token, key, &validation)?.claims)
}

async fn load_profile(state: &AppState, user_id: Uuid) -> Result<Option<Profile>, AppError> {
    Ok(sqlx::query_as::<_, Profile>(
        "SELECT id, display_name, mode, timezone, theme_preference, created_at FROM profiles WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?)
}

/// Resolves the current user from cookies, transparently refreshing an
/// expired access token via the refresh token when possible (and rewriting
/// the cookies with the new pair) — same as a browser SDK's silent refresh,
/// just done server-side since this app has no client-side Supabase JS.
async fn resolve_user(state: &AppState, cookies: &Cookies) -> Result<Option<User>, AppError> {
    let Some(access_token) = cookies.get(ACCESS_COOKIE).map(|c| c.value().to_string()) else {
        return Ok(None);
    };

    let claims = match verify_access_token(&access_token, &state.jwt_decoding_key) {
        Ok(claims) => claims,
        Err(_) => {
            // Could be expired, could be missing/garbled — either way, only a
            // refresh token gives us a path back to a valid session.
            let Some(refresh_token) = cookies.get(REFRESH_COOKIE).map(|c| c.value().to_string()) else {
                return Ok(None);
            };
            let Ok(session) = refresh_session(state, &refresh_token).await else {
                clear_session_cookies(cookies);
                return Ok(None);
            };
            set_session_cookies(cookies, &session);
            match verify_access_token(&session.access_token, &state.jwt_decoding_key) {
                Ok(claims) => claims,
                Err(_) => return Ok(None), // shouldn't happen: we just minted this token
            }
        }
    };

    let Some(profile) = load_profile(state, claims.sub).await? else {
        // JWT is valid but the profiles row hasn't landed yet — signup's
        // trigger fires in the same transaction as auth.users, so this is
        // essentially never expected, but fail closed instead of panicking.
        return Ok(None);
    };

    Ok(Some(User {
        id: profile.id,
        email: claims.email.unwrap_or_default(),
        display_name: profile.display_name,
        mode: profile.mode,
        timezone: profile.timezone,
        theme_preference: profile.theme_preference,
        created_at: profile.created_at,
    }))
}

/// The logged-in user, resolved from the session cookies. Rejects
/// (redirecting to `/login`) when there's no valid session.
pub struct CurrentUser(pub User);

#[async_trait]
impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        match OptionalCurrentUser::from_request_parts(parts, state).await?.0 {
            Some(user) => Ok(CurrentUser(user)),
            None => Err(AppError::Unauthorized),
        }
    }
}

/// Same lookup as `CurrentUser`, but never rejects — for pages like `/login`
/// that behave differently when someone is already signed in.
pub struct OptionalCurrentUser(pub Option<User>);

#[async_trait]
impl FromRequestParts<AppState> for OptionalCurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let cookies = Cookies::from_request_parts(parts, state).await.map_err(|_| AppError::Unauthorized)?;
        Ok(OptionalCurrentUser(resolve_user(state, &cookies).await?))
    }
}
