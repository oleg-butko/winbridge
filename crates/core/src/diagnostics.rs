//! Presentation-only redaction. Never executes or modifies the source task.
use serde_json::Value;

pub fn validate_redactions(values: &[String]) -> Result<(), String> {
    if values.len() > 32 || values.iter().any(|v| v.is_empty() || v.len() > 4096) {
        return Err("Use at most 32 nonempty redaction values, each up to 4096 UTF-8 bytes".into());
    }
    Ok(())
}
fn sensitive(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "api-key",
        "apikey",
        "authorization",
        "credential",
    ]
    .iter()
    .any(|key| name.contains(key))
}
pub fn redact(value: &mut Value, values: &[String]) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if sensitive(key) {
                    *value = Value::String("[REDACTED]".into());
                } else if key == "arguments" {
                    redact_arguments(value);
                    redact(value, values);
                } else {
                    redact(value, values);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| redact(v, values)),
        Value::String(text) => {
            // Longest literals first so one redaction cannot expose the suffix of another.
            let mut values = values.iter().collect::<Vec<_>>();
            values.sort_by_key(|v| std::cmp::Reverse(v.len()));
            for value in values {
                *text = text.replace(value, "[REDACTED]");
            }
        }
        _ => {}
    }
}
fn redact_arguments(value: &mut Value) {
    let Some(args) = value.as_array_mut() else {
        return;
    };
    let mut hide_next = false;
    for arg in args {
        if hide_next {
            *arg = Value::String("[REDACTED]".into());
            hide_next = false;
            continue;
        }
        let Some(text) = arg.as_str() else {
            continue;
        };
        if let Some((key, _)) = text.split_once(['=', ':']) {
            if sensitive(key) || encoded_flag(key) {
                *arg = Value::String(format!("{key}=[REDACTED]"));
            }
        } else if encoded_flag(text)
            || ((text.starts_with('-') || text.starts_with('/')) && sensitive(text))
        {
            hide_next = true;
        }
    }
}
fn encoded_flag(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower == "-ec" || (lower.starts_with("-e") && "-encodedcommand".starts_with(&lower))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redaction_covers_json_escapes_overlapping_literals_credentials_and_encoded_payloads() {
        let mut value = serde_json::json!({"command":{"arguments":["--token","actual-secret","--password=pwd","-EncodedCommand","QQ==","Привет ✓","-enco","Qg==","-EncodedCommand:Qw==","Authorization:Bearer-test"]},"nested":{"apiKey":"key"},"stdout":"secret-long / secret / quote\"value\\\n"});
        redact(
            &mut value,
            &[
                "secret".into(),
                "secret-long".into(),
                "quote\"value\\\n".into(),
            ],
        );
        let json = value.to_string();
        for secret in [
            "actual-secret",
            "pwd",
            "QQ==",
            "Qg==",
            "Qw==",
            "Bearer-test",
            "secret-long",
            "quote",
        ] {
            assert!(!json.contains(secret));
        }
        assert_eq!(value["command"]["arguments"][5], "Привет ✓");
        assert!(validate_redactions(&["".into()]).is_err());
        assert!(validate_redactions(&vec!["a".into(); 33]).is_err());
    }
}
