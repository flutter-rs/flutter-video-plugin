use crate::audio::{AudioPlayer, AudioStream};
use crate::video::VideoPlayer;
use av_codec::common::CodecList;
use av_codec::decoder::Codecs as DecCodecs;
use av_codec::decoder::Context as DecContext;
use av_data::frame::ArcFrame;
pub use av_data::frame::MediaKind;
use av_data::params;
use av_format::buffer::AccReader;
use av_format::demuxer::*;
use av_vorbis::decoder::VORBIS_DESCR;
use flutter_engine::texture_registry::Texture;
use flutter_plugins::prelude::*;
use libopus::decoder::OPUS_DESCR;
use libvpx::decoder::VP9_DESCR;
use matroska::demuxer::MkvDemuxer;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicI8, Ordering};
use std::sync::mpsc;
use std::thread;

#[derive(Debug)]
pub enum PlayerError {
    Format(av_format::error::Error),
    Codec(av_codec::error::Error),
    Audio(crate::audio::AudioError),
    Io(std::io::Error),
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Format(err) => err.fmt(f),
            Self::Codec(err) => err.fmt(f),
            Self::Audio(err) => err.fmt(f),
            Self::Io(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for PlayerError {}

impl From<av_format::error::Error> for PlayerError {
    fn from(error: av_format::error::Error) -> Self {
        Self::Format(error)
    }
}

impl From<av_codec::error::Error> for PlayerError {
    fn from(error: av_codec::error::Error) -> Self {
        Self::Codec(error)
    }
}
impl From<crate::audio::AudioError> for PlayerError {
    fn from(error: crate::audio::AudioError) -> Self {
        Self::Audio(error)
    }
}

impl From<std::io::Error> for PlayerError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<PlayerError> for MethodCallError {
    fn from(error: PlayerError) -> Self {
        MethodCallError::from_error(error)
    }
}

struct PlaybackContext {
    decoders: HashMap<isize, DecContext>,
    demuxer: Context,
    pub video: Option<params::VideoInfo>,
    pub audio: Option<params::AudioInfo>,
}

impl PlaybackContext {
    pub fn from_path(path: &Path) -> Result<Self, PlayerError> {
        let r = File::open(path)?;
        let ar = AccReader::with_capacity(4 * 1024, r);

        let mut c = Context::new(Box::new(MkvDemuxer::new()), Box::new(ar));

        c.read_headers()?;

        let decoders = DecCodecs::from_list(&[VP9_DESCR, OPUS_DESCR, VORBIS_DESCR]);

        let mut video_info = None;
        let mut audio_info = None;
        let mut decs: HashMap<isize, DecContext> = HashMap::with_capacity(2);
        for st in &c.info.streams {
            // TODO stream selection
            if let Some(ref codec_id) = st.params.codec_id {
                if let Some(mut ctx) = DecContext::by_name(&decoders, codec_id) {
                    if let Some(ref extradata) = st.params.extradata {
                        ctx.set_extradata(extradata);
                    }
                    ctx.configure()?;
                    decs.insert(st.index as isize, ctx);
                    match st.params.kind {
                        Some(params::MediaKind::Video(ref info)) => {
                            video_info = Some(info.clone());
                        }
                        Some(params::MediaKind::Audio(ref info)) => {
                            audio_info = Some(info.clone());
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(Self {
            decoders: decs,
            demuxer: c,
            video: video_info,
            audio: audio_info,
        })
    }

    pub fn decode_one(&mut self) -> Result<Option<ArcFrame>, PlayerError> {
        match self.demuxer.read_event()? {
            Event::NewPacket(pkt) => {
                if let Some(dec) = self.decoders.get_mut(&pkt.stream_index) {
                    dec.send_packet(&pkt)?;
                    Ok(dec.receive_frame().ok())
                } else {
                    println!("Skipping packet at index {}", pkt.stream_index);
                    Ok(None)
                }
            }
            Event::Eof => Ok(None),
            event => {
                println!("Unsupported event {:?}", event);
                unimplemented!();
            }
        }
    }
}

pub struct Player {
    audio: Option<AudioStream>,
    width: i64,
    height: i64,
    state: Arc<AtomicI8>,
}

impl Drop for Player {
    fn drop(&mut self) {
        self.state.store(-1, Ordering::Relaxed);
    }
}

impl Player {
    pub fn from_path(path: &Path, texture: Texture) -> Result<Self, PlayerError> {
        let mut context = PlaybackContext::from_path(path)?;
        let (v_s, v_r) = mpsc::sync_channel(24);
        let (a_s, a_r) = mpsc::channel();

        let state = Arc::new(AtomicI8::new(0));
        let state_c1 = state.clone();
        let state_c2 = state.clone();
        let audio_info = context.audio.take().expect("audio channel");
        let audio = AudioPlayer::new(&audio_info)?;
        let audio_stream = audio.create_stream(a_r)?;

        let video_info = context.video.take().expect("video channel");
        let video = VideoPlayer::new(&video_info, texture);
        video.create_stream(v_r, state_c2);

        // decoder task
        thread::spawn(move || loop {
            if state_c1.load(Ordering::Relaxed) < 0 {
                state_c1.store(-2, Ordering::Relaxed);
                break;
            }
            if let Ok(Some(frame)) = context.decode_one() {
                match frame.kind {
                    MediaKind::Video(_) => {
                        if let Err(err) = v_s.send(frame) {
                            eprintln!("Thread#{:?}:Video {}", thread::current().id(), err);
                        }
                    }
                    MediaKind::Audio(_) => {
                        if let Err(err) = a_s.send(frame) {
                            eprintln!("Thread#{:?}:Audio {}", thread::current().id(), err);
                        }
                    }
                }
            }
        });

        Ok(Self {
            audio: Some(audio_stream),
            width: video_info.width as _,
            height: video_info.height as _,
            state,
        })
    }

    pub fn width(&self) -> i64 {
        self.width
    }

    pub fn height(&self) -> i64 {
        self.height
    }

    pub fn play(&self) -> Result<(), PlayerError> {
        if let Some(audio) = &self.audio {
            audio.play()?;
        }
        self.state.store(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn pause(&self) -> Result<(), PlayerError> {
        if let Some(audio) = &self.audio {
            audio.pause()?;
        }
        self.state.store(0, Ordering::Relaxed);
        Ok(())
    }

    pub fn position(&self) -> i64 {
        0
    }

    pub fn seek_to(&self, _location: i64) {}

    pub fn set_volume(&self, volume: f64) {
        if let Some(stream) = &self.audio {
            stream.set_volume(volume);
        }
    }

    pub fn set_looping(&self, _looping: bool) {}
}
