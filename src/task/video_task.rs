use std::sync::{atomic::AtomicBool, Arc};

use async_channel::{Receiver, Sender};
use tokio::{net::TcpListener, sync::RwLock};

use crate::{parse::video_parse::VideoParse, LabelMapQueue};

pub async fn start_video_task(addr: &str, label_map: Arc<RwLock<LabelMapQueue<Vec<u8>>>>) {
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let label_map = label_map.clone();
        let (mut s, mut r) = async_channel::unbounded::<Vec<u8>>();
        let queue = crate::Queue::new(
            AtomicBool::new(false),
            &mut s as *mut Sender<Vec<u8>>,
            &mut r as *mut Receiver<Vec<u8>>,
        );
        tokio::spawn(async move {
            let video = VideoParse::new(None, None, None);
            if let Err(e) = video.decode(&mut socket, label_map, Box::new(queue)).await {
                log::error!("{:?}", e);
            }
        });
    }
}
