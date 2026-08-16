use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use axum::Form;
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::auth::{end_session, hash_password, start_session, verify_password, OptionalCurrentUser};
use crate::error::AppError;
use crate::models::{User, UserMode};
use crate::state::AppState;
use crate::templates::{LoginTemplate, SignupTemplate};

#[derive(Deserialize)]
pub struct SignupForm {
    pub display_name: String,
    pub email: String,
    pub password: String,
    pub mode: UserMode,
}

pub async fn signup_form(OptionalCurrentUser(user): OptionalCurrentUser) -> impl IntoResponse {
    if user.is_some() {
        return Redirect::to("/").into_response();
    }
    SignupTemplate { error: None }.into_response()
}

pub async fn signup(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<SignupForm>,
) -> Result<impl IntoResponse, AppError> {
    let email = form.email.trim().to_lowercase();
    let display_name = form.display_name.trim().to_string();

    if email.is_empty() || display_name.is_empty() {
        return Ok(SignupTemplate { error: Some("Name and email are required.".into()) }.into_response());
    }
    if form.password.len() < 8 {
        return Ok(
            SignupTemplate { error: Some("Password must be at least 8 characters.".into()) }.into_response(),
        );
    }

    let existing = sqlx::query_scalar::<_, i64>("SELECT id FROM users WHERE email = ?")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;
    if existing.is_some() {
        return Ok(SignupTemplate { error: Some("That email is already registered.".into()) }.into_response());
    }

    let password_hash = hash_password(&form.password)?;

    let user_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (email, password_hash, display_name, mode) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(&email)
    .bind(&password_hash)
    .bind(&display_name)
    .bind(form.mode)
    .fetch_one(&state.pool)
    .await?;

    let cookie = start_session(&state.pool, user_id).await?;
    cookies.add(cookie);

    Ok(Redirect::to("/").into_response())
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginQuery {
    #[serde(default)]
    pub oauth: Option<String>,
}

pub async fn login_form(
    OptionalCurrentUser(user): OptionalCurrentUser,
    Query(query): Query<LoginQuery>,
) -> impl IntoResponse {
    if user.is_some() {
        return Redirect::to("/").into_response();
    }
    let error = match query.oauth.as_deref() {
        Some("unavailable") => Some("Google/GitHub sign-in isn't connected yet — use email for now.".to_string()),
        _ => None,
    };
    LoginTemplate { error }.into_response()
}

pub async fn login(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<LoginForm>,
) -> Result<impl IntoResponse, AppError> {
    let email = form.email.trim().to_lowercase();

    let user = sqlx::query_as::<_, User>(
        r#"SELECT id, email, password_hash, display_name, mode, timezone, theme_preference, created_at
           FROM users WHERE email = ?"#,
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?;

    let Some(user) = user else {
        return Ok(LoginTemplate { error: Some("Incorrect email or password.".into()) }.into_response());
    };
    if !verify_password(&form.password, &user.password_hash) {
        return Ok(LoginTemplate { error: Some("Incorrect email or password.".into()) }.into_response());
    }

    let cookie = start_session(&state.pool, user.id).await?;
    cookies.add(cookie);

    Ok(Redirect::to("/").into_response())
}

pub async fn logout(State(state): State<AppState>, cookies: Cookies) -> Result<impl IntoResponse, AppError> {
    end_session(&state.pool, &cookies).await?;
    Ok(Redirect::to("/login"))
}

// This SQLite build is a temporary fallback (see main.rs) while the real
// Supabase + OAuth backend is waiting on credentials. login.html/signup.html
// are shared between both builds and already render real
// /auth/oauth/:provider links, so this build needs *some* handler behind
// them — a friendly bounce back to /login rather than a raw 404.
pub async fn oauth_start(Path(_provider): Path<String>) -> impl IntoResponse {
    Redirect::to("/login?oauth=unavailable")
}
