use amqprs::connection::Connection;
use std::sync::Arc;

use tokio::{net::TcpListener, sync::RwLock};

use crate::{parse::video_parse::VideoParse, rabbitmq::RabbitmqConn, LabelMapFlag};

pub async fn start_video_task(
    addr: &str,
    label_map: Arc<RwLock<LabelMapFlag>>,
    rabbitmq: Arc<RabbitmqConn>,
    conn: Arc<Connection>,
) {
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();

        let ip_map = label_map.clone();
        let rabbitmq = rabbitmq.clone();
        let conn = conn.clone();

        tokio::spawn(async move {
            let video = VideoParse::new(None, None, None);
            if let Err(e) = video.decode(&mut socket, ip_map, rabbitmq, conn).await {
                log::error!("{:?}", e);
            }
        });
    }
}
