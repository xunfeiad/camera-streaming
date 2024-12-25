use ::camera_streaming::{config::Configuration, error::Result};
use camera_streaming::LabelMapQueue;
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

    let label_map = Arc::new(RwLock::new(LabelMapQueue::new()));

    let task_video =
        camera_streaming::task::video_task::start_video_task(&video_server_addr, label_map.clone());

    let task_server = camera_streaming::task::web_server_task::start_web_server_task(
        &web_server_addr,
        label_map.clone(),
    );
    tokio::join!(task_video, task_server);
    Ok(())
}
