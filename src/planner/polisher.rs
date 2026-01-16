use crate::config::{ApiFormat, LlmConfig, UserPreferences};
use crate::error::{MorrowError, Result};
use crate::planner::engine::ScheduledItem;
use serde::{Deserialize, Serialize};

/// Polished schedule item with enhanced titles and suggestions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolishedItem {
    pub time: String,
    pub duration: u32,
    pub title: String,
    pub suggestion: Option<String>,  // Optional tip or suggestion
}

pub struct Polisher {
    config: LlmConfig,
    client: reqwest::Client,
}

impl Polisher {
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

    /// Polish schedule titles and add helpful suggestions
    pub async fn polish_schedule(
        &self,
        schedule: &[ScheduledItem],
        preferences: &UserPreferences,
        date: &str,
        day_of_week: &str,
    ) -> Result<Vec<PolishedItem>> {
        if schedule.is_empty() {
            return Ok(Vec::new());
        }

        let system_prompt = self.build_system_prompt();
        let user_prompt = self.build_user_prompt(schedule, preferences, date, day_of_week);

        let response = self.call_llm(&system_prompt, &user_prompt).await?;
        self.parse_response(&response, schedule)
    }

    fn build_system_prompt(&self) -> String {
        r#"You are a schedule polisher. Improve schedule item titles and add helpful suggestions.

For each item, you may:
1. Improve the title to be more descriptive and motivating
2. Add a brief suggestion (optional, only if helpful)
3. Keep the original meaning and timing intact

Rules:
- Keep titles concise (under 30 characters if possible)
- Use consistent language (match user's language preference)
- Suggestions should be actionable and brief
- Don't change time or duration
- For breaks, add relaxation suggestions
- For work sessions, add focus tips
- For meals, add healthy eating reminders

Output ONLY valid JSON array, no markdown:
[
  {"time": "07:30", "duration": 30, "title": "起床洗漱", "suggestion": null},
  {"time": "09:00", "duration": 25, "title": "专注写代码 #1", "suggestion": "先处理最难的任务"},
  ...
]"#.to_string()
    }

    fn build_user_prompt(
        &self,
        schedule: &[ScheduledItem],
        preferences: &UserPreferences,
        date: &str,
        day_of_week: &str,
    ) -> String {
        let schedule_json = schedule
            .iter()
            .map(|item| {
                serde_json::json!({
                    "time": item.time,
                    "duration": item.duration,
                    "title": item.title,
                    "type": format!("{:?}", item.item_type)
                })
            })
            .collect::<Vec<_>>();

        let context = if let Some(bio) = &preferences.bio {
            format!("User context: {}\n\n", bio)
        } else {
            String::new()
        };

        format!(
            "{}Date: {} ({})\n\nSchedule to polish:\n{}",
            context,
            date,
            day_of_week,
            serde_json::to_string_pretty(&schedule_json).unwrap_or_default()
        )
    }

    async fn call_llm(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        match self.config.api_format {
            ApiFormat::OpenAI => self.call_openai(system_prompt, user_prompt).await,
            ApiFormat::Anthropic => self.call_anthropic(system_prompt, user_prompt).await,
            ApiFormat::Gemini => self.call_gemini(system_prompt, user_prompt).await,
        }
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

    fn parse_response(
        &self,
        response: &str,
        original: &[ScheduledItem],
    ) -> Result<Vec<PolishedItem>> {
        let json_str = response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let polished: Vec<RawPolishedItem> = serde_json::from_str(json_str)
            .map_err(|e| MorrowError::Llm(format!(
                "Failed to parse polished schedule: {}. Response: {}",
                e, response
            )))?;

        // Match polished items with original by time
        let mut result = Vec::new();
        for orig in original {
            let matching = polished
                .iter()
                .find(|p| p.time == orig.time)
                .map(|p| PolishedItem {
                    time: p.time.clone(),
                    duration: p.duration.unwrap_or(orig.duration),
                    title: p.title.clone(),
                    suggestion: p.suggestion.clone(),
                })
                .unwrap_or_else(|| PolishedItem {
                    time: orig.time.clone(),
                    duration: orig.duration,
                    title: orig.title.clone(),
                    suggestion: None,
                });
            result.push(matching);
        }

        Ok(result)
    }

    /// Simple fallback that just converts without LLM
    pub fn fallback_polish(schedule: &[ScheduledItem]) -> Vec<PolishedItem> {
        schedule
            .iter()
            .map(|item| PolishedItem {
                time: item.time.clone(),
                duration: item.duration,
                title: item.title.clone(),
                suggestion: None,
            })
            .collect()
    }
}

#[derive(Debug, Deserialize)]
struct RawPolishedItem {
    time: String,
    duration: Option<u32>,
    title: String,
    suggestion: Option<String>,
}
