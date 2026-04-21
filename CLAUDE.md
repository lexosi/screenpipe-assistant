# screenpipe-assistant — Claude Code Context

## Project purpose

Background daemon that bridges [Screenpipe](https://github.com/mediar-ai/screenpipe) and Claude/Telegram.
It polls Screenpipe's SQLite database every N seconds, detects new clipboard events that originated
in Windows Terminal, sends the terminal output to the Claude API, and forwards Claude's reply to a
Telegram bot. Intended to run persistently alongside Screenpipe on a Windows machine.

---

## Architecture overview

```
main.rs
  └─ parses subcommand ("run" | "test")
  └─ initialises combined file+terminal logger (simplelog)
  └─ loads config.toml via config.rs
  └─ "run"  → monitor::run()  (infinite loop)
  └─ "test" → telegram::send_message() (one-shot smoke test)

monitor.rs  (poll loop)
  └─ opens SQLite with rusqlite (new connection each tick)
  └─ queries ui_events for new clipboard rows from WindowsTerminal.exe
  └─ calls claude::send_to_claude()  for each row
  └─ calls telegram::send_message()  with Claude's reply
  └─ persists last processed id in state.json

claude.rs   → POST https://api.anthropic.com/v1/messages
telegram.rs → POST https://api.telegram.org/bot{token}/sendMessage
config.rs   → deserialises config.toml with serde + toml
```

All HTTP is **synchronous** (`reqwest` blocking feature, no async runtime in user code).

---

## File-by-file reference

### `src/main.rs`
- Entry point. No business logic.
- Builds the log path and state path from `std::env::current_exe().parent()` — this means
  `config.toml`, `state.json`, and `screenpipe-assistant.log` must live **next to the binary**,
  not in the working directory.
- `CombinedLogger` from `simplelog` writes to stdout and to the log file simultaneously.
- Exits with code 1 on any startup error.

### `src/config.rs`
- Single public struct `Config` (serde `Deserialize`).
- `Config::load(path: &Path)` reads and parses `config.toml`; returns a boxed error with a
  human-readable message if the file is missing or malformed.
- `poll_interval_secs` and `max_clipboard_length` have serde defaults (3 and 2000) so they are
  optional in the TOML file.

### `src/monitor.rs`
- Private `State { last_id: i64 }` is loaded from / saved to `state.json` (JSON via serde_json).
  If the file is absent or unparseable it defaults to `last_id = 0`.
- `run()` loops forever: calls `poll()`, saves state on any progress, then sleeps.
- `poll()` opens a fresh SQLite connection each tick (avoids stale WAL handles).
- Clipboard text longer than `max_clipboard_length` chars is truncated by the private `truncate()`
  helper which finds the correct UTF-8 char boundary (safe for non-ASCII output).
- After sending to Claude and Telegram, `state.last_id` is updated **per row** so partial progress
  is preserved even if a later row fails.

### `src/claude.rs`
- Model hardcoded: `claude-haiku-4-5-20251001`.
- System prompt hardcoded in the `SYSTEM_PROMPT` constant:
  > "You are a terminal assistant. The user sends you output from their Windows terminal.
  >  Suggest the next command to run, explain in 1 line why, and put the command on a separate
  >  line prefixed with CMD:"
- Request struct uses `max_tokens: 1024`.
- On non-2xx status the raw response body is included in the error message.

### `src/telegram.rs`
- Calls `sendMessage` endpoint. No parse_mode set (plain text).
- On non-2xx status the raw response body is included in the error message.
- Telegram's 4096-char message limit is not enforced here; Claude's 1024-token cap keeps
  responses well within that limit in practice.

---

## Build instructions (Windows — CMD only, never PowerShell)

```cmd
REM 1. Open a plain Command Prompt (cmd.exe)

REM 2. Activate the MSVC 64-bit toolchain
"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"

REM    If you have Community/Professional instead of BuildTools, adjust the path:
REM    "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"

REM 3. Tell CMake (used by the rusqlite bundled build) to use the VS 2022 generator
set CMAKE_GENERATOR=Visual Studio 17 2022

REM 4. Build
cd F:\proyectosprog\screenpipe-assistant
cargo build --release
```

Output binary: `target\release\screenpipe-assistant.exe`

**Why CMD?** PowerShell handles environment variable inheritance differently; `vcvars64.bat`
modifies PATH/LIB/INCLUDE for the current shell session and may not propagate correctly under PS.

**Why `bundled` feature for rusqlite?** It compiles SQLite from source into the binary so no
external `sqlite3.dll` is required at runtime. This needs `cl.exe` (MSVC) on PATH, which
`vcvars64.bat` provides.

---

## config.toml format

Place `config.toml` in the **same directory as the executable** (e.g. `target\release\`).

```toml
db_path               = "C:/Users/lexos/.screenpipe/db.sqlite"
anthropic_api_key     = "sk-ant-api03-..."
telegram_bot_token    = "123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ..."
telegram_chat_id      = "5718720346"
poll_interval_secs    = 3       # optional, default 3
max_clipboard_length  = 2000    # optional, default 2000
```

| Field                  | Type   | Required | Description |
|------------------------|--------|----------|-------------|
| `db_path`              | string | yes      | Absolute path to Screenpipe's SQLite file |
| `anthropic_api_key`    | string | yes      | Claude API key |
| `telegram_bot_token`   | string | yes      | Token from @BotFather |
| `telegram_chat_id`     | string | yes      | Numeric chat/user ID (as a string) |
| `poll_interval_secs`   | u64    | no       | Seconds between DB polls (default 3) |
| `max_clipboard_length` | usize  | no       | Char limit before truncation (default 2000) |

`config.toml` is in `.gitignore` — never committed.

---

## Database schema (relevant subset)

Table: **`ui_events`**

| Column       | Type    | Notes |
|--------------|---------|-------|
| `id`         | INTEGER | Auto-increment primary key; used to track progress |
| `event_type` | TEXT    | Filtered on `'clipboard'` |
| `app_name`   | TEXT    | Filtered on `'WindowsTerminal.exe'` |
| `text_content` | TEXT  | The clipboard payload sent to Claude; may be NULL (handled) |
| `timestamp`  | TEXT    | ISO 8601 with nanoseconds, e.g. `2026-04-18T15:30:28.444141700+00:00` |

The monitor does **not** parse `timestamp`; it uses only the `id` column for ordering and
deduplication.

Query used:
```sql
SELECT id, text_content
FROM ui_events
WHERE event_type = 'clipboard'
  AND app_name   = 'WindowsTerminal.exe'
  AND id > ?1
ORDER BY id ASC
```

---

## state.json structure

```json
{ "last_id": 42 }
```

- Written next to the executable after each successful batch.
- If missing or unparseable, the monitor starts from `id = 0` (processes all historical events
  on first run — this can produce a burst of Telegram messages if the DB already has data).
- To reset, delete `state.json` or set `last_id` to the current max id in the DB.

---

## Known gotchas

- **First-run burst**: If Screenpipe already has clipboard events in the DB, the first run will
  process all of them sequentially. Pre-seed `state.json` with a high `last_id` to skip history.

- **Binary-relative paths**: `config.toml`, `state.json`, and the log file are resolved relative
  to the executable, not the working directory. Running the binary from a different directory
  (e.g. `target\release\screenpipe-assistant run` from the project root) still reads config from
  `target\release\config.toml`.

- **Blocking HTTP**: Each Claude + Telegram call blocks the poll thread. If the Anthropic API is
  slow, a single poll tick can take several seconds. The sleep happens *after* the calls complete,
  so the effective interval is `poll_interval_secs + network_latency`.

- **No retry logic**: If Claude or Telegram returns an error, the event is still marked as
  processed (`last_id` is updated). A Telegram failure does not block subsequent events.

- **SQLite WAL mode**: A new connection is opened each tick; this avoids holding a long-lived
  read transaction that would conflict with Screenpipe's writer. If Screenpipe uses WAL mode
  (likely), concurrent reads are safe.

- **Truncation is by char count, not bytes**: `max_clipboard_length` counts Unicode scalar values.
  A 2000-char limit on a string containing multi-byte characters truncates at the correct boundary.

- **Model ID**: The full versioned model ID `claude-haiku-4-5-20251001` is required by the API
  even though the user-facing name is `claude-haiku-4-5`.

---

## Subcommands

| Subcommand | Behaviour |
|------------|-----------|
| `run`      | Start the monitor loop (runs indefinitely, Ctrl-C to stop) |
| `test`     | Send a single test message to Telegram and exit; verifies bot token and chat ID |

```cmd
screenpipe-assistant.exe run
screenpipe-assistant.exe test
```

---

## Files excluded from git (`.gitignore`)

```
target/       — Cargo build output
.claude/      — Claude Code session data
config.toml   — Contains API keys
state.json    — Runtime state
*.log         — Log files
```
