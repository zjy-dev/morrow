use crate::config::UserPreferences;
use crate::google::Task;
use chrono::{NaiveTime, Duration};

use serde::{Deserialize, Serialize};

/// Time slot representing available time range
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSlot {
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub slot_type: SlotType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SlotType {
    Available,      // Free time for tasks
    Fixed,          // Fixed activities (meals, sleep prep)
    Buffer,         // Buffer/transition time
}

/// Fixed activity extracted from preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedActivity {
    pub name: String,
    pub start: NaiveTime,
    pub duration_minutes: u32,
}

/// Time hint extracted from task notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeHint {
    pub preferred_start: Option<NaiveTime>,
    pub preferred_end: Option<NaiveTime>,
    pub duration_hint: Option<u32>,  // minutes
    pub priority: Priority,
    pub time_period: Option<TimePeriod>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Priority {
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TimePeriod {
    Morning,    // 6:00 - 12:00
    Afternoon,  // 12:00 - 18:00
    Evening,    // 18:00 - 22:00
}

/// Preprocessed task with extracted hints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessedTask {
    pub id: usize,
    pub title: String,
    pub notes: Option<String>,
    pub hints: TimeHint,
}

/// Day constraints extracted from user preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayConstraints {
    pub wake_time: NaiveTime,
    pub sleep_time: NaiveTime,
    pub fixed_activities: Vec<FixedActivity>,
    pub available_slots: Vec<TimeSlot>,
    pub total_available_minutes: u32,
}

impl Default for TimeHint {
    fn default() -> Self {
        Self {
            preferred_start: None,
            preferred_end: None,
            duration_hint: None,
            priority: Priority::Normal,
            time_period: None,
        }
    }
}

pub struct Preprocessor;

impl Preprocessor {
    /// Parse user preferences to extract day constraints
    pub fn extract_constraints(preferences: &UserPreferences) -> DayConstraints {
        let prefs = &preferences.prefs;
        
        // Parse wake time (default 7:30)
        let wake_time = Self::parse_time_from_pref(prefs.get("wake_up"))
            .unwrap_or_else(|| NaiveTime::from_hms_opt(7, 30, 0).unwrap());
        
        // Parse sleep time (default 23:00)
        let sleep_time = Self::parse_time_from_pref(prefs.get("sleep"))
            .unwrap_or_else(|| NaiveTime::from_hms_opt(23, 0, 0).unwrap());
        
        // Extract fixed activities
        let mut fixed_activities = Vec::new();
        
        // Morning routine (30 min after wake)
        fixed_activities.push(FixedActivity {
            name: "起床洗漱".to_string(),
            start: wake_time,
            duration_minutes: 30,
        });
        
        // Breakfast
        if let Some(breakfast_time) = Self::parse_time_from_pref(prefs.get("breakfast")) {
            fixed_activities.push(FixedActivity {
                name: "早餐".to_string(),
                start: breakfast_time,
                duration_minutes: 30,
            });
        } else {
            // Default: 30 min after wake
            let breakfast_start = wake_time + Duration::minutes(30);
            fixed_activities.push(FixedActivity {
                name: "早餐".to_string(),
                start: breakfast_start,
                duration_minutes: 30,
            });
        }
        
        // Lunch
        if let Some(lunch_time) = Self::parse_time_from_pref(prefs.get("lunch")) {
            fixed_activities.push(FixedActivity {
                name: "午餐".to_string(),
                start: lunch_time,
                duration_minutes: 60,
            });
        } else {
            fixed_activities.push(FixedActivity {
                name: "午餐".to_string(),
                start: NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
                duration_minutes: 60,
            });
        }
        
        // Dinner
        if let Some(dinner_time) = Self::parse_time_from_pref(prefs.get("dinner")) {
            fixed_activities.push(FixedActivity {
                name: "晚餐".to_string(),
                start: dinner_time,
                duration_minutes: 60,
            });
        } else {
            fixed_activities.push(FixedActivity {
                name: "晚餐".to_string(),
                start: NaiveTime::from_hms_opt(18, 30, 0).unwrap(),
                duration_minutes: 60,
            });
        }
        
        // Shower
        if let Some(shower_time) = Self::parse_time_from_pref(prefs.get("shower")) {
            fixed_activities.push(FixedActivity {
                name: "洗澡".to_string(),
                start: shower_time,
                duration_minutes: 30,
            });
        } else {
            // Default: 1.5 hours before sleep
            let shower_start = sleep_time - Duration::minutes(90);
            fixed_activities.push(FixedActivity {
                name: "洗澡".to_string(),
                start: shower_start,
                duration_minutes: 30,
            });
        }
        
        // Sleep preparation
        let sleep_prep_start = sleep_time - Duration::minutes(30);
        fixed_activities.push(FixedActivity {
            name: "睡前准备".to_string(),
            start: sleep_prep_start,
            duration_minutes: 30,
        });
        
        // Sort by start time
        fixed_activities.sort_by_key(|a| a.start);
        
        // Calculate available slots
        let available_slots = Self::calculate_available_slots(
            wake_time,
            sleep_time,
            &fixed_activities,
        );
        
        let total_available_minutes: u32 = available_slots
            .iter()
            .filter(|s| s.slot_type == SlotType::Available)
            .map(|s| Self::slot_duration_minutes(s))
            .sum();
        
        DayConstraints {
            wake_time,
            sleep_time,
            fixed_activities,
            available_slots,
            total_available_minutes,
        }
    }
    
    /// Parse time from preference string
    fn parse_time_from_pref(pref: Option<&String>) -> Option<NaiveTime> {
        let pref = pref?;
        Self::extract_time_from_text(pref)
    }
    
    /// Extract time from natural language text
    fn extract_time_from_text(text: &str) -> Option<NaiveTime> {
        // Pattern: HH:MM or H:MM
        let re = regex::Regex::new(r"(\d{1,2})[:\s点](\d{0,2})").ok()?;
        if let Some(caps) = re.captures(text) {
            let hour: u32 = caps.get(1)?.as_str().parse().ok()?;
            let minute: u32 = caps.get(2)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            if hour < 24 && minute < 60 {
                return NaiveTime::from_hms_opt(hour, minute, 0);
            }
        }
        
        // Pattern: "X点" or "X点半"
        if text.contains("点半") {
            let re = regex::Regex::new(r"(\d{1,2})点半").ok()?;
            if let Some(caps) = re.captures(text) {
                let hour: u32 = caps.get(1)?.as_str().parse().ok()?;
                if hour < 24 {
                    return NaiveTime::from_hms_opt(hour, 30, 0);
                }
            }
        }
        
        None
    }
    
    /// Calculate available time slots between fixed activities
    fn calculate_available_slots(
        wake_time: NaiveTime,
        sleep_time: NaiveTime,
        fixed_activities: &[FixedActivity],
    ) -> Vec<TimeSlot> {
        let mut slots = Vec::new();
        let mut current_time = wake_time;
        
        for activity in fixed_activities {
            // Available slot before this activity
            if current_time < activity.start {
                let gap_minutes = (activity.start - current_time).num_minutes();
                if gap_minutes > 10 {
                    // Add buffer before fixed activity
                    let buffer_start = activity.start - Duration::minutes(5);
                    if current_time < buffer_start {
                        slots.push(TimeSlot {
                            start: current_time,
                            end: buffer_start,
                            slot_type: SlotType::Available,
                        });
                    }
                    slots.push(TimeSlot {
                        start: buffer_start,
                        end: activity.start,
                        slot_type: SlotType::Buffer,
                    });
                }
            }
            
            // Fixed activity slot
            let activity_end = activity.start + Duration::minutes(activity.duration_minutes as i64);
            slots.push(TimeSlot {
                start: activity.start,
                end: activity_end,
                slot_type: SlotType::Fixed,
            });
            
            current_time = activity_end;
        }
        
        // Remaining time until sleep
        if current_time < sleep_time {
            slots.push(TimeSlot {
                start: current_time,
                end: sleep_time,
                slot_type: SlotType::Available,
            });
        }
        
        slots
    }
    
    fn slot_duration_minutes(slot: &TimeSlot) -> u32 {
        (slot.end - slot.start).num_minutes() as u32
    }
    
    /// Preprocess tasks and extract time hints
    pub fn preprocess_tasks(tasks: &[Task]) -> Vec<PreprocessedTask> {
        tasks
            .iter()
            .enumerate()
            .map(|(id, task)| {
                let hints = Self::extract_hints(&task.title, task.notes.as_deref());
                PreprocessedTask {
                    id,
                    title: task.title.clone(),
                    notes: task.notes.clone(),
                    hints,
                }
            })
            .collect()
    }
    
    /// Extract time hints from task title and notes
    fn extract_hints(title: &str, notes: Option<&str>) -> TimeHint {
        let combined = format!("{} {}", title, notes.unwrap_or(""));
        let text = combined.to_lowercase();
        
        let mut hints = TimeHint::default();
        
        // Extract time period
        if text.contains("早上") || text.contains("上午") || text.contains("morning") {
            hints.time_period = Some(TimePeriod::Morning);
        } else if text.contains("下午") || text.contains("afternoon") {
            hints.time_period = Some(TimePeriod::Afternoon);
        } else if text.contains("晚上") || text.contains("evening") || text.contains("傍晚") {
            hints.time_period = Some(TimePeriod::Evening);
        }
        
        // Extract priority
        if text.contains("urgent") || text.contains("紧急") || text.contains("重要") 
           || text.contains("必须") || text.contains("优先") {
            hints.priority = Priority::High;
        } else if text.contains("可选") || text.contains("如果有时间") || text.contains("optional") {
            hints.priority = Priority::Low;
        }
        
        // Extract duration hints
        if let Some(duration) = Self::extract_duration(&text) {
            hints.duration_hint = Some(duration);
        }
        
        // Extract specific time
        if let Some(time) = Self::extract_time_from_text(&combined) {
            hints.preferred_start = Some(time);
        }
        
        hints
    }
    
    /// Extract duration from text (e.g., "2 hours", "30 min", "1小时")
    fn extract_duration(text: &str) -> Option<u32> {
        // Pattern: X hours / X 小时
        let hour_re = regex::Regex::new(r"(\d+)\s*(?:hours?|小时|个小时)").ok()?;
        if let Some(caps) = hour_re.captures(text) {
            let hours: u32 = caps.get(1)?.as_str().parse().ok()?;
            return Some(hours * 60);
        }
        
        // Pattern: X min / X 分钟
        let min_re = regex::Regex::new(r"(\d+)\s*(?:min(?:utes?)?|分钟?)").ok()?;
        if let Some(caps) = min_re.captures(text) {
            let minutes: u32 = caps.get(1)?.as_str().parse().ok()?;
            return Some(minutes);
        }
        
        // Pattern: 半小时
        if text.contains("半小时") || text.contains("半个小时") {
            return Some(30);
        }
        
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_time() {
        assert_eq!(
            Preprocessor::extract_time_from_text("7:30左右"),
            Some(NaiveTime::from_hms_opt(7, 30, 0).unwrap())
        );
        assert_eq!(
            Preprocessor::extract_time_from_text("12点"),
            Some(NaiveTime::from_hms_opt(12, 0, 0).unwrap())
        );
        assert_eq!(
            Preprocessor::extract_time_from_text("6点半"),
            Some(NaiveTime::from_hms_opt(6, 30, 0).unwrap())
        );
    }
    
    #[test]
    fn test_extract_duration() {
        assert_eq!(Preprocessor::extract_duration("2 hours"), Some(120));
        assert_eq!(Preprocessor::extract_duration("30 min"), Some(30));
        assert_eq!(Preprocessor::extract_duration("1小时"), Some(60));
        assert_eq!(Preprocessor::extract_duration("半小时"), Some(30));
    }
}
