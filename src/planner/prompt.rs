use crate::config::UserPreferences;
use crate::error::{MorrowError, Result};
use crate::google::Task;
use chrono::{Duration, Utc};
use chrono_tz::Tz;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PlanningInput {
    pub date: String,
    pub day_of_week: String,
    pub user_preferences: serde_json::Value,
    pub tasks: Vec<TaskInfo>,
}

#[derive(Debug, Serialize)]
pub struct TaskInfo {
    pub title: String,
    /// Task notes may contain time hints like "morning", "2 hours", "after lunch"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl From<&Task> for TaskInfo {
    fn from(task: &Task) -> Self {
        Self {
            title: task.title.clone(),
            notes: task.notes.clone(),
        }
    }
}

pub fn build_planning_input(
    preferences: &UserPreferences,
    tasks: &[Task],
    timezone: &str,
) -> Result<PlanningInput> {
    let tz: Tz = timezone.parse().map_err(|_| {
        MorrowError::Config(format!(
            "Invalid timezone: '{}'. Use IANA format like 'Asia/Shanghai' or 'America/New_York'",
            timezone
        ))
    })?;
    let tomorrow = (Utc::now().with_timezone(&tz) + Duration::days(1)).date_naive();
    Ok(PlanningInput {
        date: tomorrow.format("%Y-%m-%d").to_string(),
        day_of_week: tomorrow.format("%A").to_string(),
        user_preferences: preferences.to_json(),
        tasks: tasks.iter().map(TaskInfo::from).collect(),
    })
}

pub fn build_system_prompt() -> String {
    r#"You are a daily schedule planner. Your task is to create a practical, time-blocked schedule for tomorrow based on the user's preferences and tasks.

Rules:
1. Create a realistic schedule that respects the user's preferences (wake time, meals, sleep, etc.)
2. Allocate appropriate time for each task based on its title and notes
3. Pay attention to time hints in task notes (e.g., "morning", "2 hours", "after lunch", "urgent")
4. Include breaks and buffer time between tasks
5. If no time hint is given, estimate reasonable duration based on task complexity
6. If user provides a "bio" (self description), consider their life habits and physical conditions
7. Output ONLY a valid JSON array, no other text

Pomodoro Technique Guidelines:
- For focused work tasks, apply the Pomodoro Technique: 25 min work + 5 min break
- After 4 pomodoros (4×25 min work + 3×5 min break = 115 min), add a 35 min long break
- A full pomodoro cycle = 2.5 hours (4 work sessions + 3 short breaks + 1 long break)
- If the task is followed by a different activity (meal, meeting, etc.), skip the long break = 1h55min for 4 pomodoros
- For short tasks under 25 min, no need to apply pomodoro
- Label pomodoro work blocks clearly (e.g., "专注工作 #1", "短休息", "长休息")

Output format - a JSON array of scheduled items:
[
  {"time": "07:30", "duration": 30, "title": "起床洗漱"},
  {"time": "08:00", "duration": 30, "title": "早餐"},
  ...
]

Each item must have:
- time: 24-hour format "HH:MM"
- duration: minutes (integer)
- title: task description (string)
"#.to_string()
}

pub fn build_user_prompt(input: &PlanningInput) -> String {
    format!(
        "Please create a schedule for {} ({}).\n\nUser preferences:\n{}\n\nTasks to schedule:\n{}",
        input.date,
        input.day_of_week,
        serde_json::to_string_pretty(&input.user_preferences).unwrap_or_default(),
        serde_json::to_string_pretty(&input.tasks).unwrap_or_default()
    )
}
