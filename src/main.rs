mod app;
mod error;
mod state;
mod todos;

use app::build_app;
use sqlx::mysql::MySqlPoolOptions;
use state::AppState;
use std::env;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL nao foi definida");

    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    tracing::info!("Banco conectado e migrations aplicadas");

    let state = AppState { db: pool };
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    tracing::info!("Servidor rodando em http://127.0.0.1:3000");

    axum::serve(listener, app).await.unwrap();

    Ok(())
}
