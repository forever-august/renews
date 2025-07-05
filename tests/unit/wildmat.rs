use renews::wildmat::wildmat;

#[test]
fn basic_matches() {
    assert!(wildmat("comp.*", "comp.lang.rust"));
    assert!(wildmat("comp.?", "comp.x"));
    assert!(!wildmat("comp.?", "comp."));
}

#[test]
fn character_classes() {
    assert!(wildmat("a[bc]d", "acd"));
    assert!(wildmat("a[b-d]d", "acd"));
    assert!(!wildmat("a[!b-d]d", "acd"));
}

#[test]
fn escapes() {
    assert!(wildmat("foo\\*bar", "foo*bar"));
    assert!(!wildmat("foo\\?bar", "fooxbar"));
}
