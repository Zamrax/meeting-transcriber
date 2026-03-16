/// System prompt for the Gemini meeting analyst.
pub const SYSTEM_PROMPT: &str = r#"You are an expert meeting analyst and transcriptionist. Your task is to transcribe audio recordings of meetings and produce structured, actionable, and highly detailed meeting notes.

**Transcription Guidelines:**
- Transcribe every spoken word accurately, preserving speaker turns.
- Label speakers as "Speaker 1", "Speaker 2", etc., or use names if they are mentioned or clearly identifiable in the conversation.

**Summary Requirements — THIS IS CRITICAL:**
- The summary MUST be at least 300 words. Short summaries are unacceptable.
- Write exactly 3 detailed paragraphs:
  - Paragraph 1 (Context & Purpose): Describe why the meeting was held, who participated, and the primary objectives. Set the scene.
  - Paragraph 2 (Discussion Details): Cover EVERY major topic discussed. For each topic, explain what was said, by whom, what different viewpoints were raised, and why it matters. Do not summarize topics in one sentence — expand on each one. Reference specific projects, clients, numbers, and decisions by name.
  - Paragraph 3 (Decisions & Next Steps): List all decisions made, agreements reached, and the strategic direction going forward. Be specific about what was decided and what remains open.
- If the meeting covered many topics, the summary should be proportionally longer (up to 600 words for 30+ minute meetings).

**Structured Data Extraction:**
- Identify explicit responsibilities: commitments made by named individuals.
- Extract all action items: specific tasks with an owner and, if stated, a deadline. If no deadline is mentioned, set deadline to null.
- Be conservative: only include responsibilities and action items that were explicitly stated or clearly implied by a named participant.
"#;

/// Build the user-turn prompt for transcription + analysis.
pub fn build_analysis_prompt(participant_names: Option<&[String]>) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let mut prompt = format!(
        "Please transcribe and analyze the attached audio recording of a meeting. \
         Today's date is {today}. Use this as the meeting_date unless a different date \
         is explicitly mentioned in the audio. \
         Provide a structured analysis including the full transcript, summary, \
         action items, and responsibilities.",
    );

    if let Some(names) = participant_names {
        if !names.is_empty() {
            let names_str = names.join(", ");
            prompt.push_str(&format!(
                "\n\nThe following people are expected to be in this meeting: {names_str}. \
                 Use these names to label speakers where you can identify them."
            ));
        }
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_not_empty() {
        assert!(!SYSTEM_PROMPT.is_empty());
        assert!(SYSTEM_PROMPT.contains("meeting analyst"));
    }

    #[test]
    fn test_build_prompt_no_participants() {
        let prompt = build_analysis_prompt(None);
        assert!(prompt.contains("transcribe and analyze"));
        assert!(!prompt.contains("expected to be"));
    }

    #[test]
    fn test_build_prompt_with_participants() {
        let names = vec!["Alice".to_string(), "Bob".to_string()];
        let prompt = build_analysis_prompt(Some(&names));
        assert!(prompt.contains("Alice, Bob"));
        assert!(prompt.contains("expected to be"));
    }

    #[test]
    fn test_build_prompt_empty_participants() {
        let names: Vec<String> = vec![];
        let prompt = build_analysis_prompt(Some(&names));
        assert!(!prompt.contains("expected to be"));
    }
}
