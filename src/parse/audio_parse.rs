use crate::config::Configuration;
use crate::error::{CaptureError, Result};
use crate::parse::video_parse::{parse_label_data, peek_stream_data};
use crate::parse::ResponseError;
use crate::{DeviceEnum, DeviceFlag, IsEnd, Label, LabelFlagMap, LabelReceiverMap};
use async_channel::{Receiver, Sender};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tracing::{error, info};

#[derive(Default)]
pub struct Audio;

impl Audio {
    pub async fn encode(
        &self,
        mut socket: TcpStream,
        configuration: &Configuration,
        handle: Handle,
        is_end: Arc<IsEnd>,
    ) -> Result<()> {
        // Send ConfigData
        let configuration = configuration.to_bytes()?;
        let data = configuration.len() as u16;
        let size = data.to_be_bytes();
        socket.write_all(&size).await.unwrap();
        socket.write_all(configuration.as_slice()).await?;

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(CaptureError::NoAudioDeviceFindError)?;
        let config = device.default_input_config()?;
        let is_end_clone = is_end.clone();

        let is_end_clone1 = is_end.clone();

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                handle.block_on(async {
                    let data: Vec<u8> =
                        data.iter().copied().flat_map(|x| x.to_be_bytes()).collect();
                    if let Err(e) = socket.write_all(data.as_slice()).await {
                        error!(Error = "Audio::encode::build_input_stream", "{}", e);
                        is_end_clone.store(true, Ordering::Release);
                    }
                })
            },
            move |err| {
                is_end_clone1.store(true, Ordering::Release);
                error!("Error occurred: {:?}", err);
            },
            Some(Duration::from_secs(10)),
        )?;
        stream.play()?;

        loop {
            if is_end.load(Ordering::Acquire) {
                break;
            }
        }
        Ok(())
    }

    pub async fn send_to_web(
        &self,
        stream: &mut TcpStream,
        label_flag_map: Arc<LabelFlagMap>,
        label_receiver_map: Arc<LabelReceiverMap>,
    ) -> Result<()> {
        let mut peek_buf = [0u8; 1000];
        peek_stream_data(stream, &mut peek_buf).await?;
        let s: String = String::from_utf8_lossy(&peek_buf).to_string();
        let label = parse_label_data(&s)?;
        if !label_flag_map.is_labeled(&label).await {
            return Err(CaptureError::InvalidLabel("the label is not registered."));
        }
        label_flag_map
            .set_flag(&label, true, DeviceEnum::Audio)
            .await?;
        stream.write_all(crate::constant::BASE_RESPONSE).await?;

        let label_receiver_map = label_receiver_map.0.read().await;
        let receiver = label_receiver_map
            .get(&label)
            .ok_or(CaptureError::EmptyLabelName)?;

        match &receiver.audio_receiver {
            Some(receiver) => {
                while let Ok(msg) = receiver.recv().await {
                    let msg: Vec<u8> = msg.into_iter().flat_map(|x| x.to_be_bytes()).collect();
                    let header = format!(
                        "--frame\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\n\r\n",
                        msg.len()
                    );
                    let packet = [header.as_bytes(), msg.as_slice()].concat();

                    if let Err(_e) = stream.write_all(&packet).await {
                        receiver.close();
                        break;
                    }
                }
            }
            None => {
                error!("Audio identifier is closed.")
            }
        }

        Ok(())
    }

    pub async fn decode(
        &self,
        stream: &mut TcpStream,
        sender: Sender<Vec<u8>>,
        receiver: Receiver<Vec<u8>>,
        label_flag_map: Arc<LabelFlagMap>,
        label_receiver_map: Arc<LabelReceiverMap>,
    ) -> Result<()> {
        let mut size_buffer = [0; 2];
        stream.read_exact(&mut size_buffer).await?;
        let data_size = u16::from_be_bytes(size_buffer) as usize;
        let mut buf = vec![0; data_size];
        stream.read_exact(&mut buf).await?;
        let config: Configuration = serde_json::from_str(std::str::from_utf8(&buf)?)?;
        let label: Label = config.label.clone().into();
        label_flag_map
            .insert(label, DeviceFlag::new(false, false))
            .await?;
        label_receiver_map
            .insert(label, receiver, DeviceEnum::Audio)
            .await?;
        info!("New connection from {}", stream.peer_addr().unwrap());
        loop {
            let mut buf = vec![0; 4];

            if let Err(e) = stream.read_exact(&mut buf).await {
                error!("Error reading frame data: {}", e);
                label_receiver_map.remove(label).await?;
                break;
            }

            if let (true, _) = label_flag_map.get_flag(&label).await? {
                sender.send(buf.clone()).await?;
            }
        }
        Ok(())
    }
}

impl ResponseError for Audio {}
