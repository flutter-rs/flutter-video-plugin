use av_data::frame::{ArcFrame, FrameBufferConv, MediaKind};
use av_data::params::AudioInfo;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Format, SampleFormat, SampleRate, Shape, Stream};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum AudioError {
    NoOutputDevice,
    SupportedFormats(cpal::SupportedFormatsError),
    FormatNotSupported(Format),
    BuildStream(cpal::BuildStreamError),
    PlayStream(cpal::PlayStreamError),
    PauseStream(cpal::PauseStreamError),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let msg = match self {
            Self::NoOutputDevice => "no output device available",
            Self::SupportedFormats(err) => return err.fmt(f),
            Self::FormatNotSupported(format) => {
                return write!(f, "format {:?} not supported", format)
            }
            Self::BuildStream(err) => return err.fmt(f),
            Self::PlayStream(err) => return err.fmt(f),
            Self::PauseStream(err) => return err.fmt(f),
        };
        write!(f, "{}", msg)
    }
}

impl std::error::Error for AudioError {}

impl From<cpal::SupportedFormatsError> for AudioError {
    fn from(error: cpal::SupportedFormatsError) -> Self {
        Self::SupportedFormats(error)
    }
}

impl From<cpal::BuildStreamError> for AudioError {
    fn from(error: cpal::BuildStreamError) -> Self {
        Self::BuildStream(error)
    }
}

impl From<cpal::PlayStreamError> for AudioError {
    fn from(error: cpal::PlayStreamError) -> Self {
        Self::PlayStream(error)
    }
}

impl From<cpal::PauseStreamError> for AudioError {
    fn from(error: cpal::PauseStreamError) -> Self {
        Self::PauseStream(error)
    }
}

pub struct AudioStream {
    stream: Arc<Mutex<Stream>>,
    volume: Arc<Mutex<f64>>,
}

impl AudioStream {
    pub fn play(&self) -> Result<(), AudioError> {
        self.stream.lock().unwrap().play()?;
        Ok(())
    }

    pub fn pause(&self) -> Result<(), AudioError> {
        self.stream.lock().unwrap().pause()?;
        Ok(())
    }

    pub fn set_volume(&self, volume: f64) {
        *self.volume.lock().unwrap() = volume;
    }
}

unsafe impl Send for AudioStream {}
unsafe impl Sync for AudioStream {}

pub struct AudioPlayer {
    device: Device,
    shape: Shape,
}

impl AudioPlayer {
    pub fn new(audio: &AudioInfo) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;
        let format = Format {
            channels: audio.map.as_ref().map(|m| m.len() as _).unwrap_or_default(),
            sample_rate: SampleRate(audio.rate as _),
            data_type: SampleFormat::I16,
        };
        let supported_formats = device.supported_output_formats()?;
        let mut supported = false;
        for supported_format in supported_formats {
            if supported_format.min_sample_rate <= format.sample_rate
                && supported_format.max_sample_rate >= format.sample_rate
                && supported_format.channels == format.channels
                && supported_format.data_type == format.data_type
            {
                supported = true;
                break;
            }
        }
        if !supported {
            return Err(AudioError::FormatNotSupported(format));
        }
        Ok(Self {
            device,
            shape: format.shape(),
        })
    }

    pub fn create_stream(&self, rx: Receiver<ArcFrame>) -> Result<AudioStream, AudioError> {
        let volume = Arc::new(Mutex::new(1.0));
        let volume2 = volume.clone();
        let mut frame = None;
        let mut in_off = 0;
        let stream = self.device.build_output_stream::<i16, _, _>(
            &self.shape,
            move |buffer| {
                let volume = { *volume.lock().unwrap() };
                let mut out_len = buffer.len();
                let mut out_off = 0;
                while out_len > 0 {
                    if frame.is_none() {
                        if let Ok(f) = rx.recv() {
                            frame = Some(f);
                            in_off = 0;
                        }
                    }
                    if let Some(f) = frame.as_ref() {
                        if let MediaKind::Audio(info) = &f.kind {
                            let samples = info.samples * info.map.len();
                            let data: &[i16] = f.buf.as_slice(0).unwrap();
                            let in_len = samples - in_off;
                            let len = out_len.min(in_len);

                            for (out_i, in_i) in (out_off..out_off + len).zip(in_off..in_off + len)
                            {
                                buffer[out_i] = (data[in_i] as f64 * volume) as i16;
                            }

                            in_off += len;
                            out_off += len;
                            out_len -= len;

                            if in_len == len {
                                frame = None;
                            }
                        }
                    }
                }
            },
            |error| {
                eprintln!("{}", error);
            },
        )?;
        Ok(AudioStream {
            stream: Arc::new(Mutex::new(stream)),
            volume: volume2,
        })
    }
}
