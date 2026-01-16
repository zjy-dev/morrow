# Morrow (明日)

[![Release](https://img.shields.io/github/v/release/zjy-dev/morrow)](https://github.com/zjy-dev/morrow/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

LLM-powered daily schedule planner with Google Tasks integration.

## Features

- Reads tasks from a designated Google Tasks list
- Uses LLM to create an optimized daily schedule based on your preferences
- **Pomodoro Technique**: Applies 25min work + 5min break cycles for focused work
- **User Bio**: Describe your lifestyle and health conditions for personalized scheduling
- Outputs the schedule to a separate Google Tasks list
- Supports multiple LLM providers (OpenAI, Anthropic, Gemini)
- BYOK (Bring Your Own Key) - you control your API keys
- Cross-platform: Linux, macOS, Windows

## Installation

### Linux/macOS

```bash
curl -sSL https://raw.githubusercontent.com/zjy-dev/morrow/main/install.sh | bash
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/zjy-dev/morrow/main/install.ps1 | iex
```

### From Source

```bash
git clone https://github.com/zjy-dev/morrow.git
cd morrow
cargo build --release
```

## Quick Start

### 1. Create Google Cloud Project

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a new project
3. Enable the Google Tasks API
4. Create OAuth 2.0 credentials (Desktop application type)
5. Download the credentials

### 2. Set Environment Variables

```bash
# Google OAuth (required for authentication)
export MORROW_GOOGLE_CLIENT_ID=your-client-id
export MORROW_GOOGLE_CLIENT_SECRET=your-client-secret

# LLM API Key (required for planning)
export MORROW_LLM_API_KEY=your-api-key
```

### 3. Initialize Configuration

```bash
morrow config init
```

Config file location:
- Linux: `~/.config/morrow/config.yaml`
- macOS: `~/Library/Application Support/morrow/config.yaml`
- Windows: `%APPDATA%\morrow\config.yaml`

### 4. Authenticate with Google

```bash
morrow auth
```

### 5. Plan Tomorrow

```bash
morrow plan
```

## Configuration

See [config.example.yaml](config.example.yaml) for a complete example with comments.

```yaml
timezone: Asia/Shanghai

google:
  source_list: "Tomorrow Tasks"    # Your task list to read from
  output_list: "Morrow Schedule"   # List where schedule is written

llm:
  api_format: openai               # openai, anthropic, or gemini
  base_url: "https://api.openai.com/v1"
  model: "gpt-4o"

preferences:
  # Optional: Describe your lifestyle for personalized scheduling
  bio: |
    I'm a programmer who sits for long hours.
    I prefer handling complex tasks in the morning.
  
  wake_up: "7:30"
  sleep: "Before 11pm"
  breakfast: "30 min after waking up"
  lunch: "12:00-13:00"
  dinner: "18:30-19:30"
  # Add any custom preferences...
```

## Commands

```bash
morrow auth                  # Authenticate with Google
morrow plan                  # Generate tomorrow's schedule
morrow plan --config <path>  # Use custom config file
morrow config init           # Interactive configuration setup
morrow config show           # Display current configuration
morrow config path           # Show config file path
```

## GitHub Actions

Run morrow automatically via GitHub Actions:

1. Add repository secrets:
   - `MORROW_LLM_API_KEY`
   - `MORROW_GOOGLE_CLIENT_ID`
   - `MORROW_GOOGLE_CLIENT_SECRET`
   - `MORROW_GOOGLE_REFRESH_TOKEN` (from `~/.config/morrow/credentials.json`)

2. Trigger manually via GitHub Actions UI or schedule with cron

See [.github/workflows/plan.yml](.github/workflows/plan.yml) for the workflow configuration.

## How It Works

1. **Read Tasks**: Fetches incomplete tasks from your source list
2. **Check Output**: Verifies the output list is empty (prevents overwriting)
3. **Generate Schedule**: Sends preferences + tasks to LLM with Pomodoro rules
4. **Write Schedule**: Creates time-blocked tasks in reverse order (newest at bottom)

## Changelog

### v0.1.0
- Initial release
- Google Tasks integration
- Multi-LLM provider support (OpenAI, Anthropic, Gemini)
- Interactive configuration with `morrow config init`
- User bio and preferences support
- Pomodoro Technique integration
- Cross-platform binary releases

## License

MIT
