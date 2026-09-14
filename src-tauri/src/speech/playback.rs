use super::model::{AudioError, Cancellation, MAX_AUDIO_BYTES, MAX_DURATION_SECONDS};
use std::io::Cursor;

pub struct PcmAudio {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}
impl PcmAudio {
    pub fn duration_seconds(&self) -> f64 {
        self.samples.len() as f64 / self.channels as f64 / self.sample_rate as f64
    }
}

pub fn decode_wav(audio: Vec<u8>, cancel: &Cancellation) -> Result<PcmAudio, AudioError> {
    if audio.len() > MAX_AUDIO_BYTES {
        return Err(AudioError::new("audio_limit_exceeded", "音频超过内存限制"));
    }
    let invalid = || AudioError::new("unsupported_audio", "无效或不支持的 WAV 音频");
    let mut reader = hound::WavReader::new(Cursor::new(audio)).map_err(|_| invalid())?;
    let spec = reader.spec();
    if !(1..=2).contains(&spec.channels)
        || !(8000..=96000).contains(&spec.sample_rate)
        || !(1..=32).contains(&spec.bits_per_sample)
    {
        return Err(invalid());
    }
    let length = reader.len() as usize;
    if length == 0
        || length > MAX_AUDIO_BYTES / 4
        || length as f64 / spec.channels as f64 / spec.sample_rate as f64 > MAX_DURATION_SECONDS
    {
        return Err(AudioError::new(
            "audio_limit_exceeded",
            "音频时长或解码大小超出限制",
        ));
    }
    let mut samples = Vec::with_capacity(length);
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for value in reader.samples::<f32>() {
                cancel.check()?;
                let v = value.map_err(|_| invalid())?;
                if !v.is_finite() {
                    return Err(invalid());
                }
                samples.push(v.clamp(-1.0, 1.0));
            }
        }
        hound::SampleFormat::Int => {
            let divisor = 2_f64.powi(spec.bits_per_sample as i32 - 1);
            for value in reader.samples::<i32>() {
                cancel.check()?;
                samples.push((value.map_err(|_| invalid())? as f64 / divisor) as f32);
            }
        }
    }
    if samples.len() != length || length % spec.channels as usize != 0 {
        return Err(invalid());
    }
    Ok(PcmAudio {
        samples,
        channels: spec.channels,
        sample_rate: spec.sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wav(samples: &[i16]) -> Vec<u8> {
        let mut data = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(
                &mut data,
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 16000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for sample in samples {
                writer.write_sample(*sample).unwrap();
            }
            writer.finalize().unwrap();
        }
        data.into_inner()
    }
    #[test]
    fn decode_pcm_and_reject_truncated_data() {
        let audio = decode_wav(wav(&[0, 16384, -16384]), &Cancellation::default()).unwrap();
        assert_eq!(audio.samples, vec![0.0, 0.5, -0.5]);
        assert_eq!(audio.sample_rate, 16000);
        assert_eq!(audio.channels, 1);
        assert!(decode_wav(b"ID3-not-a-wav".to_vec(), &Cancellation::default()).is_err());
        let mut broken = wav(&[1, 2, 3]);
        broken.truncate(12);
        assert!(decode_wav(broken, &Cancellation::default()).is_err());
    }
    #[test]
    fn cancellation_stops_decode_wav() {
        let cancel = Cancellation::default();
        cancel.cancel();
        assert_eq!(
            decode_wav(wav(&[1, 2]), &cancel).err().unwrap().code,
            "cancelled"
        );
    }
    #[test]
    fn oversized_wav_header_is_rejected_before_pcm_allocation() {
        let mut audio = wav(&[1, 2]);
        let offset = audio.windows(4).position(|w| w == b"data").unwrap() + 4;
        audio[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_wav(audio, &Cancellation::default()).is_err());
    }
}
