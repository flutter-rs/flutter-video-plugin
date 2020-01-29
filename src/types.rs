#![allow(dead_code)]
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateArgs {
    pub uri: Option<String>,
    pub format_hint: Option<VideoFormat>,
    pub asset: Option<String>,
    pub package: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VideoFormat {
    Dash,
    Hls,
    Ss,
    Other,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureIdArgs {
    pub texture_id: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLoopingArgs {
    pub texture_id: i64,
    pub looping: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetVolumeArgs {
    pub texture_id: i64,
    pub volume: f64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeekToArgs {
    pub texture_id: i64,
    pub location: i64,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoEvent {
    event: VideoEventType,
    width: Option<i64>,
    height: Option<i64>,
    duration: Option<i64>,
    values: Option<Vec<(i64, i64)>>,
}

impl VideoEvent {
    pub fn initialized(width: i64, height: i64, duration: i64) -> Self {
        Self {
            event: VideoEventType::Initialized,
            width: Some(width),
            height: Some(height),
            duration: Some(duration),
            ..Default::default()
        }
    }

    pub fn completed() -> Self {
        Self {
            event: VideoEventType::Completed,
            ..Default::default()
        }
    }

    pub fn buffering_update(values: Vec<(i64, i64)>) -> Self {
        Self {
            event: VideoEventType::BufferingUpdate,
            values: Some(values),
            ..Default::default()
        }
    }

    pub fn buffering_start() -> Self {
        Self {
            event: VideoEventType::BufferingStart,
            ..Default::default()
        }
    }

    pub fn buffering_end() -> Self {
        Self {
            event: VideoEventType::BufferingEnd,
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VideoEventType {
    Initialized,
    Completed,
    BufferingUpdate,
    BufferingStart,
    BufferingEnd,
    Unknown,
}

impl Default for VideoEventType {
    fn default() -> Self {
        Self::Unknown
    }
}
