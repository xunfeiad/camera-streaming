use std::sync::Arc;
use tokio::{net::TcpListener, sync::RwLock};

use crate::rabbitmq::RabbitmqConn;
use crate::{error::Result, parse::video_parse::VideoParse, IPMapFlag};

pub async fn start_web_server_task(
    addr: &str,
    ip_map: Arc<RwLock<IPMapFlag>>,
    rabbitmq_conn: Arc<RabbitmqConn>,
) -> Result<()> {
    let listener = TcpListener::bind(&addr).await.unwrap();
    println!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let ip_map = Arc::clone(&ip_map); // 克隆 Arc，避免所有权转移
        let rabbitmq_conn = Arc::clone(&rabbitmq_conn);
        tokio::spawn(async move {
            let video = VideoParse::new(None, None, None);
            video
                .send_to_web(&mut socket, ip_map, rabbitmq_conn)
                .await
                .unwrap();
        });
    }
    Ok(())
}
