use std::sync::Arc;

use mlua::{Lua, Table};
use vibege_audio::{AudioSystem, ChannelKind};

pub fn register_audio_api(
    lua: &Lua,
    audio: &Option<Arc<AudioSystem>>,
) -> Result<Option<Table>, String> {
    let Some(sys) = audio else {
        return Ok(None);
    };

    let audio_table = lua.create_table().map_err(|e| e.to_string())?;

    // Preload default test tones
    sys.load_test_tone("hit", 220.0, 0.08);
    sys.load_test_tone("score", 440.0, 0.15);
    sys.load_test_tone("bounce", 330.0, 0.05);

    let s = Arc::clone(sys);
    audio_table
        .set(
            "play_hit",
            lua.create_function(move |_, ()| {
                let _ = s.play_cached("hit", ChannelKind::Sfx);
                Ok(())
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

    let s2 = Arc::clone(sys);
    audio_table
        .set(
            "play_score",
            lua.create_function(move |_, ()| {
                let _ = s2.play_cached("score", ChannelKind::Sfx);
                Ok(())
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

    let s3 = Arc::clone(sys);
    audio_table
        .set(
            "play_bounce",
            lua.create_function(move |_, ()| {
                let _ = s3.play_cached("bounce", ChannelKind::Sfx);
                Ok(())
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

    let s4 = Arc::clone(sys);
    audio_table
        .set(
            "play",
            lua.create_function(move |_, (key, channel_name): (String, Option<String>)| {
                let channel = channel_name
                    .as_deref()
                    .map(name_to_channel)
                    .unwrap_or(ChannelKind::Sfx);
                let result = s4.play_cached(&key, channel);
                match result {
                    Ok(_) => Ok(()),
                    Err(e) => Err(mlua::Error::external(e.to_string())),
                }
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

    let s5 = Arc::clone(sys);
    audio_table
        .set(
            "set_music_volume",
            lua.create_function(move |_, vol: f32| {
                s5.set_music_volume(vol);
                Ok(())
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

    let s6 = Arc::clone(sys);
    audio_table
        .set(
            "set_sfx_volume",
            lua.create_function(move |_, vol: f32| {
                s6.set_sfx_volume(vol);
                Ok(())
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

    let s7 = Arc::clone(sys);
    audio_table
        .set(
            "set_channel_volume",
            lua.create_function(move |_, (channel_name, vol): (String, f32)| {
                s7.set_channel_volume(name_to_channel(&channel_name), vol);
                Ok(())
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

    let s8 = Arc::clone(sys);
    audio_table
        .set(
            "set_channel_mute",
            lua.create_function(move |_, (channel_name, muted): (String, bool)| {
                s8.set_channel_mute(name_to_channel(&channel_name), muted);
                Ok(())
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

    Ok(Some(audio_table))
}

fn name_to_channel(name: &str) -> ChannelKind {
    match name.to_lowercase().as_str() {
        "master" => ChannelKind::Master,
        "music" => ChannelKind::Music,
        "sfx" => ChannelKind::Sfx,
        "ui" => ChannelKind::Ui,
        "ambient" => ChannelKind::Ambient,
        "voice" => ChannelKind::Voice,
        _ => ChannelKind::Sfx,
    }
}
