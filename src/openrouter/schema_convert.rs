use serde_json::Value;

/// Full analysis schema. Used as the base for [`meeting_summary_schema`], which is
/// what actually goes on the wire now that the transcript is assembled locally.
///
/// This is plain JSON Schema (OpenAI-compatible), not Gemini's dialect: nullable
/// fields use a type union rather than the `nullable` keyword, and the free-form
/// `responsibilities` map is described with `additionalProperties`.
pub(crate) fn meeting_analysis_schema() -> Value {
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
                "description": "Map of person name to list of responsibilities they accepted",
                "additionalProperties": {
                    "type": "array",
                    "items": {"type": "string"}
                }
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
                            "type": ["string", "null"],
                            "description": "Deadline if mentioned, null otherwise"
                        }
                    },
                    "required": ["owner", "description"],
                    "additionalProperties": false
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
        ],
        "additionalProperties": false
    })
}

/// Schema for the final analysis pass over an already-assembled transcript.
///
/// Identical to [`meeting_analysis_schema`] minus `transcript`: the caller
/// already holds the transcript, and asking a model to echo a 90-minute one
/// back would risk its output token limit.
pub fn meeting_summary_schema() -> Value {
    let mut schema = meeting_analysis_schema();

    if let Some(properties) = schema
        .get_mut("properties")
        .and_then(|p| p.as_object_mut())
    {
        properties.remove("transcript");
    }
    if let Some(required) = schema.get_mut("required").and_then(|r| r.as_array_mut()) {
        required.retain(|field| field != "transcript");
    }

    schema
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_summary_schema_drops_transcript() {
        let schema = super::meeting_summary_schema();
        assert!(schema["properties"].get("transcript").is_none());
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(!required.contains(&"transcript"));
        assert!(required.contains(&"summary"));
    }

    use super::*;

    #[test]
    fn test_schema_has_required_fields() {
        let schema = meeting_analysis_schema();
        let required = schema["required"].as_array().unwrap();
        let required_names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
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
        assert_eq!(
            items["properties"]["deadline"]["type"],
            serde_json::json!(["string", "null"])
        );
    }

    #[test]
    fn test_responsibilities_is_string_list_map() {
        let schema = meeting_analysis_schema();
        let responsibilities = &schema["properties"]["responsibilities"];
        assert_eq!(responsibilities["additionalProperties"]["type"], "array");
        assert_eq!(
            responsibilities["additionalProperties"]["items"]["type"],
            "string"
        );
    }
}
