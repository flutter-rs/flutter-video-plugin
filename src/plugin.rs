use crate::player::Player;
use crate::types::*;
use flutter_plugins::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

const PLUGIN_NAME: &str = module_path!();
const CHANNEL_NAME: &str = "flutter.io/videoPlayer";

#[derive(Debug)]
struct InvalidTextureId;

impl std::fmt::Display for InvalidTextureId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "invalid texture id")
    }
}

impl std::error::Error for InvalidTextureId {}

impl From<InvalidTextureId> for MethodCallError {
    fn from(error: InvalidTextureId) -> Self {
        MethodCallError::from_error(error)
    }
}

#[derive(Default)]
pub struct VideoPlugin {
    handler: Arc<RwLock<Handler>>,
}

#[derive(Default)]
struct Handler {
    streams: HashMap<i64, Arc<RwLock<StreamHandler>>>,
}

impl Plugin for VideoPlugin {
    fn plugin_name() -> &'static str {
        PLUGIN_NAME
    }

    fn init_channels(&mut self, registrar: &mut ChannelRegistrar) {
        let method_handler = Arc::downgrade(&self.handler);
        registrar.register_channel(StandardMethodChannel::new(CHANNEL_NAME, method_handler));
    }
}

impl MethodCallHandler for Handler {
    fn on_method_call(
        &mut self,
        call: MethodCall,
        engine: FlutterEngine,
    ) -> Result<Value, MethodCallError> {
        match call.method.as_str() {
            "create" => {
                let args: CreateArgs = from_value(&call.args)?;

                // create texture
                let texture = engine.create_texture();
                let texture_id = texture.id();

                // create player
                let player = if let Some(asset) = args.asset.as_ref() {
                    let path = engine.assets().join(asset);
                    Player::from_path(&path, texture)?
                } else {
                    unimplemented!();
                };

                // register channel
                let channel = format!("{}/videoEvents{}", CHANNEL_NAME, texture_id);
                let handler = Arc::new(RwLock::new(StreamHandler::new(channel.clone(), player)));
                let stream_handler = Arc::downgrade(&handler);
                self.streams.insert(texture_id, handler);
                engine.with_channel_registrar(PLUGIN_NAME, |registrar| {
                    registrar.register_channel(EventChannel::new(channel, stream_handler));
                });

                Ok(to_value(TextureIdArgs { texture_id })?)
            }
            "init" => Ok(Value::Null),
            "setLooping" => {
                let args: SetLoopingArgs = from_value(&call.args)?;
                let stream = self.streams.get(&args.texture_id).ok_or(InvalidTextureId)?;
                stream.read().unwrap().player.set_looping(args.looping);
                Ok(Value::Null)
            }
            "setVolume" => {
                let args: SetVolumeArgs = from_value(&call.args)?;
                let stream = self.streams.get(&args.texture_id).ok_or(InvalidTextureId)?;
                stream.read().unwrap().player.set_volume(args.volume);
                Ok(Value::Null)
            }
            "pause" => {
                let args: TextureIdArgs = from_value(&call.args)?;
                let stream = self.streams.get(&args.texture_id).ok_or(InvalidTextureId)?;
                stream.read().unwrap().player.pause()?;
                Ok(Value::Null)
            }
            "play" => {
                let args: TextureIdArgs = from_value(&call.args)?;
                let stream = self.streams.get(&args.texture_id).ok_or(InvalidTextureId)?;
                stream.read().unwrap().player.play()?;
                Ok(Value::Null)
            }
            "position" => {
                let args: TextureIdArgs = from_value(&call.args)?;
                let stream = self.streams.get(&args.texture_id).ok_or(InvalidTextureId)?;
                let position = stream.read().unwrap().player.position();
                Ok(Value::I64(position))
            }
            "seekTo" => {
                let args: SeekToArgs = from_value(&call.args)?;
                let stream = self.streams.get(&args.texture_id).ok_or(InvalidTextureId)?;
                stream.read().unwrap().player.seek_to(args.location);
                Ok(Value::Null)
            }
            "dispose" => {
                let args: TextureIdArgs = from_value(&call.args)?;
                let texture_id = &args.texture_id;
                let channel = format!("{}/videoEvents{}", CHANNEL_NAME, texture_id);
                self.streams.remove(texture_id).ok_or(InvalidTextureId)?;
                engine.remove_channel(&channel);
                Ok(Value::Null)
            }
            _ => Err(MethodCallError::NotImplemented),
        }
    }
}

struct StreamHandler {
    channel: String,
    player: Player,
}

impl StreamHandler {
    fn new(channel: String, player: Player) -> Self {
        Self { channel, player }
    }
}

impl EventHandler for StreamHandler {
    fn on_listen(
        &mut self,
        _value: Value,
        engine: FlutterEngine,
    ) -> Result<Value, MethodCallError> {
        let channel_name = self.channel.clone();
        let width = self.player.width();
        let height = self.player.height();
        engine.run_on_platform_thread(move |engine| {
            engine.with_channel(&channel_name, move |channel| {
                if let Some(channel) = channel.try_as_method_channel() {
                    let value = to_value(VideoEvent::initialized(width, height, 1)).unwrap();
                    channel.send_success_event(&value);
                }
            });
        });
        Ok(Value::Null)
    }

    fn on_cancel(&mut self, _engine: FlutterEngine) -> Result<Value, MethodCallError> {
        Ok(Value::Null)
    }
}
