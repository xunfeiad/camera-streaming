use camera_streaming::{
    config::Configuration,
    error::Result,
    task::{encode_audio_task, encode_video_task},
    IsEnd,
};
use clap::Parser;
use std::sync::Arc;
use tokio::runtime::Handle;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_line_number(true).init();
    info!(
        server = "Camera-Stream-Client",
        "Preparing to start server."
    );
    let cli: Arc<Configuration> = Arc::new(Configuration::parse());
    let is_end = Arc::new(IsEnd::new(false));
    let video_config = cli.clone();

    let is_end_video = is_end.clone();
    let video_task = tokio::spawn(async move {
        if let Err(e) = encode_video_task(video_config, is_end_video).await {
            error!("Error occurred when encode video data: {e}");
        }
    });
    let audio_config = cli.clone();

    let is_end_audio = is_end.clone();

    let audio_task = tokio::spawn(async move {
        if let Err(e) = encode_audio_task(audio_config, Handle::current(), is_end_audio).await {
            error!("Error occurred when encode video data: {e}");
        }
    });

    let (video_res, audio_res) = tokio::join!(video_task, audio_task);
    match video_res {
        Ok(_) => {}
        Err(e) => {
            error!(Error = "Video Encode", "{}", e.to_string())
        }
    }
    match audio_res {
        Ok(_) => {}
        Err(e) => {
            error!(Error = "Audio Encode", "{}", e.to_string())
        }
    }
    Ok(())
}
