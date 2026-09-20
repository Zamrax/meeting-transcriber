# <img src="meeting-transcriber.ico" width="32" height="32" alt="icon"> Meeting Transcriber

A cross-platform desktop application that **records**, **transcribes**, and **analyzes** meetings using audio-capable LLMs through [OpenRouter](https://openrouter.ai). Built in Rust for small binaries and native performance.

---

## ✨ Features

| Feature | Description |
|---------|-------------|
| 🎙️ **System + Mic Recording** | Captures both system audio (remote participants) and microphone (you) simultaneously |
| ✨ **AI Transcription** | Full verbatim transcript with speaker labels via OpenRouter |
| 🧠 **Smart Analysis** | Detailed multi-paragraph summary, action items with deadlines, per-person responsibilities |
| 📄 **Markdown Export** | Download structured meeting notes as `.md` with YAML frontmatter |
| 📓 **Obsidian Integration** | One-click save to your Obsidian vault under `Meeting Notes/` |
| 📝 **Notion Integration** | Push meeting notes as a new Notion page with formatted blocks |
| 📤 **Upload WAV** | Re-analyze a previously saved recording when the model was unavailable |
| 📊 **Live Recording Feedback** | Real-time sample counter and silence detection during recording |
| ⏱️ **Long Meeting Support** | Records up to 90 minutes; resumable upload for large files |
| 🌙 **Dark Theme** | Polished dark UI built with egui |
| 💻 **Cross-Platform** | Windows, macOS, and Linux from a single Rust codebase |

---

## 📋 Prerequisites

### All Platforms

- [Rust toolchain](https://rustup.rs/) (1.70+)
- An [OpenRouter API key](https://openrouter.ai/keys)

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
| **OpenRouter API Key** | Yes | Create one at [openrouter.ai/keys](https://openrouter.ai/keys) |
| **Transcribe model** | Yes | Reads the audio. A speech-to-text model or an audio-capable chat model. Default: `microsoft/mai-transcribe-2` |
| **Analyze model** | Yes | Reads the transcript and writes the notes. Any text model. Default: `google/gemini-2.5-flash` |
| **Participants** | No | Comma-separated names for better speaker labeling |
| **Obsidian Vault Path** | No | Absolute path to your vault for one-click export |
| **Notion Token + Page ID** | No | For Notion integration |

Alternatively, create a `.env` file in the project root (see `.env.example`):

```env
OPENROUTER_API_KEY=your-key-here
```

### 3. Record

1. Select **System + Mic** (default), **System Audio**, or **Microphone**
2. Choose your audio devices from the dropdowns
3. Click **Start Recording** — the live sample counter confirms audio is flowing
4. When done, click **Stop Recording** — analysis begins automatically
5. Browse results in the **Summary**, **Action Items**, **Responsibilities**, and **Transcript** tabs
6. Export via **Download .md**, **Save to Obsidian**, or **Push to Notion**

> **Long recordings:** OpenRouter has no file-upload API, so audio is inlined as base64 and the upstream provider caps a request at ~20 MB. Recordings are therefore encoded to 32 kbps mono MP3 and, past 25 minutes, split into parts: each part is transcribed on its own, then one final text-only call turns the stitched transcript into the title, summary, responsibilities, and action items. A 90-minute meeting costs 4 API calls in total, and the UI shows which part is in flight.
>
> Because each call carries only one chunk (~48k audio tokens at 25 minutes), a 128k-context model is enough — the meeting's total length does not have to fit in the context window. The model does have to accept audio input; see the table below.

> **Tip:** If the model fails to analyze a recording (e.g., the model was overloaded), the WAV file is still saved. Click **Upload WAV** to re-submit it for analysis later.

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
| 🌐 HTTP | `reqwest` + `rustls` | OpenRouter & Notion API (no OpenSSL dependency) |
| 🎵 MP3 | `mp3lame-encoder` | Compresses audio for upload (bundles LAME, LGPL-3.0) |
| 🔄 Serialization | `serde` / `serde_json` | JSON parsing and schema |
| 🎵 WAV | `hound` | WAV file encoding |
| ⚙️ Config | `confy` | TOML-based persistent settings |
| 📂 File dialogs | `rfd` | Native open/save dialogs |

## ✅ Testing

```bash
cargo test
```

Unit tests cover WAV assembly, stereo-to-mono conversion, resampling, stream mixing, schema serialization, all export formats, MP3 encoding, WAV decoding and resampling, OpenRouter client response parsing, credential scrubbing, silence detection, and config persistence.


## 🧠 Choosing a model

Two models are configured separately, because the pipeline runs in two phases that bill
differently. The **transcribe** model reads the audio, once per 25-minute chunk. The **analyze**
model reads only the finished transcript and writes the notes, so a cheap transcriber can be paired
with a stronger text model at little extra cost.

Both dropdowns are populated live from OpenRouter at startup, sorted cheapest first, with the
context window and an estimated cost per hour of recording on each row. The list is cached on disk,
so later launches populate instantly and still work offline; a small built-in list is the final
fallback. Either field can be typed by hand to use a model the catalog omits.

### Two kinds of transcription model

The transcribe list mixes two kinds, and the app calls each through its own endpoint:

- **Dedicated speech-to-text** (MAI-Transcribe, Whisper, Parakeet, Qwen3 ASR, Deepgram, …) — 21
  models, fetched with `?output_modalities=transcription`, called through
  `/v1/audio/transcriptions`. Much cheaper, and they take no instructions: most return an
  unbroken wall of text with **no speaker labels**.
- **Chat models that accept audio** (Gemini, GPT Audio) — 32 models, called through
  `/v1/chat/completions`. Roughly 10x the price, but they follow the prompt, so they attempt
  speaker labels and know where the chunk sits in the meeting.

**The default is `microsoft/mai-transcribe-2`**, which gets the best of both: speech-to-text
pricing (~$0.10/hour) with real speaker diarization rather than a model inferring speakers from
audio.

### Diarization

Diarization is working out **who** spoke, not just what was said. Without it a transcript reads as
one unbroken block; with it, it is split into `Speaker 1: …` / `Speaker 2: …` turns, which is what
lets action items be attributed to a person.

Settings has a **Label speakers (diarization)** checkbox. It is enabled only for models that can
actually do it — the MAI-Transcribe family — and the dialog says which case you are in: that the
chosen model returns plain text, that speaker turns are coming, or that the model *can* label
speakers and the box is worth ticking. When it is on, the participant names are also sent as
keyword hints so their spellings come back right.

If speaker labels don't matter, `openai/whisper-large-v3-turbo` costs about **$0.01/hour**, roughly
a tenth of the default. Diarization options are Azure-specific, so they are only sent to
MAI-Transcribe models; every other speech-to-text model returns plain text.

Cost estimates assume ~1,920 audio tokens per minute and ~130 words per minute of speech.
Speech-to-text pricing is published in three different units (per second, per hour, per token) with
nothing in the API saying which applies, so the app infers it from the shape of the price. Treat
every figure as a comparison aid, not a bill — OpenRouter's dashboard is authoritative.

## 📄 License

MIT

This binary statically links LAME (via `mp3lame-encoder`), which is LGPL-3.0. Distributing the
built binary therefore carries LGPL relink obligations for that component; the project's own
source stays MIT.
