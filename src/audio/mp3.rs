use mp3lame_encoder::{Bitrate, Builder, FlushNoGap, MonoPcm, Quality};

use super::TARGET_SAMPLE_RATE;

/// Bitrate used for uploads. Speech at 16 kHz mono stays intelligible here,
/// and it keeps a 25-minute chunk around 6 MB — well inside the ~20 MB cap on
/// a single OpenRouter request.
const BITRATE: Bitrate = Bitrate::Kbps32;

/// Bytes per second of encoded audio at [`BITRATE`], used to check that a
/// chunk stays inside the per-request size limit.
#[allow(dead_code)]
pub const BYTES_PER_SECOND: usize = 32 * 1000 / 8;

/// Encode 16 kHz mono PCM to MP3.
pub fn encode_mono_16k(samples: &[i16]) -> Result<Vec<u8>, String> {
    let mut encoder = Builder::new()
        .ok_or("Failed to allocate MP3 encoder")?
        .with_num_channels(1)
        .map_err(|e| format!("MP3 encoder rejected channel count: {e:?}"))?
        .with_sample_rate(TARGET_SAMPLE_RATE)
        .map_err(|e| format!("MP3 encoder rejected sample rate: {e:?}"))?
        .with_brate(BITRATE)
        .map_err(|e| format!("MP3 encoder rejected bitrate: {e:?}"))?
        .with_quality(Quality::Good)
        .map_err(|e| format!("MP3 encoder rejected quality: {e:?}"))?
        .build()
        .map_err(|e| format!("Failed to initialize MP3 encoder: {e:?}"))?;

    let mut out = Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(samples.len()));
    encoder
        .encode_to_vec(MonoPcm(samples), &mut out)
        .map_err(|e| format!("MP3 encoding failed: {e:?}"))?;
    encoder
        .flush_to_vec::<FlushNoGap>(&mut out)
        .map_err(|e| format!("MP3 flush failed: {e:?}"))?;

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 440 Hz tone at 16 kHz, `seconds` long.
    fn tone(seconds: usize) -> Vec<i16> {
        (0..TARGET_SAMPLE_RATE as usize * seconds)
            .map(|i| {
                let t = i as f32 / TARGET_SAMPLE_RATE as f32;
                ((t * 440.0 * std::f32::consts::TAU).sin() * 8000.0) as i16
            })
            .collect()
    }

    #[test]
    fn test_encodes_to_mp3_frames() {
        let mp3 = encode_mono_16k(&tone(1)).unwrap();
        assert!(!mp3.is_empty());
        // MPEG audio frames start with 11 set sync bits.
        assert_eq!(mp3[0], 0xFF);
        assert_eq!(mp3[1] & 0xE0, 0xE0);
    }

    #[test]
    fn test_compression_ratio_is_near_bitrate() {
        let seconds = 4;
        let mp3 = encode_mono_16k(&tone(seconds)).unwrap();
        let expected = BYTES_PER_SECOND * seconds;
        // Allow generous slack for frame padding and the flush tail.
        assert!(
            mp3.len() < expected * 2,
            "expected roughly {expected} bytes, got {}",
            mp3.len()
        );
    }

    #[test]
    fn test_empty_input_is_not_an_error() {
        assert!(encode_mono_16k(&[]).is_ok());
    }
}
