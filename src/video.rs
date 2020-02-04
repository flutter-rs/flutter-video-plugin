#![allow(clippy::many_single_char_names)]
use av_data::frame::{ArcFrame, FrameBufferConv, MediaKind};
use av_data::params::VideoInfo;
use av_data::rational::Rational64;
use crossbeam::atomic::AtomicCell;
use flutter_engine::texture_registry::Texture;
use image::{Rgba, RgbaImage};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerState {
    Playing,
    Paused,
    Stopped,
}

pub struct VideoStream {
    state: Arc<AtomicCell<PlayerState>>,
}

impl VideoStream {
    pub fn play(&self) {
        self.state.store(PlayerState::Playing);
    }

    pub fn pause(&self) {
        self.state.store(PlayerState::Paused);
    }
}

impl Drop for VideoStream {
    fn drop(&mut self) {
        self.state.store(PlayerState::Stopped);
    }
}

pub struct VideoPlayer {
    width: usize,
    height: usize,
    texture: Texture,
}

impl VideoPlayer {
    pub fn new(info: &VideoInfo, texture: Texture) -> Self {
        let width = info.width;
        let height = info.height;
        Self {
            width,
            height,
            texture,
        }
    }

    pub fn create_stream(self, rx: Receiver<ArcFrame>) -> VideoStream {
        let width = self.width;
        let height = self.height;
        let texture = self.texture;
        let state = Arc::new(AtomicCell::new(PlayerState::Paused));
        let state2 = state.clone();
        thread::spawn(move || {
            let mut prev_pts = None;
            let mut now = Instant::now();
            loop {
                match state2.load() {
                    PlayerState::Playing => {}
                    PlayerState::Paused => {
                        thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                    PlayerState::Stopped => break,
                }

                if let Ok(frame) = rx.recv() {
                    let pts = frame.t.pts.unwrap();
                    let timebase = frame.t.timebase.unwrap();
                    let pts = Rational64::from_integer(pts * 1_000_000_000);
                    let pts = (pts * timebase).to_integer();
                    if let Some(prev) = prev_pts {
                        let elapsed = now.elapsed();
                        if pts > prev {
                            let sleep_time = Duration::new(0, (pts - prev) as u32);
                            if elapsed < sleep_time {
                                log::trace!(
                                    "Sleep for {} - {:?}",
                                    pts - prev,
                                    sleep_time - elapsed
                                );
                                thread::sleep(sleep_time - elapsed);
                            }
                        }
                    }
                    now = Instant::now();
                    prev_pts = Some(pts);

                    if let MediaKind::Video(_) = frame.kind {
                        let y_plane: &[u8] = frame.buf.as_slice(0).unwrap();
                        let y_stride = frame.buf.linesize(0).unwrap() as usize;
                        let u_plane: &[u8] = frame.buf.as_slice(1).unwrap();
                        //let u_stride = frame.buf.linesize(1).unwrap() as usize;
                        let v_plane: &[u8] = frame.buf.as_slice(2).unwrap();
                        //let v_stride = frame.buf.linesize(2).unwrap() as usize;

                        let img = RgbaImage::from_fn(width as u32, height as u32, |x, y| {
                            let (cx, cy) = (x as usize, y as usize);
                            let y = y_plane[cy * y_stride + cx] as f64;
                            let u = u_plane[cy / 2 * width / 2 + cx / 2] as f64;
                            let v = v_plane[cy / 2 * width / 2 + cx / 2] as f64;
                            let r = 1.164 * (y - 16.0) + 1.596 * (v - 128.0);
                            let g = 1.164 * (y - 16.0) - 0.391 * (u - 128.0) - 0.813 * (v - 128.0);
                            let b = 1.164 * (y - 16.0) + 2.018 * (u - 128.0);
                            Rgba([clamp(r), clamp(g), clamp(b), 255])
                        });
                        texture.post_frame_rgba(img);
                    }
                }
            }
        });
        VideoStream { state }
    }
}

fn clamp(value: f64) -> u8 {
    if value <= 0.0 {
        return 0;
    }
    if value >= 255.0 {
        return 255;
    }
    value as u8
}
