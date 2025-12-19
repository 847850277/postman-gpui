use serde_json::{from_str, to_string_pretty, Value};

/// Attempts to pretty-print JSON content.
/// If the content is valid JSON, returns formatted JSON with indentation.
/// If not valid JSON, returns the original content unchanged.
pub fn format_response_body(body: &str) -> String {
    // Try to parse as JSON
    match from_str::<Value>(body) {
        Ok(json_value) => {
            // Successfully parsed as JSON, pretty-print it
            match to_string_pretty(&json_value) {
                Ok(formatted) => formatted,
                Err(_) => body.to_string(), // Fallback to original if formatting fails
            }
        }
        Err(_) => {
            // Not valid JSON, return as-is
            body.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_valid_json() {
        let input = r#"{"name":"John","age":30,"city":"New York"}"#;
        let output = format_response_body(input);
        
        // Should contain line breaks
        assert!(output.contains('\n'));
        // Should be valid JSON
        assert!(from_str::<Value>(&output).is_ok());
    }

    #[test]
    fn test_format_invalid_json() {
        let input = "This is not JSON content";
        let output = format_response_body(input);
        
        // Should remain unchanged
        assert_eq!(output, input);
    }

    #[test]
    fn test_format_empty_string() {
        let input = "";
        let output = format_response_body(input);
        
        // Empty string should remain empty
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_already_formatted_json() {
        let input = r#"{
  "name": "John",
  "age": 30
}"#;
        let output = format_response_body(input);
        
        // Should still be valid JSON
        assert!(from_str::<Value>(&output).is_ok());
        // Should contain line breaks
        assert!(output.contains('\n'));
    }

    #[test]
    fn test_format_json_array() {
        let input = r#"[{"id":1,"name":"Item 1"},{"id":2,"name":"Item 2"}]"#;
        let output = format_response_body(input);
        
        // Should contain line breaks
        assert!(output.contains('\n'));
        // Should be valid JSON
        assert!(from_str::<Value>(&output).is_ok());
    }
}
