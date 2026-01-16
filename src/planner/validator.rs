use crate::planner::engine::{ScheduledItem, ItemType};
use crate::planner::preprocessor::{DayConstraints, PreprocessedTask};
use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

/// Validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub warnings: Vec<ValidationWarning>,
    pub errors: Vec<ValidationError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub code: WarningCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: ErrorCode,
    pub message: String,
    pub affected_items: Vec<usize>,  // indices in schedule
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum WarningCode {
    TaskNotScheduled,      // A task wasn't scheduled
    ShortBreak,            // Break shorter than recommended
    LongWorkBlock,         // Work block longer than 2 hours without break
    LateNightTask,         // Task scheduled close to sleep time
    EarlyMorningTask,      // Task scheduled right after wake up
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ErrorCode {
    TimeOverlap,           // Two items overlap in time
    ExceedsDayBounds,      // Item extends past sleep time
    InvalidTimeFormat,     // Time format is invalid
    NegativeDuration,      // Duration is 0 or negative
}

pub struct Validator;

impl Validator {
    /// Validate the generated schedule
    pub fn validate(
        schedule: &[ScheduledItem],
        constraints: &DayConstraints,
        tasks: &[PreprocessedTask],
    ) -> ValidationResult {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        
        // 1. Check for time overlaps
        Self::check_overlaps(schedule, &mut errors);
        
        // 2. Check day bounds
        Self::check_day_bounds(schedule, constraints, &mut errors);
        
        // 3. Check all tasks are scheduled
        Self::check_task_coverage(schedule, tasks, &mut warnings);
        
        // 4. Check for long work blocks without breaks
        Self::check_work_breaks(schedule, &mut warnings);
        
        // 5. Check for late night tasks
        Self::check_late_tasks(schedule, constraints, &mut warnings);
        
        // 6. Validate time formats
        Self::check_time_formats(schedule, &mut errors);
        
        ValidationResult {
            is_valid: errors.is_empty(),
            warnings,
            errors,
        }
    }
    
    fn check_overlaps(schedule: &[ScheduledItem], errors: &mut Vec<ValidationError>) {
        for i in 0..schedule.len() {
            for j in (i + 1)..schedule.len() {
                if Self::items_overlap(&schedule[i], &schedule[j]) {
                    errors.push(ValidationError {
                        code: ErrorCode::TimeOverlap,
                        message: format!(
                            "Time overlap between '{}' at {} and '{}' at {}",
                            schedule[i].title, schedule[i].time,
                            schedule[j].title, schedule[j].time
                        ),
                        affected_items: vec![i, j],
                    });
                }
            }
        }
    }
    
    fn items_overlap(a: &ScheduledItem, b: &ScheduledItem) -> bool {
        let a_start = Self::parse_time(&a.time);
        let b_start = Self::parse_time(&b.time);
        
        if a_start.is_none() || b_start.is_none() {
            return false;
        }
        
        let a_start = a_start.unwrap();
        let b_start = b_start.unwrap();
        let a_end = a_start + chrono::Duration::minutes(a.duration as i64);
        let b_end = b_start + chrono::Duration::minutes(b.duration as i64);
        
        // Check overlap: a starts before b ends AND a ends after b starts
        a_start < b_end && a_end > b_start
    }
    
    fn check_day_bounds(
        schedule: &[ScheduledItem],
        constraints: &DayConstraints,
        errors: &mut Vec<ValidationError>,
    ) {
        for (i, item) in schedule.iter().enumerate() {
            if let Some(start) = Self::parse_time(&item.time) {
                let end = start + chrono::Duration::minutes(item.duration as i64);
                
                if end > constraints.sleep_time {
                    errors.push(ValidationError {
                        code: ErrorCode::ExceedsDayBounds,
                        message: format!(
                            "'{}' ends at {} which is past sleep time {}",
                            item.title,
                            end.format("%H:%M"),
                            constraints.sleep_time.format("%H:%M")
                        ),
                        affected_items: vec![i],
                    });
                }
                
                if start < constraints.wake_time {
                    errors.push(ValidationError {
                        code: ErrorCode::ExceedsDayBounds,
                        message: format!(
                            "'{}' starts at {} which is before wake time {}",
                            item.title,
                            start.format("%H:%M"),
                            constraints.wake_time.format("%H:%M")
                        ),
                        affected_items: vec![i],
                    });
                }
            }
        }
    }
    
    fn check_task_coverage(
        schedule: &[ScheduledItem],
        tasks: &[PreprocessedTask],
        warnings: &mut Vec<ValidationWarning>,
    ) {
        for task in tasks {
            let is_scheduled = schedule.iter().any(|item| {
                item.task_id == Some(task.id)
            });
            
            if !is_scheduled {
                warnings.push(ValidationWarning {
                    code: WarningCode::TaskNotScheduled,
                    message: format!("Task '{}' was not scheduled", task.title),
                });
            }
        }
    }
    
    fn check_work_breaks(schedule: &[ScheduledItem], warnings: &mut Vec<ValidationWarning>) {
        let mut consecutive_work_minutes = 0u32;
        let mut last_end: Option<NaiveTime> = None;
        
        for item in schedule {
            let is_work = matches!(item.item_type, ItemType::Task | ItemType::PomodoroWork);
            let is_break = matches!(
                item.item_type,
                ItemType::PomodoroBreak | ItemType::PomodoroLong | ItemType::Fixed
            );
            
            if let Some(start) = Self::parse_time(&item.time) {
                // Check if this is continuous from last item
                let is_continuous = last_end
                    .map(|end| (start - end).num_minutes() < 5)
                    .unwrap_or(false);
                
                if is_work {
                    if is_continuous {
                        consecutive_work_minutes += item.duration;
                    } else {
                        consecutive_work_minutes = item.duration;
                    }
                    
                    if consecutive_work_minutes > 120 {
                        warnings.push(ValidationWarning {
                            code: WarningCode::LongWorkBlock,
                            message: format!(
                                "Work block exceeds 2 hours without break ending at '{}'",
                                item.title
                            ),
                        });
                        consecutive_work_minutes = 0;
                    }
                } else if is_break {
                    consecutive_work_minutes = 0;
                }
                
                last_end = Some(start + chrono::Duration::minutes(item.duration as i64));
            }
        }
    }
    
    fn check_late_tasks(
        schedule: &[ScheduledItem],
        constraints: &DayConstraints,
        warnings: &mut Vec<ValidationWarning>,
    ) {
        let late_threshold = constraints.sleep_time - chrono::Duration::hours(1);
        
        for item in schedule {
            if matches!(item.item_type, ItemType::Task | ItemType::PomodoroWork) {
                if let Some(start) = Self::parse_time(&item.time) {
                    if start >= late_threshold {
                        warnings.push(ValidationWarning {
                            code: WarningCode::LateNightTask,
                            message: format!(
                                "'{}' is scheduled close to sleep time",
                                item.title
                            ),
                        });
                    }
                }
            }
        }
    }
    
    fn check_time_formats(schedule: &[ScheduledItem], errors: &mut Vec<ValidationError>) {
        for (i, item) in schedule.iter().enumerate() {
            if Self::parse_time(&item.time).is_none() {
                errors.push(ValidationError {
                    code: ErrorCode::InvalidTimeFormat,
                    message: format!("Invalid time format '{}' for '{}'", item.time, item.title),
                    affected_items: vec![i],
                });
            }
            
            if item.duration == 0 {
                errors.push(ValidationError {
                    code: ErrorCode::NegativeDuration,
                    message: format!("Zero duration for '{}'", item.title),
                    affected_items: vec![i],
                });
            }
        }
    }
    
    fn parse_time(time_str: &str) -> Option<NaiveTime> {
        NaiveTime::parse_from_str(time_str, "%H:%M").ok()
    }
    
    /// Attempt to fix common validation errors
    pub fn auto_fix(
        schedule: &mut Vec<ScheduledItem>,
        constraints: &DayConstraints,
    ) -> Vec<String> {
        let mut fixes = Vec::new();
        
        // Sort by time first
        schedule.sort_by(|a, b| a.time.cmp(&b.time));
        
        // Fix overlaps by shifting items
        let mut i = 0;
        while i < schedule.len().saturating_sub(1) {
            if Self::items_overlap(&schedule[i], &schedule[i + 1]) {
                let prev_end = Self::parse_time(&schedule[i].time)
                    .map(|t| t + chrono::Duration::minutes(schedule[i].duration as i64));
                
                if let Some(new_start) = prev_end {
                    if new_start < constraints.sleep_time {
                        fixes.push(format!(
                            "Shifted '{}' from {} to {}",
                            schedule[i + 1].title,
                            schedule[i + 1].time,
                            new_start.format("%H:%M")
                        ));
                        schedule[i + 1].time = new_start.format("%H:%M").to_string();
                    } else {
                        // Can't fit, remove the item
                        fixes.push(format!(
                            "Removed '{}' - doesn't fit in schedule",
                            schedule[i + 1].title
                        ));
                        schedule.remove(i + 1);
                        continue;
                    }
                }
            }
            i += 1;
        }
        
        // Remove items that exceed day bounds
        schedule.retain(|item| {
            if let Some(start) = Self::parse_time(&item.time) {
                let end = start + chrono::Duration::minutes(item.duration as i64);
                if end > constraints.sleep_time {
                    fixes.push(format!("Removed '{}' - exceeds sleep time", item.title));
                    return false;
                }
            }
            true
        });
        
        fixes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_items_overlap() {
        let a = ScheduledItem {
            time: "09:00".to_string(),
            duration: 60,
            title: "Task A".to_string(),
            item_type: ItemType::Task,
            task_id: Some(0),
        };
        let b = ScheduledItem {
            time: "09:30".to_string(),
            duration: 30,
            title: "Task B".to_string(),
            item_type: ItemType::Task,
            task_id: Some(1),
        };
        let c = ScheduledItem {
            time: "10:00".to_string(),
            duration: 30,
            title: "Task C".to_string(),
            item_type: ItemType::Task,
            task_id: Some(2),
        };
        
        assert!(Validator::items_overlap(&a, &b));
        assert!(!Validator::items_overlap(&a, &c));
    }
}
