use base64::Engine;
use reqwest::blocking::Client;
use serde_json::Value;

use crate::schema::MeetingAnalysis;

use super::prompt::{build_analysis_prompt, SYSTEM_PROMPT};
use super::schema_convert::meeting_analysis_schema;

/// Threshold for inline base64 upload vs File API upload.
const INLINE_THRESHOLD_BYTES: usize = 15 * 1024 * 1024;

const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
const UPLOAD_URL: &str = "https://generativelanguage.googleapis.com/upload/v1beta/files";

/// Gemini REST API client for meeting audio analysis.
#[derive(Debug)]
pub struct GeminiClient {
    client: Client,
    api_key: String,
    model: String,
}

impl GeminiClient {
    pub fn new(api_key: &str, model: &str) -> Result<Self, String> {
        if api_key.is_empty() {
            return Err("Gemini API key is required".into());
        }
        // 90-min recording = ~165 MB upload + Gemini transcription time.
        // Upload at 5 Mbps ≈ 4.5 min, transcription can take several minutes more.
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(900))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

        Ok(Self {
            client,
            api_key: api_key.to_string(),
            model: model.to_string(),
        })
    }

    /// Transcribe and analyze a WAV audio recording.
    /// Takes ownership of wav_bytes to avoid copying ~165 MB for 90-min recordings.
    pub fn analyze_audio(
        &self,
        wav_bytes: Vec<u8>,
        participant_names: Option<&[String]>,
    ) -> Result<MeetingAnalysis, String> {
        if wav_bytes.len() <= INLINE_THRESHOLD_BYTES {
            self.analyze_inline(&wav_bytes, participant_names)
        } else {
            self.analyze_via_file_api(wav_bytes, participant_names)
        }
    }

    /// Send audio as inline base64 bytes (< 15 MB).
    fn analyze_inline(
        &self,
        wav_bytes: &[u8],
        participant_names: Option<&[String]>,
    ) -> Result<MeetingAnalysis, String> {
        let b64 = base64::engine::general_purpose::STANDARD.encode(wav_bytes);
        let prompt = build_analysis_prompt(participant_names);

        let body = serde_json::json!({
            "system_instruction": {
                "parts": [{"text": SYSTEM_PROMPT}]
            },
            "contents": [{
                "parts": [
                    {
                        "inline_data": {
                            "mime_type": "audio/wav",
                            "data": b64
                        }
                    },
                    {"text": prompt}
                ]
            }],
            "generationConfig": self.generation_config()
        });

        let response = self.call_generate_content(&body)?;
        self.parse_response(&response)
    }

    /// Upload audio via Gemini File API then analyze (>= 15 MB).
    fn analyze_via_file_api(
        &self,
        wav_bytes: Vec<u8>,
        participant_names: Option<&[String]>,
    ) -> Result<MeetingAnalysis, String> {
        let (file_name, file_uri) = self.upload_file(wav_bytes)?;

        let result = (|| {
            let prompt = build_analysis_prompt(participant_names);

            let body = serde_json::json!({
                "system_instruction": {
                    "parts": [{"text": SYSTEM_PROMPT}]
                },
                "contents": [{
                    "parts": [
                        {
                            "file_data": {
                                "mime_type": "audio/wav",
                                "file_uri": file_uri
                            }
                        },
                        {"text": prompt}
                    ]
                }],
                "generationConfig": self.generation_config()
            });

            let response = self.call_generate_content(&body)?;
            self.parse_response(&response)
        })();

        // Always clean up the uploaded file
        let _ = self.delete_file(&file_name);

        result
    }

    fn generation_config(&self) -> Value {
        serde_json::json!({
            "response_mime_type": "application/json",
            "response_schema": meeting_analysis_schema()
        })
    }

    fn generate_content_url(&self) -> String {
        format!("{BASE_URL}/models/{}:generateContent", self.model)
    }

    /// Build a request with the API key in a header (not in the URL).
    fn authenticated_post(&self, url: &str) -> reqwest::blocking::RequestBuilder {
        self.client
            .post(url)
            .header("x-goog-api-key", &self.api_key)
    }

    fn call_generate_content(&self, body: &Value) -> Result<Value, String> {
        let url = self.generate_content_url();
        let resp = self
            .authenticated_post(&url)
            .json(body)
            .send()
            .map_err(|e| format!("Gemini API request failed: {e}"))?;

        let status = resp.status();
        let text = resp
            .text()
            .map_err(|e| format!("Failed to read response body: {e}"))?;

        if !status.is_success() {
            return Err(format!("Gemini API error ({status}): {text}"));
        }

        serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse Gemini response JSON: {e}"))
    }

    fn parse_response(&self, response: &Value) -> Result<MeetingAnalysis, String> {
        let text = response
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.get(0))
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| {
                format!(
                    "Unexpected Gemini response structure: {}",
                    serde_json::to_string_pretty(response).unwrap_or_default()
                )
            })?;

        serde_json::from_str(text).map_err(|e| {
            format!("Failed to parse MeetingAnalysis from Gemini response: {e}")
        })
    }

    fn upload_file(&self, wav_bytes: Vec<u8>) -> Result<(String, String), String> {
        // Step 1: Initiate resumable upload
        let metadata = serde_json::json!({"file": {"display_name": "meeting_audio.wav"}});
        let initiate_resp = self.client
            .post(UPLOAD_URL)
            .header("x-goog-api-key", &self.api_key)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header("X-Goog-Upload-Header-Content-Length", wav_bytes.len().to_string())
            .header("X-Goog-Upload-Header-Content-Type", "audio/wav")
            .header("Content-Type", "application/json")
            .body(metadata.to_string())
            .send()
            .map_err(|e| format!("File upload initiation failed: {e}"))?;

        if !initiate_resp.status().is_success() {
            let text = initiate_resp.text().unwrap_or_default();
            return Err(format!("File upload initiation error: {text}"));
        }

        let upload_url = initiate_resp
            .headers()
            .get("x-goog-upload-url")
            .and_then(|v| v.to_str().ok())
            .ok_or("Missing upload URL in initiation response")?
            .to_string();

        // Step 2: Upload the actual file data
        let upload_resp = self.client
            .put(&upload_url)
            .header("X-Goog-Upload-Command", "upload, finalize")
            .header("X-Goog-Upload-Offset", "0")
            .header("Content-Type", "audio/wav")
            .body(wav_bytes)
            .send()
            .map_err(|e| format!("File upload failed: {e}"))?;

        let status = upload_resp.status();
        let text = upload_resp
            .text()
            .map_err(|e| format!("Failed to read upload response: {e}"))?;

        if !status.is_success() {
            return Err(format!("File upload error ({status}): {text}"));
        }

        let json: Value = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse upload response: {e}"))?;

        let file = json.get("file").unwrap_or(&json);
        let name = file
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or("Missing file name in upload response")?
            .to_string();
        let uri = file
            .get("uri")
            .and_then(|u| u.as_str())
            .ok_or("Missing file URI in upload response")?
            .to_string();

        Ok((name, uri))
    }

    fn delete_file(&self, file_name: &str) -> Result<(), String> {
        let url = format!("{BASE_URL}/{file_name}");
        self.client
            .delete(&url)
            .header("x-goog-api-key", &self.api_key)
            .send()
            .map_err(|e| format!("File delete failed: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_requires_api_key() {
        let result = GeminiClient::new("", "gemini-2.5-flash");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("required"));
    }

    #[test]
    fn test_client_creation() {
        let client = GeminiClient::new("test-key", "gemini-2.5-flash");
        assert!(client.is_ok());
    }

    #[test]
    fn test_parse_valid_response() {
        let client = GeminiClient::new("test-key", "gemini-2.5-flash").unwrap();
        let response = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "text": serde_json::json!({
                            "meeting_title": "Test",
                            "meeting_date": "2026-03-15",
                            "transcript": "Hello",
                            "summary": "A test",
                            "responsibilities": {},
                            "action_items": []
                        }).to_string()
                    }]
                }
            }]
        });

        let result = client.parse_response(&response);
        assert!(result.is_ok());
        let analysis = result.unwrap();
        assert_eq!(analysis.meeting_title, "Test");
    }

    #[test]
    fn test_parse_invalid_response() {
        let client = GeminiClient::new("test-key", "gemini-2.5-flash").unwrap();
        let response = serde_json::json!({"error": "bad"});
        let result = client.parse_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn test_inline_threshold() {
        assert_eq!(INLINE_THRESHOLD_BYTES, 15 * 1024 * 1024);
    }

    #[test]
    fn test_generate_content_url_has_no_key() {
        let client = GeminiClient::new("secret-key", "gemini-2.5-flash").unwrap();
        let url = client.generate_content_url();
        assert!(!url.contains("secret-key"));
        assert!(!url.contains("key="));
    }
}
