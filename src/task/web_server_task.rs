use amqprs::connection::Connection;
use std::sync::Arc;
use tokio::{net::TcpListener, sync::RwLock};

use crate::rabbitmq::RabbitmqConn;
use crate::{parse::video_parse::VideoParse, LabelMapFlag};

pub async fn start_web_server_task(
    addr: &str,
    label_map: Arc<RwLock<LabelMapFlag>>,
    rabbitmq_conn: Arc<RabbitmqConn>,
    conn: Arc<Connection>,
) {
    let listener = TcpListener::bind(&addr).await.unwrap();
    println!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let ip_map = label_map.clone();
        let rabbitmq_conn = rabbitmq_conn.clone();
        let conn = conn.clone();

        tokio::spawn(async move {
            let video = VideoParse::new(None, None, None);
            if let Err(e) = video
                .send_to_web(&mut socket, ip_map, rabbitmq_conn, conn)
                .await
            {
                log::error!("{:?}", e)
            }
        });
    }
}
