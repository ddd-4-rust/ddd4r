//! Focused, framework-neutral utility functions.

#![forbid(unsafe_code)]

use serde::Serialize;

/// Serializes a value to deterministic JSON by recursively sorting object keys.
///
/// # Errors
///
/// Returns [`serde_json::Error`] when the value cannot be converted to or
/// serialized from the canonical JSON representation.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let mut value = serde_json::to_value(value)?;
    sort_json(&mut value);
    serde_json::to_string(&value)
}

fn sort_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            let original = std::mem::take(object);
            let mut entries = original.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            for (_, value) in &mut entries {
                sort_json(value);
            }
            object.extend(entries);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json(value);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_json;
    use serde_json::json;

    #[test]
    fn canonical_json_sorts_nested_objects() {
        let value = json!({"z": 1, "a": {"y": 2, "b": 3}});
        assert_eq!(
            canonical_json(&value).unwrap(),
            r#"{"a":{"b":3,"y":2},"z":1}"#
        );
    }
}
