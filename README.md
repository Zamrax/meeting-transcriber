# <img src="meeting-transcriber.ico" width="32" height="32" alt="icon"> Meeting Transcriber

A cross-platform desktop application that **records**, **transcribes**, and **analyzes** meetings using Google Gemini AI.

---

## ✨ Features

| Feature | Description |
|---------|-------------|
| 🎙️ **System + Mic Recording** | Captures both system audio (remote participants) and microphone (you) simultaneously |
| ✨ **AI Transcription** | Full verbatim transcript with speaker labels via Gemini API |
| 🧠 **Smart Analysis** | Executive summary, action items with deadlines, per-person responsibilities |
| 📄 **Markdown Export** | Download structured meeting notes as `.md` with YAML frontmatter |
| 📓 **Obsidian Integration** | One-click save to your Obsidian vault under `Meeting Notes/` |
| 📝 **Notion Integration** | Push meeting notes as a new Notion page with formatted blocks |
| 🌙 **Dark Theme** | Polished dark UI built with egui |
| 💻 **Cross-Platform** | Windows, macOS, and Linux from a single codebase |

## 🚀 Quick Start

### 1. Get a Gemini API Key

Get a free API key from [Google AI Studio](https.aistudio.google.com/apikey).

### 2. Build & Run

```bash
# Clone the repository
git clone <repo-url>
cd meeting-transcriber

# Run in debug mode
cargo run

# Or build a release binary
cargo build --release
```

### 3. Configure

Click **Settings** in the app and enter:
- **API Key** — Your Gemini API key
- **Model** — Choose from gemini-3.1-flash-lite-preview, gemini-2.5-flash, etc.
- **Participants** — Optional comma-separated names for better speaker labeling
- **Obsidian Vault Path** — For one-click Obsidian export
- **Notion Token + Page ID** — For Notion integration

### 4. Record

1. Select **System + Mic** (default), **System Audio**, or **Microphone**
2. Choose your audio devices
3. Click **Start Recording**
4. When done, click **Stop Recording** — analysis begins automatically
5. Browse results in the **Summary**, **Action Items**, **Responsibilities**, and **Transcript** tabs
6. Export via **Download .md**, **Save to Obsidian**, or **Push to Notion**

## 🎤 Audio Modes

| Mode | What it captures | Use case |
|------|-----------------|----------|
| **System + Mic** | Remote participants (speakers) + your voice (microphone) | Video calls, online meetings |
| **System Audio** | Only system/speaker output | Recording a presentation or webinar |
| **Microphone** | Only microphone input | In-person meetings |

## 💻 Platform Support

| Platform | Audio Backend | Loopback Method |
|----------|--------------|-----------------|
| **Windows** | WASAPI | Output device loopback capture |
| **macOS** | CoreAudio | BlackHole virtual audio device |
| **Linux** | PulseAudio / ALSA | PulseAudio monitor source |

> **macOS note:** Install [BlackHole](https://existential.audio/blackhole/) for system audio capture.
>
> **Linux note:** PulseAudio monitor sources are used automatically.

## 📦 Building Release Binaries

```bash
# Windows (from Linux cross-compile)
cargo build --release --target=x86_64-pc-windows-gnu

# macOS
cargo build --release

# Linux
cargo build --release
```

### Binary Size Optimization

The release profile is configured for minimal binary size:

```toml
[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Link-time optimization
codegen-units = 1   # Single codegen unit
panic = "abort"     # No unwinding overhead
strip = true        # Strip debug symbols
```

Expected binary size: **5-12 MB** (vs 80-150 MB for the Python/PySide6 version).

## ⚙️ Configuration

Settings are persisted to your OS config directory via [confy](https://crates.io/crates/confy):

| OS | Location |
|----|----------|
| Windows | `%APPDATA%\meeting-transcriber\default-config.toml` |
| macOS | `~/Library/Application Support/meeting-transcriber/default-config.toml` |
| Linux | `~/.config/meeting-transcriber/default-config.toml` |

You can also use a `.env` file in the project root:

```env
GEMINI_API_KEY=your-key-here
NOTION_TOKEN=your-notion-token
NOTION_PARENT_PAGE_ID=your-page-id
OBSIDIAN_VAULT_PATH=/path/to/vault
```

## 🛠️ Tech Stack

| Component | Crate | Purpose |
|-----------|-------|---------|
| 🎨 GUI | `eframe` / `egui` | Immediate-mode cross-platform UI |
| 🔊 Audio | `cpal` | Cross-platform audio capture |
| 🌐 HTTP | `reqwest` + `rustls` | Gemini & Notion API (no OpenSSL) |
| 🔄 Serialization | `serde` / `serde_json` | JSON parsing and schema |
| 🎵 WAV | `hound` | WAV file read/write |
| ⚙️ Config | `confy` | TOML-based persistent settings |
| 📂 File dialogs | `rfd` | Native open/save dialogs |

## ✅ Testing

```bash
cargo test
```

54 unit tests covering:
- WAV assembly, stereo-to-mono, resampling, stream mixing
- Schema serialization/deserialization
- Markdown, Obsidian, and Notion export formats
- Gemini client response parsing
- Credential scrubbing
- Config persistence

## 📄 License

MIT
