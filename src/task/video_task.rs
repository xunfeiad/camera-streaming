use std::sync::Arc;

use tokio::{net::TcpListener, sync::RwLock};
use tracing::{error, info};

use crate::{parse::video_parse::VideoParse, LabelFlagMap, LabelReceiverMap};

pub async fn start_video_task(
    addr: &str,
    label_flag_map: Arc<LabelFlagMap>,
    label_receiver_map: Arc<RwLock<LabelReceiverMap<Vec<u8>>>>,
) {
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let label_flag_map = label_flag_map.clone();
        let label_receiver_map = label_receiver_map.clone();

        tokio::spawn(async move {
            let video = VideoParse::new(None, None, None);
            let (sender, receiver) = async_channel::unbounded();
            if let Err(e) = video
                .decode(
                    &mut socket,
                    sender,
                    receiver,
                    label_flag_map,
                    label_receiver_map,
                )
                .await
            {
                error!("{:?}", e);
            }
        });
    }
}
