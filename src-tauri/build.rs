fn main() {
    if std::env::var_os("DATABASE_URL").is_none() {
        let manifest_dir =
            std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
        let database_path = manifest_dir.join("dev").join("catalog-dev.sqlite");
        println!(
            "cargo:rustc-env=DATABASE_URL=sqlite:{}",
            database_path.display().to_string().replace('\\', "/")
        );
    }
    println!("cargo:rerun-if-env-changed=DATABASE_URL");
    println!("cargo:rerun-if-changed=sql/migrations/0001_catalog.sql");
    tauri_build::build()
}
