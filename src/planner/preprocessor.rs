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
    /// Check if sleep time is past midnight (e.g., 02:00 means next day)
    fn is_overnight(wake_time: NaiveTime, sleep_time: NaiveTime) -> bool {
        sleep_time < wake_time
    }
    
    /// Calculate minutes between two times, handling overnight case
    fn minutes_between(start: NaiveTime, end: NaiveTime, overnight: bool) -> i64 {
        let diff = (end - start).num_minutes();
        if overnight && diff < 0 {
            // Add 24 hours worth of minutes
            diff + 24 * 60
        } else if !overnight && diff < 0 {
            0
        } else {
            diff
        }
    }
    
    /// Check if time a is before time b, considering overnight schedule
    fn time_before(a: NaiveTime, b: NaiveTime, wake_time: NaiveTime, overnight: bool) -> bool {
        if !overnight {
            return a < b;
        }
        // For overnight: times after wake are "earlier" in the day than times before wake
        let a_after_wake = a >= wake_time;
        let b_after_wake = b >= wake_time;
        
        match (a_after_wake, b_after_wake) {
            (true, true) => a < b,   // Both in evening, normal compare
            (false, false) => a < b, // Both after midnight, normal compare
            (true, false) => true,   // a is evening, b is after midnight, a is earlier
            (false, true) => false,  // a is after midnight, b is evening, b is earlier
        }
    }

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
            // Default: 1.5 hours before sleep (handle overnight)
            let overnight = Self::is_overnight(wake_time, sleep_time);
            let shower_start = if overnight && sleep_time < NaiveTime::from_hms_opt(1, 30, 0).unwrap() {
                // Sleep is very early morning, shower should be late night
                NaiveTime::from_hms_opt(23, 0, 0).unwrap()
            } else {
                // Normal case or late night sleep
                let mins = sleep_time.signed_duration_since(NaiveTime::from_hms_opt(0, 0, 0).unwrap()).num_minutes();
                let shower_mins = if mins >= 90 { mins - 90 } else { mins + 24 * 60 - 90 };
                NaiveTime::from_hms_opt((shower_mins / 60) as u32 % 24, (shower_mins % 60) as u32, 0).unwrap()
            };
            fixed_activities.push(FixedActivity {
                name: "洗澡".to_string(),
                start: shower_start,
                duration_minutes: 30,
            });
        }
        
        // Sleep preparation (30 min before sleep, handle overnight)
        let sleep_mins = sleep_time.signed_duration_since(NaiveTime::from_hms_opt(0, 0, 0).unwrap()).num_minutes();
        let prep_mins = if sleep_mins >= 30 { sleep_mins - 30 } else { sleep_mins + 24 * 60 - 30 };
        let sleep_prep_start = NaiveTime::from_hms_opt((prep_mins / 60) as u32 % 24, (prep_mins % 60) as u32, 0).unwrap();
        fixed_activities.push(FixedActivity {
            name: "睡前准备".to_string(),
            start: sleep_prep_start,
            duration_minutes: 30,
        });
        
        // Sort by time considering overnight schedule
        let overnight = Self::is_overnight(wake_time, sleep_time);
        fixed_activities.sort_by(|a, b| {
            let a_order = Self::time_order(a.start, wake_time, overnight);
            let b_order = Self::time_order(b.start, wake_time, overnight);
            a_order.cmp(&b_order)
        });
        
        // Filter out activities outside wake-sleep range
        fixed_activities.retain(|activity| {
            Self::time_in_range(activity.start, wake_time, sleep_time, overnight)
        });
        
        // Calculate available slots
        let available_slots = Self::calculate_available_slots(
            wake_time,
            sleep_time,
            &fixed_activities,
            overnight,
        );
        
        let total_available_minutes: u32 = available_slots
            .iter()
            .filter(|s| s.slot_type == SlotType::Available)
            .map(|s| Self::slot_duration_minutes(s, overnight))
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
    
    /// Get sort order for time considering overnight schedule
    fn time_order(time: NaiveTime, wake_time: NaiveTime, overnight: bool) -> u32 {
        if !overnight {
            return time.signed_duration_since(NaiveTime::from_hms_opt(0, 0, 0).unwrap()).num_minutes() as u32;
        }
        // For overnight: times >= wake_time come first, then times < wake_time
        let mins = time.signed_duration_since(NaiveTime::from_hms_opt(0, 0, 0).unwrap()).num_minutes() as u32;
        let wake_mins = wake_time.signed_duration_since(NaiveTime::from_hms_opt(0, 0, 0).unwrap()).num_minutes() as u32;
        if mins >= wake_mins {
            mins - wake_mins
        } else {
            mins + (24 * 60 - wake_mins)
        }
    }
    
    /// Check if time is within wake-sleep range
    fn time_in_range(time: NaiveTime, wake_time: NaiveTime, sleep_time: NaiveTime, overnight: bool) -> bool {
        if !overnight {
            return time >= wake_time && time < sleep_time;
        }
        // For overnight: valid if >= wake OR < sleep
        time >= wake_time || time < sleep_time
    }
    
    /// Calculate available time slots between fixed activities
    fn calculate_available_slots(
        wake_time: NaiveTime,
        sleep_time: NaiveTime,
        fixed_activities: &[FixedActivity],
        overnight: bool,
    ) -> Vec<TimeSlot> {
        let mut slots = Vec::new();
        let mut current_time = wake_time;
        
        for activity in fixed_activities {
            // Available slot before this activity
            let gap_minutes = Self::minutes_between(current_time, activity.start, overnight);
            if gap_minutes > 10 {
                // Add buffer before fixed activity
                let buffer_mins = activity.start.signed_duration_since(NaiveTime::from_hms_opt(0, 0, 0).unwrap()).num_minutes();
                let buffer_start_mins = if buffer_mins >= 5 { buffer_mins - 5 } else { buffer_mins + 24 * 60 - 5 };
                let buffer_start = NaiveTime::from_hms_opt(
                    (buffer_start_mins / 60) as u32 % 24,
                    (buffer_start_mins % 60) as u32,
                    0
                ).unwrap();
                
                if Self::minutes_between(current_time, buffer_start, overnight) > 0 {
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
            
            // Fixed activity slot
            let end_mins = activity.start.signed_duration_since(NaiveTime::from_hms_opt(0, 0, 0).unwrap()).num_minutes()
                + activity.duration_minutes as i64;
            let activity_end = NaiveTime::from_hms_opt(
                (end_mins / 60) as u32 % 24,
                (end_mins % 60) as u32,
                0
            ).unwrap();
            
            slots.push(TimeSlot {
                start: activity.start,
                end: activity_end,
                slot_type: SlotType::Fixed,
            });
            
            current_time = activity_end;
        }
        
        // Remaining time until sleep
        let remaining = Self::minutes_between(current_time, sleep_time, overnight);
        if remaining > 0 {
            slots.push(TimeSlot {
                start: current_time,
                end: sleep_time,
                slot_type: SlotType::Available,
            });
        }
        
        slots
    }
    
    fn slot_duration_minutes(slot: &TimeSlot, overnight: bool) -> u32 {
        let diff = Self::minutes_between(slot.start, slot.end, overnight);
        diff.max(0) as u32
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
