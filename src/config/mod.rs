mod user_prefs;
mod llm_config;

pub use user_prefs::*;
pub use llm_config::*;

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleConfig {
    pub source_list: String,
    pub output_list: String,
}

impl Default for GoogleConfig {
    fn default() -> Self {
        Self {
            source_list: "Tomorrow Tasks".to_string(),
            output_list: "Morrow Schedule".to_string(),
        }
    }
}

fn default_timezone() -> String {
    "Asia/Shanghai".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub google: GoogleConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub preferences: UserPreferences,
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            google: GoogleConfig::default(),
            llm: LlmConfig::default(),
            preferences: UserPreferences::default(),
            timezone: default_timezone(),
        }
    }
}

impl AppConfig {
    pub fn load(config_path: Option<PathBuf>) -> Result<Self> {
        let path = config_path.unwrap_or_else(Self::default_config_path);
        
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        let config: AppConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self, config_path: Option<PathBuf>) -> Result<()> {
        let path = config_path.unwrap_or_else(Self::default_config_path);
        
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = self.to_commented_yaml();
        std::fs::write(&path, content)?;
        Ok(())
    }

    fn yaml_scalar_lines(value: &str) -> Vec<String> {
        let mut serialized = serde_yaml::to_string(value)
            .unwrap_or_else(|_| format!("\"{}\"", value));
        if serialized.starts_with("---") {
            let mut lines: Vec<&str> = serialized.lines().collect();
            if lines.first() == Some(&"---") {
                lines.remove(0);
            }
            if lines.last() == Some(&"...") {
                lines.pop();
            }
            serialized = lines.join("\n");
        }
        serialized
            .trim_end_matches('\n')
            .lines()
            .map(|line| line.to_string())
            .collect()
    }

    fn yaml_scalar_inline(value: &str) -> String {
        let lines = Self::yaml_scalar_lines(value);
        if lines.len() == 1 {
            return lines[0].clone();
        }
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        format!("\"{}\"", escaped)
    }

    fn yaml_key(key: &str) -> String {
        let sanitized = key.replace('\n', " ");
        Self::yaml_scalar_inline(&sanitized)
    }

    fn push_yaml_kv(
        lines: &mut Vec<String>,
        indent: usize,
        key: &str,
        value: &str,
        comment: Option<&str>,
    ) {
        let key = Self::yaml_key(key);
        let value_lines = Self::yaml_scalar_lines(value);
        let indent_str = " ".repeat(indent);
        let first_value = value_lines
            .get(0)
            .cloned()
            .unwrap_or_else(|| "\"\"".to_string());
        let mut first_line = format!("{}{}: {}", indent_str, key, first_value);
        if value_lines.len() == 1 {
            if let Some(comment) = comment {
                first_line.push_str(&format!("  # {}", comment));
            }
        }
        lines.push(first_line);
        for line in value_lines.iter().skip(1) {
            lines.push(format!("{}  {}", indent_str, line));
        }
    }

    fn to_commented_yaml(&self) -> String {
        let mut lines = Vec::new();
        
        lines.push("# ============================================================================".to_string());
        lines.push("# Morrow 配置文件".to_string());
        lines.push("# ============================================================================".to_string());
        lines.push("# 运行 `morrow config init` 可重新配置".to_string());
        lines.push("# 运行 `morrow config path` 可查看配置文件路径".to_string());
        lines.push("# ============================================================================".to_string());
        lines.push(String::new());
        
        lines.push("# [必填] 时区设置 (IANA 格式: Asia/Shanghai, America/New_York, etc.)".to_string());
        Self::push_yaml_kv(
            &mut lines,
            0,
            "timezone",
            &self.timezone,
            None,
        );
        lines.push(String::new());
        
        lines.push("# [必填] Google Tasks 配置".to_string());
        lines.push("google:".to_string());
        Self::push_yaml_kv(
            &mut lines,
            2,
            "source_list",
            &self.google.source_list,
            Some("读取待办事项的源列表"),
        );
        Self::push_yaml_kv(
            &mut lines,
            2,
            "output_list",
            &self.google.output_list,
            Some("写入生成日程的目标列表"),
        );
        lines.push(String::new());
        
        lines.push("# [必填] LLM 配置 (API Key 通过 MORROW_LLM_API_KEY 环境变量设置)".to_string());
        lines.push("llm:".to_string());
        Self::push_yaml_kv(
            &mut lines,
            2,
            "api_format",
            &format!("{:?}", self.llm.api_format).to_lowercase(),
            Some("openai / anthropic / gemini"),
        );
        Self::push_yaml_kv(&mut lines, 2, "base_url", &self.llm.base_url, None);
        Self::push_yaml_kv(&mut lines, 2, "model", &self.llm.model, None);
        lines.push(String::new());
        
        lines.push("# [可选] 用户偏好设置 (自然语言描述，可自由添加字段)".to_string());
        lines.push("preferences:".to_string());
        
        if let Some(bio) = &self.preferences.bio {
            lines.push("  # 用户自述：生活习惯、身体状况、工作性质等".to_string());
            lines.push("  bio: |".to_string());
            for bio_line in bio.lines() {
                lines.push(format!("    {}", bio_line));
            }
        } else {
            lines.push("  # bio: |  # [可选] 用户自述".to_string());
            lines.push("  #   我是一名程序员，久坐较多，需要定期起来活动。".to_string());
        }
        
        for (key, value) in &self.preferences.prefs {
            Self::push_yaml_kv(&mut lines, 2, key, value, None);
        }
        lines.push("  # 可添加自定义字段: commute, exercise, focus_time, nap 等".to_string());
        lines.push(String::new());
        
        lines.join("\n")
    }

    pub fn default_config_path() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            dirs::home_dir()
                .unwrap_or_default()
                .join("Library/Application Support/morrow/config.yaml")
        }
        #[cfg(target_os = "windows")]
        {
            dirs::config_dir()
                .unwrap_or_default()
                .join("morrow/config.yaml")
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".config/morrow/config.yaml")
        }
    }

    pub fn credentials_path() -> PathBuf {
        Self::default_config_path()
            .parent()
            .unwrap()
            .join("credentials.json")
    }
}
