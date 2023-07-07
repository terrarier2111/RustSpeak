use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;
use cpal::{BufferSize, ChannelCount, Device, Host, InputCallbackInfo, OutputCallbackInfo, Sample, SampleRate, Stream, StreamConfig, StreamError};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_derive::Deserialize;
use serde_derive::Serialize;
use crate::data_structures::byte_buf_ring::BBRing;

const SAMPLE_RATE: u32 = 44100 / 4; // 44.1kHz

pub struct Audio {
    io_src: AudioIOSource,
    stream_settings: AudioStreamSettings,
    pub buffer: Arc<BBRing>,
}

impl Audio {

    pub fn from_cfg(cfg: &AudioConfig) -> anyhow::Result<Option<Self>> {
        let default_host = cpal::default_host();
        let mut input_device = None;
        let mut output_device = None;
        for device in default_host.devices()?.into_iter() {
            let dev_name = device.name()?;
            let input = dev_name == cfg.input_name;
            let output = dev_name == cfg.output_name;
            if input && output {
                println!("single {}", dev_name);
                return Ok(Some(Self {
                    io_src: AudioIOSource::Single(device),
                    stream_settings: AudioStreamSettings::new(AudioMode::Mono, FrequencyQuality::Low).unwrap(),
                    buffer: Arc::new(BBRing::new(8096)),
                }));
            } else if input {
                input_device = Some(device);
            } else if output {
                output_device = Some(device);
            }
            if input_device.is_some() {
                if let Some(output_dev) = output_device {
                    println!("multi!");
                    return Ok(Some(Audio {
                        io_src: AudioIOSource::Dual { input: input_device.unwrap(), output: output_dev, },
                        stream_settings: AudioStreamSettings::new(AudioMode::Mono, FrequencyQuality::Low).unwrap(),
                        buffer: Arc::new(BBRing::new(8096)),
                    }));
                }
            }
        }
        Ok(None)
    }

    pub fn start_record(&self, handler: impl Fn(&[i16], &InputCallbackInfo) + Send + 'static) -> anyhow::Result<Stream> {
        let (audio_mode, freq_quality) = self.stream_settings.get(); // FIXME: use this and don't always use default!
        let cfg = self.io_src.input().default_input_config()?.into()/*config()*//*StreamConfig {
            channels: <AudioMode as Into<u16>>::into(audio_mode.unwrap()) as ChannelCount,
            sample_rate: SampleRate(44100), // 44.1 khZ
            buffer_size: BufferSize::Fixed(freq_quality.unwrap().into()),
        }*/;
        // StreamConfig { channels: 2, sample_rate: SampleRate(44100), buffer_size: Default }
        // println!("{:?}", cfg);
        let stream = self.io_src.input().build_input_stream(&cfg, handler, |err| {
            panic!("An error occurred while playing back the stream!");
        }, None)?;
        // self.input_stream.store(Some(Arc::new(stream)));
        stream.play()?;

        Ok(stream)
    }

    pub fn play_back(&self, data_callback: impl Fn(&mut [i16], &OutputCallbackInfo) + Send + 'static) -> anyhow::Result<Stream> {
        let (audio_mode, freq_quality) = self.stream_settings.get();
        let cfg = /*self.io_src.output().default_output_config()?.config()*/StreamConfig {
            channels: <AudioMode as Into<u16>>::into(audio_mode.unwrap()) as ChannelCount,
            sample_rate: SampleRate(SAMPLE_RATE),
            buffer_size: BufferSize::Fixed(freq_quality.unwrap().into()),
        };
        let stream = self.io_src.output().build_output_stream(&cfg, data_callback, |err| {
            panic!("An error occurred while playing back the stream!");
        }, None)?;
        stream.play()?;
        println!("playing...");

        Ok(stream)
    }

}

enum AudioIOSource {
    Single(Device),
    Dual {
        input: Device,
        output: Device,
    },
}

impl AudioIOSource {

    fn input(&self) -> &Device {
        match self {
            AudioIOSource::Single(dev) => dev,
            AudioIOSource::Dual { input, .. } => input,
        }
    }

    fn output(&self) -> &Device {
        match self {
            AudioIOSource::Single(dev) => dev,
            AudioIOSource::Dual { input: _input, output } => output,
        }
    }

}

struct AudioStreamSettings(AtomicU64);

impl AudioStreamSettings {

    fn new(audio_mode: AudioMode, freq_quality: FrequencyQuality) -> Option<Self> {
        let audio_mode_inner = match audio_mode {
            AudioMode::Mono => Some(1),
            AudioMode::Stereo => Some(2),
            AudioMode::SurroundSound(channels) => {
                if channels.0 >= 3 {
                    Some(channels.0)
                } else {
                    None
                }
            },
        };
        if let Some(audio_mode) = audio_mode_inner {
            let freq_quality_inner = match freq_quality {
                FrequencyQuality::Low => 512,
                FrequencyQuality::Medium => 256,
                FrequencyQuality::High => 128,
            };
            Some(Self(AtomicU64::new(freq_quality_inner as u64 | ((audio_mode as u64) << 32))))
        } else {
            None
        }
    }

    fn set(&self, audio_mode: AudioMode, freq_quality: FrequencyQuality) -> bool {
        let audio_mode_inner = match audio_mode {
            AudioMode::Mono => Some(1),
            AudioMode::Stereo => Some(2),
            AudioMode::SurroundSound(channels) => {
                if channels.0 >= 3 {
                    Some(channels.0)
                } else {
                    None
                }
            },
        };
        if let Some(audio_mode) = audio_mode_inner {
            let freq_quality_inner = match freq_quality {
                FrequencyQuality::Low => 512,
                FrequencyQuality::Medium => 256,
                FrequencyQuality::High => 128,
            };
            self.0.store(freq_quality_inner as u64 | ((audio_mode as u64) << 32), Ordering::Release);
            true
        } else {
            false
        }
    }

    fn get(&self) -> (Option<AudioMode>, Option<FrequencyQuality>) {
        let inner = self.0.load(Ordering::Acquire);
        let audio_mode_inner = (inner >> 32) as u16;
        let audio_mode = match audio_mode_inner {
            0 => None,
            1 => Some(AudioMode::Mono),
            2 => Some(AudioMode::Stereo),
            _ => Some(AudioMode::SurroundSound(SurroundSoundChannels(audio_mode_inner))),
        };
        let freq_quality_inner = inner as u32;
        let freq_quality = match freq_quality_inner {
            512 => Some(FrequencyQuality::High),
            256 => Some(FrequencyQuality::Medium),
            128 => Some(FrequencyQuality::High),
            _ => None,
        };
        (audio_mode, freq_quality)
    }

}

#[derive(Serialize, Deserialize)]
pub struct AudioConfig {
    pub input_name: String,
    pub output_name: String,
}

impl AudioConfig {

    pub fn new() -> anyhow::Result<Option<Self>> {
        let default_host = cpal::default_host();
        let input = match default_host.default_input_device() {
            None => {
                return Ok(None);
            }
            Some(input) => input.name(),
        }?;
        let output = match default_host.default_output_device() {
            None => {
                return Ok(None);
            }
            Some(output) => output.name(),
        }?;
        Ok(Some(Self {
            input_name: input,
            output_name: output,
        }))
    }

}

#[derive(Copy, Clone, PartialEq)]
pub enum FrequencyQuality {
    Low = 512, // ~12ms
    Medium = 256, // ~6ms
    High = 128, // ~3ms
}

impl Into<u32> for FrequencyQuality {
    fn into(self) -> u32 {
        match self {
            FrequencyQuality::Low => 512,
            FrequencyQuality::Medium => 256,
            FrequencyQuality::High => 128,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum AudioMode {
    Mono/* = 1*/,
    Stereo/* = 2*/,
    SurroundSound(SurroundSoundChannels),
}

impl Into<u16> for AudioMode {
    fn into(self) -> u16 {
        let tmp = match self {
            AudioMode::Mono => 1,
            AudioMode::Stereo => 2,
            AudioMode::SurroundSound(channels) => channels.0,
        };
        tmp as u16
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct SurroundSoundChannels(u16);
