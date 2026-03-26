# <img src="meeting-transcriber.ico" width="32" height="32" alt="icon"> Meeting Transcriber

A cross-platform desktop application that **records**, **transcribes**, and **analyzes** meetings using Google Gemini AI. Built in Rust for small binaries and native performance.

---

## ✨ Features

| Feature | Description |
|---------|-------------|
| 🎙️ **System + Mic Recording** | Captures both system audio (remote participants) and microphone (you) simultaneously |
| ✨ **AI Transcription** | Full verbatim transcript with speaker labels via Gemini API |
| 🧠 **Smart Analysis** | Detailed multi-paragraph summary, action items with deadlines, per-person responsibilities |
| 📄 **Markdown Export** | Download structured meeting notes as `.md` with YAML frontmatter |
| 📓 **Obsidian Integration** | One-click save to your Obsidian vault under `Meeting Notes/` |
| 📝 **Notion Integration** | Push meeting notes as a new Notion page with formatted blocks |
| 📊 **Live Recording Feedback** | Real-time sample counter and silence detection during recording |
| ⏱️ **Long Meeting Support** | Records up to 90 minutes; resumable upload for large files |
| 🌙 **Dark Theme** | Polished dark UI built with egui |
| 💻 **Cross-Platform** | Windows, macOS, and Linux from a single Rust codebase |

---

## 📋 Prerequisites

### All Platforms

- [Rust toolchain](https://rustup.rs/) (1.70+)
- A [Gemini API key](https://aistudio.google.com/apikey) (free tier available)

### Windows

No additional dependencies. WASAPI is used for audio capture and is built into Windows.

> **Note:** If building from source on Windows, the MSVC build tools are required (installed with Visual Studio or the [Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)).

### macOS

**For system audio capture**, install [BlackHole](https://existential.audio/blackhole/):

```bash
brew install blackhole-2ch
```

Then configure a Multi-Output Device so audio flows to both your speakers and BlackHole:

1. Open **Audio MIDI Setup** (Spotlight > "Audio MIDI Setup")
2. Click the **+** button at the bottom-left and choose **Create Multi-Output Device**
3. Check both your speakers/headphones **and** BlackHole 2ch
4. Go to **System Settings > Sound > Output** and select the Multi-Output Device

> Without this setup, BlackHole will be listed as a device but will capture silence.

**Running the .app bundle:** The app is not signed with an Apple Developer certificate, so macOS will block it. To unblock, open Terminal and run:

```bash
xattr -cr /path/to/MeetingAssistant.app
```

> **Tip:** Type `xattr -cr ` (with a trailing space) then drag and drop the `.app` file from Finder into the Terminal window — it will fill in the path automatically.

### Linux

Install the ALSA development libraries (required to build `cpal`):

```bash
# Debian / Ubuntu
sudo apt-get install libasound2-dev

# Fedora
sudo dnf install alsa-lib-devel

# Arch
sudo pacman -S alsa-lib
```

If you want a GTK file dialog (for the save/export dialogs), also install:

```bash
# Debian / Ubuntu
sudo apt-get install libgtk-3-dev
```

PulseAudio monitor sources are detected automatically for system audio capture.

---

## 🚀 Quick Start

### 1. Build & Run

```bash
git clone https://github.com/Zamrax/meeting-transcriber.git
cd meeting-transcriber
cargo run
```

### 2. Configure

Click **Settings** in the app and enter:

| Setting | Required | Description |
|---------|----------|-------------|
| **Gemini API Key** | Yes | Get one free at [aistudio.google.com/apikey](https://aistudio.google.com/apikey) |
| **Model** | Yes | Default: `gemini-2.5-flash`. Also supports `gemini-2.5-pro`, `gemini-2.0-flash`, etc. |
| **Participants** | No | Comma-separated names for better speaker labeling |
| **Obsidian Vault Path** | No | Absolute path to your vault for one-click export |
| **Notion Token + Page ID** | No | For Notion integration |

Alternatively, create a `.env` file in the project root (see `.env.example`):

```env
GEMINI_API_KEY=your-key-here
```

### 3. Record

1. Select **System + Mic** (default), **System Audio**, or **Microphone**
2. Choose your audio devices from the dropdowns
3. Click **Start Recording** — the live sample counter confirms audio is flowing
4. When done, click **Stop Recording** — analysis begins automatically
5. Browse results in the **Summary**, **Action Items**, **Responsibilities**, and **Transcript** tabs
6. Export via **Download .md**, **Save to Obsidian**, or **Push to Notion**

---

## 🎤 Audio Modes

| Mode | What it captures | Use case |
|------|-----------------|----------|
| **System + Mic** | Remote participants (speakers) + your voice (microphone) | Video calls, online meetings |
| **System Audio** | Only system/speaker output | Recording a presentation or webinar |
| **Microphone** | Only microphone input | In-person meetings |

## 💻 Platform Audio Support

| Platform | Audio Backend | System Audio Method |
|----------|--------------|---------------------|
| **Windows** | WASAPI | Output device loopback capture (built-in) |
| **macOS** | CoreAudio | [BlackHole](https://existential.audio/blackhole/) virtual audio device |
| **Linux** | PulseAudio / ALSA | PulseAudio monitor source (auto-detected) |

---

## 📦 Building Release Binaries

```bash
# Windows (native MSVC)
cargo build --release

# Windows (cross-compile from Linux)
cargo build --release --target=x86_64-pc-windows-gnu

# macOS (Apple Silicon)
cargo build --release --target=aarch64-apple-darwin

# macOS (Intel)
cargo build --release --target=x86_64-apple-darwin

# Linux
cargo build --release
```

Release binaries are written to `target/release/` (or `target/<target>/release/` for cross-compilation).

The release profile is tuned for minimal binary size (~5-12 MB):

```toml
[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Link-time optimization
codegen-units = 1   # Single codegen unit
panic = "abort"     # No unwinding overhead
strip = true        # Strip debug symbols
```

---

## ⚙️ Configuration

Settings are persisted to your OS config directory via [confy](https://crates.io/crates/confy):

| OS | Location |
|----|----------|
| Windows | `%APPDATA%\meeting-transcriber\default-config.toml` |
| macOS | `~/Library/Application Support/meeting-transcriber/default-config.toml` |
| Linux | `~/.config/meeting-transcriber/default-config.toml` |

---

## 🛠️ Tech Stack

| Component | Crate | Purpose |
|-----------|-------|---------|
| 🎨 GUI | `eframe` / `egui` | Immediate-mode cross-platform UI |
| 🔊 Audio | `cpal` | Cross-platform audio capture |
| 🌐 HTTP | `reqwest` + `rustls` | Gemini & Notion API (no OpenSSL dependency) |
| 🔄 Serialization | `serde` / `serde_json` | JSON parsing and schema |
| 🎵 WAV | `hound` | WAV file encoding |
| ⚙️ Config | `confy` | TOML-based persistent settings |
| 📂 File dialogs | `rfd` | Native open/save dialogs |

## ✅ Testing

```bash
cargo test
```

Unit tests cover WAV assembly, stereo-to-mono conversion, resampling, stream mixing, schema serialization, all export formats, Gemini client response parsing, credential scrubbing, silence detection, and config persistence.

## 📄 License

MIT
