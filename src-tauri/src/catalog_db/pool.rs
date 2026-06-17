use crate::catalog_db::CatalogDbError;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use std::path::Path;
use std::time::Duration;

pub async fn open_pool(
    path: &Path,
    create_if_missing: bool,
    max_connections: u32,
) -> Result<SqlitePool, CatalogDbError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create_if_missing)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_millis(5_000));

    Ok(SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await?)
}
