use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single action item extracted from the meeting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionItem {
    pub owner: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
}

/// Complete structured output from a single Gemini transcription + analysis call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeetingAnalysis {
    pub meeting_title: String,
    pub meeting_date: String,
    pub transcript: String,
    pub summary: String,
    pub responsibilities: HashMap<String, Vec<String>>,
    pub action_items: Vec<ActionItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_item_with_deadline() {
        let item = ActionItem {
            owner: "Alice".into(),
            description: "Review PR".into(),
            deadline: Some("2026-03-20".into()),
        };
        let json = serde_json::to_string(&item).unwrap();
        let parsed: ActionItem = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, item);
    }

    #[test]
    fn test_action_item_without_deadline() {
        let json = r#"{"owner":"Bob","description":"Fix bug"}"#;
        let item: ActionItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.deadline, None);
    }

    #[test]
    fn test_meeting_analysis_roundtrip() {
        let analysis = MeetingAnalysis {
            meeting_title: "Sprint Planning".into(),
            meeting_date: "2026-03-15".into(),
            transcript: "Speaker 1: Hello".into(),
            summary: "Quick sync meeting".into(),
            responsibilities: HashMap::from([
                ("Alice".into(), vec!["Backend work".into()]),
            ]),
            action_items: vec![ActionItem {
                owner: "Alice".into(),
                description: "Deploy v2".into(),
                deadline: None,
            }],
        };
        let json = serde_json::to_string(&analysis).unwrap();
        let parsed: MeetingAnalysis = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, analysis);
    }

    #[test]
    fn test_empty_collections() {
        let analysis = MeetingAnalysis {
            meeting_title: "Empty".into(),
            meeting_date: "Unknown".into(),
            transcript: "".into(),
            summary: "".into(),
            responsibilities: HashMap::new(),
            action_items: vec![],
        };
        let json = serde_json::to_string(&analysis).unwrap();
        let parsed: MeetingAnalysis = serde_json::from_str(&json).unwrap();
        assert!(parsed.action_items.is_empty());
        assert!(parsed.responsibilities.is_empty());
    }
}
