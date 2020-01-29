use av_data::frame::{ArcFrame, FrameBufferConv, MediaKind};
use av_data::params::VideoInfo;
use av_data::rational::Rational64;
use flutter_engine::texture_registry::Texture;
use image::{Rgba, RgbaImage};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub struct VideoStream {
    playing: Arc<AtomicBool>,
}

impl VideoStream {
    pub fn play(&self) {
        self.playing.store(true, Ordering::Relaxed);
    }

    pub fn pause(&self) {
        self.playing.store(false, Ordering::Relaxed);
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
        let playing = Arc::new(AtomicBool::new(false));
        let playing2 = playing.clone();
        thread::spawn(move || {
            let mut prev_pts = None;
            let mut now = Instant::now();
            loop {
                if !playing2.load(Ordering::Relaxed) {
                    //println!("not playing sleeping");
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }

                if let Ok(frame) = rx.recv() {
                    //println!("received video frame");
                    let pts = frame.t.pts.unwrap();
                    let timebase = frame.t.timebase.unwrap();
                    //println!("{} {}", pts, timebase);
                    let pts = (Rational64::from_integer(pts * 1000000000) * timebase).to_integer();
                    if let Some(prev) = prev_pts {
                        let elapsed = now.elapsed();
                        if pts > prev {
                            let sleep_time = Duration::new(0, (pts - prev) as u32);
                            if elapsed < sleep_time {
                                //println!("Sleep for {} - {:?}", pts - prev, sleep_time - elapsed);
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
                        let u_stride = frame.buf.linesize(1).unwrap() as usize;
                        let v_plane: &[u8] = frame.buf.as_slice(2).unwrap();
                        let v_stride = frame.buf.linesize(2).unwrap() as usize;
                        //println!("width: {} height: {}", width, height);
                        //println!("y_plane len: {} stride: {}", y_plane.len(), y_stride);
                        //println!("u_plane len: {} stride: {}", u_plane.len(), u_stride);
                        //println!("v_plane len: {} stride: {}", v_plane.len(), v_stride);

                        let img = RgbaImage::from_fn(width as u32, height as u32, |x, y| {
                            let (cx, cy) = (x as usize, y as usize);
                            let y = y_plane[cy * y_stride + cx] as f64;
                            let u = u_plane[cy / 2 * width / 2 + cx / 2] as f64;
                            let v = v_plane[cy / 2 * width / 2 + cx / 2] as f64;
                            let r = 1.164 * (y - 16.0) + 1.596* (v - 128.0);
                            let g = 1.164 * (y - 16.0) - 0.391 * (u - 128.0) - 0.813 * (v - 128.0);
                            let b = 1.164 * (y - 16.0) + 2.018 * (u - 128.0);
                            Rgba([clamp(r), clamp(g), clamp(b), 255])
                        });
                        texture.post_frame_rgba(img);
                    }
                }
            }
        });
        VideoStream { playing }
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
