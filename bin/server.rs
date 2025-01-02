use camera_streaming::{config::Configuration, error::Result, LabelFlagMap, LabelReceiverMap};
use std::sync::Arc;
use tracing::info;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_line_number(true).init();
    info!(
        server = "Camera-Stream-Server",
        "Preparing to start server."
    );

    let configuration = Arc::new(Configuration::parse_yaml("config.yaml")?);

    let video_receiver_addr = format!("0.0.0.0:{}", &configuration.video_receiver_port);
    let video_server_addr = format!("0.0.0.0:{}", &configuration.video_server_port);
    let audio_receiver_addr = format!("0.0.0.0:{}", &configuration.audio_receiver_port);
    let audio_server_addr = format!("0.0.0.0:{}", &configuration.audio_server_port);
    let label_flag_map: Arc<LabelFlagMap> = Arc::new(LabelFlagMap::default());
    let label_receiver_map: Arc<LabelReceiverMap> = Arc::new(LabelReceiverMap::default());
    let video_decode_task = camera_streaming::task::decode_video_task(
        &video_receiver_addr,
        label_flag_map.clone(),
        label_receiver_map.clone(),
    );

    let audio_decode_task = camera_streaming::task::decode_audio_task(
        &audio_receiver_addr,
        label_flag_map.clone(),
        label_receiver_map.clone(),
    );

    let video_web_task = camera_streaming::task::start_video_web_server_task(
        &video_server_addr,
        label_flag_map.clone(),
        label_receiver_map.clone(),
    );

    let audio_web_task = camera_streaming::task::start_audio_web_server_task(
        &audio_server_addr,
        label_flag_map,
        label_receiver_map,
    );
    tokio::join!(
        video_decode_task,
        audio_decode_task,
        video_web_task,
        audio_web_task
    );
    Ok(())
}
