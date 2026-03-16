use std::io::Cursor;

use super::TARGET_SAMPLE_RATE;

/// Convert interleaved multi-channel PCM (i16) to mono by averaging channels.
pub fn stereo_to_mono(audio_data: &[u8], channels: u16) -> Vec<u8> {
    if channels <= 1 {
        return audio_data.to_vec();
    }
    let sample_width = 2usize; // i16
    let frame_size = sample_width * channels as usize;
    let num_frames = audio_data.len() / frame_size;
    let mut mono = Vec::with_capacity(num_frames * sample_width);

    for f in 0..num_frames {
        let mut sum: i32 = 0;
        for ch in 0..channels as usize {
            let offset = f * frame_size + ch * sample_width;
            if offset + 1 < audio_data.len() {
                let sample = i16::from_le_bytes([audio_data[offset], audio_data[offset + 1]]);
                sum += sample as i32;
            }
        }
        let avg = (sum / channels as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        mono.extend_from_slice(&avg.to_le_bytes());
    }
    mono
}

/// Resample mono PCM (i16) using linear interpolation.
pub fn resample(audio_data: &[u8], src_rate: u32, dst_rate: u32) -> Vec<u8> {
    if src_rate == dst_rate {
        return audio_data.to_vec();
    }
    let sample_width = 2usize;
    let num_samples = audio_data.len() / sample_width;
    if num_samples == 0 {
        return vec![];
    }

    let ratio = src_rate as f64 / dst_rate as f64;
    let out_len = ((num_samples as f64) / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(out_len * sample_width);

    // Read all source samples
    let samples: Vec<i16> = (0..num_samples)
        .map(|i| {
            let offset = i * sample_width;
            i16::from_le_bytes([audio_data[offset], audio_data[offset + 1]])
        })
        .collect();

    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = src_pos - idx as f64;

        let s0 = samples[idx.min(num_samples - 1)] as f64;
        let s1 = samples[(idx + 1).min(num_samples - 1)] as f64;
        let interpolated = s0 + frac * (s1 - s0);
        let sample = interpolated.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16;
        output.extend_from_slice(&sample.to_le_bytes());
    }
    output
}

/// Assemble raw PCM chunks into a 16 kHz mono WAV file.
///
/// Steps:
/// 1. Join all chunks into raw PCM data
/// 2. Convert stereo/multi-channel to mono (if channels > 1)
/// 3. Resample to 16 kHz (if sample_rate != 16000)
/// 4. Wrap in a WAV container
pub fn assemble_wav(chunks: &[Vec<u8>], sample_rate: u32, channels: u16) -> Vec<u8> {
    // 1. Join chunks
    let total_len: usize = chunks.iter().map(|c| c.len()).sum();
    let mut raw = Vec::with_capacity(total_len);
    for chunk in chunks {
        raw.extend_from_slice(chunk);
    }

    if raw.is_empty() {
        return create_empty_wav();
    }

    // 2. Stereo to mono
    let mono = stereo_to_mono(&raw, channels);

    // 3. Resample to target
    let resampled = resample(&mono, sample_rate, TARGET_SAMPLE_RATE);

    // 4. Wrap in WAV
    write_wav(&resampled, TARGET_SAMPLE_RATE, 1)
}

/// Mix two mono 16kHz PCM streams (i16 LE bytes) by summing and clamping.
/// If one stream is shorter, the longer stream plays solo for the remainder.
pub fn mix_mono_streams(a: &[u8], b: &[u8]) -> Vec<u8> {
    let len = a.len().max(b.len());
    let sample_count = len / 2;
    let mut out = Vec::with_capacity(len);

    for i in 0..sample_count {
        let offset = i * 2;
        let sa = if offset + 1 < a.len() {
            i16::from_le_bytes([a[offset], a[offset + 1]]) as i32
        } else {
            0
        };
        let sb = if offset + 1 < b.len() {
            i16::from_le_bytes([b[offset], b[offset + 1]]) as i32
        } else {
            0
        };
        let mixed = (sa + sb).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        out.extend_from_slice(&mixed.to_le_bytes());
    }
    out
}

/// Create an empty but valid WAV file (44 bytes header only).
fn create_empty_wav() -> Vec<u8> {
    write_wav(&[], TARGET_SAMPLE_RATE, 1)
}

/// Public wrapper for write_wav, used by dual capture mixing.
pub fn write_wav_public(pcm_data: &[u8], sample_rate: u32, channels: u16) -> Vec<u8> {
    write_wav(pcm_data, sample_rate, channels)
}

/// Write raw PCM i16 data into a WAV container using hound.
/// Returns an empty WAV on encoding errors rather than panicking.
fn write_wav(pcm_data: &[u8], sample_rate: u32, channels: u16) -> Vec<u8> {
    match write_wav_inner(pcm_data, sample_rate, channels) {
        Ok(data) => data,
        Err(e) => {
            log::error!("WAV encoding failed: {e}");
            // Return a valid empty WAV rather than panicking
            write_wav_inner(&[], sample_rate, channels).unwrap_or_default()
        }
    }
}

fn write_wav_inner(pcm_data: &[u8], sample_rate: u32, channels: u16) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|e| format!("WAV writer creation failed: {e}"))?;
        let num_samples = pcm_data.len() / 2;
        for i in 0..num_samples {
            let sample = i16::from_le_bytes([pcm_data[i * 2], pcm_data[i * 2 + 1]]);
            writer
                .write_sample(sample)
                .map_err(|e| format!("WAV sample write failed: {e}"))?;
        }
        writer
            .finalize()
            .map_err(|e| format!("WAV finalize failed: {e}"))?;
    }
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stereo_to_mono_passthrough() {
        let mono_data = vec![0x00, 0x01, 0x00, 0x02];
        let result = stereo_to_mono(&mono_data, 1);
        assert_eq!(result, mono_data);
    }

    #[test]
    fn test_stereo_to_mono_averages() {
        // Two stereo frames: (100, 200) and (300, 400)
        let mut data = Vec::new();
        data.extend_from_slice(&100i16.to_le_bytes());
        data.extend_from_slice(&200i16.to_le_bytes());
        data.extend_from_slice(&300i16.to_le_bytes());
        data.extend_from_slice(&400i16.to_le_bytes());

        let result = stereo_to_mono(&data, 2);
        assert_eq!(result.len(), 4); // 2 mono samples * 2 bytes

        let s0 = i16::from_le_bytes([result[0], result[1]]);
        let s1 = i16::from_le_bytes([result[2], result[3]]);
        assert_eq!(s0, 150); // (100+200)/2
        assert_eq!(s1, 350); // (300+400)/2
    }

    #[test]
    fn test_resample_same_rate() {
        let data = vec![0x00, 0x01, 0x00, 0x02];
        let result = resample(&data, 16000, 16000);
        assert_eq!(result, data);
    }

    #[test]
    fn test_resample_empty() {
        let result = resample(&[], 44100, 16000);
        assert!(result.is_empty());
    }

    #[test]
    fn test_resample_downsample() {
        // Create 44100 samples of silence at 44100 Hz = 1 second
        let src_samples = 44100usize;
        let data: Vec<u8> = vec![0u8; src_samples * 2];
        let result = resample(&data, 44100, 16000);
        // Should produce ~16000 samples
        let out_samples = result.len() / 2;
        assert!(out_samples >= 15900 && out_samples <= 16100);
    }

    #[test]
    fn test_assemble_wav_produces_valid_wav() {
        let mut chunk = Vec::new();
        for _ in 0..1000 {
            chunk.extend_from_slice(&100i16.to_le_bytes());
        }
        let wav = assemble_wav(&[chunk], 16000, 1);
        // WAV files start with "RIFF"
        assert_eq!(&wav[0..4], b"RIFF");
        // Format should be "WAVE"
        assert_eq!(&wav[8..12], b"WAVE");
        // Should be more than just the header
        assert!(wav.len() > 44);
    }

    #[test]
    fn test_assemble_wav_empty_chunks() {
        let wav = assemble_wav(&[], 16000, 1);
        assert_eq!(&wav[0..4], b"RIFF");
        // Empty WAV should be just the header (44 bytes for standard WAV)
    }

    #[test]
    fn test_assemble_wav_stereo_resampled() {
        // Create stereo audio at 44100 Hz
        let mut chunk = Vec::new();
        for i in 0..4410 {
            let sample = (i % 100) as i16;
            // Left channel
            chunk.extend_from_slice(&sample.to_le_bytes());
            // Right channel
            chunk.extend_from_slice(&sample.to_le_bytes());
        }
        let wav = assemble_wav(&[chunk], 44100, 2);
        assert_eq!(&wav[0..4], b"RIFF");
        assert!(wav.len() > 44);
    }

    // --- BlackHole 16ch downmix tests ---

    #[test]
    fn test_stereo_to_mono_16_channels_produces_correct_sample_count() {
        // BlackHole 16ch: 16 channels per frame, i16 samples
        let channels: u16 = 16;
        let num_frames = 100usize;
        let frame_size = channels as usize * 2; // 2 bytes per i16
        let mut data = vec![0u8; num_frames * frame_size];

        // Fill each frame so that channel 0 = 1000, all others = 0.
        // Expected mono average = 1000 / 16 = 62 (integer division in i32 then cast).
        for f in 0..num_frames {
            let offset = f * frame_size;
            let sample: i16 = 1000;
            data[offset..offset + 2].copy_from_slice(&sample.to_le_bytes());
            // Remaining 15 channels stay 0 (already zeroed)
        }

        let result = stereo_to_mono(&data, channels);
        // One mono sample per frame, 2 bytes each
        assert_eq!(result.len(), num_frames * 2);
    }

    #[test]
    fn test_stereo_to_mono_16_channels_averages_correctly() {
        // Two frames, each with channel 0 = 1600 and all other 15 channels = 0.
        // Average per frame = 1600 / 16 = 100.
        let channels: u16 = 16;
        let frame_size = channels as usize * 2;
        let mut data = vec![0u8; 2 * frame_size];

        for f in 0..2usize {
            let offset = f * frame_size;
            let sample: i16 = 1600;
            data[offset..offset + 2].copy_from_slice(&sample.to_le_bytes());
        }

        let result = stereo_to_mono(&data, channels);
        assert_eq!(result.len(), 4); // 2 mono frames * 2 bytes

        let s0 = i16::from_le_bytes([result[0], result[1]]);
        let s1 = i16::from_le_bytes([result[2], result[3]]);
        assert_eq!(s0, 100);
        assert_eq!(s1, 100);
    }

    #[test]
    fn test_stereo_to_mono_16_channels_clamps_overflow() {
        // All 16 channels = i16::MAX (32767). Sum = 16 * 32767, which overflows i16 but
        // the implementation accumulates in i32 and divides before clamping.
        // Average = 32767 (no clamping needed at this value, but verify no panic).
        let channels: u16 = 16;
        let frame_size = channels as usize * 2;
        let mut data = vec![0u8; frame_size]; // 1 frame

        for ch in 0..channels as usize {
            let offset = ch * 2;
            data[offset..offset + 2].copy_from_slice(&i16::MAX.to_le_bytes());
        }

        let result = stereo_to_mono(&data, channels);
        assert_eq!(result.len(), 2);
        let sample = i16::from_le_bytes([result[0], result[1]]);
        assert_eq!(sample, i16::MAX);
    }

    // --- 48kHz 2ch (BlackHole-typical) resampling tests ---

    #[test]
    fn test_assemble_wav_48khz_2ch_produces_valid_wav() {
        // Simulate one second of 48kHz stereo audio (BlackHole-typical capture rate).
        // 48000 frames * 2 channels * 2 bytes = 192000 bytes raw PCM.
        let sample_rate = 48000u32;
        let channels = 2u16;
        let num_frames = 4800usize; // 0.1 s — enough to verify without huge allocation
        let mut chunk = Vec::with_capacity(num_frames * channels as usize * 2);
        for i in 0..num_frames {
            let sample = ((i % 200) as i16).saturating_sub(100); // -100..99 range
            // Left channel
            chunk.extend_from_slice(&sample.to_le_bytes());
            // Right channel
            chunk.extend_from_slice(&sample.to_le_bytes());
        }

        let wav = assemble_wav(&[chunk], sample_rate, channels);

        // Must begin with a valid RIFF/WAVE header
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        // Must contain PCM samples beyond the header
        assert!(wav.len() > 44, "WAV should contain audio data beyond the 44-byte header");
    }

    #[test]
    fn test_resample_48khz_to_16khz_output_length() {
        // 48000 Hz → 16000 Hz: ratio 3:1, so output should be ~1/3 of input samples.
        let src_samples = 4800usize; // 0.1 s at 48kHz
        let data: Vec<u8> = (0..src_samples)
            .flat_map(|i| ((i % 1000) as i16).to_le_bytes())
            .collect();

        let result = resample(&data, 48000, 16000);
        let out_samples = result.len() / 2;
        // Allow ±5 samples tolerance for ceiling rounding in the implementation
        assert!(
            out_samples >= 1595 && out_samples <= 1605,
            "Expected ~1600 output samples from 4800 input at 3:1 ratio, got {}",
            out_samples
        );
    }

    #[test]
    fn test_resample_48khz_to_16khz_silent_stays_silent() {
        // Silent 48kHz input must produce silent 16kHz output (all zeros).
        let data = vec![0u8; 4800 * 2]; // 4800 silent mono samples
        let result = resample(&data, 48000, 16000);
        assert!(!result.is_empty());
        assert!(
            result.iter().all(|&b| b == 0),
            "Resampling silence should produce all-zero output"
        );
    }
}
