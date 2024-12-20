use crate::error::CaptureError;
use crate::rabbitmq::RabbitmqConn;
use crate::{config::Configuration, error::Result, Auth, LabelMapFlag};
use amqprs::callbacks::DefaultChannelCallback;
use amqprs::connection::Connection;
use chrono::Local;
use opencv::{
    core::{Mat, Size, Vector},
    imgcodecs::{self, imencode, IMWRITE_JPEG_QUALITY},
    prelude::*,
    videoio::{VideoCapture, VideoWriter},
};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const BASE_RESPONSE: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\n\r\n";

pub struct VideoParse {
    pub protocol: Option<(char, char, char, char)>,
    pub frame_size: Option<(i32, i32)>,
    pub fps: Option<f64>,
}

pub enum StreamType {
    // Server generate label to identify the camera.
    LabelGenerate,
    // Authentication
    Auth,
    // Send video stream
    VideoSend,
    Other,
}

impl<'a> From<&'a u8> for StreamType {
    fn from(value: &'a u8) -> Self {
        match value {
            0 => StreamType::LabelGenerate,
            1 => StreamType::Auth,
            2 => StreamType::VideoSend,
            _ => StreamType::Other,
        }
    }
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
    pub async fn get_label(&self, label_map: Arc<RwLock<LabelMapFlag>>, ip_addr: &str) -> bool {
        let label_map = label_map.read().await;
        let flag = label_map.get(ip_addr);
        match flag {
            Some(true) => true,
            _ => false,
        }
    }

    pub async fn decode(
        &self,
        stream: &mut TcpStream,
        label_map: Arc<RwLock<LabelMapFlag>>,
        rabbitmq: Arc<RabbitmqConn>,
        conn: Arc<Connection>,
    ) -> Result<()> {
        let mut size_buffer = [0; 2];
        stream.read_exact(&mut size_buffer).await?;
        let data_size = u16::from_be_bytes(size_buffer) as usize;
        let mut buf = vec![];
        buf.resize(data_size, 0);
        stream.read_exact(&mut buf).await?;
        let config: Configuration = serde_json::from_str(std::str::from_utf8(&buf)?)?;
        if let Some(ref label) = config.label {
            self.add_label(label_map.clone(), label).await?;
            log::info!("Add label: {} to label_map successfully", label);
        } else {
            return Err(CaptureError::EmptyLabelName);
        }
        let mut writer = self.set_video_writer(stream.peer_addr().unwrap(), &config);

        log::info!("New connection from {}", stream.peer_addr().unwrap());

        // rabbitmq
        let ch = conn.open_channel(None).await?;

        ch.register_callback(DefaultChannelCallback).await?;
        let (publish_args, props) = rabbitmq
            .declare_queue(&ch, &config.label.as_ref().unwrap(), true)
            .await?;

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

            let flag = self.get_label(label_map.clone(), config.label.as_ref().unwrap()).await;

            if flag == true {
                rabbitmq
                    .send(&ch, publish_args.clone(), props.clone(), buf.clone())
                    .await?;
            }

            if let Some(ref mut writer) = writer {
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
        if let Some(ref mut writer) = writer {
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

    pub async fn set_label(
        &self,
        label_map: Arc<RwLock<LabelMapFlag>>,
        label: String,
    ) -> Result<()> {
        let mut label_map = label_map.write().await;
        let _ = label_map
            .entry(label)
            .and_modify(|x| *x = true)
            .or_insert(true);
        Ok(())
    }

    pub async fn add_label(&self, label_map: Arc<RwLock<LabelMapFlag>>, label: &str) -> Result<()> {
        let mut label_map = label_map.write().await;
        if label_map.get(label).is_some() {
            Err(CaptureError::DuplicatedLabelError)
        } else {
            label_map.insert(label.to_string(), false);
            Ok(())
        }
    }
    pub async fn send_to_web(
        &self,
        mut stream: &mut TcpStream,
        label_map: Arc<RwLock<LabelMapFlag>>,
        rabbitmq_conn: Arc<RabbitmqConn>,
        conn: Arc<Connection>,
    ) -> Result<()> {
        let mut peek_buf = [0u8; 1000];
        peek_stream_data(&mut stream, &mut peek_buf).await?;
        let s = String::from_utf8_lossy(&peek_buf).to_string();
        if let Some(ref label) = parse_http_data_ip(&s) {
            self.set_label(label_map, label.to_string()).await?;
            stream.write_all(BASE_RESPONSE).await?;
            let ch = conn.open_channel(None).await?;
            rabbitmq_conn.clear_spec_queue(&ch, label).await?;
            ch.register_callback(DefaultChannelCallback).await?;
            let mut consumer_message = rabbitmq_conn.get_rx(&ch, label.to_string()).await?;
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

pub async fn peek_stream_data(stream: &mut TcpStream, buf: &mut [u8; 1000]) -> Result<()> {
    stream.peek(buf).await?;
    Ok(())
}

pub fn parse_http_data_ip(s: &str) -> Option<String> {
    let mut request = http_parse::Request::new();
    request.parse_from_str(s);
    let querys = request.query();
    let headers = request.headers();
    let mut label: Option<String> = None;
    querys.iter().for_each(|x| {
        if x.name() == "label" {
            label = Some(x.value().to_string())
        }
    });
    if label.is_none() {
        headers.iter().for_each(|x| {
            if x.name() == "label" {
                label = Some(x.value().to_string())
            }
        });
    }

    label
}

#[test]
pub fn test_parse_http_data_ip() -> Result<()> {
    let s1 = "GET /?a=b&label=127.0.0.1 HTTP/1.1\r\nHost: localhost:54322\r\nConnection: keep-alive\r\nCache-Control: max-age=0\r\nsec-ch-ua: \"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"\r\nsec-ch-ua-mobile: ?0\r\nsec-ch-ua-platform: \"macOS\"\r\nUpgrade-Insecure-Requests: 1\r\nUser-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7\r\nSec-Fetch-Site: none\r\nSec-Fetch-Mode: navigate\r\nSec-Fetch-User: ?1\r\nSec-Fetch-Dest: document\r\nAccept-Encoding: gzip, deflate, br, zstd\r\nAccept-Language: zh-CN,zh;q=0.9\r\n\r\n\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let s2 = parse_http_data_ip(s1);
    assert_eq!(Some(String::from("127.0.0.1")), s2);
    let s3 = "GET /?a=b HTTP/1.1\r\nHost: localhost:54322\r\nlabel: 127.0.0.1\r\nConnection: keep-alive\r\nCache-Control: max-age=0\r\nsec-ch-ua: \"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"\r\nsec-ch-ua-mobile: ?0\r\nsec-ch-ua-platform: \"macOS\"\r\nUpgrade-Insecure-Requests: 1\r\nUser-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7\r\nSec-Fetch-Site: none\r\nSec-Fetch-Mode: navigate\r\nSec-Fetch-User: ?1\r\nSec-Fetch-Dest: document\r\nAccept-Encoding: gzip, deflate, br, zstd\r\nAccept-Language: zh-CN,zh;q=0.9\r\n\r\n\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let s4 = parse_http_data_ip(s3);
    assert_eq!(Some(String::from("127.0.0.1")), s4);

    let s5 = "GET /?a=b&label=127.0.0.2 HTTP/1.1\r\nHost: localhost:54322\r\nlabel: 127.0.0.1\r\nConnection: keep-alive\r\nCache-Control: max-age=0\r\nsec-ch-ua: \"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"\r\nsec-ch-ua-mobile: ?0\r\nsec-ch-ua-platform: \"macOS\"\r\nUpgrade-Insecure-Requests: 1\r\nUser-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7\r\nSec-Fetch-Site: none\r\nSec-Fetch-Mode: navigate\r\nSec-Fetch-User: ?1\r\nSec-Fetch-Dest: document\r\nAccept-Encoding: gzip, deflate, br, zstd\r\nAccept-Language: zh-CN,zh;q=0.9\r\n\r\n\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let s6 = parse_http_data_ip(s5);
    assert_eq!(Some(String::from("127.0.0.2")), s6);
    let s7 = "GET /?a=b HTTP/1.1\r\nHost: localhost:54322\r\nConnection: keep-alive\r\nCache-Control: max-age=0\r\nsec-ch-ua: \"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"\r\nsec-ch-ua-mobile: ?0\r\nsec-ch-ua-platform: \"macOS\"\r\nUpgrade-Insecure-Requests: 1\r\nUser-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7\r\nSec-Fetch-Site: none\r\nSec-Fetch-Mode: navigate\r\nSec-Fetch-User: ?1\r\nSec-Fetch-Dest: document\r\nAccept-Encoding: gzip, deflate, br, zstd\r\nAccept-Language: zh-CN,zh;q=0.9\r\n\r\n\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let s8 = parse_http_data_ip(s7);
    assert_eq!(None, s8);
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
