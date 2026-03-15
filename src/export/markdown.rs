use crate::schema::MeetingAnalysis;

/// Escape a string for safe inclusion as a YAML value.
fn yaml_str(value: &str) -> String {
    if value.contains(':')
        || value.contains('#')
        || value.contains('\'')
        || value.contains('"')
        || value.contains('\n')
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

/// Escape a string for safe inclusion in a Markdown table cell.
fn md_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

/// Generate a filename like `2026-03-13 Product Sync.md`.
pub fn get_filename(analysis: &MeetingAnalysis) -> String {
    let date = if analysis.meeting_date == "Unknown" {
        chrono::Local::now().format("%Y-%m-%d").to_string()
    } else {
        analysis.meeting_date.clone()
    };
    let title = analysis
        .meeting_title
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], " ")
        .trim()
        .to_string();
    format!("{date} {title}.md")
}

/// Convert a MeetingAnalysis to a full Markdown document.
pub fn to_markdown(analysis: &MeetingAnalysis) -> String {
    let mut md = String::new();

    // YAML frontmatter
    md.push_str("---\n");
    md.push_str(&format!("title: {}\n", yaml_str(&analysis.meeting_title)));
    md.push_str(&format!("date: {}\n", yaml_str(&analysis.meeting_date)));
    md.push_str("tags:\n  - meeting-notes\n");
    md.push_str("---\n\n");

    // Title
    md.push_str(&format!("# {}\n\n", analysis.meeting_title));

    // Summary
    md.push_str("## Summary\n\n");
    md.push_str(&analysis.summary);
    md.push_str("\n\n");

    // Action Items table
    if !analysis.action_items.is_empty() {
        md.push_str("## Action Items\n\n");
        md.push_str("| Owner | Task | Deadline |\n");
        md.push_str("|-------|------|----------|\n");
        for item in &analysis.action_items {
            let deadline = item
                .deadline
                .as_deref()
                .unwrap_or("\u{2014}"); // em-dash
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                md_cell(&item.owner),
                md_cell(&item.description),
                md_cell(deadline),
            ));
        }
        md.push('\n');
    }

    // Responsibilities
    if !analysis.responsibilities.is_empty() {
        md.push_str("## Responsibilities\n\n");
        let mut names: Vec<&String> = analysis.responsibilities.keys().collect();
        names.sort();
        for name in names {
            md.push_str(&format!("### {name}\n\n"));
            if let Some(items) = analysis.responsibilities.get(name) {
                for item in items {
                    md.push_str(&format!("- {item}\n"));
                }
            }
            md.push('\n');
        }
    }

    // Transcript
    md.push_str("## Transcript\n\n");
    md.push_str(&analysis.transcript);
    md.push('\n');

    md
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
            transcript: "Speaker 1: Hello\nSpeaker 2: Hi".into(),
            summary: "Discussed sprint goals.".into(),
            responsibilities: HashMap::from([
                ("Alice".into(), vec!["Backend API".into(), "Database migration".into()]),
                ("Bob".into(), vec!["Frontend UI".into()]),
            ]),
            action_items: vec![
                ActionItem {
                    owner: "Alice".into(),
                    description: "Deploy v2".into(),
                    deadline: Some("2026-03-20".into()),
                },
                ActionItem {
                    owner: "Bob".into(),
                    description: "Review PR".into(),
                    deadline: None,
                },
            ],
        }
    }

    #[test]
    fn test_to_markdown_has_frontmatter() {
        let md = to_markdown(&sample_analysis());
        assert!(md.starts_with("---\n"));
        assert!(md.contains("title: Sprint Planning"));
        assert!(md.contains("date: 2026-03-15"));
        assert!(md.contains("tags:\n  - meeting-notes"));
    }

    #[test]
    fn test_to_markdown_has_sections() {
        let md = to_markdown(&sample_analysis());
        assert!(md.contains("# Sprint Planning"));
        assert!(md.contains("## Summary"));
        assert!(md.contains("## Action Items"));
        assert!(md.contains("## Responsibilities"));
        assert!(md.contains("## Transcript"));
    }

    #[test]
    fn test_to_markdown_action_items_table() {
        let md = to_markdown(&sample_analysis());
        assert!(md.contains("| Alice | Deploy v2 | 2026-03-20 |"));
        assert!(md.contains("| Bob | Review PR | \u{2014} |")); // em-dash for null deadline
    }

    #[test]
    fn test_to_markdown_responsibilities_sorted() {
        let md = to_markdown(&sample_analysis());
        let alice_pos = md.find("### Alice").unwrap();
        let bob_pos = md.find("### Bob").unwrap();
        assert!(alice_pos < bob_pos);
    }

    #[test]
    fn test_get_filename() {
        let analysis = sample_analysis();
        assert_eq!(get_filename(&analysis), "2026-03-15 Sprint Planning.md");
    }

    #[test]
    fn test_get_filename_unknown_date() {
        let mut analysis = sample_analysis();
        analysis.meeting_date = "Unknown".into();
        let filename = get_filename(&analysis);
        // Should use today's date
        assert!(filename.ends_with("Sprint Planning.md"));
        assert!(!filename.starts_with("Unknown"));
    }

    #[test]
    fn test_get_filename_sanitizes_chars() {
        let mut analysis = sample_analysis();
        analysis.meeting_title = "Meeting: Q1/Q2 Review".into();
        let filename = get_filename(&analysis);
        assert!(!filename.contains(':'));
        assert!(!filename.contains('/'));
    }

    #[test]
    fn test_yaml_str_escaping() {
        assert_eq!(yaml_str("simple"), "simple");
        assert_eq!(yaml_str("has: colon"), "\"has: colon\"");
        assert_eq!(yaml_str("has \"quotes\""), "\"has \\\"quotes\\\"\"");
    }

    #[test]
    fn test_md_cell_escaping() {
        assert_eq!(md_cell("normal"), "normal");
        assert_eq!(md_cell("has | pipe"), "has \\| pipe");
        assert_eq!(md_cell("has\nnewline"), "has newline");
    }
}
