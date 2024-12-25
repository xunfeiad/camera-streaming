use std::sync::Arc;
use tokio::{net::TcpListener, sync::RwLock};

use crate::{parse::video_parse::VideoParse, LabelMapQueue};

pub async fn start_web_server_task(addr: &str, label_map: Arc<RwLock<LabelMapQueue<Vec<u8>>>>) {
    let listener = TcpListener::bind(&addr).await.unwrap();
    println!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let label_map = label_map.clone();

        tokio::spawn(async move {
            let video = VideoParse::new(None, None, None);
            if let Err(e) = video.send_to_web(&mut socket, label_map).await {
                log::error!("{:?}", e)
            }
        });
    }
}
