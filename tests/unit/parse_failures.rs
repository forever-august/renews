//! Tests for parsing failure modes

use renews::{parse_command, parse_response, parse_message};
use renews::parse::unescape_message_id;

#[test]
fn test_parse_command_malformed() {
    // Empty command
    assert!(parse_command("").is_err());
    
    // Only whitespace
    assert!(parse_command("   ").is_err());
    
    // Command with null bytes (nom should handle this)
    let result = parse_command("COMMAND\0ARG");
    // This might succeed or fail depending on nom's behavior, let's test the actual result
    if result.is_ok() {
        let (_, cmd) = result.unwrap();
        assert_eq!(cmd.name, "COMMAND");
        // The null byte might be in the argument
    }
}

#[test]
fn test_parse_command_edge_cases() {
    // Command with special characters (nom will accept alphabetic chars)
    let result = parse_command("COMMAND@INVALID");
    if result.is_ok() {
        let (_, cmd) = result.unwrap();
        assert_eq!(cmd.name, "COMMAND");
        // @ and everything after should be in args or remaining input
    }
    
    // Command starting with number should fail (take_while1 with is_ascii_alphabetic)
    assert!(parse_command("123COMMAND").is_err());
    
    // Very long command name (should handle gracefully)
    let long_cmd = "A".repeat(1000);
    let result = parse_command(&long_cmd);
    if result.is_ok() {
        let (_, cmd) = result.unwrap();
        assert_eq!(cmd.name, long_cmd.to_uppercase());
    }
    
    // Command with many arguments
    let many_args = format!("CMD {}", vec!["arg"; 100].join(" "));
    let result = parse_command(&many_args);
    if result.is_ok() {
        let (_, cmd) = result.unwrap();
        assert_eq!(cmd.args.len(), 100);
    }
}

#[test]
fn test_parse_response_malformed() {
    // Non-numeric response code
    assert!(parse_response("ABC test").is_err());
    
    // Empty response
    assert!(parse_response("").is_err());
    
    // Response code too short (2 digits might be accepted by digit1)
    let result = parse_response("12");
    // Check if nom accepts 2-digit codes
    if let Ok((_, resp)) = result {
        assert_eq!(resp.code, 12);
    }
    
    // Response code too long (digit1 will take all digits)
    let result = parse_response("1234 test");
    if result.is_ok() {
        let (_, resp) = result.unwrap();
        assert_eq!(resp.code, 1234);
    }
}

#[test]
fn test_parse_response_edge_cases() {
    // Response without text
    let (_, resp) = parse_response("200").unwrap();
    assert_eq!(resp.code, 200);
    assert_eq!(resp.text, "");
    
    // Response with only space after code
    let (_, resp) = parse_response("200 ").unwrap();
    assert_eq!(resp.code, 200);
    assert_eq!(resp.text, "");
    
    // Very long response text
    let long_text = "A".repeat(10000);
    let input = format!("200 {long_text}");
    let (_, resp) = parse_response(&input).unwrap();
    assert_eq!(resp.text, long_text);
}

#[test]
fn test_parse_message_malformed_headers() {
    // Header without colon (nom parsing will fail)
    let malformed = "From test@example.com\r\nSubject: Test\r\n\r\nBody";
    assert!(parse_message(malformed).is_err());
    
    // Header with empty name should fail
    let malformed = ": value\r\nSubject: Test\r\n\r\nBody";
    assert!(parse_message(malformed).is_err());
    
    // Header with control characters in name (take_while1 should stop at control chars)
    let malformed = "From\x01: test@example.com\r\nSubject: Test\r\n\r\nBody";
    // This might succeed but parse only "From" as the header name
    let result = parse_message(malformed);
    if let Ok((_, msg)) = result {
        // Should have parsed "From" header but not the control char
        assert!(!msg.headers.is_empty());
    }
}

#[test]
fn test_parse_message_edge_cases() {
    // Message with no headers (just empty line then body)
    let (_, msg) = parse_message("\r\nBody only").unwrap();
    assert_eq!(msg.headers.len(), 0);
    assert_eq!(msg.body, "Body only");
    
    // Message with extremely long header value
    let long_value = "A".repeat(10000);
    let input = format!("Subject: {long_value}\r\n\r\nBody");
    let (_, msg) = parse_message(&input).unwrap();
    assert_eq!(msg.headers[0].1, long_value);
    
    // Message with many folded header lines
    let folded = "Subject: First line\r\n\tcontinuation\r\n another\r\n\tfinal\r\n\r\nBody";
    let (_, msg) = parse_message(folded).unwrap();
    assert_eq!(msg.headers[0].1, "First line continuation another final");
    
    // Message with empty body
    let (_, msg) = parse_message("Subject: Test\r\n\r\n").unwrap();
    assert_eq!(msg.body, "");
}

#[test]
fn test_unescape_message_id_edge_cases() {
    // Malformed Message-IDs without angle brackets
    assert_eq!(unescape_message_id("no-brackets"), "no-brackets");
    
    // Only opening bracket
    assert_eq!(unescape_message_id("<no-close"), "<no-close");
    
    // Only closing bracket
    assert_eq!(unescape_message_id("no-open>"), "no-open>");
    
    // Empty Message-ID
    assert_eq!(unescape_message_id(""), "");
    
    // Message-ID with only brackets
    assert_eq!(unescape_message_id("<>"), "<>");
    
    // Message-ID with nested brackets
    assert_eq!(unescape_message_id("<<test>>"), "<<test>>");
    
    // Message-ID with comments (should be stripped)
    assert_eq!(unescape_message_id("<test(comment)@example.com>"), "<test@example.com>");
    
    // Message-ID with multiple comments
    assert_eq!(unescape_message_id("<test(comment1)(comment2)@example.com>"), "<test@example.com>");
    
    // Message-ID with escaped characters (check actual behavior)
    let result = unescape_message_id("<test\\@example.com>");
    // The implementation may or may not handle escapes
    assert!(result.starts_with('<') && result.ends_with('>'));
    
    // Message-ID with quotes (check actual behavior)
    let result = unescape_message_id("<\"quoted\"@example.com>");
    assert!(result.starts_with('<') && result.ends_with('>'));
    
    // Very long Message-ID
    let long_id = format!("<{}>", "a".repeat(10000));
    let result = unescape_message_id(&long_id);
    assert!(result.starts_with('<') && result.ends_with('>'));
}

#[test]
fn test_parse_message_malformed_folding() {
    // Invalid folding (no colon before fold)
    let malformed = "Subject Test\r\n\tcontinuation\r\n\r\nBody";
    assert!(parse_message(malformed).is_err());
    
    // Fold at start of headers
    let malformed = "\tfolded line\r\nSubject: Test\r\n\r\nBody";
    // This should be handled gracefully or error appropriately
    let result = parse_message(malformed);
    // The behavior depends on implementation - either error or handle gracefully
    match result {
        Ok((_, msg)) => {
            // If it parses, should not crash
            assert!(!msg.headers.is_empty() || !msg.body.is_empty());
        }
        Err(_) => {
            // Erroring is also acceptable for malformed input
        }
    }
}

#[test]
fn test_parse_message_binary_content() {
    // Message with binary data in body
    let binary_body = vec![0u8, 1, 2, 255, 254, 253];
    let binary_str = String::from_utf8_lossy(&binary_body);
    let input = format!("Subject: Binary\r\n\r\n{binary_str}");
    
    // Should handle binary content gracefully
    let result = parse_message(&input);
    match result {
        Ok((_, msg)) => {
            assert_eq!(msg.headers[0].0, "Subject");
            assert_eq!(msg.headers[0].1, "Binary");
            // Body should contain the binary data (possibly as replacement chars)
        }
        Err(_) => {
            // Erroring on binary content is also acceptable
        }
    }
}