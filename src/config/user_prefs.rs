use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserPreferences {
    /// 用户自述：生活习惯、身体情况等综述（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(flatten)]
    pub prefs: IndexMap<String, String>,
}

impl UserPreferences {
    pub fn with_defaults() -> Self {
        let mut prefs = IndexMap::new();
        prefs.insert("wake_up".to_string(), "7:30左右".to_string());
        prefs.insert("sleep".to_string(), "尽量11点前睡觉".to_string());
        prefs.insert("breakfast".to_string(), "起床后半小时".to_string());
        prefs.insert("lunch".to_string(), "12点到1点之间".to_string());
        prefs.insert("dinner".to_string(), "6点半到7点半".to_string());
        prefs.insert("shower".to_string(), "一般晚饭后洗澡".to_string());
        Self { bio: None, prefs }
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(bio) = &self.bio {
            map.insert("bio".to_string(), serde_json::Value::String(bio.clone()));
        }
        for (k, v) in &self.prefs {
            map.insert(k.clone(), serde_json::Value::String(v.clone()));
        }
        serde_json::Value::Object(map)
    }
}
