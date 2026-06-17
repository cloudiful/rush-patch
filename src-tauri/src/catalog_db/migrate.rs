use crate::catalog_db::CatalogDbError;
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;

pub(crate) static MIGRATOR: Migrator = sqlx::migrate!("sql/migrations");

pub(crate) async fn migrate_catalog_db(pool: &SqlitePool) -> Result<(), CatalogDbError> {
    MIGRATOR.run(pool).await?;
    Ok(())
}
