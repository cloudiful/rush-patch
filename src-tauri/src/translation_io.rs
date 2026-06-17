use std::fs;
use std::path::Path;
use thiserror::Error;

use crate::terminology::CanonicalTerm;

#[derive(Debug, Error)]
pub enum TranslationIoError {
    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Default)]
pub struct TranslationResources {
    pub glossary: Option<String>,
    pub glossary_terms: Vec<CanonicalTerm>,
    pub do_not_translate: Option<String>,
}

pub fn load_resources(
    glossary_path: Option<&str>,
    do_not_translate_path: Option<&str>,
) -> Result<TranslationResources, TranslationIoError> {
    let glossary = read_optional(glossary_path)?;
    Ok(TranslationResources {
        glossary_terms: glossary
            .as_deref()
            .map(parse_glossary_terms)
            .unwrap_or_default(),
        glossary,
        do_not_translate: read_optional(do_not_translate_path)?,
    })
}

fn read_optional(path: Option<&str>) -> Result<Option<String>, TranslationIoError> {
    let Some(path) = path.map(str::trim).filter(|path| !path.is_empty()) else {
        return Ok(None);
    };

    fs::read_to_string(Path::new(path))
        .map(Some)
        .map_err(|source| TranslationIoError::ReadFile {
            path: path.to_owned(),
            source,
        })
}

fn parse_glossary_terms(raw: &str) -> Vec<CanonicalTerm> {
    raw.lines()
        .filter_map(parse_glossary_line)
        .collect()
}

fn parse_glossary_line(line: &str) -> Option<CanonicalTerm> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
        return None;
    }

    let delimiters = ["=>", "=", "\t", ","];
    for delimiter in delimiters {
        let Some((source, target)) = trimmed.split_once(delimiter) else {
            continue;
        };
        let source = source.trim();
        let target = target.trim();
        if source.is_empty() || target.is_empty() {
            return None;
        }
        return Some(CanonicalTerm {
            source: source.to_owned(),
            target: target.to_owned(),
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_glossary_delimiters() {
        let parsed = parse_glossary_terms(
            "ロマーシャ => 罗玛夏\nバニーガール=兔女郎\nPotion\t药水\nIgnore me\n# comment",
        );

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].source, "ロマーシャ");
        assert_eq!(parsed[0].target, "罗玛夏");
        assert_eq!(parsed[2].source, "Potion");
        assert_eq!(parsed[2].target, "药水");
    }
}
