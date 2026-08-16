use jsonwebtoken::DecodingKey;
use sqlx::PgPool;

/// Shared app state. `supabase_url`/`supabase_anon_key` are used for the
/// Auth REST calls (signup/login/logout/OAuth — see `auth.rs`); `jwt_decoding_key`
/// lets every request verify a Supabase-issued access token locally (HS256)
/// instead of round-tripping to Supabase on every page load. `app_base_url`
/// is this app's own externally-reachable origin, used to build the OAuth
/// callback URL Supabase redirects back to.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub http: reqwest::Client,
    pub supabase_url: String,
    pub supabase_anon_key: String,
    pub jwt_decoding_key: DecodingKey,
    pub app_base_url: String,
}
