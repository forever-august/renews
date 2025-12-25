use regex::Regex;

#[must_use]
pub fn wildmat(pattern: &str, text: &str) -> bool {
    if let Ok(re) = pattern_to_regex(pattern) {
        re.is_match(text)
    } else {
        false
    }
}

fn pattern_to_regex(pattern: &str) -> Result<Regex, regex::Error> {
    let mut regex = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '[' => {
                if let Some(class) = parse_class(&mut chars) {
                    regex.push('[');
                    regex.push_str(&class);
                    regex.push(']');
                } else {
                    regex.push_str("\\[");
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    regex.push_str(&regex::escape(&next.to_string()));
                } else {
                    regex.push('\\');
                }
            }
            _ => regex.push_str(&regex::escape(&c.to_string())),
        }
    }
    regex.push('$');
    Regex::new(&regex)
}

fn parse_class<I>(chars: &mut std::iter::Peekable<I>) -> Option<String>
where
    I: Iterator<Item = char> + Clone,
{
    let preview = chars.clone();
    let mut found = false;
    for ch in preview {
        if ch == ']' {
            found = true;
            break;
        }
    }
    if !found {
        return None;
    }

    let mut class = String::new();
    if let Some(&first) = chars.peek()
        && (first == '!' || first == '^')
    {
        class.push('^');
        chars.next();
    }
    while let Some(ch) = chars.next() {
        if ch == ']' {
            break;
        }
        if ch == '\\' {
            if let Some(next) = chars.next() {
                class.push_str(&regex::escape(&next.to_string()));
            } else {
                class.push('\\');
            }
        } else {
            class.push(ch);
        }
    }
    Some(class)
}

#[cfg(test)]
mod tests {
    use super::wildmat;

    #[test]
    fn test_simple() {
        assert!(wildmat("foo", "foo"));
        assert!(!wildmat("foo", "bar"));
        assert!(wildmat("f?o", "foo"));
        assert!(wildmat("f*o", "fooo"));
    }

    #[test]
    fn test_char_class() {
        assert!(wildmat("b[aeiou]r", "bar"));
        assert!(!wildmat("b[!aeiou]r", "bar"));
        assert!(wildmat("b[a-z]r", "bor"));
    }

    #[test]
    fn test_escape() {
        assert!(wildmat("a\\*b", "a*b"));
        assert!(!wildmat("a\\*b", "axxb"));
    }
}
