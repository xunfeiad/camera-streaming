use ::camera_streaming::{config::Configuration, error::Result, LabelFlagMap, LabelReceiverMap};
use env_logger::Env;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main(worker_threads = 10)]
async fn main() -> Result<()> {
    let env = Env::new()
        .filter_or("Capture", "info")
        .write_style_or("MY_LOG_STYLE", "always");
    env_logger::init_from_env(env);
    let configuration = Arc::new(Configuration::parse_yaml("config.yaml")?);

    let video_server_addr = format!("0.0.0.0:{}", &configuration.send_port);
    let web_server_addr = format!("0.0.0.0:{}", &configuration.report_port);
    let label_flag_map: Arc<LabelFlagMap> = Arc::new(LabelFlagMap::default());
    let label_receiver_map: Arc<RwLock<LabelReceiverMap<Vec<u8>>>> =
        Arc::new(RwLock::new(LabelReceiverMap::default()));
    let task_video = camera_streaming::task::video_task::start_video_task(
        &video_server_addr,
        label_flag_map.clone(),
        label_receiver_map.clone(),
    );

    let task_server = camera_streaming::task::web_server_task::start_web_server_task(
        &web_server_addr,
        label_flag_map,
        label_receiver_map,
    );
    tokio::join!(task_video, task_server);
    Ok(())
}
