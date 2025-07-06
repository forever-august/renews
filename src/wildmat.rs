#[must_use]
pub fn wildmat(pattern: &str, text: &str) -> bool {
    fn inner(p: &[u8], t: &[u8]) -> bool {
        if p.is_empty() {
            return t.is_empty();
        }
        match p[0] {
            b'?' => {
                if t.is_empty() {
                    false
                } else {
                    inner(&p[1..], &t[1..])
                }
            }
            b'*' => {
                if inner(&p[1..], t) {
                    return true;
                }
                let mut i = 0;
                while i < t.len() {
                    if inner(&p[1..], &t[i + 1..]) {
                        return true;
                    }
                    i += 1;
                }
                false
            }
            b'[' => {
                if t.is_empty() {
                    return false;
                }
                let mut i = 1;
                let mut neg = false;
                if i < p.len() && (p[i] == b'!' || p[i] == b'^') {
                    neg = true;
                    i += 1;
                }
                let mut matched = false;
                let c = t[0];
                let mut prev = 0u8;
                let mut has_prev = false;
                while i < p.len() {
                    let pc = p[i];
                    if pc == b']' && i != 1 + usize::from(neg) {
                        break;
                    }
                    if pc == b'-' && has_prev && i + 1 < p.len() && p[i + 1] != b']' {
                        let end = p[i + 1];
                        if prev <= c && c <= end {
                            matched = true;
                        }
                        i += 2;
                        has_prev = false;
                        continue;
                    }
                    if pc == c {
                        matched = true;
                    }
                    prev = pc;
                    has_prev = true;
                    i += 1;
                }
                if i >= p.len() || p[i] != b']' {
                    // unterminated class treated literally
                    return !t.is_empty() && p[0] == t[0] && inner(&p[1..], &t[1..]);
                }
                if matched ^ neg {
                    inner(&p[i + 1..], &t[1..])
                } else {
                    false
                }
            }
            b'\\' => {
                if p.len() >= 2 && !t.is_empty() && p[1] == t[0] {
                    inner(&p[2..], &t[1..])
                } else {
                    false
                }
            }
            _ => {
                if !t.is_empty() && p[0] == t[0] {
                    inner(&p[1..], &t[1..])
                } else {
                    false
                }
            }
        }
    }
    inner(pattern.as_bytes(), text.as_bytes())
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
