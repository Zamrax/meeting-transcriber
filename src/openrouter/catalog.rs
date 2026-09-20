use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Public model list. Needs no API key.
const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// Dedicated speech-to-text models. These are absent from the default listing
/// and have to be requested with this filter.
const TRANSCRIPTION_MODELS_URL: &str =
    "https://openrouter.ai/api/v1/models?output_modalities=transcription";

/// Seconds in an hour, for speech-to-text models billed per second of audio.
const SECONDS_PER_HOUR: f64 = 3_600.0;

/// Speech-to-text pricing carries no unit in the API, so the unit is inferred:
/// a `prompt` price at or above this is read as per-hour, below it as
/// per-second. Today the two groups differ by three orders of magnitude
/// (0.0000033/s for Whisper Turbo vs 0.1/h for MAI-Transcribe 2), so anything
/// above this would be an implausible per-second rate of $3.60/hour.
const PER_HOUR_PRICE_THRESHOLD: f64 = 0.001;

/// Audio costs about 1,920 tokens per minute of recording.
const AUDIO_TOKENS_PER_HOUR: f64 = 1_920.0 * 60.0;

/// Rough transcript length for an hour of speech at ~130 words per minute.
const TRANSCRIPT_TOKENS_PER_HOUR: f64 = 13_000.0;

/// What the analysis pass reads (the transcript) and writes (the notes).
const ANALYSIS_IN_TOKENS_PER_HOUR: f64 = 17_000.0;
const ANALYSIS_OUT_TOKENS_PER_HOUR: f64 = 1_000.0;

/// One model as the app needs it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_length: u64,
    /// Whether the model accepts audio input, and so can transcribe.
    pub audio_input: bool,
    /// True for dedicated speech-to-text models, which return only a
    /// transcript and are called through the transcription endpoint rather
    /// than chat completions.
    #[serde(default)]
    pub transcription_only: bool,
    /// Prices are per token, and absent for models that don't publish one.
    pub prompt_price: Option<f64>,
    pub completion_price: Option<f64>,
    pub audio_price: Option<f64>,
}

impl ModelInfo {
    /// Estimated cost to transcribe one hour of audio.
    ///
    /// Speech-to-text models are billed three different ways and the API does
    /// not say which applies, so the unit is inferred from the shape of the
    /// pricing. Chat models bill per audio token as usual.
    pub fn transcription_cost_per_hour(&self) -> Option<f64> {
        if !self.transcription_only {
            let audio = self.audio_price?;
            let completion = self.completion_price.unwrap_or(0.0);
            return Some(audio * AUDIO_TOKENS_PER_HOUR + completion * TRANSCRIPT_TOKENS_PER_HOUR);
        }

        let prompt = self.prompt_price?;
        let completion = self.completion_price.unwrap_or(0.0);

        Some(if completion > 0.0 {
            // Priced per token, like the GPT-4o transcribe models.
            prompt * AUDIO_TOKENS_PER_HOUR + completion * TRANSCRIPT_TOKENS_PER_HOUR
        } else if prompt >= PER_HOUR_PRICE_THRESHOLD {
            prompt
        } else {
            prompt * SECONDS_PER_HOUR
        })
    }

    /// Estimated cost to analyze the transcript of one hour of audio.
    pub fn analysis_cost_per_hour(&self) -> Option<f64> {
        let prompt = self.prompt_price?;
        let completion = self.completion_price.unwrap_or(0.0);
        Some(prompt * ANALYSIS_IN_TOKENS_PER_HOUR + completion * ANALYSIS_OUT_TOKENS_PER_HOUR)
    }

    /// Context window rendered for a dropdown row, e.g. "1M" or "128k".
    pub fn context_label(&self) -> String {
        match self.context_length {
            0 => "—".into(),
            n if n >= 1_000_000 => format!("{}M", n / 1_000_000),
            n if n >= 1_000 => format!("{}k", n / 1_000),
            n => n.to_string(),
        }
    }

    /// Cost rendered for a dropdown row.
    pub fn cost_label(&self, cost_per_hour: Option<f64>) -> String {
        match cost_per_hour {
            Some(cost) if cost > 0.0 => format!("${cost:.2}/h"),
            Some(_) => "free".into(),
            None => "—".into(),
        }
    }
}

/// The model list plus when it was fetched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub fetched_at: String,
    pub models: Vec<ModelInfo>,
}

impl Catalog {
    /// Models that can transcribe, cheapest first.
    pub fn transcription_models(&self) -> Vec<&ModelInfo> {
        let mut models: Vec<&ModelInfo> = self
            .models
            .iter()
            .filter(|m| m.audio_input && is_synchronous(m))
            .collect();
        models.sort_by(|a, b| {
            sort_key(a.transcription_cost_per_hour())
                .total_cmp(&sort_key(b.transcription_cost_per_hour()))
        });
        models
    }

    /// Every model can analyze text; cheapest first.
    pub fn analysis_models(&self) -> Vec<&ModelInfo> {
        let mut models: Vec<&ModelInfo> = self
            .models
            .iter()
            .filter(|m| is_synchronous(m) && !m.transcription_only)
            .collect();
        models.sort_by(|a, b| {
            sort_key(a.analysis_cost_per_hour()).total_cmp(&sort_key(b.analysis_cost_per_hour()))
        });
        models
    }
}

/// Whether the app can ask this model for speaker labels.
///
/// The API publishes no capability flag for this, and each provider exposes
/// diarization through its own options, so this covers the models the app
/// actually knows how to enable it for.
pub fn supports_diarization(model_id: &str) -> bool {
    model_id.starts_with("microsoft/mai-transcribe")
}

/// `:batch` routes are asynchronous, so they cannot serve the blocking
/// chat-completions calls this app makes.
fn is_synchronous(model: &ModelInfo) -> bool {
    !model.id.ends_with(":batch")
}

/// Models with no published price sort last rather than first.
fn sort_key(cost: Option<f64>) -> f64 {
    cost.unwrap_or(f64::MAX)
}

/// Shipped fallback, used when nothing has been fetched or cached yet.
///
/// Verified against the OpenRouter model API on 2026-09-20; prices only order
/// and label the list, and the first successful refresh replaces them.
pub fn builtin() -> Catalog {
    let entry = |id: &str, name: &str, ctx: u64, audio: f64, prompt: f64, completion: f64| {
        ModelInfo {
            id: id.into(),
            name: name.into(),
            context_length: ctx,
            audio_input: true,
            transcription_only: false,
            prompt_price: Some(prompt),
            completion_price: Some(completion),
            audio_price: Some(audio),
        }
    };

    // Speech-to-text models: `prompt` is priced per hour or per second here,
    // never per token, and they carry no audio-token price.
    let asr = |id: &str, name: &str, prompt: f64| ModelInfo {
        id: id.into(),
        name: name.into(),
        context_length: 0,
        audio_input: true,
        transcription_only: true,
        prompt_price: Some(prompt),
        completion_price: Some(0.0),
        audio_price: None,
    };

    Catalog {
        fetched_at: String::new(),
        models: vec![
            asr(
                "microsoft/mai-transcribe-2",
                "Microsoft AI: MAI-Transcribe 2",
                0.1,
            ),
            asr(
                "openai/whisper-large-v3-turbo",
                "OpenAI: Whisper Large V3 Turbo",
                0.00000333,
            ),
            asr("x-ai/grok-stt-1.0", "SpaceXAI: Grok STT 1.0", 0.0000277778),
            entry(
                "google/gemini-2.5-flash",
                "Google: Gemini 2.5 Flash",
                1_048_576,
                0.000001,
                0.0000003,
                0.0000025,
            ),
            entry(
                "google/gemini-2.5-flash-lite",
                "Google: Gemini 2.5 Flash Lite",
                1_048_576,
                0.0000003,
                0.0000001,
                0.0000004,
            ),
            entry(
                "google/gemini-3.1-flash-lite",
                "Google: Gemini 3.1 Flash Lite",
                1_048_576,
                0.0000005,
                0.00000025,
                0.0000015,
            ),
            entry(
                "google/gemini-3.8-flash",
                "Google: Gemini 3.8 Flash",
                1_048_576,
                0.00000075,
                0.00000075,
                0.00000375,
            ),
            entry(
                "google/gemini-2.5-pro",
                "Google: Gemini 2.5 Pro",
                1_048_576,
                0.00000125,
                0.00000125,
                0.00001,
            ),
            entry(
                "openai/gpt-audio-mini",
                "OpenAI: GPT Audio Mini",
                128_000,
                0.0000006,
                0.0000006,
                0.0000024,
            ),
        ],
    }
}

/// Where the fetched list is cached between runs.
pub fn cache_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("meeting-transcriber").join("models.json"))
}

/// Read the cached list, if an earlier run wrote one.
pub fn load_cached() -> Option<Catalog> {
    let path = cache_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Write the list so the next launch starts with a populated dropdown.
pub fn save_cache(catalog: &Catalog) -> Result<(), String> {
    let path = cache_path().ok_or("No config directory available")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create cache dir: {e}"))?;
    }
    let raw = serde_json::to_string_pretty(catalog)
        .map_err(|e| format!("Failed to serialize model cache: {e}"))?;
    std::fs::write(&path, raw).map_err(|e| format!("Failed to write model cache: {e}"))
}

/// Fetch the current model list from OpenRouter. No API key required.
///
/// Two requests: the default listing, plus the speech-to-text models, which
/// the default listing leaves out entirely.
pub fn fetch() -> Result<Catalog, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let mut catalog = fetch_list(&client, MODELS_URL)?;

    // A failure here costs the speech-to-text options, not the whole list.
    match fetch_list(&client, TRANSCRIPTION_MODELS_URL) {
        Ok(transcription) => catalog.models.extend(transcription.models),
        Err(e) => log::warn!("Failed to fetch speech-to-text models: {e}"),
    }

    Ok(catalog)
}

fn fetch_list(client: &reqwest::blocking::Client, url: &str) -> Result<Catalog, String> {
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("Model list request failed: {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .map_err(|e| format!("Failed to read model list: {e}"))?;

    if !status.is_success() {
        return Err(super::client::http_error("Model list error", status, &text));
    }

    let json: Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse model list: {e}"))?;

    parse_catalog(&json)
}

/// Turn an OpenRouter `/models` payload into a [`Catalog`].
///
/// Every field but `id` is optional: pricing keys differ between models (audio
/// pricing lives under `audio`, and many models omit it), and a missing field
/// should drop a label rather than the whole model.
pub fn parse_catalog(json: &Value) -> Result<Catalog, String> {
    let entries = json
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or("Model list response had no `data` array")?;

    let models: Vec<ModelInfo> = entries.iter().filter_map(parse_model).collect();

    if models.is_empty() {
        return Err("Model list response contained no usable models".into());
    }

    Ok(Catalog {
        fetched_at: chrono::Utc::now().to_rfc3339(),
        models,
    })
}

/// Whether a model lists `wanted` under the given modality array.
fn modalities_contains(entry: &Value, field: &str, wanted: &str) -> bool {
    entry
        .get("architecture")
        .and_then(|a| a.get(field))
        .and_then(|m| m.as_array())
        .is_some_and(|modalities| modalities.iter().any(|m| m.as_str() == Some(wanted)))
}

fn output_modalities_contains(entry: &Value, wanted: &str) -> bool {
    modalities_contains(entry, "output_modalities", wanted)
}

fn parse_model(entry: &Value) -> Option<ModelInfo> {
    let id = entry.get("id")?.as_str()?.to_string();
    let name = entry
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or(&id)
        .to_string();

    let audio_input = modalities_contains(entry, "input_modalities", "audio");

    // A negative price is OpenRouter's sentinel for "varies by route"
    // (openrouter/auto and friends). Treat it as unknown, not as free.
    let price = |key: &str| {
        entry
            .get("pricing")
            .and_then(|p| p.get(key))
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| *v >= 0.0)
    };

    Some(ModelInfo {
        id,
        name,
        context_length: entry
            .get("context_length")
            .and_then(|c| c.as_u64())
            .unwrap_or(0),
        audio_input,
        transcription_only: output_modalities_contains(entry, "transcription"),
        prompt_price: price("prompt"),
        completion_price: price("completion"),
        audio_price: price("audio"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> Value {
        serde_json::json!({
            "data": [
                {
                    "id": "google/gemini-2.5-flash",
                    "name": "Google: Gemini 2.5 Flash",
                    "context_length": 1_048_576,
                    "architecture": {"input_modalities": ["text", "image", "audio"]},
                    "pricing": {
                        "prompt": "0.0000003",
                        "completion": "0.0000025",
                        "audio": "0.000001"
                    }
                },
                {
                    "id": "some/text-only",
                    "name": "Some: Text Only",
                    "context_length": 128_000,
                    "architecture": {"input_modalities": ["text"]},
                    "pricing": {"prompt": "0.0000001", "completion": "0.0000002"}
                },
                {
                    "id": "some/unpriced",
                    "architecture": {"input_modalities": ["text"]},
                    "pricing": {}
                }
            ]
        })
    }

    #[test]
    fn test_parse_reads_modalities_and_pricing() {
        let catalog = parse_catalog(&sample_payload()).unwrap();
        assert_eq!(catalog.models.len(), 3);

        let flash = &catalog.models[0];
        assert!(flash.audio_input);
        assert_eq!(flash.audio_price, Some(0.000001));
        assert_eq!(flash.context_label(), "1M");
    }

    #[test]
    fn test_parse_tolerates_missing_fields() {
        let catalog = parse_catalog(&sample_payload()).unwrap();
        let unpriced = catalog
            .models
            .iter()
            .find(|m| m.id == "some/unpriced")
            .unwrap();

        // Name falls back to the id, and absent prices stay absent.
        assert_eq!(unpriced.name, "some/unpriced");
        assert_eq!(unpriced.prompt_price, None);
        assert_eq!(unpriced.context_label(), "—");
        assert_eq!(unpriced.cost_label(unpriced.analysis_cost_per_hour()), "—");
    }

    #[test]
    fn test_variable_pricing_is_unknown_not_free() {
        let payload = serde_json::json!({"data": [{
            "id": "openrouter/auto",
            "architecture": {"input_modalities": ["text", "audio"]},
            "pricing": {"prompt": "-1", "completion": "-1"}
        }]});
        let catalog = parse_catalog(&payload).unwrap();
        let auto = &catalog.models[0];
        assert_eq!(auto.prompt_price, None);
        assert_eq!(auto.cost_label(auto.analysis_cost_per_hour()), "—");
    }

    #[test]
    fn test_batch_routes_are_excluded() {
        let payload = serde_json::json!({"data": [
            {
                "id": "google/gemini-2.5-flash:batch",
                "architecture": {"input_modalities": ["audio"]},
                "pricing": {"prompt": "0.0000001", "completion": "0.0000002", "audio": "0.0000005"}
            },
            {
                "id": "google/gemini-2.5-flash",
                "architecture": {"input_modalities": ["audio"]},
                "pricing": {"prompt": "0.0000003", "completion": "0.0000025", "audio": "0.000001"}
            }
        ]});
        let catalog = parse_catalog(&payload).unwrap();
        assert_eq!(catalog.models.len(), 2);

        let ids: Vec<&str> = catalog
            .transcription_models()
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(ids, vec!["google/gemini-2.5-flash"]);
        let ids: Vec<&str> = catalog
            .analysis_models()
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(ids, vec!["google/gemini-2.5-flash"]);
    }

    #[test]
    fn test_parse_rejects_malformed_payload() {
        assert!(parse_catalog(&serde_json::json!({})).is_err());
        assert!(parse_catalog(&serde_json::json!({"data": []})).is_err());
    }

    #[test]
    fn test_transcription_models_are_audio_only() {
        let catalog = parse_catalog(&sample_payload()).unwrap();
        let ids: Vec<&str> = catalog
            .transcription_models()
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(ids, vec!["google/gemini-2.5-flash"]);
    }

    #[test]
    fn test_analysis_models_sort_cheapest_first_unpriced_last() {
        let catalog = parse_catalog(&sample_payload()).unwrap();
        let ids: Vec<&str> = catalog
            .analysis_models()
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["some/text-only", "google/gemini-2.5-flash", "some/unpriced"]
        );
    }

    #[test]
    fn test_cost_estimates() {
        let catalog = parse_catalog(&sample_payload()).unwrap();
        let flash = &catalog.models[0];

        // 115_200 audio tokens/h * $0.000001 + 13_000 out tokens * $0.0000025
        let transcription = flash.transcription_cost_per_hour().unwrap();
        assert!((transcription - 0.1477).abs() < 0.001, "got {transcription}");
        assert_eq!(flash.cost_label(Some(transcription)), "$0.15/h");
    }

    #[test]
    fn test_builtin_is_usable_offline() {
        let catalog = builtin();
        assert!(!catalog.transcription_models().is_empty());
        assert!(catalog.models.iter().all(|m| m.audio_input));
    }

    /// The default model must be present offline and flagged as speech-to-text,
    /// or the client would send it to the chat endpoint.
    #[test]
    fn test_builtin_carries_default_transcription_model() {
        let catalog = builtin();
        let default = catalog
            .models
            .iter()
            .find(|m| m.id == "microsoft/mai-transcribe-2")
            .expect("default model missing from built-in list");
        assert!(default.transcription_only);
        assert_eq!(default.cost_label(default.transcription_cost_per_hour()), "$0.10/h");
    }

    #[test]
    fn test_supports_diarization() {
        assert!(supports_diarization("microsoft/mai-transcribe-2"));
        assert!(supports_diarization("microsoft/mai-transcribe-1.5"));
        assert!(!supports_diarization("openai/whisper-large-v3-turbo"));
        assert!(!supports_diarization("google/gemini-2.5-flash"));
    }

    #[test]
    fn test_builtin_asr_excluded_from_analysis() {
        let catalog = builtin();
        assert!(catalog
            .analysis_models()
            .iter()
            .all(|m| !m.transcription_only));
    }
}
