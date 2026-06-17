use jsonptr::PointerBuf;

pub fn dot_path_to_pointer(locator: &str) -> Option<PointerBuf> {
    if !locator.starts_with('$') {
        return None;
    }

    let mut tokens = Vec::new();
    let mut chars = locator.chars().peekable();
    chars.next()?;

    let mut current = String::new();
    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            '[' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                let mut index = String::new();
                for next in chars.by_ref() {
                    if next == ']' {
                        break;
                    }
                    index.push(next);
                }
                if index.is_empty() {
                    return None;
                }
                tokens.push(index);
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Some(PointerBuf::from_tokens(tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_dot_json_path_to_pointer() {
        let pointer =
            dot_path_to_pointer("$.events.1.pages[0].list[2].parameters[0]").expect("pointer");
        assert_eq!(pointer.as_str(), "/events/1/pages/0/list/2/parameters/0");
    }
}
