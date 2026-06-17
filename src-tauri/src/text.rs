use std::collections::BTreeMap;

pub fn extract_protected_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        let current = chars[index];

        if current == '\\' {
            if let Some((token, consumed)) = extract_control_code(&chars[index..]) {
                tokens.push(token);
                index += consumed;
                continue;
            }
        } else if current == '%' {
            if let Some((token, consumed)) = extract_percent_token(&chars[index..]) {
                tokens.push(token);
                index += consumed;
                continue;
            }
        } else if current == '{'
            && let Some((token, consumed)) = extract_brace_token(&chars[index..])
        {
            tokens.push(token);
            index += consumed;
            continue;
        }

        index += 1;
    }

    tokens
}

pub fn token_multiset(tokens: &[String]) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    for token in tokens {
        *map.entry(token.clone()).or_insert(0) += 1;
    }
    map
}

fn extract_control_code(chars: &[char]) -> Option<(String, usize)> {
    if chars.first().copied()? != '\\' {
        return None;
    }

    let mut index = 1;
    while index < chars.len() && chars[index].is_ascii_alphabetic() {
        index += 1;
    }

    if index == 1 || chars.get(index).copied() != Some('[') {
        return None;
    }

    let mut end = index + 1;
    while end < chars.len() && chars[end] != ']' {
        end += 1;
    }

    if end >= chars.len() || chars[end] != ']' {
        return None;
    }

    Some((chars[..=end].iter().collect(), end + 1))
}

fn extract_percent_token(chars: &[char]) -> Option<(String, usize)> {
    if chars.first().copied()? != '%' {
        return None;
    }
    if chars.get(1).copied() == Some('%') {
        return Some(("%%".to_owned(), 2));
    }

    let mut index = 1;
    while index < chars.len() && chars[index].is_ascii_digit() {
        index += 1;
    }
    if chars.get(index).copied() == Some('$') {
        index += 1;
    }

    let specifier = chars.get(index).copied()?;
    if !"sdifuxX".contains(specifier) {
        return None;
    }

    Some((chars[..=index].iter().collect(), index + 1))
}

fn extract_brace_token(chars: &[char]) -> Option<(String, usize)> {
    if chars.first().copied()? != '{' {
        return None;
    }

    let mut index = 1;
    while index < chars.len() && chars[index] != '}' {
        if !(chars[index].is_ascii_alphanumeric() || chars[index] == '_') {
            return None;
        }
        index += 1;
    }

    if index >= chars.len() || chars[index] != '}' || index == 1 {
        return None;
    }

    Some((chars[..=index].iter().collect(), index + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_exact_control_and_placeholder_tokens() {
        let tokens = extract_protected_tokens(r"\N[1] 攻击 %1$s {name} \V[12]");
        assert_eq!(tokens, vec![r"\N[1]", "%1$s", "{name}", r"\V[12]"]);
    }
}
