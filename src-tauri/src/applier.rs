use crate::catalog;
use crate::catalog_db;
use crate::domain::WorkflowEventPhase;
use crate::domain::{PatchPlan, RestorePlan, SourceKind, TranslationCatalog, TranslationStatus};
use crate::extractor_js;
use crate::extractor_json::apply_catalog_to_json_value;
use crate::patch_storage;
use crate::text_io;
use crate::validator;
use crate::workflow_events::WorkflowReporter;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error(transparent)]
    Catalog(#[from] catalog::CatalogError),
    #[error(transparent)]
    CatalogDb(#[from] catalog_db::CatalogDbError),
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to copy {from} -> {to}: {source}")]
    CopyFile {
        from: String,
        to: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse JSON {path}: {source}")]
    ParseJson {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to write JSON {path}: {source}")]
    WriteJson {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write JS {path}: {source}")]
    WriteJs {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Js(#[from] extractor_js::ExtractJsError),
    #[error("catalog file is outside game root: {0}")]
    CatalogFileOutsideGameRoot(String),
}

pub async fn apply_translated_patch(
    game_root: &str,
    catalog_path: &Path,
    reporter: &WorkflowReporter,
) -> Result<PatchPlan, ApplyError> {
    let source_root = PathBuf::from(game_root);
    let backup_root = patch_storage::backup_root(&source_root);
    let file_catalogs = catalog_db::load_file_catalogs_for_patch(catalog_path).await?;
    let planned_files = file_catalogs.len();
    let mut backup_state = BackupState::default();
    let mut updated_files = 0usize;
    let mut preserved_failed_units = 0usize;
    let mut validation_failed_units = 0usize;
    let mut validation_warning_units = 0usize;

    for (index, file_catalog) in file_catalogs.iter().enumerate() {
        reporter.progress_key(
            WorkflowEventPhase::Patch,
            "workflow.patch.applyFile",
            "Applying translated patch",
            index + 1,
            planned_files.max(1),
            Some(format!("正在写回第 {}/{} 个文件", index + 1, planned_files)),
        );
        let (validated_catalog, reports) = validator::validated_catalog(&file_catalog.catalog);
        updated_files += apply_file_catalog(
            &source_root,
            &file_catalog.file,
            file_catalog.kind,
            &validated_catalog,
            &mut backup_state,
        )?;
        preserved_failed_units += validated_catalog
            .units
            .iter()
            .filter(|unit| matches!(unit.status, TranslationStatus::Failed))
            .count();
        validation_failed_units += reports
            .iter()
            .filter(|report| matches!(report.status, crate::domain::ValidationStatus::Failed))
            .count();
        validation_warning_units += reports
            .iter()
            .filter(|report| matches!(report.status, crate::domain::ValidationStatus::Warning))
            .count();
        reporter.debug(
            WorkflowEventPhase::Patch,
            "Patch file persisted",
            Some(format!(
                "Updated {} files so far, backups {}",
                updated_files, backup_state.created_backups
            )),
            [
                ("file_index", (index + 1).to_string()),
                ("planned_files", planned_files.to_string()),
            ],
        );
    }

    reporter.info_key(
        WorkflowEventPhase::Patch,
        "workflow.patch.done",
        "Patch apply complete",
        Some(format!(
            "写回完成：更新 {} 个文件，新增 {} 个备份，保留 {} 条失败文本未写入",
            updated_files, backup_state.created_backups, preserved_failed_units
        )),
    );

    Ok(PatchPlan {
        game_root: source_root.display().to_string(),
        backup_root: backup_root.display().to_string(),
        backed_up_files: backup_state.created_backups,
        updated_files,
        preserved_failed_units,
        validation_failed_units,
        validation_warning_units,
    })
}

#[cfg(test)]
pub fn apply_patch_with_catalog(
    game_root: &str,
    catalog: &TranslationCatalog,
) -> Result<PatchPlan, ApplyError> {
    let source_root = PathBuf::from(game_root);
    let backup_root = patch_storage::backup_root(&source_root);
    let reporter = WorkflowReporter::noop();

    let (validated_catalog, reports) = validator::validated_catalog(catalog);
    let mut backup_state = BackupState::default();
    let updated_files = catalog_file_entries(&validated_catalog)
        .into_iter()
        .try_fold(0usize, |count, (file, kind)| {
            apply_file_catalog(
                &source_root,
                &file,
                kind,
                &validated_catalog,
                &mut backup_state,
            )
            .map(|updated| count + updated)
        })?;
    reporter.info_key(
        WorkflowEventPhase::Patch,
        "workflow.patch.done",
        "Patch apply complete",
        Some(format!(
            "写回完成：更新 {} 个文件，新增 {} 个备份",
            updated_files, backup_state.created_backups
        )),
    );
    let preserved_failed_units = validated_catalog
        .units
        .iter()
        .filter(|unit| matches!(unit.status, TranslationStatus::Failed))
        .count();
    let validation_failed_units = reports
        .iter()
        .filter(|report| matches!(report.status, crate::domain::ValidationStatus::Failed))
        .count();
    let validation_warning_units = reports
        .iter()
        .filter(|report| matches!(report.status, crate::domain::ValidationStatus::Warning))
        .count();

    Ok(PatchPlan {
        game_root: source_root.display().to_string(),
        backup_root: backup_root.display().to_string(),
        backed_up_files: backup_state.created_backups,
        updated_files,
        preserved_failed_units,
        validation_failed_units,
        validation_warning_units,
    })
}

pub fn restore_original_text(game_root: &str) -> Result<RestorePlan, ApplyError> {
    let source_root = PathBuf::from(game_root);
    let backup_root = patch_storage::backup_root(&source_root);
    let restored_files = if backup_root.is_dir() {
        restore_tree(&source_root, &backup_root, &backup_root)?
    } else {
        0
    };

    Ok(RestorePlan {
        game_root: source_root.display().to_string(),
        backup_root: backup_root.display().to_string(),
        restored_files,
    })
}

#[derive(Default)]
struct BackupState {
    created_backups: usize,
}

fn ensure_backup(
    source_root: &Path,
    source_file: &Path,
    backup_state: &mut BackupState,
) -> Result<(), ApplyError> {
    let backup_file = patch_storage::backup_file_path(source_root, source_file)
        .ok_or_else(|| ApplyError::CatalogFileOutsideGameRoot(source_file.display().to_string()))?;
    if backup_file.exists() {
        return Ok(());
    }

    if let Some(parent) = backup_file.parent() {
        fs::create_dir_all(parent).map_err(|source| ApplyError::CreateDir {
            path: parent.display().to_string(),
            source,
        })?;
        patch_storage::mark_hidden_work_dir(source_root);
    }

    fs::copy(source_file, &backup_file).map_err(|source| ApplyError::CopyFile {
        from: source_file.display().to_string(),
        to: backup_file.display().to_string(),
        source,
    })?;
    backup_state.created_backups += 1;
    Ok(())
}

fn restore_tree(
    source_root: &Path,
    backup_root: &Path,
    current: &Path,
) -> Result<usize, ApplyError> {
    let mut restored = 0;
    for entry in fs::read_dir(current).map_err(|source| ApplyError::CreateDir {
        path: current.display().to_string(),
        source,
    })? {
        let entry = entry.map_err(|source| ApplyError::CreateDir {
            path: current.display().to_string(),
            source,
        })?;
        let path = entry.path();
        let relative = path
            .strip_prefix(backup_root)
            .expect("path under backup root");
        let target = source_root.join(relative);

        if path.is_dir() {
            fs::create_dir_all(&target).map_err(|source| ApplyError::CreateDir {
                path: target.display().to_string(),
                source,
            })?;
            restored += restore_tree(source_root, backup_root, &path)?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| ApplyError::CreateDir {
                path: parent.display().to_string(),
                source,
            })?;
        }
        fs::copy(&path, &target).map_err(|source| ApplyError::CopyFile {
            from: path.display().to_string(),
            to: target.display().to_string(),
            source,
        })?;
        restored += 1;
    }

    Ok(restored)
}

fn apply_file_catalog(
    source_root: &Path,
    file: &str,
    kind: SourceKind,
    catalog: &TranslationCatalog,
    backup_state: &mut BackupState,
) -> Result<usize, ApplyError> {
    match kind {
        SourceKind::Json => apply_json_file(source_root, file, catalog, backup_state),
        SourceKind::Js => apply_js_file(source_root, file, catalog, backup_state),
    }
}

fn apply_json_file(
    source_root: &Path,
    file: &str,
    catalog: &TranslationCatalog,
    backup_state: &mut BackupState,
) -> Result<usize, ApplyError> {
    let source_path = PathBuf::from(file);
    let baseline = patch_storage::source_file_for(source_root, source_path.clone());
    let raw = text_io::read_text(&baseline.read_path).map_err(|source| ApplyError::ReadFile {
        path: baseline.read_path.display().to_string(),
        source,
    })?;
    let mut json: Value = serde_json::from_str(&raw).map_err(|source| ApplyError::ParseJson {
        path: source_path.display().to_string(),
        source,
    })?;
    let touched = apply_catalog_to_json_value(&source_path, &mut json, catalog);
    if touched == 0 {
        return Ok(0);
    }

    ensure_backup(source_root, &source_path, backup_state)?;
    let payload = serde_json::to_string_pretty(&json).expect("serialize updated json");
    fs::write(&source_path, payload).map_err(|source| ApplyError::WriteJson {
        path: source_path.display().to_string(),
        source,
    })?;
    Ok(1)
}

fn apply_js_file(
    source_root: &Path,
    file: &str,
    catalog: &TranslationCatalog,
    backup_state: &mut BackupState,
) -> Result<usize, ApplyError> {
    let source_path = PathBuf::from(file);
    let baseline = patch_storage::source_file_for(source_root, source_path.clone());
    let raw = text_io::read_text(&baseline.read_path).map_err(|source| ApplyError::ReadFile {
        path: baseline.read_path.display().to_string(),
        source,
    })?;

    let Some(rendered) = extractor_js::apply_catalog_to_js(&source_path, &raw, catalog)? else {
        return Ok(0);
    };

    ensure_backup(source_root, &source_path, backup_state)?;
    fs::write(&source_path, rendered).map_err(|source| ApplyError::WriteJs {
        path: source_path.display().to_string(),
        source,
    })?;
    Ok(1)
}

#[cfg(test)]
fn catalog_file_entries(catalog: &TranslationCatalog) -> Vec<(String, SourceKind)> {
    let mut files = std::collections::BTreeMap::<String, SourceKind>::new();
    for span in &catalog.spans {
        files.entry(span.file.clone()).or_insert(span.source_kind);
    }
    files.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        CatalogProject, ContextEnvelope, SourceKind, TranslationSpan, TranslationUnit,
    };
    use crate::patch_storage;
    use crate::workflow_events::WorkflowReporter;
    use std::time::{SystemTime, UNIX_EPOCH};

    const LARGE_FILE_UNIT_COUNT: usize = 5_001;

    fn temp_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("rush_patch_apply_{name}_{stamp}"))
    }

    #[test]
    fn applies_patch_with_backup_and_restore() {
        let source_root = temp_root("source");
        let data_dir = source_root.join("www").join("data");
        fs::create_dir_all(&data_dir).expect("create data dir");
        fs::write(
            data_dir.join("Map001.json"),
            r#"{"events":{"1":{"pages":[{"list":[{"code":401,"parameters":["Hello"]},{"code":102,"parameters":[["Yes","No"],0,0,2,0]}]}]}}}"#,
        )
        .expect("write map");

        let file = data_dir.join("Map001.json");
        let file_string = file.display().to_string();
        let catalog = TranslationCatalog {
            project: CatalogProject {
                game_root: source_root.display().to_string(),
                engine: "MV".to_owned(),
                generated_at: "0".to_owned(),
            },
            spans: vec![
                TranslationSpan {
                    id: format!("{file_string}::$.events.1.pages[0].list[0].parameters[0]"),
                    file: file_string.clone(),
                    source_kind: SourceKind::Json,
                    locator: "$.events.1.pages[0].list[0].parameters[0]".to_owned(),
                    source_text: "Hello".to_owned(),
                    protected_tokens: Vec::new(),
                    flags: Vec::new(),
                },
                TranslationSpan {
                    id: format!("{file_string}::$.events.1.pages[0].list[1].parameters[0][0]"),
                    file: file_string.clone(),
                    source_kind: SourceKind::Json,
                    locator: "$.events.1.pages[0].list[1].parameters[0][0]".to_owned(),
                    source_text: "Yes".to_owned(),
                    protected_tokens: Vec::new(),
                    flags: Vec::new(),
                },
                TranslationSpan {
                    id: format!("{file_string}::$.events.1.pages[0].list[1].parameters[0][1]"),
                    file: file_string.clone(),
                    source_kind: SourceKind::Json,
                    locator: "$.events.1.pages[0].list[1].parameters[0][1]".to_owned(),
                    source_text: "No".to_owned(),
                    protected_tokens: Vec::new(),
                    flags: Vec::new(),
                },
            ],
            units: vec![
                translation_unit(
                    "dialogue",
                    "dialogue",
                    &file_string,
                    "Hello",
                    Some("你好"),
                    vec![format!(
                        "{file_string}::$.events.1.pages[0].list[0].parameters[0]"
                    )],
                ),
                translation_unit(
                    "choice",
                    "choice",
                    &file_string,
                    "Yes\nNo",
                    Some("是\n否"),
                    vec![
                        format!("{file_string}::$.events.1.pages[0].list[1].parameters[0][0]"),
                        format!("{file_string}::$.events.1.pages[0].list[1].parameters[0][1]"),
                    ],
                ),
            ],
        };

        let plan = apply_patch_with_catalog(source_root.to_str().expect("utf8 source"), &catalog)
            .expect("apply patch");

        assert_eq!(plan.updated_files, 1);
        assert_eq!(plan.backed_up_files, 1);
        assert_eq!(plan.validation_failed_units, 0);
        let source_raw = fs::read_to_string(data_dir.join("Map001.json")).expect("read source");
        assert!(source_raw.contains("你好"));
        assert!(source_raw.contains("是"));
        assert!(source_raw.contains("否"));

        let backup_raw = fs::read_to_string(
            source_root
                .join(patch_storage::WORK_DIR_NAME)
                .join(patch_storage::BACKUP_DIR_NAME)
                .join("www")
                .join("data")
                .join("Map001.json"),
        )
        .expect("read backup");
        assert!(backup_raw.contains("Hello"));

        let second_plan =
            apply_patch_with_catalog(source_root.to_str().expect("utf8 source"), &catalog)
                .expect("apply patch again");
        assert_eq!(second_plan.backed_up_files, 0);

        let restore =
            restore_original_text(source_root.to_str().expect("utf8 source")).expect("restore");
        assert_eq!(restore.restored_files, 1);
        let restored_raw = fs::read_to_string(data_dir.join("Map001.json")).expect("read restored");
        assert!(restored_raw.contains("Hello"));
        assert!(!restored_raw.contains("你好"));

        fs::remove_dir_all(source_root).expect("cleanup source");
    }

    #[test]
    fn preserves_original_when_validation_fails() {
        let source_root = temp_root("invalid_source");
        let data_dir = source_root.join("www").join("data");
        fs::create_dir_all(&data_dir).expect("create data dir");
        fs::write(
            data_dir.join("Map001.json"),
            r#"{"events":{"1":{"pages":[{"list":[{"code":401,"parameters":["Hello %1$s \\N[1]"]}]}]}}}"#,
        )
        .expect("write map");

        let file = data_dir.join("Map001.json");
        let file_string = file.display().to_string();
        let catalog = TranslationCatalog {
            project: CatalogProject {
                game_root: source_root.display().to_string(),
                engine: "MV".to_owned(),
                generated_at: "0".to_owned(),
            },
            spans: vec![TranslationSpan {
                id: format!("{file_string}::$.events.1.pages[0].list[0].parameters[0]"),
                file: file_string.clone(),
                source_kind: SourceKind::Json,
                locator: "$.events.1.pages[0].list[0].parameters[0]".to_owned(),
                source_text: r"Hello %1$s \\N[1]".to_owned(),
                protected_tokens: vec!["%1$s".to_owned(), r"\N[1]".to_owned()],
                flags: vec!["has_control_code".to_owned(), "has_placeholder".to_owned()],
            }],
            units: vec![translation_unit(
                "dialogue",
                "dialogue",
                &file_string,
                r"Hello %1$s \\N[1]",
                Some("你好"),
                vec![format!(
                    "{file_string}::$.events.1.pages[0].list[0].parameters[0]"
                )],
            )],
        };

        let plan = apply_patch_with_catalog(source_root.to_str().expect("utf8 source"), &catalog)
            .expect("apply patch");

        assert_eq!(plan.updated_files, 0);
        assert_eq!(plan.backed_up_files, 0);
        assert_eq!(plan.preserved_failed_units, 1);
        assert_eq!(plan.validation_failed_units, 1);
        let source_raw = fs::read_to_string(data_dir.join("Map001.json")).expect("read source");
        assert!(source_raw.contains(r"Hello %1$s \\N[1]"));
        assert!(
            !source_root
                .join(patch_storage::WORK_DIR_NAME)
                .join(patch_storage::BACKUP_DIR_NAME)
                .exists()
        );

        fs::remove_dir_all(source_root).expect("cleanup source");
    }

    #[test]
    fn applies_all_segments_for_the_same_json_file() {
        let source_root = temp_root("segmented_json");
        let data_dir = source_root.join("www").join("data");
        fs::create_dir_all(&data_dir).expect("create data dir");

        let command_count = LARGE_FILE_UNIT_COUNT;
        let list = (0..command_count)
            .map(|index| {
                serde_json::json!({
                    "code": 401,
                    "parameters": [format!("Line {index}")]
                })
            })
            .collect::<Vec<_>>();
        let source_json = serde_json::json!({
            "events": {
                "1": {
                    "pages": [
                        {
                            "list": list
                        }
                    ]
                }
            }
        });
        let file = data_dir.join("Map001.json");
        fs::write(
            &file,
            serde_json::to_string(&source_json).expect("serialize source json"),
        )
        .expect("write source file");

        let file_string = file.display().to_string();
        let spans = (0..command_count)
            .map(|index| TranslationSpan {
                id: format!("{file_string}::$.events.1.pages[0].list[{index}].parameters[0]"),
                file: file_string.clone(),
                source_kind: SourceKind::Json,
                locator: format!("$.events.1.pages[0].list[{index}].parameters[0]"),
                source_text: format!("Line {index}"),
                protected_tokens: Vec::new(),
                flags: Vec::new(),
            })
            .collect::<Vec<_>>();
        let units = (0..command_count)
            .map(|index| TranslationUnit {
                id: format!("{file_string}::$.events.1.pages[0].list[{index}].parameters[0]"),
                group_id: format!("{file_string}::$.events.1.pages[0].list[{index}].parameters[0]"),
                semantic_kind: "dialogue".to_owned(),
                context: ContextEnvelope {
                    file: file_string.clone(),
                    json_path: Some(format!("$.events.1.pages[0].list[{index}].parameters[0]")),
                    map_id: None,
                    event_id: None,
                    page_id: None,
                    command_index: None,
                    speaker_name: None,
                    prev_texts: Vec::new(),
                    next_texts: Vec::new(),
                    block_text: None,
                    glossary_hits: Vec::new(),
                    notes: Vec::new(),
                },
                source_text: format!("Line {index}"),
                translated_text: Some(format!("Translated {index}")),
                status: TranslationStatus::Translated,
                span_ids: vec![format!(
                    "{file_string}::$.events.1.pages[0].list[{index}].parameters[0]"
                )],
            })
            .collect::<Vec<_>>();
        let catalog = TranslationCatalog {
            project: CatalogProject {
                game_root: source_root.display().to_string(),
                engine: "MZ".to_owned(),
                generated_at: "0".to_owned(),
            },
            spans,
            units,
        };

        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let catalog_path = runtime
            .block_on(crate::catalog_db::persist_catalog(
                &source_root,
                &catalog,
                &WorkflowReporter::noop(),
            ))
            .expect("persist sqlite catalog");

        let plan = runtime
            .block_on(apply_translated_patch(
                source_root.to_str().expect("utf8 root"),
                &catalog_path,
                &WorkflowReporter::noop(),
            ))
            .expect("apply segmented patch");

        assert_eq!(plan.updated_files, 1);
        assert_eq!(plan.backed_up_files, 1);
        let rendered: Value =
            serde_json::from_str(&fs::read_to_string(&file).expect("read patched source file"))
                .expect("parse patched source file");
        assert_eq!(
            rendered["events"]["1"]["pages"][0]["list"][0]["parameters"][0]
                .as_str()
                .expect("first translated line"),
            "Translated 0"
        );
        assert_eq!(
            rendered["events"]["1"]["pages"][0]["list"][command_count - 1]["parameters"][0]
                .as_str()
                .expect("last translated line"),
            format!("Translated {}", command_count - 1)
        );

        fs::remove_dir_all(source_root).expect("cleanup source");
    }

    fn translation_unit(
        id: &str,
        semantic_kind: &str,
        file: &str,
        source_text: &str,
        translated_text: Option<&str>,
        span_ids: Vec<String>,
    ) -> TranslationUnit {
        TranslationUnit {
            id: id.to_owned(),
            group_id: id.to_owned(),
            semantic_kind: semantic_kind.to_owned(),
            context: ContextEnvelope {
                file: file.to_owned(),
                json_path: None,
                map_id: None,
                event_id: None,
                page_id: None,
                command_index: None,
                speaker_name: None,
                prev_texts: Vec::new(),
                next_texts: Vec::new(),
                block_text: None,
                glossary_hits: Vec::new(),
                notes: Vec::new(),
            },
            source_text: source_text.to_owned(),
            translated_text: translated_text.map(str::to_owned),
            status: TranslationStatus::Translated,
            span_ids,
        }
    }
}
