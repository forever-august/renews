use renews::Message;
use renews::storage::common::{Headers, extract_message_id};
use smallvec::smallvec;

#[test]
fn test_extract_message_id_present() {
    let article = Message {
        headers: smallvec![
            ("From".into(), "test@example.com".into()),
            ("Message-ID".into(), "<test123@example.com>".into()),
            ("Subject".into(), "Test subject".into()),
        ],
        body: "Test body".into(),
    };

    let msg_id = extract_message_id(&article);
    assert_eq!(msg_id, Some("<test123@example.com>".into()));
}

#[test]
fn test_extract_message_id_case_insensitive() {
    let article = Message {
        headers: smallvec![
            ("From".into(), "test@example.com".into()),
            ("message-id".into(), "<test123@example.com>".into()),
            ("Subject".into(), "Test subject".into()),
        ],
        body: "Test body".into(),
    };

    let msg_id = extract_message_id(&article);
    assert_eq!(msg_id, Some("<test123@example.com>".into()));
}

#[test]
fn test_extract_message_id_missing() {
    let article = Message {
        headers: smallvec![
            ("From".into(), "test@example.com".into()),
            ("Subject".into(), "Test subject".into()),
        ],
        body: "Test body".into(),
    };

    let msg_id = extract_message_id(&article);
    assert_eq!(msg_id, None);
}

#[test]
fn test_extract_message_id_empty_headers() {
    let article = Message {
        headers: smallvec![],
        body: "Test body".into(),
    };

    let msg_id = extract_message_id(&article);
    assert_eq!(msg_id, None);
}

#[test]
fn test_headers_serialization() {
    let headers = Headers(smallvec![
        ("From".into(), "test@example.com".into()),
        ("Subject".into(), "Test subject".into()),
    ]);

    // Test that Headers can be serialized and deserialized
    let serialized = serde_json::to_string(&headers).unwrap();
    let deserialized: Headers = serde_json::from_str(&serialized).unwrap();

    assert_eq!(headers.0, deserialized.0);
}
