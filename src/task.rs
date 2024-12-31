use crate::config::Configuration;
use crate::parse::audio_parse::Audio;
use crate::{parse::video_parse::VideoParse, IsEnd, LabelFlagMap, LabelReceiverMap};
use std::sync::Arc;
use tokio::net::TcpStream;

pub async fn encode_video_task(
    cli: Arc<Configuration>,
    is_end: Arc<IsEnd>,
) -> crate::error::Result<()> {
    let stream = TcpStream::connect((cli.host.clone(), cli.video_receiver_port.clone())).await?;
    let video = VideoParse::new(None, None, None);
    video.encode(stream, &cli, is_end).await?;
    Ok(())
}

pub async fn encode_audio_task(
    cli: Arc<Configuration>,
    handle: Handle,
    is_end: Arc<IsEnd>,
) -> crate::error::Result<()> {
    let stream = TcpStream::connect((cli.host.clone(), cli.audio_receiver_port.clone())).await?;
    let audio = Audio::new();
    audio.encode(stream, &cli, handle, is_end).await?;
    Ok(())
}

use crate::parse::ResponseError;
use tokio::net::TcpListener;
use tokio::runtime::Handle;
use tracing::{error, info};

pub async fn decode_video_task(
    addr: &str,
    label_flag_map: Arc<LabelFlagMap>,
    label_receiver_map: Arc<LabelReceiverMap>,
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

pub async fn start_video_web_server_task(
    addr: &str,
    label_flag_map: Arc<LabelFlagMap>,
    label_receiver_map: Arc<LabelReceiverMap>,
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

pub async fn decode_audio_task(
    addr: &str,
    label_flag_map: Arc<LabelFlagMap>,
    label_receiver_map: Arc<LabelReceiverMap>,
) {
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let label_flag_map = label_flag_map.clone();
        let label_receiver_map = label_receiver_map.clone();

        tokio::spawn(async move {
            let audio = Audio::new();
            let (sender, receiver) = async_channel::unbounded::<Vec<u8>>();
            if let Err(e) = audio
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

pub async fn start_audio_web_server_task(
    addr: &str,
    label_flag_map: Arc<LabelFlagMap>,
    label_receiver_map: Arc<LabelReceiverMap>,
) {
    let listener = TcpListener::bind(&addr).await.unwrap();
    info!("Listening on {}", addr);
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let label_flag_map = label_flag_map.clone();
        let label_receiver_map = label_receiver_map.clone();

        tokio::spawn(async move {
            let audio = Audio::new();
            if let Err(e) = audio
                .send_to_web(&mut socket, label_flag_map, label_receiver_map)
                .await
            {
                let _ = audio.response_error(&mut socket, &e).await;
            }
        });
    }
}
