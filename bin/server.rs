use ::capture::{config::Configuration, error::Result, parse::video_parse};
use capture::rabbitmq::RabbitmqConn;
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

    let ip_map = Arc::new(RwLock::new(video_parse::init_ips()));
    let rabbitmq = Arc::new(RabbitmqConn::new(
        "localhost".to_string(),
        5672,
        "guest".to_string(),
        "guest".to_string(),
    ));

    let conn = Arc::new(rabbitmq.open_connection().await?);
    let task_video = capture::task::video_task::start_video_task(
        &video_server_addr,
        ip_map.clone(),
        rabbitmq.clone(),
        conn.clone()
    );

    let task_server = capture::task::web_server_task::start_web_server_task(
        &web_server_addr,
        ip_map.clone(),
        rabbitmq.clone(),
        conn.clone()
    );
    tokio::join!(task_video, task_server);
    Ok(())
}
