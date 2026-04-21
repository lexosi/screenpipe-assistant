# screenpipe-assistant

A background process that monitors [Screenpipe](https://github.com/mediar-ai/screenpipe)'s SQLite database for clipboard events originating from Windows Terminal, sends the content to Claude for analysis, and delivers the response to a Telegram bot.

## How it works

1. Every N seconds (default 3) the process opens the Screenpipe SQLite database and queries `ui_events` for new rows where `event_type = 'clipboard'` and `app_name = 'WindowsTerminal.exe'`.
2. The last processed row ID is persisted in `state.json` next to the executable so progress survives restarts.
3. Each new clipboard text is sent to the Claude API (`claude-haiku-4-5`) with a system prompt that asks Claude to suggest the next terminal command.
4. Claude's response is forwarded to your Telegram bot.
5. All activity is written to `screenpipe-assistant.log` in the same directory as the executable.

## Requirements

- [Rust toolchain](https://rustup.rs/) (stable, edition 2021)
- [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) with the **Desktop development with C++** workload (needed to compile SQLite via the `bundled` feature)
- [Screenpipe](https://github.com/mediar-ai/screenpipe) running and generating `ui_events` in its SQLite database
- A Claude API key from [console.anthropic.com](https://console.anthropic.com)
- A Telegram bot token from [@BotFather](https://t.me/botfather)

## Building

Open a **Command Prompt** (not PowerShell) and run:

```cmd
"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
set CMAKE_GENERATOR=Visual Studio 17 2022
cd F:\proyectosprog\screenpipe-assistant
cargo build --release
```

The binary is produced at `target\release\screenpipe-assistant.exe`.

> **Note:** `vcvars64.bat` may live under `Community` or `Professional` instead of `BuildTools` depending on your Visual Studio installation.

## Configuration

Copy the example config and fill in your credentials:

```cmd
copy config.toml.example config.toml
```

`config.toml` fields:

| Key                   | Description                                 | Default |
|-----------------------|---------------------------------------------|---------|
| `db_path`             | Full path to Screenpipe's `db.sqlite`       | —       |
| `anthropic_api_key`   | Claude API key                              | —       |
| `telegram_bot_token`  | Token from @BotFather                       | —       |
| `telegram_chat_id`    | Telegram chat / user ID to send replies to  | —       |
| `poll_interval_secs`  | How often to check the database (seconds)  | `3`     |
| `max_clipboard_length`| Maximum characters sent to Claude (truncates if longer) | `2000` |

Place `config.toml` in the **same directory as the executable** (`target\release\`), or run the binary from that directory.

## Usage

### Start monitoring

```cmd
screenpipe-assistant.exe run
```

The process runs indefinitely, logging to both the terminal and `screenpipe-assistant.log`.

### Verify Telegram connectivity

```cmd
screenpipe-assistant.exe test
```

Sends a test message to your configured chat to confirm the bot token and chat ID are correct.

## File layout

```
src/
  main.rs       — entry point, logger setup, subcommand dispatch
  config.rs     — config.toml loading and struct
  monitor.rs    — SQLite polling loop and state persistence
  claude.rs     — Claude API client
  telegram.rs   — Telegram sendMessage client
config.toml.example   — template (copy to config.toml and fill in)
```

## What is excluded from version control

`config.toml`, `state.json`, `*.log`, and `target/` are listed in `.gitignore` and are never committed.
