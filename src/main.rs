mod config;
mod error;
mod google;
mod planner;

use clap::{Parser, Subcommand};
use config::AppConfig;
use dialoguer::{Confirm, Input};
use error::{MorrowError, Result};
use google::{GoogleAuth, GoogleTasksClient, TaskInput};
use planner::{build_planning_input, build_system_prompt, build_user_prompt, Scheduler};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "morrow")]
#[command(about = "LLM-powered daily schedule planner with Google Tasks integration")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to config file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Authenticate with Google account
    Auth,
    /// Plan tomorrow's schedule
    Plan,
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,
    /// Initialize configuration file with defaults
    Init,
    /// Show config file path
    Path,
}

#[tokio::main]
async fn main() {
    if let Err(e) = dotenvy::dotenv() {
        if !matches!(e, dotenvy::Error::Io(_)) {
            eprintln!("Warning: Failed to load .env file: {}", e);
        }
    }

    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Auth => cmd_auth().await,
        Commands::Plan => cmd_plan(cli.config).await,
        Commands::Config { action } => cmd_config(action, cli.config),
    }
}

async fn cmd_auth() -> Result<()> {
    println!("Starting Google authentication...\n");
    
    let auth = GoogleAuth::new()?;
    let creds = auth.authenticate().await?;
    creds.save()?;
    
    println!("\nAuthentication successful! Credentials saved.");
    println!("You can now use 'morrow plan' to create your schedule.");
    
    Ok(())
}

async fn cmd_plan(config_path: Option<PathBuf>) -> Result<()> {
    let config = AppConfig::load(config_path)?;

    println!("Morrow - Tomorrow's Schedule Planner");
    println!("====================================\n");
    println!("Timezone: {}", config.timezone);
    println!("Source list: '{}'", config.google.source_list);
    println!("Output list: '{}'\n", config.google.output_list);
    println!("NOTE: All tasks in your source list will be scheduled for tomorrow.");
    println!("      Add time preferences in task notes (e.g., 'morning', '2 hours').\n");

    // Get valid Google credentials
    let auth = GoogleAuth::new()?;
    let creds = auth.get_valid_credentials().await?;
    let tasks_client = GoogleTasksClient::new(creds.access_token);
    
    // Find source list and get all pending tasks
    println!("Fetching tasks from '{}'...", config.google.source_list);
    let source_list = tasks_client.find_list_by_name(&config.google.source_list).await?;
    let tasks = tasks_client.get_pending_tasks(&source_list.id).await?;
    
    if tasks.is_empty() {
        println!("No tasks found in source list. Nothing to plan.");
        return Ok(());
    }
    
    println!("Found {} tasks to schedule for tomorrow.", tasks.len());
    
    // Check output list
    let output_list = tasks_client.ensure_list_exists(&config.google.output_list).await?;
    if tasks_client.has_incomplete_tasks(&output_list.id).await? {
        return Err(MorrowError::OutputListNotEmpty);
    }
    
    // Generate schedule using LLM
    println!("Generating schedule with LLM...");
    let scheduler = Scheduler::new(config.llm.clone())?;
    
    let input = build_planning_input(&config.preferences, &tasks, &config.timezone)?;
    let system_prompt = build_system_prompt();
    let user_prompt = build_user_prompt(&input);
    
    let schedule = scheduler.generate_schedule(&system_prompt, &user_prompt).await?;
    
    // Write schedule to output list
    println!("Writing schedule to '{}'...", config.google.output_list);
    let tomorrow = input.date.clone();
    
    for item in schedule.iter().rev() {
        let task = TaskInput {
            title: format!("🕒 [{}] {}", item.time, item.title),
            notes: Some(format!("Duration: {} minutes", item.duration)),
            due: Some(format!("{}T00:00:00.000Z", tomorrow)),
        };
        tasks_client.create_task(&output_list.id, task).await?;
    }
    
    println!("\nSchedule created successfully!");
    println!("\n--- Tomorrow's Schedule ({}) ---\n", tomorrow);
    for item in &schedule {
        println!("  {} - {} ({} min)", item.time, item.title, item.duration);
    }
    
    Ok(())
}

fn cmd_config(action: ConfigAction, config_path: Option<PathBuf>) -> Result<()> {
    match action {
        ConfigAction::Show => {
            let config = AppConfig::load(config_path)?;
            let yaml = serde_yaml::to_string(&config)?;
            println!("{}", yaml);
        }
        ConfigAction::Init => {
            let path = config_path.unwrap_or_else(AppConfig::default_config_path);
            
            // Load existing config or use defaults
            let existing_config = if path.exists() {
                Some(AppConfig::load(Some(path.clone()))?)
            } else {
                None
            };
            
            let defaults = existing_config.clone().unwrap_or_else(|| AppConfig {
                preferences: config::UserPreferences::with_defaults(),
                ..Default::default()
            });
            
            println!("Morrow Configuration Setup");
            println!("==========================\n");
            println!("Config file path: {}\n", path.display());
            
            if existing_config.is_some() {
                println!("An existing configuration was found. Values shown are from your current config.");
            } else {
                println!("No existing configuration found. Using default values.");
            }
            
            println!("\nYou can edit this file directly at any time.");
            
            let skip = Confirm::new()
                .with_prompt("Skip interactive setup and use current/default values?")
                .default(false)
                .interact()
                .unwrap_or(true);
            
            if skip {
                if existing_config.is_none() {
                    defaults.save(Some(path.clone()))?;
                    println!("\nConfig file created at: {}", path.display());
                } else {
                    println!("\nKeeping existing configuration.");
                }
                return Ok(());
            }
            
            println!("\n--- Google Tasks Settings ---\n");
            
            let source_list: String = Input::new()
                .with_prompt("Source task list name (your tasks to schedule)")
                .default(defaults.google.source_list.clone())
                .interact_text()
                .unwrap_or(defaults.google.source_list.clone());
            
            let output_list: String = Input::new()
                .with_prompt("Output task list name (where schedule is written)")
                .default(defaults.google.output_list.clone())
                .interact_text()
                .unwrap_or(defaults.google.output_list.clone());
            
            println!("\n--- Timezone Settings ---\n");
            
            let timezone: String = Input::new()
                .with_prompt("Timezone (e.g., Asia/Shanghai, America/New_York)")
                .default(defaults.timezone.clone())
                .interact_text()
                .unwrap_or(defaults.timezone.clone());
            
            println!("\n--- LLM Settings ---\n");
            
            let api_format: String = Input::new()
                .with_prompt("API format (openai/anthropic/gemini)")
                .default(format!("{:?}", defaults.llm.api_format).to_lowercase())
                .interact_text()
                .unwrap_or_else(|_| "openai".to_string());
            
            let base_url: String = Input::new()
                .with_prompt("API base URL")
                .default(defaults.llm.base_url.clone())
                .interact_text()
                .unwrap_or(defaults.llm.base_url.clone());
            
            let model: String = Input::new()
                .with_prompt("Model name")
                .default(defaults.llm.model.clone())
                .interact_text()
                .unwrap_or(defaults.llm.model.clone());
            
            println!("\n--- User Preferences ---\n");
            println!("Enter your daily preferences in natural language.");
            println!("Press Enter to keep the default/current value.\n");
            
            let mut prefs = defaults.preferences.clone();
            
            println!("Bio (optional): Describe your lifestyle, health conditions, work nature, etc.");
            println!("This helps the AI create a schedule tailored to you.");
            println!("Enter multiple lines, press Enter on empty line to finish.\n");
            
            let mut bio_lines: Vec<String> = Vec::new();
            if let Some(existing_bio) = &prefs.bio {
                println!("Current bio: {}", existing_bio.lines().next().unwrap_or(""));
                println!("(Enter new bio to replace, or press Enter to keep current)\n");
            }
            loop {
                let line: String = Input::new()
                    .with_prompt("bio")
                    .allow_empty(true)
                    .interact_text()
                    .unwrap_or_default();
                if line.is_empty() {
                    break;
                }
                bio_lines.push(line);
            }
            if !bio_lines.is_empty() {
                prefs.bio = Some(bio_lines.join("\n"));
            }
            
            let wake_up: String = Input::new()
                .with_prompt("Wake up time")
                .default(prefs.prefs.get("wake_up").cloned().unwrap_or_else(|| "7:30左右".to_string()))
                .interact_text()
                .unwrap_or_else(|_| "7:30左右".to_string());
            prefs.prefs.insert("wake_up".to_string(), wake_up);
            
            let sleep: String = Input::new()
                .with_prompt("Sleep time")
                .default(prefs.prefs.get("sleep").cloned().unwrap_or_else(|| "尽量11点前睡觉".to_string()))
                .interact_text()
                .unwrap_or_else(|_| "尽量11点前睡觉".to_string());
            prefs.prefs.insert("sleep".to_string(), sleep);
            
            let breakfast: String = Input::new()
                .with_prompt("Breakfast time")
                .default(prefs.prefs.get("breakfast").cloned().unwrap_or_else(|| "起床后半小时".to_string()))
                .interact_text()
                .unwrap_or_else(|_| "起床后半小时".to_string());
            prefs.prefs.insert("breakfast".to_string(), breakfast);
            
            let lunch: String = Input::new()
                .with_prompt("Lunch time")
                .default(prefs.prefs.get("lunch").cloned().unwrap_or_else(|| "12点到1点之间".to_string()))
                .interact_text()
                .unwrap_or_else(|_| "12点到1点之间".to_string());
            prefs.prefs.insert("lunch".to_string(), lunch);
            
            let dinner: String = Input::new()
                .with_prompt("Dinner time")
                .default(prefs.prefs.get("dinner").cloned().unwrap_or_else(|| "6点半到7点半".to_string()))
                .interact_text()
                .unwrap_or_else(|_| "6点半到7点半".to_string());
            prefs.prefs.insert("dinner".to_string(), dinner);
            
            let shower: String = Input::new()
                .with_prompt("Shower time")
                .default(prefs.prefs.get("shower").cloned().unwrap_or_else(|| "一般晚饭后洗澡".to_string()))
                .interact_text()
                .unwrap_or_else(|_| "一般晚饭后洗澡".to_string());
            prefs.prefs.insert("shower".to_string(), shower);
            
            // Build config
            let api_format_enum = match api_format.to_lowercase().as_str() {
                "anthropic" => config::ApiFormat::Anthropic,
                "gemini" => config::ApiFormat::Gemini,
                _ => config::ApiFormat::OpenAI,
            };
            
            let new_config = AppConfig {
                google: config::GoogleConfig {
                    source_list,
                    output_list,
                },
                llm: config::LlmConfig {
                    api_format: api_format_enum,
                    base_url,
                    model,
                },
                preferences: prefs,
                timezone,
            };
            
            new_config.save(Some(path.clone()))?;
            println!("\nConfiguration saved to: {}", path.display());
            println!("\nYou can add more custom preferences by editing the file directly.");
            println!("Don't forget to set MORROW_LLM_API_KEY environment variable!");
        }
        ConfigAction::Path => {
            let path = config_path.unwrap_or_else(AppConfig::default_config_path);
            println!("{}", path.display());
        }
    }
    Ok(())
}
