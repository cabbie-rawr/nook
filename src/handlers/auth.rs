use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use axum::Form;
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::auth::{self, OptionalCurrentUser};
use crate::error::AppError;
use crate::models::UserMode;
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

    let mode_str = match form.mode {
        UserMode::Student => "student",
        UserMode::Work => "work",
    };

    // The `profiles` row is created by Supabase's `on_auth_user_created`
    // trigger (see supabase/schema.sql) the moment auth.users gets this
    // row — nothing more to insert here.
    match auth::sign_up(&state, &email, &form.password, &display_name, mode_str).await {
        Ok(session) => {
            auth::set_session_cookies(&cookies, &session);
            Ok(Redirect::to("/").into_response())
        }
        Err(AppError::Auth(msg)) => Ok(SignupTemplate { error: Some(msg) }.into_response()),
        Err(err) => Err(err),
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

pub async fn login_form(OptionalCurrentUser(user): OptionalCurrentUser) -> impl IntoResponse {
    if user.is_some() {
        return Redirect::to("/").into_response();
    }
    LoginTemplate { error: None }.into_response()
}

pub async fn login(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<LoginForm>,
) -> Result<impl IntoResponse, AppError> {
    let email = form.email.trim().to_lowercase();

    match auth::sign_in(&state, &email, &form.password).await {
        Ok(session) => {
            auth::set_session_cookies(&cookies, &session);
            Ok(Redirect::to("/").into_response())
        }
        // Same message regardless of *why* Supabase rejected it (unknown
        // email vs. wrong password vs. unconfirmed) — matches the original
        // handler's behavior of never letting a login attempt reveal which
        // part was wrong.
        Err(AppError::Auth(_)) => {
            Ok(LoginTemplate { error: Some("Incorrect email or password.".into()) }.into_response())
        }
        Err(err) => Err(err),
    }
}

pub async fn logout(State(state): State<AppState>, cookies: Cookies) -> Result<impl IntoResponse, AppError> {
    if let Some(token) = cookies.get(auth::ACCESS_COOKIE).map(|c| c.value().to_string()) {
        auth::sign_out(&state, &token).await;
    }
    auth::clear_session_cookies(&cookies);
    Ok(Redirect::to("/login"))
}

// ---------------------------------------------------------------------------
// OAuth (Google / GitHub) — /auth/oauth/:provider starts it, Supabase
// redirects the browser through the provider and back to /auth/callback.
// ---------------------------------------------------------------------------

pub async fn oauth_start(
    Path(provider): Path<String>,
    State(state): State<AppState>,
    cookies: Cookies,
) -> Result<impl IntoResponse, AppError> {
    if !auth::is_supported_oauth_provider(&provider) {
        return Err(AppError::NotFound);
    }

    let (verifier, challenge) = auth::generate_pkce_pair();
    auth::set_oauth_verifier_cookie(&cookies, &verifier);

    let redirect_to = format!("{}/auth/callback", state.app_base_url);
    let authorize_url = auth::oauth_authorize_url(&state, &provider, &challenge, &redirect_to);
    Ok(Redirect::to(&authorize_url))
}

#[derive(Deserialize)]
pub struct OAuthCallbackQuery {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

pub async fn oauth_callback(
    Query(params): Query<OAuthCallbackQuery>,
    State(state): State<AppState>,
    cookies: Cookies,
) -> Result<impl IntoResponse, AppError> {
    // A stray/expired visit to this URL shouldn't leave a verifier cookie
    // sitting around either way.
    let verifier = auth::take_oauth_verifier_cookie(&cookies);

    if let Some(description) = params.error_description.or(params.error) {
        return Ok(LoginTemplate { error: Some(description) }.into_response());
    }
    let Some(code) = params.code else {
        return Ok(LoginTemplate { error: Some("Sign-in didn't complete — try again.".into()) }.into_response());
    };
    let Some(verifier) = verifier else {
        return Ok(LoginTemplate {
            error: Some("That sign-in link expired — try again.".into()),
        }
        .into_response());
    };

    match auth::exchange_oauth_code(&state, &code, &verifier).await {
        Ok(session) => {
            auth::set_session_cookies(&cookies, &session);
            Ok(Redirect::to("/").into_response())
        }
        Err(AppError::Auth(msg)) => Ok(LoginTemplate { error: Some(msg) }.into_response()),
        Err(err) => Err(err),
    }
}
