mod auth;
mod error;
mod handlers;
mod models;
mod state;
mod templates;
mod view;

use axum::routing::{get, post};
use axum::Router;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
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

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://nook.db".to_string());
    let connect_options = SqliteConnectOptions::from_str(&database_url)?.create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("migrations applied");

    tokio::fs::create_dir_all("storage").await.ok();

    let state = AppState { pool };

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
        // Google/GitHub buttons are baked into the shared login/signup
        // templates; this build has no real OAuth backend yet, so just
        // bounce back to /login instead of 404ing (see handlers/auth.rs).
        .route("/auth/oauth/:provider", get(handlers::auth::oauth_start))
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
