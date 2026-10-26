use std::str::FromStr;
use std::time::Duration;

use chrono::NaiveDate;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, Transaction};

use crate::web::error::{AppError, AppResult};

pub type Pool = SqlitePool;

// Two sqlx 0.7 + SQLite pitfalls, both seen as flaky "database is locked" /
// stale reads under concurrency:
// - never use `INSERT ... RETURNING` with `fetch_one`: the statement isn't
//   run to completion, so its autocommit isn't committed (other connections
//   don't see the row, the write lock stays held). Use `execute()` and
//   `last_insert_rowid()`.
// - start write transactions with `begin_write`, not `pool.begin()`.

/// Today, in the server's time zone: the date stored by refreshes and
/// lookups (`YYYY-MM-DD` with `to_string()`), and the date the collection is
/// valued on.
pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Commit, or roll back for a dry run (the maintenance commands and the
/// refreshes, `--dry-run`).
pub async fn commit_unless_dry_run(tx: Transaction<'_, Sqlite>, dry_run: bool) -> sqlx::Result<()> {
    if dry_run {
        tx.rollback().await
    } else {
        tx.commit().await
    }
}

/// WAL so readers never block the writer; writers wait for each other.
pub fn connect_options(db_url: &str) -> SqliteConnectOptions {
    SqliteConnectOptions::from_str(db_url)
        .expect("invalid DATABASE_URL")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(10))
}

/// Start a transaction holding the write lock from the start (`BEGIN
/// IMMEDIATE`, which sqlx 0.7 can't issue). With a plain deferred `BEGIN`, a
/// transaction that reads then writes fails at once with "database is
/// locked" when another connection (e.g. the background card-data linking)
/// wrote in between; holding the lock makes it wait instead. Use this for
/// every transaction that writes.
pub async fn begin_write(pool: &Pool) -> sqlx::Result<Transaction<'static, Sqlite>> {
    let mut tx = pool.begin().await?;
    // a write statement matching no row: takes the write lock, changes nothing
    sqlx::query("UPDATE deck SET id = id WHERE 0")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// Whether `table` has a row with this id.
pub async fn row_exists(conn: &mut SqliteConnection, table: &str, id: i64) -> sqlx::Result<bool> {
    let found: Option<i64> = sqlx::query_scalar(&format!("SELECT id FROM {table} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(found.is_some())
}

/// A 404 unless `table` has a row with this id.
pub async fn exists(conn: &mut SqliteConnection, table: &str, id: i64) -> AppResult<()> {
    if row_exists(conn, table, id).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn init_pool() -> Pool {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:data/app.db".to_string());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options(&db_url))
        .await
        .expect("failed to connect to sqlite database");

    sqlx::migrate!("../../migrations")
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
        f.write_str(&crate::web::filters::money(&self.0).map_err(|_| std::fmt::Error)?)
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

/// Serialised as a string with two decimals, e.g. `"12.50"` (exact in JSON).
impl serde::Serialize for Money {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Push `(?, ?, …)` binding these ids, for an `IN` list.
pub fn push_ids(qb: &mut QueryBuilder<'_, Sqlite>, ids: impl IntoIterator<Item = i64>) {
    qb.push("(");
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    sep.push_unseparated(")");
}
