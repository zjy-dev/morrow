use crate::config::AppConfig;
use crate::error::Result;
use crate::google::Task;
use crate::planner::preprocessor::{DayConstraints, Preprocessor, PreprocessedTask};
use crate::planner::estimator::Estimator;
use crate::planner::engine::{SchedulerEngine, ScheduledItem};
use crate::planner::validator::{Validator, ValidationResult};
use crate::planner::polisher::{Polisher, PolishedItem};
use chrono::{Duration, Utc};
use chrono_tz::Tz;

/// Pipeline execution result with detailed info
pub struct PipelineResult {
    pub schedule: Vec<PolishedItem>,
    pub validation: ValidationResult,
    pub stats: PipelineStats,
}

/// Statistics about the pipeline execution
pub struct PipelineStats {
    pub total_tasks: usize,
    pub scheduled_tasks: usize,
    pub total_scheduled_minutes: u32,
    pub available_minutes: u32,
    pub pomodoro_sessions: usize,
}

/// Main pipeline orchestrator
pub struct Pipeline {
    config: AppConfig,
}

impl Pipeline {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    /// Execute the full planning pipeline
    pub async fn execute(&self, tasks: &[Task]) -> Result<PipelineResult> {
        println!("  [1/5] Preprocessing tasks and extracting constraints...");
        
        // Step 1: Preprocess
        let constraints = Preprocessor::extract_constraints(&self.config.preferences);
        let preprocessed_tasks = Preprocessor::preprocess_tasks(tasks);
        
        println!("        - Wake: {}, Sleep: {}", 
            constraints.wake_time.format("%H:%M"),
            constraints.sleep_time.format("%H:%M")
        );
        println!("        - Available time: {} minutes", constraints.total_available_minutes);
        println!("        - Fixed activities: {}", constraints.fixed_activities.len());
        
        // Step 2: Estimate task durations using LLM
        println!("  [2/5] Estimating task durations with LLM...");
        let estimator = Estimator::new(self.config.llm.clone())?;
        let estimates = estimator.estimate_tasks(&preprocessed_tasks, &self.config.preferences).await?;
        
        let total_estimated: u32 = estimates.iter().map(|e| e.estimated_duration).sum();
        println!("        - Total estimated time: {} minutes", total_estimated);
        
        // Step 3: Schedule using deterministic algorithm
        println!("  [3/5] Scheduling tasks using constraint solver...");
        let mut schedule = SchedulerEngine::generate_schedule(
            &constraints,
            &preprocessed_tasks,
            &estimates,
        );
        
        println!("        - Generated {} schedule items", schedule.len());
        
        // Step 4: Validate and auto-fix
        println!("  [4/5] Validating schedule...");
        let mut validation = Validator::validate(&schedule, &constraints, &preprocessed_tasks);
        
        if !validation.is_valid {
            println!("        - Found {} errors, attempting auto-fix...", validation.errors.len());
            let fixes = Validator::auto_fix(&mut schedule, &constraints);
            for fix in &fixes {
                println!("        - {}", fix);
            }
            // Re-validate after fixes
            validation = Validator::validate(&schedule, &constraints, &preprocessed_tasks);
        }
        
        if !validation.warnings.is_empty() {
            println!("        - {} warnings:", validation.warnings.len());
            for warning in &validation.warnings {
                println!("          - {}", warning.message);
            }
        }
        
        // Step 5: Polish with LLM
        println!("  [5/5] Polishing schedule with LLM...");
        let (date, day_of_week) = self.get_tomorrow_info()?;
        
        let polished = match Polisher::new(self.config.llm.clone()) {
            Ok(polisher) => {
                match polisher.polish_schedule(&schedule, &self.config.preferences, &date, &day_of_week).await {
                    Ok(polished) => polished,
                    Err(e) => {
                        println!("        - Polish failed, using original: {}", e);
                        Polisher::fallback_polish(&schedule)
                    }
                }
            }
            Err(e) => {
                println!("        - Polish skipped: {}", e);
                Polisher::fallback_polish(&schedule)
            }
        };
        
        // Calculate stats
        let stats = self.calculate_stats(&schedule, &preprocessed_tasks, &constraints);
        
        Ok(PipelineResult {
            schedule: polished,
            validation,
            stats,
        })
    }
    
    fn get_tomorrow_info(&self) -> Result<(String, String)> {
        let tz: Tz = self.config.timezone.parse().map_err(|_| {
            crate::error::MorrowError::Config(format!(
                "Invalid timezone: '{}'",
                self.config.timezone
            ))
        })?;
        let tomorrow = (Utc::now().with_timezone(&tz) + Duration::days(1)).date_naive();
        Ok((
            tomorrow.format("%Y-%m-%d").to_string(),
            tomorrow.format("%A").to_string(),
        ))
    }
    
    fn calculate_stats(
        &self,
        schedule: &[ScheduledItem],
        tasks: &[PreprocessedTask],
        constraints: &DayConstraints,
    ) -> PipelineStats {
        let scheduled_task_ids: std::collections::HashSet<_> = schedule
            .iter()
            .filter_map(|item| item.task_id)
            .collect();
        
        let total_scheduled_minutes: u32 = schedule.iter().map(|item| item.duration).sum();
        
        let pomodoro_sessions = schedule
            .iter()
            .filter(|item| matches!(item.item_type, crate::planner::engine::ItemType::PomodoroWork))
            .count();
        
        PipelineStats {
            total_tasks: tasks.len(),
            scheduled_tasks: scheduled_task_ids.len(),
            total_scheduled_minutes,
            available_minutes: constraints.total_available_minutes,
            pomodoro_sessions,
        }
    }

    /// Get tomorrow's date string
    pub fn get_tomorrow_date(&self) -> Result<String> {
        let (date, _) = self.get_tomorrow_info()?;
        Ok(date)
    }
}
