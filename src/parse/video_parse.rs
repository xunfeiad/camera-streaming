use crate::rabbitmq::RabbitmqConn;
use crate::{config::Configuration, error::{CaptureError, Result}, Auth, IPMapFlag, error};
use amqprs::callbacks::DefaultChannelCallback;
use chrono::Local;
use opencv::{
    core::{Mat, Size, Vector},
    imgcodecs::{self, imencode, IMWRITE_JPEG_QUALITY},
    prelude::*,
    videoio::{VideoCapture, VideoWriter},
};
use std::collections::HashMap;
use std::{net::SocketAddr, sync::Arc};
use amqprs::connection::Connection;
use tokio::sync::RwLock;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const BASE_RESPONSE: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\n\r\n";

// TODO
const IPS: [&'static str; 1] = ["127.0.0.1"];

pub struct VideoParse {
    pub protocol: Option<(char, char, char, char)>,
    pub frame_size: Option<(i32, i32)>,
    pub fps: Option<f64>,
}

impl VideoParse {
    pub fn new(
        protocol: Option<(char, char, char, char)>,
        frame_size: Option<(i32, i32)>,
        fps: Option<f64>,
    ) -> VideoParse {
        Self {
            protocol: protocol.or_else(|| Some(('m', 'p', '4', 'v'))),
            frame_size: frame_size.or_else(|| Some((1920, 1080))),
            fps: fps.or_else(|| Some(60.0)),
        }
    }
    pub async fn get_ip_map_flag(&self, ip_map: Arc<RwLock<IPMapFlag>>, ip_addr: &str) -> bool {
        let ip_map = ip_map.read().await;
        let flag = ip_map.get(ip_addr);
        match flag {
            Some(true) => true,
            _ => false,
        }
    }

    pub async fn decode(
        &self,
        stream: &mut TcpStream,
        ip_map: Arc<RwLock<IPMapFlag>>,
        rabbitmq: Arc<RabbitmqConn>,
        conn: Arc<Connection>
    ) -> Result<()> {
        // rabbitmq
        let ch = conn.open_channel(None).await?;
        ch.register_callback(DefaultChannelCallback).await?;
        let (publish_args, props) = rabbitmq
            .declare_queue(&ch, &stream.peer_addr()?.ip().to_string(), true)
            .await?;

        let mut size_buffer = [0; 2];
        stream.read_exact(&mut size_buffer).await?;
        let data_size = u16::from_be_bytes(size_buffer) as usize;
        let mut buf = vec![];
        buf.resize(data_size, 0);
        stream.read_exact(&mut buf).await?;
        let config: Configuration = serde_json::from_str(std::str::from_utf8(&buf)?)?;

        let mut writer = self
            .set_video_writer(stream.peer_addr().unwrap(), &config);

        log::info!("New connection from {}", stream.peer_addr().unwrap());

        loop {
            let mut buf = vec![];

            // Read the exact image size
            let mut size_buffer = [0; 4];
            if let Err(e) = stream.read_exact(&mut size_buffer).await {
                log::error!("{:?}", e);
                break;
            }
            let frame_size = u32::from_be_bytes(size_buffer) as usize;

            // Confirm to receive the whole data
            buf.resize(frame_size, 0); // Adjust the buffer size

            if let Err(e) = stream.read_exact(&mut buf).await {
                eprintln!("Error reading frame data: {}", e);
                break;
            }

            let ip = self
                .get_ip_map_flag(ip_map.clone(), &stream.peer_addr()?.ip().to_string())
                .await;

            if ip == true {
                rabbitmq
                    .send(&ch, publish_args.clone(), props.clone(), buf.clone())
                    .await?;
            }


            if let Some(ref mut writer) = writer{
                // Encode and write
                let buf = Mat::from_slice(&buf)?;

                let size = Size::new(self.frame_size.unwrap().0, self.frame_size.unwrap().1);

                let frame = imgcodecs::imdecode(&buf, imgcodecs::IMREAD_COLOR)?;
                if frame.size()?.width != size.width || frame.size()?.height != size.height {
                    continue;
                }

                writer.write(&frame)?;
            }

        }
        if let Some(ref mut writer) = writer{
            writer.release()?;
        }
        Ok(())
    }

    // Setup the video writer
    pub fn set_video_writer(&self, ip: SocketAddr, config: &Configuration) -> Option<VideoWriter> {
        let remote = ip.to_string().replace(".", "_").replace(":", "_");
        let p = self.protocol.unwrap();
        let f = self.frame_size.unwrap();
        let fourcc = VideoWriter::fourcc(p.0, p.1, p.2, p.3).ok()?;
        let frame_size = Size::new(f.0, f.1);

        let is_transcribe = match config.is_transcribe.to_lowercase().as_str() {
            "true" => true,
            "false" => false,
            _ => false,
        };
        if is_transcribe {
            let filename = format!("{}_{}.mp4", remote, Local::now().timestamp().to_string());
            let writer = VideoWriter::new(&filename, fourcc, config.fps, frame_size, true).ok()?;
            Some(writer)
        } else {
            None
        }
    }

    pub async fn encode(
        &self,
        mut stream: TcpStream,
        mut cap: VideoCapture,
        config: Configuration,
    ) -> Result<()> {
        let mut frame = Mat::default();
        let params = vec![IMWRITE_JPEG_QUALITY, config.quality.into()];
        let config = config.to_bytes()?;
        // Send ConfigData
        let data = config.len() as u16;
        let size = data.to_be_bytes();
        stream.write_all(&size).await?;
        stream.write_all(&config.as_slice()).await?;

        loop {
            cap.read(&mut frame)?;

            // Encode the `JPEG` image format.
            let mut buffer = Vector::new();
            imencode(".jpg", &frame, &mut buffer, &Vector::from(params.clone()))?;

            // First, send the image size.
            let buffer_size = buffer.len() as u32;
            let size_bytes = buffer_size.to_be_bytes(); // 转换为大端字节序

            if let Err(e) = stream.write_all(&size_bytes).await {
                log::error!("Error occured: {}", e);
                break;
            }

            // Second, send real image data.
            if let Err(e) = stream.write_all(buffer.as_slice()).await {
                log::error!("Error occured: {}", e);
                break;
            }
        }
        Ok(())
    }

    pub async fn set_ip_map_true(&self, ip_map: Arc<RwLock<IPMapFlag>>, ip_addr: String) -> error::Result<()> {
        let mut ip_map = ip_map.write().await;
        let _ = *ip_map
            .entry(ip_addr)
            .and_modify(|x| *x = true)
            .or_insert(true);
        Ok(())
    }

    pub async fn send_to_web(
        &self,
        mut stream: &mut TcpStream,
        ip_map: Arc<RwLock<IPMapFlag>>,
        rabbitmq_conn: Arc<RabbitmqConn>,
        conn: Arc<Connection>
    ) -> Result<()> {
        read_stream_data(&mut stream).await?;
        let ip_str = stream.peer_addr()?.ip().to_string();
        self.set_ip_map_true(ip_map, ip_str.to_string()).await?;
        stream.write_all(BASE_RESPONSE).await?;
        let ch = conn.open_channel(None).await?;
        rabbitmq_conn.clear_spec_queue(&ch, &ip_str).await?;
        ch.register_callback(DefaultChannelCallback).await?;
        let mut consumer_message = rabbitmq_conn.get_rx(&ch, ip_str).await?;
        while let Some(msg) = consumer_message.recv().await {
            if let Some(msg) = msg.content {

                let header = format!(
                    "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                    msg.len()
                );
                let packet = [header.as_bytes(), msg.as_slice()].concat();

                if let Err(e) = stream.write_all(&packet).await {
                    eprintln!("Failed to send video frame: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }
}

pub fn to_u8(auth: String) -> Auth {
    let mut arr = [0u8; 100];
    let bytes = auth.as_bytes();
    arr[..bytes.len()].copy_from_slice(bytes);
    arr
}

pub async fn read_stream_data(stream: &mut TcpStream) -> Result<()>{
    let mut buf = [0u8;1000];
    stream.peek(&mut buf).await?;
    Ok(())
}

#[test]
pub fn test_to_u8() {
    let password = String::from("xunfei");
    let arr = to_u8(password.clone());
    let binding = String::from_utf8_lossy(&arr).to_string();
    let password2 = binding.split_at(password.len()).0;
    assert_eq!(password, password2);

    let password = String::from("");
    let arr = to_u8(password.clone());
    let binding = String::from_utf8_lossy(&arr).to_string();
    let password2 = binding.split_at(password.len()).0;
    assert_eq!(password, password2);

    let password = String::from("xunfei1");
    let arr = to_u8(String::from("xunfei2"));
    let binding = String::from_utf8_lossy(&arr).to_string();
    let password2 = binding.split_at(password.len()).0;
    assert_ne!(password, password2);
}

// Initial all the ip map.
pub fn init_ips() -> IPMapFlag {
    let mut hashmap: IPMapFlag = HashMap::with_capacity(IPS.len());
    for el in IPS.iter() {
        hashmap.insert(el.to_string(), false);
    }
    hashmap
}
