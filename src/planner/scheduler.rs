use crate::config::{ApiFormat, LlmConfig};
use crate::error::{MorrowError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledItem {
    pub time: String,
    pub duration: u32,
    pub title: String,
}

pub struct Scheduler {
    config: LlmConfig,
    client: reqwest::Client,
}

impl Scheduler {
    pub fn new(config: LlmConfig) -> Result<Self> {
        if config.get_api_key().is_none() {
            return Err(MorrowError::Config(
                "MORROW_LLM_API_KEY environment variable not set".to_string(),
            ));
        }
        Ok(Self {
            config,
            client: reqwest::Client::new(),
        })
    }

    pub async fn generate_schedule(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<Vec<ScheduledItem>> {
        let response = match self.config.api_format {
            ApiFormat::OpenAI => self.call_openai(system_prompt, user_prompt).await?,
            ApiFormat::Anthropic => self.call_anthropic(system_prompt, user_prompt).await?,
            ApiFormat::Gemini => self.call_gemini(system_prompt, user_prompt).await?,
        };

        self.parse_schedule(&response)
    }

    async fn call_openai(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let api_key = self.config.get_api_key().unwrap();
        let url = format!("{}/chat/completions", self.config.base_url);

        let body = serde_json::json!({
            "model": self.config.model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "temperature": 0.7
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;
        
        if !status.is_success() {
            return Err(MorrowError::Llm(format!("API error {}: {}", status, text)));
        }

        let json: serde_json::Value = serde_json::from_str(&text)?;
        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| MorrowError::Llm("Invalid response format".to_string()))
    }

    async fn call_anthropic(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let api_key = self.config.get_api_key().unwrap();
        let url = format!("{}/messages", self.config.base_url);

        let body = serde_json::json!({
            "model": self.config.model,
            "max_tokens": 4096,
            "system": system_prompt,
            "messages": [
                {"role": "user", "content": user_prompt}
            ]
        });

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;
        
        if !status.is_success() {
            return Err(MorrowError::Llm(format!("API error {}: {}", status, text)));
        }

        let json: serde_json::Value = serde_json::from_str(&text)?;
        json["content"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| MorrowError::Llm("Invalid response format".to_string()))
    }

    async fn call_gemini(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let api_key = self.config.get_api_key().unwrap();
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.config.base_url, self.config.model, api_key
        );

        let body = serde_json::json!({
            "contents": [{
                "parts": [{"text": format!("{}\n\n{}", system_prompt, user_prompt)}]
            }]
        });

        let resp = self.client.post(&url).json(&body).send().await?;

        let status = resp.status();
        let text = resp.text().await?;
        
        if !status.is_success() {
            return Err(MorrowError::Llm(format!("API error {}: {}", status, text)));
        }

        let json: serde_json::Value = serde_json::from_str(&text)?;
        json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| MorrowError::Llm("Invalid response format".to_string()))
    }

    fn parse_schedule(&self, response: &str) -> Result<Vec<ScheduledItem>> {
        let json_str = response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let items: Vec<ScheduledItem> = serde_json::from_str(json_str)
            .map_err(|e| MorrowError::Llm(format!("Failed to parse schedule: {}. Response: {}", e, response)))?;

        Ok(items)
    }
}
