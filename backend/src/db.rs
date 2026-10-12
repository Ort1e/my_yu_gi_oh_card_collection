use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

pub type Pool = SqlitePool;

pub async fn init_pool() -> Pool {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:app.db".to_string());

    let options = SqliteConnectOptions::from_str(&db_url)
        .expect("invalid DATABASE_URL")
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .expect("failed to connect to sqlite database");

    sqlx::migrate!("../migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    pool
}

/// A money amount. Stored as TEXT (exact decimal, see the migration),
/// displayed with two decimals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Money(pub rust_decimal::Decimal);

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::filters::money(&self.0).map_err(|_| std::fmt::Error)?)
    }
}

impl sqlx::Type<sqlx::Sqlite> for Money {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }

    fn compatible(ty: &sqlx::sqlite::SqliteTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for Money {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let text = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        Ok(Money(text.parse()?))
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for Money {
    fn encode_by_ref(
        &self,
        buf: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'q>>,
    ) -> sqlx::encode::IsNull {
        <String as sqlx::Encode<sqlx::Sqlite>>::encode(self.0.to_string(), buf)
    }
}
