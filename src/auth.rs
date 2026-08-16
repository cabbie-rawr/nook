//! Password hashing, session tokens, and the `CurrentUser` / `OptionalCurrentUser`
//! extractors that every protected handler pulls the logged-in user from.

use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sqlx::SqlitePool;
use tower_cookies::cookie::time::Duration as CookieDuration;
use tower_cookies::{Cookie, Cookies};

use crate::error::AppError;
use crate::models::User;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "session_id";
const SESSION_DAYS: i64 = 30;

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("failed to hash password: {e}"))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

fn new_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Creates a session row for `user_id` and returns the cookie to set on the response.
pub async fn start_session(pool: &SqlitePool, user_id: i64) -> Result<Cookie<'static>, AppError> {
    let token = new_session_token();
    let expires_at: DateTime<Utc> = Utc::now() + Duration::days(SESSION_DAYS);

    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&token)
        .bind(user_id)
        .bind(expires_at)
        .execute(pool)
        .await?;

    let mut cookie = Cookie::new(SESSION_COOKIE, token);
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    cookie.set_max_age(CookieDuration::days(SESSION_DAYS));
    Ok(cookie)
}

/// Deletes the session server-side and returns the cookie that expires it in the browser.
pub async fn end_session(pool: &SqlitePool, cookies: &Cookies) -> Result<(), AppError> {
    if let Some(existing) = cookies.get(SESSION_COOKIE) {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(existing.value())
            .execute(pool)
            .await?;
    }
    let mut removal = Cookie::new(SESSION_COOKIE, "");
    removal.set_path("/");
    cookies.remove(removal);
    Ok(())
}

/// The logged-in user, resolved from the session cookie. Rejects (redirecting to
/// `/login`) when the cookie is missing, unknown, or expired.
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
        let cookies = Cookies::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Unauthorized)?;

        let Some(token) = cookies.get(SESSION_COOKIE).map(|c| c.value().to_string()) else {
            return Ok(OptionalCurrentUser(None));
        };

        let user = sqlx::query_as::<_, User>(
            r#"SELECT users.id, users.email, users.password_hash, users.display_name,
                      users.mode, users.timezone, users.theme_preference, users.created_at
               FROM users
               JOIN sessions ON sessions.user_id = users.id
               WHERE sessions.id = ? AND sessions.expires_at > ?"#,
        )
        .bind(&token)
        .bind(Utc::now())
        .fetch_optional(&state.pool)
        .await?;

        Ok(OptionalCurrentUser(user))
    }
}
