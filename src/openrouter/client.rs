use base64::Engine;
use reqwest::blocking::Client;
use serde_json::Value;

use super::catalog;
use crate::audio::{mp3, wav, TARGET_SAMPLE_RATE};
use crate::schema::{MeetingAnalysis, MeetingSummary};

use super::prompt::{
    build_chunk_transcription_prompt, build_transcript_analysis_prompt, SYSTEM_PROMPT,
    TRANSCRIBE_SYSTEM_PROMPT,
};
use super::schema_convert::meeting_summary_schema;

/// Log the full API error body and return a safe user-facing message.
pub(crate) fn http_error(context: &str, status: reqwest::StatusCode, body: &str) -> String {
    log::error!("{context} ({status}): {body}");
    format!("{context}: HTTP {status}. Check logs for details.")
}

/// Seconds of audio per upload. OpenRouter has no file-upload API, so audio is
/// inlined as base64 (+33% size) and the upstream provider caps a request at
/// roughly 20 MB — Gemini's documented inline limit. 25 minutes of 32 kbps MP3
/// is about 6 MB, which leaves ample headroom.
const CHUNK_SECONDS: usize = 25 * 60;

/// Samples per chunk at the recorder's 16 kHz mono output.
const CHUNK_SAMPLES: usize = CHUNK_SECONDS * TARGET_SAMPLE_RATE as usize;

/// Refuse to send a chunk larger than this; base64 of it must stay under the
/// provider's request cap.
const MAX_REQUEST_AUDIO_BYTES: usize = 14 * 1024 * 1024;

const BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Optional attribution headers OpenRouter shows on its dashboard.
const REFERER: &str = "https://github.com/zamrax/meeting-transcriber";
const TITLE: &str = "Meeting Transcriber";

/// How to run one analysis: which models, and how to call them.
#[derive(Debug, Clone, Copy)]
pub struct ClientConfig<'a> {
    pub api_key: &'a str,
    /// Reads the audio.
    pub transcription_model: &'a str,
    /// Reads the transcript and writes the notes.
    pub analysis_model: &'a str,
    /// True when the transcription model is a dedicated speech-to-text model,
    /// which is called through the transcription endpoint.
    pub transcription_is_asr: bool,
    /// Ask for speaker labels, where the model supports it.
    pub diarization: bool,
}

/// OpenRouter client for meeting audio analysis.
#[derive(Debug)]
pub struct OpenRouterClient {
    client: Client,
    api_key: String,
    /// Reads the audio. Must accept audio input.
    transcription_model: String,
    /// True when the transcription model is a dedicated speech-to-text model,
    /// which is called through the transcription endpoint instead of chat
    /// completions.
    transcription_is_asr: bool,
    /// Whether to ask for speaker labels.
    diarization: bool,
    /// Reads the transcript and writes the notes. Text-only.
    analysis_model: String,
}

impl OpenRouterClient {
    pub fn new(config: ClientConfig<'_>) -> Result<Self, String> {
        if config.api_key.is_empty() {
            return Err("OpenRouter API key is required".into());
        }
        if config.transcription_model.is_empty() {
            return Err("A transcription model is required".into());
        }
        if config.analysis_model.is_empty() {
            return Err("An analysis model is required".into());
        }
        // A long recording is several megabytes of upload plus minutes of
        // transcription time on the provider side, per chunk.
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(900))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

        Ok(Self {
            client,
            api_key: config.api_key.to_string(),
            transcription_model: config.transcription_model.to_string(),
            transcription_is_asr: config.transcription_is_asr,
            diarization: config.diarization,
            analysis_model: config.analysis_model.to_string(),
        })
    }

    /// Transcribe and analyze a WAV recording of any length.
    ///
    /// Audio is transcribed with the transcription model, one call per
    /// [`CHUNK_SECONDS`] chunk, then a single text-only call asks the analysis
    /// model for the title, summary, responsibilities, and action items.
    ///
    /// `progress` receives a status line before each network call.
    pub fn analyze_audio(
        &self,
        wav_bytes: Vec<u8>,
        participant_names: Option<&[String]>,
        progress: &dyn Fn(&str),
    ) -> Result<MeetingAnalysis, String> {
        progress("Preparing audio...");
        let samples = wav::decode_to_mono_16k(&wav_bytes)?;
        drop(wav_bytes);

        if samples.is_empty() {
            return Err("Recording contains no audio.".into());
        }

        let chunk_count = samples.len().div_ceil(CHUNK_SAMPLES);
        let mut transcript_parts = Vec::with_capacity(chunk_count);

        for (index, chunk) in samples.chunks(CHUNK_SAMPLES).enumerate() {
            let part = index + 1;
            if chunk_count == 1 {
                progress("Transcribing audio...");
            } else {
                progress(&format!("Transcribing part {part} of {chunk_count}..."));
            }
            let audio = encode_chunk(chunk)?;
            let text = if self.transcription_is_asr {
                self.transcribe_chunk_asr(audio, participant_names)?
            } else {
                self.transcribe_chunk_chat(audio, part, chunk_count, participant_names)?
            };
            if text.trim().is_empty() {
                return Err(format!(
                    "Part {part} of {chunk_count} came back with no transcript."
                ));
            }
            transcript_parts.push(text);
        }

        let transcript = transcript_parts.join("\n\n");
        progress("Summarizing transcript...");
        let summary = self.analyze_transcript(&transcript, participant_names)?;
        Ok(summary.into_analysis(transcript))
    }

    /// Transcribe one chunk with a chat model that accepts audio input.
    ///
    /// These models follow instructions, so the prompt asks for speaker labels
    /// and tells them where the chunk sits in the recording.
    fn transcribe_chunk_chat(
        &self,
        audio: Vec<u8>,
        part: usize,
        total: usize,
        participant_names: Option<&[String]>,
    ) -> Result<String, String> {
        let prompt = build_chunk_transcription_prompt(part, total, participant_names);
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&audio);

        let body = serde_json::json!({
            "model": self.transcription_model,
            "messages": [
                {"role": "system", "content": TRANSCRIBE_SYSTEM_PROMPT},
                {"role": "user", "content": audio_content(audio_b64, &prompt)}
            ]
        });

        let response = self.call_chat_completions(&body)?;
        Ok(message_content(&response)?.trim().to_string())
    }

    /// Transcribe one chunk with a dedicated speech-to-text model.
    ///
    /// This is the OpenAI-compatible multipart endpoint. These models take no
    /// instructions, so speaker labels and chunk position are not available
    /// here — the transcript comes back as plain text.
    fn transcribe_chunk_asr(
        &self,
        audio: Vec<u8>,
        participant_names: Option<&[String]>,
    ) -> Result<String, String> {
        let part = reqwest::blocking::multipart::Part::bytes(audio)
            .file_name("chunk.mp3")
            .mime_str("audio/mpeg")
            .map_err(|e| format!("Failed to build audio upload: {e}"))?;

        // verbose_json carries the per-segment speaker labels; a model that
        // ignores it still returns the plain `text` field.
        let mut form = reqwest::blocking::multipart::Form::new()
            .text("model", self.transcription_model.clone())
            .text("response_format", "verbose_json")
            .part("file", part);

        if let Some(options) = self.azure_provider_options(participant_names) {
            form = form.text("provider", options.to_string());
        }

        let resp = self
            .authenticated_post(&format!("{BASE_URL}/audio/transcriptions"))
            .multipart(form)
            .send()
            .map_err(|e| format!("Transcription request failed: {e}"))?;

        let status = resp.status();
        let text = resp
            .text()
            .map_err(|e| format!("Failed to read transcription response: {e}"))?;

        if !status.is_success() {
            return Err(http_error("Transcription API error", status, &text));
        }

        let json: Value = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse transcription response JSON: {e}"))?;

        Ok(transcript_from_response(&json)?)
    }

    /// Azure-hosted MAI-Transcribe models take diarization and keyword biasing
    /// through provider options. Other speech-to-text models reject them, so
    /// they are only sent where they apply.
    fn azure_provider_options(&self, participant_names: Option<&[String]>) -> Option<Value> {
        if !self.diarization || !catalog::supports_diarization(&self.transcription_model) {
            return None;
        }

        let mut azure = serde_json::json!({"diarization": {"enabled": true}});

        // Participant names bias recognition toward the spellings we expect.
        if let Some(names) = participant_names {
            if !names.is_empty() {
                azure["phraseList"] = serde_json::json!({"phrases": names});
            }
        }

        Some(serde_json::json!({"options": {"azure": azure}}))
    }

    /// Final text-only pass: analyze the stitched transcript.
    fn analyze_transcript(
        &self,
        transcript: &str,
        participant_names: Option<&[String]>,
    ) -> Result<MeetingSummary, String> {
        let prompt = build_transcript_analysis_prompt(transcript, participant_names);

        let body = serde_json::json!({
            "model": self.analysis_model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": prompt}
            ],
            "response_format": json_schema_format("meeting_summary", meeting_summary_schema())
        });

        let response = self.call_chat_completions(&body)?;
        let content = message_content(&response)?;
        serde_json::from_str(strip_code_fence(content))
            .map_err(|e| format!("Failed to parse meeting summary from OpenRouter response: {e}"))
    }

    fn chat_completions_url(&self) -> String {
        format!("{BASE_URL}/chat/completions")
    }

    /// Build a request with the API key in the Authorization header.
    fn authenticated_post(&self, url: &str) -> reqwest::blocking::RequestBuilder {
        self.client
            .post(url)
            .bearer_auth(&self.api_key)
            .header("HTTP-Referer", REFERER)
            .header("X-Title", TITLE)
    }

    fn call_chat_completions(&self, body: &Value) -> Result<Value, String> {
        let url = self.chat_completions_url();
        let resp = self
            .authenticated_post(&url)
            .json(body)
            .send()
            .map_err(|e| format!("OpenRouter API request failed: {e}"))?;

        let status = resp.status();
        let text = resp
            .text()
            .map_err(|e| format!("Failed to read response body: {e}"))?;

        if !status.is_success() {
            return Err(http_error("OpenRouter API error", status, &text));
        }

        serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse OpenRouter response JSON: {e}"))
    }
}

/// Encode one chunk of PCM to MP3, rejecting anything oversized.
fn encode_chunk(samples: &[i16]) -> Result<Vec<u8>, String> {
    let encoded = mp3::encode_mono_16k(samples)?;

    if encoded.len() > MAX_REQUEST_AUDIO_BYTES {
        return Err(format!(
            "Encoded audio chunk is {} MB, above the {} MB per-request limit.",
            encoded.len() / (1024 * 1024),
            MAX_REQUEST_AUDIO_BYTES / (1024 * 1024)
        ));
    }

    Ok(encoded)
}

/// A user message carrying one audio attachment plus instructions.
fn audio_content(audio_b64: String, prompt: &str) -> Value {
    serde_json::json!([
        {
            "type": "input_audio",
            "input_audio": {
                "data": audio_b64,
                "format": "mp3"
            }
        },
        {"type": "text", "text": prompt}
    ])
}

/// `strict` is left off: `responsibilities` is a free-form name -> list map,
/// which providers' strict modes reject. The schema still goes up as a strong
/// hint, and the prompts restate the shape.
fn json_schema_format(name: &str, schema: Value) -> Value {
    serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": name,
            "strict": false,
            "schema": schema
        }
    })
}

/// Render a transcription response as text, using speaker labels when the
/// model returned them.
fn transcript_from_response(response: &Value) -> Result<String, String> {
    if let Some(message) = response
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return Err(format!("Transcription API error: {message}"));
    }

    if let Some(diarized) = diarized_transcript(response) {
        return Ok(diarized);
    }

    response
        .get("text")
        .and_then(|t| t.as_str())
        .map(|t| t.trim().to_string())
        .ok_or_else(|| {
            format!(
                "Unexpected transcription response structure: {}",
                serde_json::to_string_pretty(response).unwrap_or_default()
            )
        })
}

/// Join diarized segments into "Speaker 1: ..." lines, merging consecutive
/// segments from the same speaker. Returns None when the response carries no
/// speaker labels, leaving the caller to fall back to the plain text.
fn diarized_transcript(response: &Value) -> Option<String> {
    let segments = response.get("segments")?.as_array()?;

    let mut lines: Vec<String> = Vec::new();
    let mut current_speaker: Option<String> = None;
    let mut saw_speaker = false;

    for segment in segments {
        let text = segment.get("text").and_then(|t| t.as_str())?.trim();
        if text.is_empty() {
            continue;
        }

        // Providers spell the field either way, and number or name the speaker.
        let speaker = segment
            .get("speaker")
            .or_else(|| segment.get("speaker_id"))
            .map(|s| match s.as_str() {
                Some(name) => name.to_string(),
                None => format!("Speaker {s}"),
            });

        match speaker {
            Some(speaker) => {
                saw_speaker = true;
                if current_speaker.as_deref() == Some(speaker.as_str()) {
                    if let Some(last) = lines.last_mut() {
                        last.push(' ');
                        last.push_str(text);
                    }
                } else {
                    lines.push(format!("{speaker}: {text}"));
                    current_speaker = Some(speaker);
                }
            }
            None => {
                current_speaker = None;
                lines.push(text.to_string());
            }
        }
    }

    if !saw_speaker || lines.is_empty() {
        return None;
    }
    Some(lines.join("\n"))
}

/// Pull the assistant message text out of a chat-completions response.
fn message_content(response: &Value) -> Result<&str, String> {
    // OpenRouter reports some upstream failures with a 200 + error object.
    if let Some(message) = response
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return Err(format!("OpenRouter API error: {message}"));
    }

    response
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| {
            format!(
                "Unexpected OpenRouter response structure: {}",
                serde_json::to_string_pretty(response).unwrap_or_default()
            )
        })
}

/// Models that ignore `response_format` may wrap JSON in a Markdown fence.
fn strip_code_fence(content: &str) -> &str {
    let trimmed = content.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    // Drop an optional language tag on the opening fence line.
    let rest = rest.split_once('\n').map(|(_, body)| body).unwrap_or("");
    rest.trim_end().strip_suffix("```").unwrap_or(rest).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MODEL: &str = "google/gemini-2.5-flash";
    const TEST_ANALYSIS_MODEL: &str = "google/gemini-2.5-flash-lite";

    fn analysis_json() -> String {
        serde_json::json!({
            "meeting_title": "Test",
            "meeting_date": "2026-03-15",
            "transcript": "Hello",
            "summary": "A test",
            "responsibilities": {},
            "action_items": []
        })
        .to_string()
    }

    #[test]
    fn test_client_requires_api_key() {
        let result = OpenRouterClient::new(ClientConfig { api_key: "", transcription_model: TEST_MODEL, analysis_model: TEST_ANALYSIS_MODEL, transcription_is_asr: false, diarization: true });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("required"));
    }

    #[test]
    fn test_client_requires_model() {
        let result = OpenRouterClient::new(ClientConfig { api_key: "test-key", transcription_model: "", analysis_model: TEST_ANALYSIS_MODEL, transcription_is_asr: false, diarization: true });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("required"));
    }

    #[test]
    fn test_client_creation() {
        assert!(OpenRouterClient::new(ClientConfig { api_key: "test-key", transcription_model: TEST_MODEL, analysis_model: TEST_ANALYSIS_MODEL, transcription_is_asr: false, diarization: true }).is_ok());
    }

    #[test]
    fn test_message_content_extracts_text() {
        let response = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": analysis_json()}}]
        });
        let content = message_content(&response).unwrap();
        let analysis: MeetingAnalysis = serde_json::from_str(content).unwrap();
        assert_eq!(analysis.meeting_title, "Test");
    }

    #[test]
    fn test_message_content_surfaces_error_payload() {
        let response = serde_json::json!({
            "error": {"code": 502, "message": "Provider returned error"}
        });
        let err = message_content(&response).unwrap_err();
        assert!(err.contains("Provider returned error"));
    }

    #[test]
    fn test_message_content_rejects_empty_choices() {
        let response = serde_json::json!({"choices": []});
        assert!(message_content(&response).is_err());
    }

    #[test]
    fn test_audio_content_shape() {
        let content = audio_content("QUJD".into(), "do the thing");
        assert_eq!(content[0]["type"], "input_audio");
        assert_eq!(content[0]["input_audio"]["data"], "QUJD");
        assert_eq!(content[0]["input_audio"]["format"], "mp3");
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], "do the thing");
    }

    #[test]
    fn test_json_schema_format_shape() {
        let format = json_schema_format("meeting_summary", meeting_summary_schema());
        assert_eq!(format["type"], "json_schema");
        assert_eq!(format["json_schema"]["name"], "meeting_summary");
        assert_eq!(format["json_schema"]["schema"]["type"], "object");
    }

    #[test]
    fn test_requires_analysis_model() {
        let result = OpenRouterClient::new(ClientConfig { api_key: "test-key", transcription_model: TEST_MODEL, analysis_model: "", transcription_is_asr: false, diarization: true });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("analysis model"));
    }

    #[test]
    fn test_transcription_plain_text_response() {
        let ok = serde_json::json!({"text": "  Hello there  "});
        assert_eq!(transcript_from_response(&ok).unwrap(), "Hello there");

        let err = serde_json::json!({"error": {"message": "model unavailable"}});
        assert!(transcript_from_response(&err)
            .unwrap_err()
            .contains("model unavailable"));

        let malformed = serde_json::json!({"segments": []});
        assert!(transcript_from_response(&malformed).is_err());
    }

    #[test]
    fn test_diarized_segments_become_speaker_lines() {
        let response = serde_json::json!({
            "text": "ignored when segments carry speakers",
            "segments": [
                {"speaker": "Speaker 1", "text": "Morning."},
                {"speaker": "Speaker 1", "text": "Shall we start?"},
                {"speaker": "Speaker 2", "text": "Yes."}
            ]
        });
        assert_eq!(
            transcript_from_response(&response).unwrap(),
            "Speaker 1: Morning. Shall we start?\nSpeaker 2: Yes."
        );
    }

    #[test]
    fn test_numeric_speaker_ids_are_labelled() {
        let response = serde_json::json!({
            "segments": [{"speaker_id": 0, "text": "Hi"}, {"speaker_id": 1, "text": "Hello"}]
        });
        assert_eq!(
            transcript_from_response(&response).unwrap(),
            "Speaker 0: Hi\nSpeaker 1: Hello"
        );
    }

    #[test]
    fn test_segments_without_speakers_fall_back_to_text() {
        let response = serde_json::json!({
            "text": "the whole thing",
            "segments": [{"text": "the whole"}, {"text": "thing"}]
        });
        assert_eq!(transcript_from_response(&response).unwrap(), "the whole thing");
    }

    /// The toggle must win even on a model that supports diarization.
    #[test]
    fn test_diarization_can_be_turned_off() {
        let client = OpenRouterClient::new(ClientConfig {
            api_key: "k",
            transcription_model: "microsoft/mai-transcribe-2",
            analysis_model: TEST_ANALYSIS_MODEL,
            transcription_is_asr: true,
            diarization: false,
        })
        .unwrap();
        assert!(client.azure_provider_options(None).is_none());
    }

    #[test]
    fn test_azure_options_only_for_mai_models() {
        let mai = OpenRouterClient::new(ClientConfig { api_key: "k", transcription_model: "microsoft/mai-transcribe-2", analysis_model: TEST_ANALYSIS_MODEL, transcription_is_asr: true, diarization: true })
            .unwrap();
        let names = vec!["Alice".to_string(), "Bob".to_string()];
        let options = mai.azure_provider_options(Some(&names)).unwrap();
        assert_eq!(options["options"]["azure"]["diarization"]["enabled"], true);
        assert_eq!(options["options"]["azure"]["phraseList"]["phrases"][0], "Alice");

        // Without participants, no keyword biasing is sent.
        let options = mai.azure_provider_options(None).unwrap();
        assert!(options["options"]["azure"].get("phraseList").is_none());

        // Other speech-to-text models would reject these options.
        let whisper =
            OpenRouterClient::new(ClientConfig { api_key: "k", transcription_model: "openai/whisper-large-v3-turbo", analysis_model: TEST_ANALYSIS_MODEL, transcription_is_asr: true, diarization: true })
                .unwrap();
        assert!(whisper.azure_provider_options(Some(&names)).is_none());
    }

    #[test]
    fn test_asr_flag_selects_endpoint() {
        let chat = OpenRouterClient::new(ClientConfig { api_key: "k", transcription_model: TEST_MODEL, analysis_model: TEST_ANALYSIS_MODEL, transcription_is_asr: false, diarization: true }).unwrap();
        assert!(!chat.transcription_is_asr);
        let asr =
            OpenRouterClient::new(ClientConfig { api_key: "k", transcription_model: "openai/whisper-large-v3-turbo", analysis_model: TEST_ANALYSIS_MODEL, transcription_is_asr: true, diarization: true })
                .unwrap();
        assert!(asr.transcription_is_asr);
    }

    #[test]
    fn test_each_phase_uses_its_own_model() {
        let client = OpenRouterClient::new(ClientConfig { api_key: "k", transcription_model: TEST_MODEL, analysis_model: TEST_ANALYSIS_MODEL, transcription_is_asr: false, diarization: true }).unwrap();
        assert_eq!(client.transcription_model, TEST_MODEL);
        assert_eq!(client.analysis_model, TEST_ANALYSIS_MODEL);
    }

    #[test]
    fn test_chat_url_has_no_key() {
        let client = OpenRouterClient::new(ClientConfig { api_key: "secret-key", transcription_model: TEST_MODEL, analysis_model: TEST_ANALYSIS_MODEL, transcription_is_asr: false, diarization: true }).unwrap();
        let url = client.chat_completions_url();
        assert!(!url.contains("secret-key"));
        assert!(!url.contains("key="));
    }

    #[test]
    fn test_chunk_sizing_fits_request_limit() {
        // A full chunk must encode to well under the per-request cap.
        let chunk_bytes = CHUNK_SECONDS * mp3::BYTES_PER_SECOND;
        assert!(chunk_bytes < MAX_REQUEST_AUDIO_BYTES);
        // ...and base64 of it must stay under the provider's ~20 MB cap.
        assert!(chunk_bytes * 4 / 3 < 20 * 1024 * 1024);
    }

    #[test]
    fn test_encode_chunk_returns_mp3_bytes() {
        let samples = vec![0i16; TARGET_SAMPLE_RATE as usize];
        let encoded = encode_chunk(&samples).unwrap();
        assert!(!encoded.is_empty());
        // MPEG audio frames start with 11 set sync bits.
        assert_eq!(encoded[0], 0xFF);
        assert_eq!(encoded[1] & 0xE0, 0xE0);
    }

    #[test]
    fn test_strip_code_fence() {
        assert_eq!(strip_code_fence("{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_code_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_code_fence("```\n{\"a\":1}\n```"), "{\"a\":1}");
    }
}
