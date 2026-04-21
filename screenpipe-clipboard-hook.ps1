# screenpipe-assistant — PowerShell clipboard hook
#
# Automatically copies each command's output to the clipboard after it
# finishes executing.  Screenpipe then sees it as a clipboard event from
# WindowsTerminal.exe, and screenpipe-assistant forwards it to Claude.
#
# No manual Ctrl+C required.
#
# HOW TO INSTALL
# --------------
# 1. Find (or create) your PowerShell profile file:
#        notepad $PROFILE
#
# 2. Add this line anywhere near the end:
#        . "F:\proyectosprog\screenpipe-assistant\screenpipe-clipboard-hook.ps1"
#
# 3. Restart Windows Terminal (or reload the profile with: . $PROFILE)
#
# NOTE: If your profile already defines a custom `prompt` function this script
# will overwrite it.  Move your prompt customisation inside the `prompt`
# function below instead.
#
# HOW IT WORKS
# ------------
# PowerShell's Start-Transcript writes every prompt line + command output to a
# temp file.  The overridden `prompt` function reads whatever was added since
# the last prompt, strips ANSI escape codes and transcript metadata, then
# copies the result to the clipboard via Set-Clipboard.

# Per-session transcript file — one file per PID so multiple terminal windows
# don't collide.
$script:_SpTranscriptPath = Join-Path $env:TEMP "screenpipe_pwsh_$PID.txt"
$script:_SpOffset         = 0

Start-Transcript -Path $script:_SpTranscriptPath -Force | Out-Null

function prompt {
    # Stop the transcript to guarantee all buffered output is flushed to disk
    # before we read the file.
    Stop-Transcript | Out-Null

    try {
        $full = [System.IO.File]::ReadAllText(
            $script:_SpTranscriptPath,
            [System.Text.Encoding]::UTF8
        )
    } catch {
        $full = ''
    }

    $chunk = if ($full.Length -gt $script:_SpOffset) {
        $full.Substring($script:_SpOffset)
    } else {
        ''
    }
    # Always advance the offset so metadata blocks are never re-read.
    $script:_SpOffset = $full.Length

    # Only copy when a command was actually run.  On the very first prompt
    # invocation (session start) Get-History returns nothing, which prevents
    # the transcript header from being copied as if it were command output.
    if ($chunk -and (Get-History -Count 1)) {
        # Remove ANSI / VT100 escape sequences the terminal renderer injects.
        $chunk = $chunk -replace '\x1b\[[0-9;]*[A-Za-z]', ''

        # Remove transcript metadata lines: the **** delimiters, the header
        # key-value pairs (Start time, Username, etc.), and the informational
        # "Transcript started/stopped" sentences.
        $lines = ($chunk -split '\r?\n') | Where-Object {
            $_ -notmatch '^\*{4,}' -and
            $_ -notmatch '^(Transcript |Windows PowerShell transcript|Start time|End time|Username|RunAs User|Machine|Host Application|Process ID|PSVersion|PSEdition|PSCompatibleVersions|BuildVersion|CLRVersion|WSManStackVersion|PSRemotingProtocolVersion|SerializationVersion)'
        }

        $output = ($lines -join "`n").Trim()
        if ($output) {
            Set-Clipboard -Value $output
        }
    }

    # Restart the transcript in append mode for the next command.
    Start-Transcript -Path $script:_SpTranscriptPath -Append -Force | Out-Null

    # Standard prompt — edit the string below to customise your prompt style.
    "PS $($ExecutionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) "
}

# Clean up the temp transcript file when the PowerShell session exits.
Register-EngineEvent -SourceIdentifier PowerShell.Exiting -Action {
    Stop-Transcript -ErrorAction SilentlyContinue | Out-Null
    Remove-Item -Path $script:_SpTranscriptPath -ErrorAction SilentlyContinue
} | Out-Null
