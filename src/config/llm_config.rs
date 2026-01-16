use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApiFormat {
    OpenAI,
    Anthropic,
    Gemini,
}

impl Default for ApiFormat {
    fn default() -> Self {
        Self::OpenAI
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub api_format: ApiFormat,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_model() -> String {
    "gpt-4o".to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            api_format: ApiFormat::default(),
            base_url: default_base_url(),
            model: default_model(),
        }
    }
}

impl LlmConfig {
    pub fn get_api_key(&self) -> Option<String> {
        std::env::var("MORROW_LLM_API_KEY").ok()
    }
}
