use oxc_allocator::Allocator;
use oxc_parser::{Kind, Parser, Token, config::TokensParserConfig};
use oxc_span::{SourceType, Span};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct JsStringCandidate {
    pub locator: String,
    pub decoded_text: String,
    pub quote: char,
    pub content_start: usize,
    pub content_end: usize,
}

pub fn extract_js_strings(path: &Path, raw: &str) -> Vec<JsStringCandidate> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_else(|_| SourceType::default());
    let parse_return = Parser::new(&allocator, raw, source_type)
        .with_config(TokensParserConfig)
        .parse();
    let tokens = parse_return.tokens.iter().copied().collect::<Vec<_>>();
    let mut results = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        if token.kind() != Kind::Str {
            continue;
        }

        let Some(candidate) = candidate_from_token(raw, token.span()) else {
            continue;
        };

        if should_extract_string(raw, &tokens, index, &candidate.decoded_text) {
            results.push(candidate);
        }
    }

    results
}

pub fn escape_for_quote(text: &str, quote: char) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '"' if quote == '"' => escaped.push_str("\\\""),
            '\'' if quote == '\'' => escaped.push_str("\\'"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn candidate_from_token(raw: &str, span: Span) -> Option<JsStringCandidate> {
    let start = span.start as usize;
    let end = span.end as usize;
    if end <= start + 1 || end > raw.len() {
        return None;
    }

    let literal = &raw[start..end];
    let quote = literal.chars().next()?;
    if !matches!(quote, '"' | '\'') {
        return None;
    }

    let decoded = decode_string_literal(literal, quote)?;
    Some(JsStringCandidate {
        locator: format!("byte:{start}:{end}"),
        decoded_text: decoded,
        quote,
        content_start: start + quote.len_utf8(),
        content_end: end - quote.len_utf8(),
    })
}

fn decode_string_literal(literal: &str, quote: char) -> Option<String> {
    let mut chars = literal.chars();
    if chars.next()? != quote || literal.chars().last()? != quote {
        return None;
    }

    let inner = &literal[quote.len_utf8()..literal.len() - quote.len_utf8()];
    let mut decoded = String::new();
    let mut iter = inner.chars().peekable();

    while let Some(ch) = iter.next() {
        if ch != '\\' {
            decoded.push(ch);
            continue;
        }

        let escaped = iter.next()?;
        match escaped {
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            '\\' => decoded.push('\\'),
            '\'' => decoded.push('\''),
            '"' => decoded.push('"'),
            'u' => {
                if iter.peek() == Some(&'{') {
                    iter.next();
                    let mut hex = String::new();
                    while let Some(&next) = iter.peek() {
                        iter.next();
                        if next == '}' {
                            break;
                        }
                        hex.push(next);
                    }
                    let codepoint = u32::from_str_radix(&hex, 16).ok()?;
                    decoded.push(char::from_u32(codepoint)?);
                } else {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        hex.push(iter.next()?);
                    }
                    let codepoint = u32::from_str_radix(&hex, 16).ok()?;
                    decoded.push(char::from_u32(codepoint)?);
                }
            }
            other => decoded.push(other),
        }
    }

    Some(decoded)
}

fn should_extract_string(raw: &str, tokens: &[Token], index: usize, decoded: &str) -> bool {
    if !is_translatable(decoded) {
        return false;
    }

    let prev_kind = index
        .checked_sub(1)
        .and_then(|idx| tokens.get(idx))
        .map(|token| token.kind());
    let next_kind = tokens.get(index + 1).map(|token| token.kind());

    if matches!(prev_kind, Some(Kind::Plus)) || matches!(next_kind, Some(Kind::Plus)) {
        return false;
    }
    if matches!(next_kind, Some(Kind::Colon)) {
        return false;
    }
    if is_object_property_value(tokens, index) {
        return false;
    }
    if is_import_export_source(tokens, index) {
        return false;
    }
    if is_register_command_arg(raw, tokens, index) {
        return false;
    }
    if looks_like_all_caps_code(decoded) {
        return false;
    }
    if looks_like_ascii_identifier(decoded) {
        return false;
    }
    if looks_like_embedded_code(decoded) {
        return false;
    }

    true
}

fn is_import_export_source(tokens: &[Token], index: usize) -> bool {
    let prev_kind = index
        .checked_sub(1)
        .and_then(|idx| tokens.get(idx))
        .map(|token| token.kind());
    matches!(prev_kind, Some(Kind::From | Kind::Import | Kind::Export))
}

fn is_register_command_arg(raw: &str, tokens: &[Token], index: usize) -> bool {
    let mut paren_depth = 0usize;
    for scan in (0..index).rev() {
        match tokens[scan].kind() {
            Kind::RParen => paren_depth += 1,
            Kind::LParen => {
                if paren_depth == 0 {
                    let Some(previous) = scan.checked_sub(1).and_then(|idx| tokens.get(idx)) else {
                        return false;
                    };
                    let ident = token_source(raw, previous.span());
                    return ident.ends_with("registerCommand");
                }
                paren_depth -= 1;
            }
            _ => {}
        }
    }
    false
}

fn is_object_property_value(tokens: &[Token], index: usize) -> bool {
    let prev_kind = index
        .checked_sub(1)
        .and_then(|idx| tokens.get(idx))
        .map(|token| token.kind());
    if !matches!(prev_kind, Some(Kind::Colon)) {
        return false;
    }

    let mut brace_depth = 0usize;
    for scan in (0..index).rev() {
        match tokens[scan].kind() {
            Kind::RCurly => brace_depth += 1,
            Kind::LCurly => {
                if brace_depth == 0 {
                    return true;
                }
                brace_depth -= 1;
            }
            Kind::Semicolon | Kind::LParen => return false,
            _ => {}
        }
    }

    false
}

fn token_source(raw: &str, span: Span) -> &str {
    &raw[span.start as usize..span.end as usize]
}

fn looks_like_all_caps_code(decoded: &str) -> bool {
    let compact: String = decoded.chars().filter(|ch| !ch.is_whitespace()).collect();
    !compact.is_empty()
        && compact
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-'))
}

fn looks_like_ascii_identifier(decoded: &str) -> bool {
    let trimmed = decoded.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && trimmed.chars().any(|ch| ch.is_ascii_alphabetic())
}

fn looks_like_embedded_code(decoded: &str) -> bool {
    let trimmed = decoded.trim();
    if !trimmed.contains('\n') {
        return false;
    }

    let code_markers = [
        "precision ",
        "uniform ",
        "varying ",
        "attribute ",
        "sampler2D",
        "gl_FragColor",
        "void main",
        "return ",
        "function ",
    ];
    if code_markers.iter().any(|marker| trimmed.contains(marker)) {
        return true;
    }

    let punctuation = trimmed
        .chars()
        .filter(|ch| matches!(ch, '{' | '}' | ';' | '(' | ')' | '='))
        .count();
    punctuation >= 6
}

fn is_translatable(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let letter_count = trimmed.chars().filter(|ch| ch.is_alphabetic()).count();
    let non_ascii_count = trimmed.chars().filter(|ch| !ch.is_ascii()).count();
    let has_space = trimmed.contains(' ');
    let has_sentence_punct = trimmed.contains('!') || trimmed.contains('?');

    non_ascii_count > 0 || has_space || has_sentence_punct || letter_count >= 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn extracts_safe_static_plugin_strings() {
        let raw = concat!(
            "const message = \"Hello hero\";\n",
            "const shader = \"varying vec2 vTextureCoord;\\nuniform sampler2D uSampler;\\nvoid main() {}\";\n",
            "const jp = '\\u3053\\u3093\\u306b\\u3061\\u306f';\n",
            "const key = \"INTERNAL_STATUS\";\n",
            "const word = \"move\";\n",
            "const object = { label: \"Skip me\" };\n",
            "const dynamic = \"A\" + suffix;\n",
            "const template = `Hello ${name}`;\n",
            "import data from \"mod\";\n",
            "PluginManager.registerCommand(\"RushPatch\", \"ShowHint\", function() {});\n"
        );

        let entries = extract_js_strings(Path::new("Plugin.js"), raw);
        let texts = entries
            .iter()
            .map(|entry| entry.decoded_text.as_str())
            .collect::<Vec<_>>();

        assert!(texts.contains(&"Hello hero"));
        assert!(texts.contains(&"\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}"));
        assert!(!texts.iter().any(|text| text.contains("vTextureCoord")));
        assert!(!texts.contains(&"INTERNAL_STATUS"));
        assert!(!texts.contains(&"move"));
        assert!(!texts.contains(&"Skip me"));
        assert!(!texts.contains(&"A"));
        assert!(!texts.contains(&"mod"));
        assert!(!texts.contains(&"RushPatch"));
        assert!(!texts.contains(&"ShowHint"));
    }
}
