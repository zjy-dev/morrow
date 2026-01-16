use crate::planner::preprocessor::{DayConstraints, PreprocessedTask, Priority, SlotType, TimeSlot, TimePeriod};
use crate::planner::estimator::TaskEstimate;
use chrono::{NaiveTime, Duration};
use serde::{Deserialize, Serialize};

/// A scheduled item in the final schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledItem {
    pub time: String,        // HH:MM format
    pub duration: u32,       // minutes
    pub title: String,
    pub item_type: ItemType,
    pub task_id: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ItemType {
    Task,           // User task
    Fixed,          // Fixed activity (meal, shower)
    PomodoroWork,   // Pomodoro work session
    PomodoroBreak,  // Short break (5 min)
    PomodoroLong,   // Long break (35 min)
    Buffer,         // Buffer/transition time
}

/// Task to be scheduled with all necessary info
#[derive(Debug, Clone)]
struct SchedulableTask {
    id: usize,
    title: String,
    duration: u32,
    priority: Priority,
    preferred_period: Option<TimePeriod>,
    requires_focus: bool,
    can_split: bool,
    remaining_duration: u32,
}

pub struct SchedulerEngine;

impl SchedulerEngine {
    /// Generate schedule using deterministic algorithm
    pub fn generate_schedule(
        constraints: &DayConstraints,
        tasks: &[PreprocessedTask],
        estimates: &[TaskEstimate],
    ) -> Vec<ScheduledItem> {
        let mut schedule = Vec::new();
        
        // 1. Add fixed activities first
        for activity in &constraints.fixed_activities {
            schedule.push(ScheduledItem {
                time: activity.start.format("%H:%M").to_string(),
                duration: activity.duration_minutes,
                title: activity.name.clone(),
                item_type: ItemType::Fixed,
                task_id: None,
            });
        }
        
        // 2. Prepare schedulable tasks
        let mut schedulable: Vec<SchedulableTask> = tasks
            .iter()
            .filter_map(|task| {
                let estimate = estimates.iter().find(|e| e.task_id == task.id)?;
                Some(SchedulableTask {
                    id: task.id,
                    title: task.title.clone(),
                    duration: estimate.estimated_duration,
                    priority: estimate.priority,
                    preferred_period: estimate.preferred_period,
                    requires_focus: estimate.requires_focus,
                    can_split: estimate.can_split,
                    remaining_duration: estimate.estimated_duration,
                })
            })
            .collect();
        
        // Sort by priority and preferred period
        schedulable.sort_by(|a, b| {
            match (&a.priority, &b.priority) {
                (Priority::High, Priority::High) => std::cmp::Ordering::Equal,
                (Priority::High, _) => std::cmp::Ordering::Less,
                (_, Priority::High) => std::cmp::Ordering::Greater,
                (Priority::Normal, Priority::Normal) => std::cmp::Ordering::Equal,
                (Priority::Normal, Priority::Low) => std::cmp::Ordering::Less,
                (Priority::Low, Priority::Normal) => std::cmp::Ordering::Greater,
                (Priority::Low, Priority::Low) => std::cmp::Ordering::Equal,
            }
        });
        
        // 3. Get available slots
        let available_slots: Vec<&TimeSlot> = constraints
            .available_slots
            .iter()
            .filter(|s| s.slot_type == SlotType::Available)
            .collect();
        
        // 4. Assign tasks to slots
        let mut slot_usage: Vec<SlotUsage> = available_slots
            .iter()
            .map(|s| SlotUsage {
                slot: (*s).clone(),
                used_minutes: 0,
                items: Vec::new(),
            })
            .collect();
        
        for task in &mut schedulable {
            Self::assign_task_to_slots(task, &mut slot_usage);
        }
        
        // 5. Build final schedule from slot usage
        for usage in &slot_usage {
            for item in &usage.items {
                schedule.push(item.clone());
            }
        }
        
        // 6. Sort by time
        schedule.sort_by(|a, b| a.time.cmp(&b.time));
        
        schedule
    }
    
    fn assign_task_to_slots(task: &mut SchedulableTask, slots: &mut [SlotUsage]) {
        // Find best slot based on preferred period
        let preferred_slots: Vec<usize> = slots
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                if let Some(period) = task.preferred_period {
                    Self::slot_matches_period(&s.slot, period)
                } else {
                    true
                }
            })
            .map(|(i, _)| i)
            .collect();
        
        let slot_order: Vec<usize> = if preferred_slots.is_empty() {
            (0..slots.len()).collect()
        } else {
            let mut order = preferred_slots.clone();
            for i in 0..slots.len() {
                if !order.contains(&i) {
                    order.push(i);
                }
            }
            order
        };
        
        for slot_idx in slot_order {
            if task.remaining_duration == 0 {
                break;
            }
            
            let slot = &mut slots[slot_idx];
            let available = Self::slot_available_minutes(&slot.slot) - slot.used_minutes;
            
            if available < 15 {
                continue;
            }
            
            let allocate = if task.can_split {
                task.remaining_duration.min(available)
            } else if available >= task.remaining_duration {
                task.remaining_duration
            } else {
                continue;
            };
            
            // Apply pomodoro if requires focus and long enough
            if task.requires_focus && allocate >= 25 {
                Self::add_pomodoro_session(slot, task, allocate);
            } else {
                Self::add_simple_task(slot, task, allocate);
            }
            
            task.remaining_duration -= allocate;
        }
    }
    
    fn add_simple_task(slot: &mut SlotUsage, task: &SchedulableTask, duration: u32) {
        let start_time = slot.slot.start + Duration::minutes(slot.used_minutes as i64);
        slot.items.push(ScheduledItem {
            time: start_time.format("%H:%M").to_string(),
            duration,
            title: task.title.clone(),
            item_type: ItemType::Task,
            task_id: Some(task.id),
        });
        slot.used_minutes += duration;
    }
    
    fn add_pomodoro_session(slot: &mut SlotUsage, task: &SchedulableTask, max_duration: u32) {
        let mut remaining = max_duration;
        let mut pomodoro_count = 0;
        
        while remaining >= 25 {
            let start_time = slot.slot.start + Duration::minutes(slot.used_minutes as i64);
            
            // Add work session
            slot.items.push(ScheduledItem {
                time: start_time.format("%H:%M").to_string(),
                duration: 25,
                title: format!("{} (专注 #{})", task.title, pomodoro_count + 1),
                item_type: ItemType::PomodoroWork,
                task_id: Some(task.id),
            });
            slot.used_minutes += 25;
            remaining -= 25;
            pomodoro_count += 1;
            
            // Add break if there's time
            if remaining >= 5 {
                let break_start = slot.slot.start + Duration::minutes(slot.used_minutes as i64);
                
                if pomodoro_count == 4 && remaining >= 35 {
                    // Long break after 4 pomodoros
                    slot.items.push(ScheduledItem {
                        time: break_start.format("%H:%M").to_string(),
                        duration: 35,
                        title: "长休息".to_string(),
                        item_type: ItemType::PomodoroLong,
                        task_id: None,
                    });
                    slot.used_minutes += 35;
                    remaining -= 35;
                    pomodoro_count = 0;
                } else if remaining >= 5 && remaining < 25 + 5 {
                    // Short break but not enough for another pomodoro
                    slot.items.push(ScheduledItem {
                        time: break_start.format("%H:%M").to_string(),
                        duration: 5,
                        title: "短休息".to_string(),
                        item_type: ItemType::PomodoroBreak,
                        task_id: None,
                    });
                    slot.used_minutes += 5;
                    remaining -= 5;
                    break;
                } else if remaining >= 30 {
                    // Short break with more pomodoros to come
                    slot.items.push(ScheduledItem {
                        time: break_start.format("%H:%M").to_string(),
                        duration: 5,
                        title: "短休息".to_string(),
                        item_type: ItemType::PomodoroBreak,
                        task_id: None,
                    });
                    slot.used_minutes += 5;
                    remaining -= 5;
                }
            }
        }
    }
    
    fn slot_matches_period(slot: &TimeSlot, period: TimePeriod) -> bool {
        let (period_start, period_end) = match period {
            TimePeriod::Morning => (
                NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            ),
            TimePeriod::Afternoon => (
                NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
            ),
            TimePeriod::Evening => (
                NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(23, 0, 0).unwrap(),
            ),
        };
        
        // Slot overlaps with period
        slot.start < period_end && slot.end > period_start
    }
    
    fn slot_available_minutes(slot: &TimeSlot) -> u32 {
        (slot.end - slot.start).num_minutes() as u32
    }
}

#[derive(Debug)]
struct SlotUsage {
    slot: TimeSlot,
    used_minutes: u32,
    items: Vec<ScheduledItem>,
}
