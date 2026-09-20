pub mod wav;
pub mod mp3;
pub mod devices;
pub mod capture;
#[cfg(target_os = "linux")]
pub mod pulse;

/// Target sample rate for speech-model compatibility.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;
