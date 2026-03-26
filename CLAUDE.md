# Meeting Transcriber (Rust)

## Project Overview

Cross-platform desktop application that records meeting audio, sends it to the Gemini API for transcription and analysis, and produces structured meeting notes with export to Markdown, Obsidian, and Notion.

Rebuilt from a Python/PySide6 original into Rust for smaller binaries and native performance.

## Architecture

```
src/
  main.rs              Entry point, eframe app launch, icon loading
  schema.rs            ActionItem + MeetingAnalysis serde structs
  config.rs            Persistent config via confy (TOML), env var fallback
  audio/
    mod.rs             TARGET_SAMPLE_RATE constant (16kHz)
    wav.rs             Stereo-to-mono, resampling, WAV assembly, stream mixing
    devices.rs         cpal device enumeration (per-platform loopback + mic)
    capture.rs         Single + dual recording via cpal streams on dedicated threads
  gemini/
    mod.rs
    prompt.rs          System prompt + user prompt builder (injects today's date)
    schema_convert.rs  Gemini-compatible JSON Schema (hand-built, no schemars)
    client.rs          REST client: inline (<15MB) and File API upload paths
  export/
    mod.rs
    markdown.rs        YAML frontmatter + sections + action items table
    obsidian.rs        Vault file writer with collision avoidance
    notion.rs          Direct REST API (no SDK), block chunking at 1900 chars
  ui/
    mod.rs
    theme.rs           Dark color palette, section_frame, primary/secondary buttons
    app.rs             Main eframe::App, analysis worker thread, credential scrubbing
    recorder_panel.rs  Source/device selectors, start/stop, timer, pulse indicator, Upload WAV
    results_panel.rs   Tabbed results (Summary/Actions/Responsibilities/Transcript), exports
    settings.rs        Modal settings dialog with grid-aligned form fields
```

## Key Design Decisions

- **egui/eframe** for GUI: zero native deps, small binary, built-in dark theme
- **cpal** for audio: WASAPI (Win), CoreAudio (macOS), ALSA/PulseAudio (Linux)
- **reqwest + rustls**: no OpenSSL dependency
- **API key in header**: uses `x-goog-api-key` header, never in URLs
- **cpal Stream is !Send**: recording streams live on dedicated threads, communicate via mpsc channels
- **Dual capture**: System + Mic mode opens two cpal streams, mixes to mono after resampling
- **Upload WAV**: Allows re-analysis of previously saved recordings via file picker (rfd), bypassing the record step and feeding WAV bytes directly into the Gemini analysis pipeline

## Build

```bash
# Debug
cargo run

# Release (Windows)
cargo build --release --target=x86_64-pc-windows-gnu

# Release (Linux)
cargo build --release
```

## Testing

```bash
cargo test
```

54 unit tests covering schema, WAV processing, exports, Gemini client parsing, credential scrubbing.

## Security Notes

- API key sent via `x-goog-api-key` header only (never in URLs)
- Error messages scrubbed for credential patterns before display
- Config file contains secrets in plaintext (confy TOML) — user should be aware
- Obsidian export has path traversal guard via canonicalize check
