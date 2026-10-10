use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Public provider settings. API keys cannot be serialized by this type.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettings {
    pub cloud_ai_enabled: bool,
    pub provider_kind: String,
    pub model: String,
    pub base_url: String,
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            cloud_ai_enabled: false,
            provider_kind: "compatible".into(),
            model: "agnes-3.0-flash".into(),
            base_url: "https://apihub.agnes-ai.com/v1".into(),
        }
    }
}

fn path() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir)
        .join("ScreenplayStudio").join("agent-provider.json")
}

pub fn load() -> ProviderSettings {
    std::fs::read(path()).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

pub fn save(settings: &ProviderSettings) -> Result<(), String> {
    let path = path();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|error| error.to_string())?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    std::fs::rename(temp, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_provider_metadata_contains_no_credential_field() {
        let json = serde_json::to_value(ProviderSettings::default()).unwrap();
        assert_eq!(json["model"], "agnes-3.0-flash");
        assert_eq!(json["baseUrl"], "https://apihub.agnes-ai.com/v1");
        assert!(json.get("apiKey").is_none());
        assert_eq!(json["cloudAiEnabled"], false);
    }
}

#[cfg(test)]
mod ipc_contract_tests {
    #[test]
    fn config_dto_matches_frontend_and_never_exposes_key() {
        let dto = crate::AgentConfigDto { cloud_ai_enabled: true,
            provider_kind: "compatible".into(), model: "agnes-3.0-flash".into(),
            api_key_set: true, base_url: "https://apihub.agnes-ai.com/v1".into() };
        let value = serde_json::to_value(dto).unwrap();
        assert_eq!(value["cloudAiEnabled"], true);
        assert_eq!(value["apiKeySet"], true);
        assert_eq!(value["baseUrl"], "https://apihub.agnes-ai.com/v1");
        assert!(value.get("api_key_set").is_none());
        assert!(value.get("apiKey").is_none());
        assert_eq!(value.as_object().unwrap().len(), 5);
    }
}
