mod auth;
mod error;
mod handlers;
mod models;
mod state;
mod templates;
mod view;

use axum::routing::{get, post};
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_cookies::CookieManagerLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "nook=debug,tower_http=debug".into()))
        .init();

    // `.env` shipping with blank `KEY=` lines (as this project's does, so the
    // file exists with the right shape before you've filled it in) makes
    // `std::env::var` return `Ok("")`, not `Err` — check for blank too, or a
    // missing value silently turns into a much more confusing error deep
    // inside sqlx/reqwest instead of this message.
    fn require_env(key: &str, hint: &str) -> anyhow::Result<String> {
        match std::env::var(key) {
            Ok(v) if !v.trim().is_empty() => Ok(v),
            _ => Err(anyhow::anyhow!("{key} is not set — see .env.example ({hint})")),
        }
    }

    let database_url = require_env("DATABASE_URL", "Supabase Project Settings → Database → Connection string")?;
    let supabase_url =
        require_env("SUPABASE_URL", "Project Settings → API")?.trim_end_matches('/').to_string();
    let supabase_anon_key = require_env("SUPABASE_ANON_KEY", "Project Settings → API")?;
    let supabase_jwt_secret =
        require_env("SUPABASE_JWT_SECRET", "Project Settings → API → JWT Settings")?;
    // This app's own reachable origin — used to build the OAuth callback URL
    // Supabase redirects back to after Google/GitHub. Defaults to where this
    // binary listens locally; set it explicitly once deployed anywhere else.
    let app_base_url = std::env::var("APP_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:3000".to_string())
        .trim_end_matches('/')
        .to_string();

    let pool = PgPoolOptions::new().max_connections(5).connect(&database_url).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("migrations applied");

    tokio::fs::create_dir_all("storage").await.ok();

    let state = AppState {
        pool,
        http: reqwest::Client::new(),
        supabase_url,
        supabase_anon_key,
        jwt_decoding_key: auth::build_decoding_key(&supabase_jwt_secret),
        app_base_url,
    };

    let app = Router::new()
        // Today (home)
        .route("/", get(handlers::today::shell))
        .route("/partials/focus", get(handlers::today::focus))
        .route("/partials/up_next", get(handlers::today::up_next))
        .route("/partials/due_soon", get(handlers::today::due_soon))
        .route("/partials/momentum", get(handlers::today::momentum))
        .route("/partials/jump_back_in", get(handlers::today::jump_back_in))
        .route("/partials/space_progress", get(handlers::today::space_progress))
        .route("/partials/getting_started", get(handlers::today::getting_started))
        .route("/onboarding/dismiss", post(handlers::today::dismiss_onboarding))
        .route("/api/search", get(handlers::today::search))
        .route("/api/layout", get(handlers::today::get_layout).post(handlers::today::save_layout))
        // Auth
        .route("/signup", get(handlers::auth::signup_form).post(handlers::auth::signup))
        .route("/login", get(handlers::auth::login_form).post(handlers::auth::login))
        .route("/logout", post(handlers::auth::logout))
        .route("/auth/oauth/:provider", get(handlers::auth::oauth_start))
        .route("/auth/callback", get(handlers::auth::oauth_callback))
        // Spaces
        .route("/spaces", get(handlers::spaces::dashboard).post(handlers::spaces::create_space))
        .route("/spaces/:id", get(handlers::spaces::space_detail))
        .route("/spaces/:id/edit", post(handlers::spaces::edit_space))
        .route("/spaces/:id/archive", post(handlers::spaces::archive_space))
        .route("/spaces/:id/delete", post(handlers::spaces::delete_space))
        .route("/spaces/:id/tasks", post(handlers::tasks::create_task))
        // Tasks
        .route("/tasks/:id", get(handlers::task_detail::show))
        .route("/tasks/:id/status", post(handlers::tasks::update_status))
        .route("/tasks/:id/delete", post(handlers::tasks::delete_task))
        .route("/tasks/:id/complete-toggle", post(handlers::task_detail::complete_toggle))
        .route("/tasks/:id/log-minutes", post(handlers::task_detail::log_minutes))
        .route("/tasks/:id/plan-steps.json", get(handlers::task_detail::plan_steps_json))
        .route("/tasks/:id/steps", post(handlers::task_detail::create_step))
        .route("/tasks/:id/attachments", post(handlers::task_detail::upload_attachment))
        .route("/steps/:id/toggle", post(handlers::task_detail::toggle_step))
        .route("/steps/:id/delete", post(handlers::task_detail::delete_step))
        .route("/attachments/:id/download", get(handlers::task_detail::download_attachment))
        .route("/attachments/:id/delete", post(handlers::task_detail::delete_attachment))
        // Calendar / schedule blocks
        .route("/calendar", get(handlers::calendar::show))
        .route("/schedule-blocks", post(handlers::calendar::create))
        .route("/schedule-blocks/:id/delete", post(handlers::calendar::delete))
        // Static assets
        .nest_service("/static", ServeDir::new("static"))
        .layer(TraceLayer::new_for_http())
        .layer(CookieManagerLayer::new())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    tracing::info!("listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
