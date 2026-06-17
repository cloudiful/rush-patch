use std::path::{Path, PathBuf};

pub const WORK_DIR_NAME: &str = ".rush-patch";
pub const BACKUP_DIR_NAME: &str = "backups";

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub logical_path: PathBuf,
    pub read_path: PathBuf,
}

pub fn work_dir(game_root: &Path) -> PathBuf {
    game_root.join(WORK_DIR_NAME)
}

pub fn backup_root(game_root: &Path) -> PathBuf {
    work_dir(game_root).join(BACKUP_DIR_NAME)
}

pub fn backup_file_path(game_root: &Path, source_file: &Path) -> Option<PathBuf> {
    source_file
        .strip_prefix(game_root)
        .ok()
        .map(|relative| backup_root(game_root).join(relative))
}

pub fn source_file_for(game_root: &Path, source_file: PathBuf) -> SourceFile {
    let read_path = backup_file_path(game_root, &source_file)
        .filter(|backup| backup.exists())
        .unwrap_or_else(|| source_file.clone());

    SourceFile {
        logical_path: source_file,
        read_path,
    }
}

pub fn source_files_for(game_root: &Path, files: &[String]) -> Vec<SourceFile> {
    files
        .iter()
        .map(|file| source_file_for(game_root, PathBuf::from(file)))
        .collect()
}

#[cfg(windows)]
pub fn mark_hidden_work_dir(game_root: &Path) {
    let _ = std::process::Command::new("attrib")
        .arg("+h")
        .arg(work_dir(game_root))
        .status();
}

#[cfg(not(windows))]
pub fn mark_hidden_work_dir(_game_root: &Path) {}
