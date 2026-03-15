use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Environment variable mapping for config keys.
const ENV_MAP: &[(&str, &str)] = &[
    ("gemini_api_key", "GEMINI_API_KEY"),
    ("notion_token", "NOTION_TOKEN"),
    ("notion_parent_page_id", "NOTION_PARENT_PAGE_ID"),
    ("obsidian_vault_path", "OBSIDIAN_VAULT_PATH"),
];

/// Persistent application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub gemini_api_key: String,
    #[serde(default = "default_model")]
    pub gemini_model: String,
    #[serde(default)]
    pub participants: String,
    #[serde(default)]
    pub obsidian_vault_path: String,
    #[serde(default)]
    pub notion_token: String,
    #[serde(default)]
    pub notion_parent_page_id: String,
}

fn default_model() -> String {
    "gemini-2.5-flash".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gemini_api_key: String::new(),
            gemini_model: default_model(),
            participants: String::new(),
            obsidian_vault_path: String::new(),
            notion_token: String::new(),
            notion_parent_page_id: String::new(),
        }
    }
}

impl Config {
    /// Load config from disk, falling back to env vars then defaults.
    /// Note: expects dotenvy::dotenv() to have been called in main() already.
    pub fn load() -> Self {
        let mut cfg: Config = confy::load("meeting-transcriber", None).unwrap_or_default();
        // Override empty fields with env vars
        for &(key, env_var) in ENV_MAP {
            if let Ok(val) = std::env::var(env_var) {
                if !val.is_empty() {
                    match key {
                        "gemini_api_key" if cfg.gemini_api_key.is_empty() => {
                            cfg.gemini_api_key = val;
                        }
                        "notion_token" if cfg.notion_token.is_empty() => {
                            cfg.notion_token = val;
                        }
                        "notion_parent_page_id" if cfg.notion_parent_page_id.is_empty() => {
                            cfg.notion_parent_page_id = val;
                        }
                        "obsidian_vault_path" if cfg.obsidian_vault_path.is_empty() => {
                            cfg.obsidian_vault_path = val;
                        }
                        _ => {}
                    }
                }
            }
        }
        cfg
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<(), confy::ConfyError> {
        confy::store("meeting-transcriber", None, self)
    }

    /// Get the config file path (for debugging).
    pub fn file_path() -> Option<PathBuf> {
        confy::get_configuration_file_path("meeting-transcriber", None).ok()
    }

    /// Get value by key name, with env var fallback.
    pub fn get(&self, key: &str) -> String {
        let stored = match key {
            "gemini_api_key" => &self.gemini_api_key,
            "gemini_model" => &self.gemini_model,
            "participants" => &self.participants,
            "obsidian_vault_path" => &self.obsidian_vault_path,
            "notion_token" => &self.notion_token,
            "notion_parent_page_id" => &self.notion_parent_page_id,
            _ => return String::new(),
        };
        if !stored.is_empty() {
            return stored.clone();
        }
        // Fallback to env var
        let env_map: HashMap<&str, &str> = ENV_MAP.iter().copied().collect();
        if let Some(&env_key) = env_map.get(key) {
            std::env::var(env_key).unwrap_or_default()
        } else {
            String::new()
        }
    }

    /// Set value by key name.
    pub fn set(&mut self, key: &str, value: String) {
        match key {
            "gemini_api_key" => self.gemini_api_key = value,
            "gemini_model" => self.gemini_model = value,
            "participants" => self.participants = value,
            "obsidian_vault_path" => self.obsidian_vault_path = value,
            "notion_token" => self.notion_token = value,
            "notion_parent_page_id" => self.notion_parent_page_id = value,
            _ => {}
        }
    }

    /// Get participant names as a vec (comma-separated).
    pub fn participant_names(&self) -> Vec<String> {
        self.participants
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_model() {
        let cfg = Config::default();
        assert_eq!(cfg.gemini_model, "gemini-2.5-flash");
    }

    #[test]
    fn test_participant_names_parsing() {
        let mut cfg = Config::default();
        cfg.participants = "Alice, Bob, Charlie".into();
        let names = cfg.participant_names();
        assert_eq!(names, vec!["Alice", "Bob", "Charlie"]);
    }

    #[test]
    fn test_participant_names_empty() {
        let cfg = Config::default();
        assert!(cfg.participant_names().is_empty());
    }

    #[test]
    fn test_get_set() {
        let mut cfg = Config::default();
        cfg.set("gemini_api_key".into(), "test-key".into());
        assert_eq!(cfg.get("gemini_api_key"), "test-key");
    }

    #[test]
    fn test_get_unknown_key() {
        let cfg = Config::default();
        assert_eq!(cfg.get("nonexistent"), "");
    }
}
