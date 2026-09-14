use super::{
    model::{AudioError, Cancellation, VoiceInfo, MAX_AUDIO_BYTES},
    provider::{SpeechSynthesisRequest, TtsProvider},
};
use std::{
    thread,
    time::{Duration, Instant},
};
use windows::{
    core::HSTRING,
    Media::SpeechSynthesis::SpeechSynthesizer,
    Storage::Streams::DataReader,
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};
use windows_future::AsyncStatus;

pub struct Apartment(std::marker::PhantomData<*mut ()>);
impl Apartment {
    pub fn initialize() -> Result<Self, AudioError> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .map_err(|_| AudioError::new("synthesis_unavailable", "无法初始化 Windows 语音服务"))?;
        Ok(Self(std::marker::PhantomData))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}

pub struct WindowsTtsProvider;
fn native_error(_: windows::core::Error) -> AudioError {
    AudioError::new(
        "synthesis_failed",
        "Windows 语音服务调用失败，请检查已安装的语音包",
    )
}

impl WindowsTtsProvider {
    pub fn voices() -> Result<Vec<VoiceInfo>, AudioError> {
        let _apartment = Apartment::initialize()?;
        let voices = SpeechSynthesizer::AllVoices().map_err(native_error)?;
        let default = SpeechSynthesizer::DefaultVoice()
            .map_err(native_error)?
            .Id()
            .map_err(native_error)?;
        let mut list = Vec::new();
        for voice in voices {
            let id = voice.Id().map_err(native_error)?;
            list.push(VoiceInfo {
                id: id.to_string(),
                name: voice.DisplayName().map_err(native_error)?.to_string(),
                language: voice.Language().map_err(native_error)?.to_string(),
                is_default: id == default,
            });
        }
        Ok(list)
    }
}

impl TtsProvider for WindowsTtsProvider {
    fn synthesize(
        &self,
        request: &SpeechSynthesisRequest,
        cancel: &Cancellation,
        deadline: Instant,
    ) -> Result<Vec<u8>, AudioError> {
        let _apartment = Apartment::initialize()?;
        cancel.check()?;
        let synth = SpeechSynthesizer::new().map_err(native_error)?;
        let result = (|| {
            let voices = SpeechSynthesizer::AllVoices().map_err(native_error)?;
            let requested = request.voice_id.as_deref().filter(|id| !id.is_empty());
            let language = request.language.as_deref().unwrap_or("zh-CN");
            let mut selected = None;
            for voice in voices {
                if let Some(id) = requested {
                    if voice.Id().map_err(native_error)?.to_string() == id {
                        selected = Some(voice);
                        break;
                    }
                } else if voice
                    .Language()
                    .map_err(native_error)?
                    .to_string()
                    .eq_ignore_ascii_case(language)
                {
                    selected = Some(voice);
                    break;
                }
            }
            let voice = selected.ok_or_else(|| {
                AudioError::new(
                    "voice_unavailable",
                    "没有可用的所选音色或语言，请在声音设置中选择已安装音色",
                )
            })?;
            synth.SetVoice(&voice).map_err(native_error)?;
            let options = synth.Options().map_err(native_error)?;
            options
                .SetSpeakingRate(request.rate.unwrap_or(1.0) as f64)
                .map_err(native_error)?;
            options
                .SetAudioPitch(request.pitch.unwrap_or(1.0) as f64)
                .map_err(native_error)?;
            options.SetAudioVolume(1.0).map_err(native_error)?;
            let operation = synth
                .SynthesizeTextToStreamAsync(&HSTRING::from(&request.text))
                .map_err(native_error)?;
            while operation.Status().map_err(native_error)? == AsyncStatus::Started {
                if cancel.is_cancelled() || Instant::now() >= deadline {
                    let _ = operation.Cancel();
                    return Err(if cancel.is_cancelled() {
                        AudioError::cancelled()
                    } else {
                        AudioError::new("synthesis_timeout", "语音合成超时")
                    });
                }
                thread::sleep(Duration::from_millis(10));
            }
            cancel.check()?;
            let stream = operation.GetResults().map_err(native_error)?;
            let data_result = (|| {
                let length = stream.Size().map_err(native_error)?;
                if length == 0 || length > MAX_AUDIO_BYTES as u64 {
                    return Err(AudioError::new(
                        "audio_limit_exceeded",
                        "合成音频大小超出限制",
                    ));
                }
                let input = stream.GetInputStreamAt(0).map_err(native_error)?;
                let reader = DataReader::CreateDataReader(&input).map_err(native_error)?;
                let load = reader.LoadAsync(length as u32).map_err(native_error)?;
                while load.Status().map_err(native_error)? == AsyncStatus::Started {
                    if cancel.is_cancelled() || Instant::now() >= deadline {
                        let _ = load.Cancel();
                        return Err(if cancel.is_cancelled() {
                            AudioError::cancelled()
                        } else {
                            AudioError::new("synthesis_timeout", "读取语音超时")
                        });
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                cancel.check()?;
                if load.GetResults().map_err(native_error)? != length as u32 {
                    return Err(AudioError::new("unsupported_audio", "语音数据不完整"));
                }
                let mut data = vec![0; length as usize];
                reader.ReadBytes(&mut data).map_err(native_error)?;
                let _ = reader.Close();
                Ok(data)
            })();
            let _ = stream.Close();
            data_result
        })();
        let _ = synth.Close();
        result
    }
}
