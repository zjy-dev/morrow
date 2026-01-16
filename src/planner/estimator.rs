use crate::config::{ApiFormat, LlmConfig, UserPreferences};
use crate::error::{MorrowError, Result};
use crate::planner::preprocessor::{PreprocessedTask, Priority, TimePeriod};
use serde::de::Error as DeError;
use serde::{Deserialize, Serialize};

/// LLM estimation result for a single task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEstimate {
    pub task_id: usize,
    pub estimated_duration: u32,  // minutes
    pub priority: Priority,
    pub preferred_period: Option<TimePeriod>,
    pub requires_focus: bool,      // Whether task needs deep focus (apply pomodoro)
    pub can_split: bool,           // Whether task can be split across time slots
}

/// Request structure for LLM estimation
#[derive(Debug, Serialize)]
struct EstimationRequest {
    tasks: Vec<TaskForEstimation>,
    user_context: Option<String>,
}

#[derive(Debug, Serialize)]
struct TaskForEstimation {
    id: usize,
    title: String,
    notes: Option<String>,
    hints: TaskHints,
}

#[derive(Debug, Serialize)]
struct TaskHints {
    duration_hint: Option<u32>,
    time_period: Option<String>,
    priority: String,
}

pub struct Estimator {
    config: LlmConfig,
    client: reqwest::Client,
}

impl Estimator {
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

    /// Estimate duration and properties for each task using LLM
    pub async fn estimate_tasks(
        &self,
        tasks: &[PreprocessedTask],
        preferences: &UserPreferences,
    ) -> Result<Vec<TaskEstimate>> {
        if tasks.is_empty() {
            return Ok(Vec::new());
        }

        let request = self.build_request(tasks, preferences);
        let system_prompt = self.build_system_prompt();
        let user_prompt = serde_json::to_string_pretty(&request)
            .map_err(|e| MorrowError::Llm(format!("Failed to serialize request: {}", e)))?;

        let response = self.call_llm(&system_prompt, &user_prompt).await?;
        self.parse_response(&response, tasks)
    }

    fn build_request(&self, tasks: &[PreprocessedTask], preferences: &UserPreferences) -> EstimationRequest {
        let tasks_for_estimation: Vec<TaskForEstimation> = tasks
            .iter()
            .map(|t| TaskForEstimation {
                id: t.id,
                title: t.title.clone(),
                notes: t.notes.clone(),
                hints: TaskHints {
                    duration_hint: t.hints.duration_hint,
                    time_period: t.hints.time_period.map(|p| format!("{:?}", p)),
                    priority: format!("{:?}", t.hints.priority),
                },
            })
            .collect();

        EstimationRequest {
            tasks: tasks_for_estimation,
            user_context: preferences.bio.clone(),
        }
    }

    fn build_system_prompt(&self) -> String {
        r#"You are a task estimation assistant. Analyze tasks and estimate their properties.

For each task, output:
- estimated_duration: realistic time in minutes (15-240 range, round to 5)
- priority: "High", "Normal", or "Low"
- preferred_period: "Morning", "Afternoon", "Evening", or null
- requires_focus: true if deep concentration needed (coding, writing, study)
- can_split: true if task can be done in multiple sessions

Rules:
1. Use hints if provided (duration_hint, time_period, priority)
2. Consider user_context for personalized estimates
3. Short tasks: 15-30 min (emails, calls, quick reviews)
4. Medium tasks: 30-90 min (meetings, focused work sessions)
5. Long tasks: 90-240 min (deep work, complex projects)
6. Morning is best for focus tasks, afternoon for meetings/collaborative work

Output ONLY valid JSON array, no markdown, no explanation:
[
  {"task_id": 0, "estimated_duration": 60, "priority": "Normal", "preferred_period": "Morning", "requires_focus": true, "can_split": false},
  ...
]"#.to_string()
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
            "temperature": 0.3,
            "response_format": {"type": "json_object"}
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
            "max_tokens": 2048,
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
            }],
            "generationConfig": {
                "responseMimeType": "application/json"
            }
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

    fn parse_response(&self, response: &str, tasks: &[PreprocessedTask]) -> Result<Vec<TaskEstimate>> {
        let json_str = response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Try to parse as array first
        let estimates: Vec<RawEstimate> = if json_str.starts_with('[') {
            serde_json::from_str(json_str)
        } else {
            // Try to extract array from object
            let obj: serde_json::Value = serde_json::from_str(json_str)?;
            if let Some(arr) = obj.get("estimates").or(obj.get("tasks")) {
                serde_json::from_value(arr.clone())
            } else {
                Err(serde_json::Error::custom("No estimates array found"))
            }
        }.map_err(|e| MorrowError::Llm(format!("Failed to parse estimates: {}. Response: {}", e, response)))?;

        // Convert and validate
        let mut result = Vec::new();
        for raw in estimates {
            let task = tasks.iter().find(|t| t.id == raw.task_id);
            if task.is_none() {
                continue;
            }

            result.push(TaskEstimate {
                task_id: raw.task_id,
                estimated_duration: raw.estimated_duration.clamp(15, 240),
                priority: Self::parse_priority(&raw.priority),
                preferred_period: raw.preferred_period.as_deref().and_then(Self::parse_period),
                requires_focus: raw.requires_focus.unwrap_or(false),
                can_split: raw.can_split.unwrap_or(true),
            });
        }

        // Fill in missing tasks with defaults
        for task in tasks {
            if !result.iter().any(|e| e.task_id == task.id) {
                result.push(TaskEstimate {
                    task_id: task.id,
                    estimated_duration: task.hints.duration_hint.unwrap_or(30),
                    priority: task.hints.priority,
                    preferred_period: task.hints.time_period,
                    requires_focus: false,
                    can_split: true,
                });
            }
        }

        result.sort_by_key(|e| e.task_id);
        Ok(result)
    }

    fn parse_priority(s: &str) -> Priority {
        match s.to_lowercase().as_str() {
            "high" => Priority::High,
            "low" => Priority::Low,
            _ => Priority::Normal,
        }
    }

    fn parse_period(s: &str) -> Option<TimePeriod> {
        match s.to_lowercase().as_str() {
            "morning" => Some(TimePeriod::Morning),
            "afternoon" => Some(TimePeriod::Afternoon),
            "evening" => Some(TimePeriod::Evening),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawEstimate {
    task_id: usize,
    estimated_duration: u32,
    priority: String,
    preferred_period: Option<String>,
    requires_focus: Option<bool>,
    can_split: Option<bool>,
}
