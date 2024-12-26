use ::camera_streaming::{config::Configuration, error::Result};
use camera_streaming::rabbitmq::RabbitmqConn;
use env_logger::Env;
use std::collections::HashMap;
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

    let label_map = Arc::new(RwLock::new(HashMap::new()));
    let rabbitmq = Arc::new(RabbitmqConn::new(
        "localhost".to_string(),
        5672,
        "guest".to_string(),
        "guest".to_string(),
    ));

    let conn = Arc::new(rabbitmq.open_connection().await?);
    let task_video = camera_streaming::task::video_task::start_video_task(
        &video_server_addr,
        label_map.clone(),
        rabbitmq.clone(),
        conn.clone(),
    );

    let task_server = camera_streaming::task::web_server_task::start_web_server_task(
        &web_server_addr,
        label_map.clone(),
        rabbitmq.clone(),
        conn.clone(),
    );
    tokio::join!(task_video, task_server);
    Ok(())
}
