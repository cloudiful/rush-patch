use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("game directory does not exist: {0}")]
    MissingRoot(String),
    #[error("required data directory not found: {0}")]
    MissingDataDir(String),
    #[error("failed to read directory {path}: {source}")]
    ReadDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub game_root: String,
    pub engine: EngineKind,
    pub has_data_dir: bool,
    pub has_plugin_dir: bool,
    pub data_files: Vec<String>,
    pub plugin_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum EngineKind {
    MV,
    MZ,
}

pub fn scan_project(game_root: &str) -> Result<ScanSummary, ScanError> {
    let root = PathBuf::from(game_root);

    if !root.exists() {
        return Err(ScanError::MissingRoot(game_root.to_owned()));
    }

    let data_dir = root.join("www").join("data");
    if !data_dir.is_dir() {
        return Err(ScanError::MissingDataDir(data_dir.display().to_string()));
    }

    let plugin_dir = root.join("www").join("js").join("plugins");
    let data_files = collect_files(&data_dir, "json")?;
    let plugin_files = if plugin_dir.is_dir() {
        collect_files(&plugin_dir, "js")?
    } else {
        Vec::new()
    };

    Ok(ScanSummary {
        game_root: game_root.to_owned(),
        engine: detect_engine(&data_files),
        has_data_dir: true,
        has_plugin_dir: plugin_dir.is_dir(),
        data_files,
        plugin_files,
    })
}

fn detect_engine(data_files: &[String]) -> EngineKind {
    if data_files
        .iter()
        .any(|file| file.ends_with("Animations.json"))
    {
        EngineKind::MZ
    } else {
        EngineKind::MV
    }
}

fn collect_files(dir: &Path, extension: &str) -> Result<Vec<String>, ScanError> {
    let mut paths = Vec::new();
    let entries = fs::read_dir(dir).map_err(|source| ScanError::ReadDir {
        path: dir.display().to_string(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| ScanError::ReadDir {
            path: dir.display().to_string(),
            source,
        })?;
        let path = entry.path();

        if path.is_file()
            && path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        {
            paths.push(path.display().to_string());
        }
    }

    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("rush_patch_{name}_{stamp}"))
    }

    #[test]
    fn detects_mv_project_and_plugin_directory() {
        let root = temp_root("mv");
        let data_dir = root.join("www").join("data");
        let plugin_dir = root.join("www").join("js").join("plugins");

        fs::create_dir_all(&plugin_dir).expect("create plugin dir");
        fs::create_dir_all(&data_dir).expect("create data dir");
        fs::write(data_dir.join("System.json"), "{}").expect("write system");
        fs::write(plugin_dir.join("Example.js"), "const name = 'x';").expect("write plugin");

        let summary = scan_project(root.to_str().expect("utf8 path")).expect("scan project");

        assert!(matches!(summary.engine, EngineKind::MV));
        assert!(summary.has_plugin_dir);
        assert_eq!(summary.data_files.len(), 1);
        assert_eq!(summary.plugin_files.len(), 1);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn detects_mz_project_from_animation_file() {
        let root = temp_root("mz");
        let data_dir = root.join("www").join("data");

        fs::create_dir_all(&data_dir).expect("create data dir");
        fs::write(data_dir.join("Animations.json"), "{}").expect("write animations");

        let summary = scan_project(root.to_str().expect("utf8 path")).expect("scan project");

        assert!(matches!(summary.engine, EngineKind::MZ));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
