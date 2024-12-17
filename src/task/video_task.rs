use std::sync::Arc;

use tokio::{
    net::TcpListener,
    sync::RwLock,
};

use crate::{error::Result, parse::video_parse::VideoParse, rabbitmq::RabbitmqConn, IPMapFlag};

pub async fn start_video_task(
    addr: &str,
    ip_map: Arc<RwLock<IPMapFlag>>,
    rabbitmq: Arc<RabbitmqConn>,
) -> Result<()> {
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();

        let ip_map = Arc::clone(&ip_map);
        let rabbitmq = rabbitmq.clone();

        tokio::spawn(async move {
            let video = VideoParse::new(None, None, None);
            video
                .decode(
                    &mut socket,
                    ip_map,
                    rabbitmq,
                )
                .await
                .unwrap();
        });
    }
}
