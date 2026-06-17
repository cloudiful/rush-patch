use rush_patch::catalog_db;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_dir = manifest_dir.join("dev");
    std::fs::create_dir_all(&dev_dir)?;
    let db_path = dev_dir.join("catalog-dev.sqlite");
    remove_if_exists(&db_path)?;
    remove_if_exists(&db_path.with_extension("sqlite-shm"))?;
    remove_if_exists(&db_path.with_extension("sqlite-wal"))?;
    let pool = catalog_db::open_pool(&db_path, true, 1).await?;
    sqlx::migrate!("sql/migrations").run(&pool).await?;
    sqlx::query_file!("sql/catalog/meta/upsert.sql", "format_version", "4")
        .execute(&pool)
        .await?;
    println!("{}", db_path.display());
    Ok(())
}

fn remove_if_exists(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Box::new(error)),
    }
}
