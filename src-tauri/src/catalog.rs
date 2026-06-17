use crate::catalog_db;
use crate::domain::TranslationCatalog;
use crate::domain::WorkflowEventPhase;
use crate::extractor_js;
use crate::extractor_json;
use crate::patch_storage;
use crate::scanner;
use crate::workflow_events::WorkflowReporter;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error(transparent)]
    Extract(#[from] extractor_json::ExtractJsonError),
    #[error(transparent)]
    ExtractJs(#[from] extractor_js::ExtractJsError),
    #[error(transparent)]
    Scan(#[from] scanner::ScanError),
    #[error(transparent)]
    CatalogDb(#[from] catalog_db::CatalogDbError),
    #[error("background task failed: {0}")]
    Background(String),
}

pub async fn build_and_persist(
    game_root: &str,
    reporter: &WorkflowReporter,
) -> Result<(TranslationCatalog, PathBuf), CatalogError> {
    let source_root = Path::new(game_root);
    let catalog_path = catalog_db::catalog_path(source_root);
    let reusable = catalog_db::load_reusable_translations(&catalog_path).await?;
    reporter.info_key(
        WorkflowEventPhase::Catalog,
        "workflow.catalog.checkReuse",
        "Loading reusable translations",
        Some(format!("正在检查 {} 中可复用的旧译文", source_root.display())),
    );
    let build_reporter = reporter.clone();
    let game_root_owned = game_root.to_owned();
    let mut catalog =
        tokio::task::spawn_blocking(move || build_catalog(&game_root_owned, &build_reporter))
            .await
            .map_err(|error| CatalogError::Background(error.to_string()))??;
    let mut reused_units = 0usize;
    for unit in &mut catalog.units {
        if let Some(previous) = reusable.get(&(unit.id.clone(), unit.source_text.clone())) {
            unit.translated_text = Some(previous.translated_text.clone());
            unit.status = previous.status.clone();
            reused_units += 1;
        }
    }
    reporter.info_key(
        WorkflowEventPhase::Catalog,
        "workflow.catalog.reuseApplied",
        "Reusable translations applied",
        Some(format!("已复用 {reused_units} 条历史译文")),
    );
    let catalog_path = catalog_db::persist_catalog(source_root, &catalog, reporter).await?;

    Ok((catalog, catalog_path))
}

pub async fn ensure_catalog_path(
    game_root: &str,
    reporter: &WorkflowReporter,
) -> Result<PathBuf, CatalogError> {
    let source_root = Path::new(game_root);
    let path = catalog_db::catalog_path(source_root);
    if catalog_db::is_catalog_valid(&path).await? {
        reporter.info_key(
            WorkflowEventPhase::Catalog,
            "workflow.catalog.reuseReady",
            "Reusing existing SQLite catalog",
            Some("已直接复用现有文本缓存，无需重新提取".to_owned()),
        );
        return Ok(path);
    }

    let (_catalog, path) = build_and_persist(game_root, reporter).await?;
    Ok(path)
}

pub fn build_catalog(
    game_root: &str,
    reporter: &WorkflowReporter,
) -> Result<TranslationCatalog, CatalogError> {
    reporter.info_key(
        WorkflowEventPhase::Scan,
        "workflow.scan.start",
        "Scanning project files",
        Some("正在检查游戏目录与可提取文件".to_owned()),
    );
    let summary = scanner::scan_project(game_root)?;
    let source_root = Path::new(game_root);
    let data_files = patch_storage::source_files_for(source_root, &summary.data_files);
    let plugin_files = patch_storage::source_files_for(source_root, &summary.plugin_files);
    reporter.info_key(
        WorkflowEventPhase::Scan,
        "workflow.scan.done",
        "Project scan complete",
        Some(format!("找到 {} 个 JSON 文件和 {} 个 JS 文件", data_files.len(), plugin_files.len())),
    );
    let (mut spans, mut units) = extractor_json::extract_entries(&data_files, reporter)?;
    let (js_spans, js_units) = extractor_js::extract_entries(&plugin_files, reporter)?;
    spans.extend(js_spans);
    units.extend(js_units);
    reporter.info_key(
        WorkflowEventPhase::Catalog,
        "workflow.catalog.extractDone",
        "Text extraction complete",
        Some(format!("已提取 {} 条文本单元，关联 {} 个写回片段", units.len(), spans.len())),
    );

    Ok(TranslationCatalog {
        project: crate::domain::CatalogProject {
            game_root: game_root.to_owned(),
            engine: format!("{:?}", summary.engine),
            generated_at: unix_timestamp_string(),
        },
        spans,
        units,
    })
}

fn unix_timestamp_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch_storage;
    use std::fs;
    use std::thread::sleep;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("rush_patch_catalog_{name}_{stamp}"))
    }

    fn cleanup_root(root: &Path) {
        let mut last_error = None;
        for _ in 0..40 {
            match fs::remove_dir_all(root) {
                Ok(()) => return,
                Err(error) => {
                    last_error = Some(error);
                    sleep(std::time::Duration::from_millis(100));
                }
            }
        }
        panic!("cleanup: {:?}", last_error.expect("cleanup error"));
    }

    #[test]
    fn rebuild_uses_original_backup_content_when_present() {
        let root = temp_root("backup");
        let data_dir = root.join("www").join("data");
        let game_file = data_dir.join("Map001.json");
        let backup_file = patch_storage::backup_root(&root)
            .join("www")
            .join("data")
            .join("Map001.json");

        fs::create_dir_all(&data_dir).expect("create data dir");
        fs::create_dir_all(backup_file.parent().expect("backup parent"))
            .expect("create backup dir");
        fs::write(
            &game_file,
            r#"{"events":{"1":{"pages":[{"list":[{"code":401,"parameters":["Translated now"]}]}]}}}"#,
        )
        .expect("write current file");
        fs::write(
            &backup_file,
            r#"{"events":{"1":{"pages":[{"list":[{"code":401,"parameters":["Original line"]}]}]}}}"#,
        )
        .expect("write backup file");

        let catalog = build_catalog(root.to_str().expect("utf8 path"), &WorkflowReporter::noop())
            .expect("build catalog");
        let dialogue = catalog
            .units
            .iter()
            .find(|unit| unit.semantic_kind == "dialogue")
            .expect("dialogue unit");

        assert_eq!(dialogue.source_text, "Original line");

        cleanup_root(&root);
    }

    #[tokio::test]
    async fn ensure_catalog_reuses_existing_db_without_rebuild() {
        let root = temp_root("ensure");
        let data_dir = root.join("www").join("data");
        fs::create_dir_all(&data_dir).expect("create data dir");
        fs::write(
            data_dir.join("Map001.json"),
            r#"{"events":{"1":{"pages":[{"list":[{"code":401,"parameters":["Original line"]}]}]}}}"#,
        )
        .expect("write source file");

        let first =
            ensure_catalog_path(root.to_str().expect("utf8 root"), &WorkflowReporter::noop())
                .await
                .expect("first build");
        fs::write(
            data_dir.join("Map001.json"),
            r#"{"events":{"1":{"pages":[{"list":[{"code":401,"parameters":["Changed line"]}]}]}}}"#,
        )
        .expect("mutate source file");

        let second =
            ensure_catalog_path(root.to_str().expect("utf8 root"), &WorkflowReporter::noop())
                .await
                .expect("reuse catalog");
        assert_eq!(first, second);

        let loaded = crate::catalog_db::load_catalog(&second)
            .await
            .expect("load catalog");
        assert_eq!(loaded.catalog.units[0].source_text, "Original line");

        cleanup_root(&root);
    }

    #[tokio::test]
    async fn ensure_catalog_emits_reuse_event_without_rebuild_logs() {
        let root = temp_root("reuse_event");
        let data_dir = root.join("www").join("data");
        fs::create_dir_all(&data_dir).expect("create data dir");
        fs::write(
            data_dir.join("Map001.json"),
            r#"{"events":{"1":{"pages":[{"list":[{"code":401,"parameters":["Original line"]}]}]}}}"#,
        )
        .expect("write source file");

        let initial_reporter = WorkflowReporter::noop();
        ensure_catalog_path(root.to_str().expect("utf8 root"), &initial_reporter)
            .await
            .expect("initial build");

        let (reporter, events) = WorkflowReporter::collector(false);
        ensure_catalog_path(root.to_str().expect("utf8 root"), &reporter)
            .await
            .expect("reuse db");
        let messages = events
            .lock()
            .expect("collector lock")
            .iter()
            .map(|event| event.message.clone())
            .collect::<Vec<_>>();

        assert!(
            messages
                .iter()
                .any(|message| message == "Reusing existing SQLite catalog")
        );
        assert!(
            !messages
                .iter()
                .any(|message| message == "Writing catalog chunks")
        );

        cleanup_root(&root);
    }
}
