use crate::{parse::video_parse::VideoParse, LabelFlagMap, LabelReceiverMap};
use std::sync::Arc;
use tokio::{net::TcpListener, sync::RwLock};
use tracing::info;

pub async fn start_web_server_task(
    addr: &str,
    label_flag_map: Arc<LabelFlagMap>,
    label_receiver_map: Arc<RwLock<LabelReceiverMap<Vec<u8>>>>,
) {
    let listener = TcpListener::bind(&addr).await.unwrap();
    info!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let label_flag_map = label_flag_map.clone();
        let label_receiver_map = label_receiver_map.clone();

        tokio::spawn(async move {
            let video = VideoParse::new(None, None, None);
            if let Err(e) = video
                .send_to_web(&mut socket, label_flag_map, label_receiver_map)
                .await
            {
                let _ = video.response_error(&mut socket, &e).await;
            }
        });
    }
}
