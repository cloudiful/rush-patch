use super::extract_entries_from_paths;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn parallel_extraction_preserves_input_file_order() {
    let root = temp_root("js_order");
    fs::create_dir_all(&root).expect("create dir");
    let first = root.join("ZLast.js");
    let second = root.join("AFirst.js");
    fs::write(&first, "const text = \"First input\";").expect("write first");
    fs::write(&second, "const text = \"Second input\";").expect("write second");

    let (_, units) =
        extract_entries_from_paths(&[first.display().to_string(), second.display().to_string()])
            .expect("extract");

    assert_eq!(units[0].source_text, "First input");
    assert_eq!(units[1].source_text, "Second input");
    fs::remove_dir_all(root).expect("cleanup");
}

fn temp_root(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    std::env::temp_dir().join(format!("rush_patch_{name}_{stamp}"))
}
