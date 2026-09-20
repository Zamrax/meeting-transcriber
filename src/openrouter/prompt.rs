/// System prompt for the meeting analyst model.
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

/// System prompt for the per-chunk transcription passes of a long recording.
pub const TRANSCRIBE_SYSTEM_PROMPT: &str = r#"You are an expert transcriptionist. Transcribe the supplied meeting audio verbatim.

- Transcribe every spoken word accurately, preserving speaker turns.
- Label speakers as "Speaker 1", "Speaker 2", etc., or use names if they are mentioned or clearly identifiable.
- Output the transcript as plain text only. Do not add a summary, commentary, headings, or any analysis.
"#;

/// Build the prompt for one chunk of a recording that was split for upload.
pub fn build_chunk_transcription_prompt(
    part: usize,
    total: usize,
    participant_names: Option<&[String]>,
) -> String {
    let mut prompt = format!(
        "This audio is part {part} of {total} of one continuous meeting recording. \
         Transcribe it verbatim. It may begin or end mid-sentence — transcribe what you hear \
         without inventing missing words and without restating earlier parts."
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

/// Build the final text-only prompt that analyzes a stitched-together transcript.
///
/// The transcript is returned to the caller unchanged, so the model is asked for
/// everything except the transcript — repeating it would risk hitting the
/// model's output token limit on a long meeting.
pub fn build_transcript_analysis_prompt(
    transcript: &str,
    participant_names: Option<&[String]>,
) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let mut prompt = format!(
        "Below is the full transcript of a meeting, transcribed in order from consecutive \
         parts of one recording. Today's date is {today}. Use this as the meeting_date unless a \
         different date is explicitly mentioned in the transcript. \
         Produce the meeting title, summary, responsibilities, and action items. \
         Do not repeat the transcript back. \
         Respond with a single JSON object matching the requested schema and nothing else."
    );

    if let Some(names) = participant_names {
        if !names.is_empty() {
            let names_str = names.join(", ");
            prompt.push_str(&format!(
                "\n\nThe following people are expected to be in this meeting: {names_str}."
            ));
        }
    }

    prompt.push_str("\n\n--- TRANSCRIPT ---\n");
    prompt.push_str(transcript);

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
    fn test_transcribe_prompt_asks_for_plain_text() {
        assert!(TRANSCRIBE_SYSTEM_PROMPT.contains("plain text"));
        assert!(TRANSCRIBE_SYSTEM_PROMPT.contains("Speaker 1"));
    }

    #[test]
    fn test_chunk_prompt_mentions_position() {
        let prompt = build_chunk_transcription_prompt(2, 4, None);
        assert!(prompt.contains("part 2 of 4"));
        assert!(prompt.contains("mid-sentence"));
    }

    #[test]
    fn test_chunk_prompt_with_participants() {
        let names = vec!["Alice".to_string(), "Bob".to_string()];
        let prompt = build_chunk_transcription_prompt(1, 2, Some(&names));
        assert!(prompt.contains("Alice, Bob"));
    }

    #[test]
    fn test_chunk_prompt_empty_participants() {
        let names: Vec<String> = vec![];
        let prompt = build_chunk_transcription_prompt(1, 1, Some(&names));
        assert!(!prompt.contains("expected to be"));
    }

    #[test]
    fn test_transcript_analysis_prompt_embeds_transcript() {
        let prompt = build_transcript_analysis_prompt("Speaker 1: Hi", None);
        assert!(prompt.contains("Speaker 1: Hi"));
        assert!(prompt.contains("Do not repeat the transcript back"));
    }

    #[test]
    fn test_transcript_analysis_prompt_with_participants() {
        let names = vec!["Alice".to_string()];
        let prompt = build_transcript_analysis_prompt("...", Some(&names));
        assert!(prompt.contains("Alice"));
    }
}
