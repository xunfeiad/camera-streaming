use std::sync::Arc;
use amqprs::connection::Connection;

use tokio::{
    net::TcpListener,
    sync::RwLock,
};

use crate::{parse::video_parse::VideoParse, rabbitmq::RabbitmqConn, IPMapFlag};

pub async fn start_video_task(
    addr: &str,
    ip_map: Arc<RwLock<IPMapFlag>>,
    rabbitmq: Arc<RabbitmqConn>,
    conn: Arc<Connection>
) {
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();

        let ip_map = ip_map.clone();
        let rabbitmq = rabbitmq.clone();
        let conn = conn.clone();

        tokio::spawn(async move {
            let video = VideoParse::new(None, None, None);
            video
                .decode(
                    &mut socket,
                    ip_map,
                    rabbitmq,
                    conn
                )
                .await
                .unwrap();
        });
    }
}
