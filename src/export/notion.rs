use reqwest::blocking::Client;
use serde_json::Value;

use crate::schema::MeetingAnalysis;

/// Notion rich_text character limit per block.
const CHUNK_SIZE: usize = 1900;

const NOTION_API_URL: &str = "https://api.notion.com/v1/pages";
const NOTION_VERSION: &str = "2022-06-28";

/// Create a Notion page under parent_page_id with the analysis.
///
/// Returns the URL of the created Notion page.
pub fn export_to_notion(
    analysis: &MeetingAnalysis,
    token: &str,
    parent_page_id: &str,
) -> Result<String, String> {
    if token.is_empty() {
        return Err("Notion integration token is required".into());
    }
    if parent_page_id.is_empty() {
        return Err("Notion parent page ID is required".into());
    }

    let client = Client::new();
    let children = build_children(analysis);

    let body = serde_json::json!({
        "parent": {
            "page_id": parent_page_id
        },
        "properties": {
            "title": {
                "title": [{
                    "text": {
                        "content": analysis.meeting_title
                    }
                }]
            }
        },
        "children": children
    });

    let resp = client
        .post(NOTION_API_URL)
        .header("Authorization", format!("Bearer {token}"))
        .header("Notion-Version", NOTION_VERSION)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("Notion API request failed: {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .map_err(|e| format!("Failed to read Notion response: {e}"))?;

    if !status.is_success() {
        return Err(format!("Notion API error ({status}): {text}"));
    }

    let json: Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse Notion response: {e}"))?;

    // Build URL from page ID
    let page_id = json
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("");
    let clean_id = page_id.replace('-', "");
    Ok(format!("https://notion.so/{clean_id}"))
}

/// Build the list of Notion block children for the page.
fn build_children(analysis: &MeetingAnalysis) -> Vec<Value> {
    let mut children = Vec::new();

    // Summary section
    children.push(heading2("Summary"));
    children.extend(text_blocks(&analysis.summary));

    // Action Items section
    if !analysis.action_items.is_empty() {
        children.push(heading2("Action Items"));
        for item in &analysis.action_items {
            let deadline = item
                .deadline
                .as_deref()
                .map(|d| format!(" (by {d})"))
                .unwrap_or_default();
            children.push(bullet(&format!(
                "{}: {}{}",
                item.owner, item.description, deadline
            )));
        }
    }

    // Responsibilities section
    if !analysis.responsibilities.is_empty() {
        children.push(heading2("Responsibilities"));
        let mut names: Vec<&String> = analysis.responsibilities.keys().collect();
        names.sort();
        for name in names {
            if let Some(items) = analysis.responsibilities.get(name) {
                for item in items {
                    children.push(bullet(&format!("{name}: {item}")));
                }
            }
        }
    }

    // Transcript section
    children.push(heading2("Transcript"));
    children.extend(text_blocks(&analysis.transcript));

    children
}

/// Split text into paragraph blocks respecting Notion's character limit.
fn text_blocks(text: &str) -> Vec<Value> {
    let mut blocks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        let chunk_end = if remaining.len() <= CHUNK_SIZE {
            remaining.len()
        } else {
            // Try to break at a newline within the chunk
            remaining[..CHUNK_SIZE]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(CHUNK_SIZE)
        };

        let chunk = &remaining[..chunk_end];
        remaining = &remaining[chunk_end..];

        blocks.push(serde_json::json!({
            "object": "block",
            "type": "paragraph",
            "paragraph": {
                "rich_text": [{
                    "type": "text",
                    "text": {
                        "content": chunk
                    }
                }]
            }
        }));
    }

    blocks
}

/// Create a Notion heading_2 block.
fn heading2(text: &str) -> Value {
    serde_json::json!({
        "object": "block",
        "type": "heading_2",
        "heading_2": {
            "rich_text": [{
                "type": "text",
                "text": {
                    "content": text
                }
            }]
        }
    })
}

/// Create a Notion bulleted_list_item block.
fn bullet(text: &str) -> Value {
    serde_json::json!({
        "object": "block",
        "type": "bulleted_list_item",
        "bulleted_list_item": {
            "rich_text": [{
                "type": "text",
                "text": {
                    "content": text
                }
            }]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ActionItem;
    use std::collections::HashMap;

    fn sample_analysis() -> MeetingAnalysis {
        MeetingAnalysis {
            meeting_title: "Sprint Planning".into(),
            meeting_date: "2026-03-15".into(),
            transcript: "Speaker 1: Hello".into(),
            summary: "Discussed sprint goals.".into(),
            responsibilities: HashMap::from([
                ("Alice".into(), vec!["Backend".into()]),
            ]),
            action_items: vec![ActionItem {
                owner: "Alice".into(),
                description: "Deploy".into(),
                deadline: Some("2026-03-20".into()),
            }],
        }
    }

    #[test]
    fn test_build_children_has_all_sections() {
        let children = build_children(&sample_analysis());
        let headings: Vec<&str> = children
            .iter()
            .filter(|c| c["type"] == "heading_2")
            .map(|c| c["heading_2"]["rich_text"][0]["text"]["content"].as_str().unwrap())
            .collect();
        assert!(headings.contains(&"Summary"));
        assert!(headings.contains(&"Action Items"));
        assert!(headings.contains(&"Responsibilities"));
        assert!(headings.contains(&"Transcript"));
    }

    #[test]
    fn test_text_blocks_short_text() {
        let blocks = text_blocks("Short text");
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0]["paragraph"]["rich_text"][0]["text"]["content"],
            "Short text"
        );
    }

    #[test]
    fn test_text_blocks_long_text_chunked() {
        let long_text = "a".repeat(4000);
        let blocks = text_blocks(&long_text);
        assert!(blocks.len() >= 2);
        // Each chunk should be <= CHUNK_SIZE
        for block in &blocks {
            let content = block["paragraph"]["rich_text"][0]["text"]["content"]
                .as_str()
                .unwrap();
            assert!(content.len() <= CHUNK_SIZE);
        }
    }

    #[test]
    fn test_bullet_format() {
        let b = bullet("Test item");
        assert_eq!(b["type"], "bulleted_list_item");
        assert_eq!(
            b["bulleted_list_item"]["rich_text"][0]["text"]["content"],
            "Test item"
        );
    }

    #[test]
    fn test_heading2_format() {
        let h = heading2("Section");
        assert_eq!(h["type"], "heading_2");
        assert_eq!(
            h["heading_2"]["rich_text"][0]["text"]["content"],
            "Section"
        );
    }

    #[test]
    fn test_export_requires_token() {
        let result = export_to_notion(&sample_analysis(), "", "page-id");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("token"));
    }

    #[test]
    fn test_export_requires_page_id() {
        let result = export_to_notion(&sample_analysis(), "token", "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("page ID"));
    }
}
