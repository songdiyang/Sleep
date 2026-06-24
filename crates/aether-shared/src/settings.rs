use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettings {
    pub ai: AiSettings,
    pub ui: UiSettings,
    pub remote: RemoteSettings,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct AiSettings {
    pub provider: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct UiSettings {
    pub theme: String,
    pub font_size: u32,
    pub sidebar_visible: bool,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct RemoteSettings {
    pub ssh_hosts: Vec<String>,
}

impl AppSettings {
    pub fn settings_path() -> PathBuf {
        let config_dir = dirs::config_dir().unwrap_or_else(|| std::env::temp_dir());
        let aether_dir = config_dir.join("Aether");
        let _ = std::fs::create_dir_all(&aether_dir);
        aether_dir.join("settings.json")
    }

    pub fn load() -> Self {
        let path = Self::settings_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str(&content) {
                return settings;
            }
        }
        Self::default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::settings_path();
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai: AiSettings {
                provider: "openai".to_string(),
                api_key: String::new(),
                base_url: None,
                model: "gpt-4".to_string(),
            },
            ui: UiSettings::default(),
            remote: RemoteSettings::default(),
        }
    }
}
