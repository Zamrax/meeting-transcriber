use serde_json::Value;

/// Keys that Gemini's JSON Schema subset does not support.
/// Note: "anyOf" is handled specially before stripping, so it's not in this list.
const UNSUPPORTED_KEYS: &[&str] = &[
    "$defs",
    "title",
    "additionalProperties",
    "default",
];

/// Build a Gemini-compatible JSON Schema for the MeetingAnalysis response.
///
/// This manually constructs the schema to match exactly what Gemini expects,
/// avoiding the need for schemars and its potential format differences.
pub fn meeting_analysis_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "meeting_title": {
                "type": "string",
                "description": "A concise descriptive title for the meeting"
            },
            "meeting_date": {
                "type": "string",
                "description": "Date of the meeting (ISO 8601 or 'Unknown')"
            },
            "transcript": {
                "type": "string",
                "description": "Full verbatim transcript with speaker labels where identifiable"
            },
            "summary": {
                "type": "string",
                "description": "Detailed multi-paragraph executive summary (minimum 300 words). Paragraph 1: meeting purpose and context. Paragraph 2: key discussion points, arguments, and viewpoints from participants. Paragraph 3: decisions reached, outcomes, and next steps. Be thorough and specific — reference actual topics, names, and details from the conversation."
            },
            "responsibilities": {
                "type": "object",
                "description": "Map of person name to list of responsibilities they accepted"
            },
            "action_items": {
                "type": "array",
                "description": "List of specific tasks with owners and optional deadlines",
                "items": {
                    "type": "object",
                    "properties": {
                        "owner": {
                            "type": "string",
                            "description": "Person responsible for this action"
                        },
                        "description": {
                            "type": "string",
                            "description": "What needs to be done"
                        },
                        "deadline": {
                            "type": "string",
                            "nullable": true,
                            "description": "Deadline if mentioned, null otherwise"
                        }
                    },
                    "required": ["owner", "description"]
                }
            }
        },
        "required": [
            "meeting_title",
            "meeting_date",
            "transcript",
            "summary",
            "responsibilities",
            "action_items"
        ]
    })
}

/// Strip keys that Gemini doesn't understand from a JSON Schema node (recursive).
/// Used if generating schema dynamically from serde in the future.
pub fn strip_unsupported_keys(node: &mut Value) {
    if let Value::Object(map) = node {
        for key in UNSUPPORTED_KEYS {
            map.remove(*key);
        }
        // Handle anyOf nullable pattern: {"anyOf": [{"type": "string"}, {"type": "null"}]}
        // Convert to: {"type": "string", "nullable": true}
        if let Some(any_of) = map.remove("anyOf") {
            if let Value::Array(variants) = &any_of {
                let non_null: Vec<&Value> = variants
                    .iter()
                    .filter(|v| v.get("type") != Some(&Value::String("null".into())))
                    .collect();
                if non_null.len() == 1 {
                    if let Value::Object(inner) = non_null[0] {
                        for (k, v) in inner {
                            map.insert(k.clone(), v.clone());
                        }
                    }
                    map.insert("nullable".into(), Value::Bool(true));
                }
            }
        }
        // Recurse into all remaining values
        for value in map.values_mut() {
            strip_unsupported_keys(value);
        }
    } else if let Value::Array(arr) = node {
        for item in arr {
            strip_unsupported_keys(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_has_required_fields() {
        let schema = meeting_analysis_schema();
        let required = schema["required"].as_array().unwrap();
        let required_names: Vec<&str> = required
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required_names.contains(&"meeting_title"));
        assert!(required_names.contains(&"transcript"));
        assert!(required_names.contains(&"action_items"));
        assert!(required_names.contains(&"responsibilities"));
    }

    #[test]
    fn test_schema_action_items_structure() {
        let schema = meeting_analysis_schema();
        let items = &schema["properties"]["action_items"]["items"];
        assert_eq!(items["type"], "object");
        assert!(items["properties"]["owner"].is_object());
        assert!(items["properties"]["deadline"]["nullable"].as_bool().unwrap());
    }

    #[test]
    fn test_strip_unsupported_keys() {
        let mut schema = serde_json::json!({
            "type": "object",
            "title": "ShouldBeRemoved",
            "$defs": {},
            "additionalProperties": false,
            "properties": {
                "name": {
                    "type": "string",
                    "title": "AlsoRemoved"
                },
                "age": {
                    "anyOf": [
                        {"type": "integer"},
                        {"type": "null"}
                    ]
                }
            }
        });
        strip_unsupported_keys(&mut schema);
        assert!(schema.get("title").is_none());
        assert!(schema.get("$defs").is_none());
        assert!(schema.get("additionalProperties").is_none());
        assert!(schema["properties"]["name"].get("title").is_none());
        // anyOf should be converted to nullable
        assert_eq!(schema["properties"]["age"]["type"], "integer");
        assert_eq!(schema["properties"]["age"]["nullable"], true);
    }
}
