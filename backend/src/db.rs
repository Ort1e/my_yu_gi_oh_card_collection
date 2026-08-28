use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

pub type Pool = SqlitePool;

pub async fn init_pool() -> Pool {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:app.db".to_string());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("failed to connect to sqlite database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    pool
}
