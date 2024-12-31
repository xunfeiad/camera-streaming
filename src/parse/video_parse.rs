use crate::{config::Configuration, constant::{BASE_RESPONSE, ERROR_RESPONSE}, error::CaptureError, error::Result, DeviceEnum, DeviceFlag, Label, LabelFlagMap, LabelReceiverMap, IsEnd};
use async_channel::{Receiver, Sender};
use chrono::Local;
use opencv::{
    core::{Mat, Size, Vector},
    imgcodecs::{self, imencode, IMWRITE_JPEG_QUALITY},
    prelude::*,
    videoio,
    videoio::{VideoCapture, VideoWriter},
};
use std::{net::SocketAddr, sync::Arc};
use std::sync::atomic::Ordering;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::{error, info};
use crate::parse::ResponseError;

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
        let mut buf = vec![];
        buf.resize(data_size, 0);
        stream.read_exact(&mut buf).await?;
        let config: Configuration = serde_json::from_str(std::str::from_utf8(&buf)?)?;
        let label: Label = config.label.clone().into();
        label_flag_map
            .insert(label, DeviceFlag::new(false, false))
            .await?;
        label_receiver_map
            .insert(label, receiver, DeviceEnum::Video)
            .await?;
        let mut writer = self.set_video_writer(stream.peer_addr().unwrap(), &config);

        info!("New connection from {}", stream.peer_addr().unwrap());

        loop {
            let mut buf = vec![];

            // Read the exact image size
            let mut size_buffer = [0; 4];
            if let Err(e) = stream.read_exact(&mut size_buffer).await {
                error!("{:?}", e);
                break;
            }
            let frame_size = u32::from_be_bytes(size_buffer) as usize;

            // Confirm to receive the whole data
            buf.resize(frame_size, 0); // Adjust the buffer size

            if let Err(e) = stream.read_exact(&mut buf).await {
                error!("Error reading frame data: {}", e);
                break;
            }

            if let (true, _) = label_flag_map.get_flag(&label).await? {
                sender.send(buf.clone()).await?;
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

    pub async fn encode(&self, mut stream: TcpStream, config: &Configuration, is_end: Arc<IsEnd>) -> Result<()> {
        let mut frame = Mat::default();
        let params = vec![IMWRITE_JPEG_QUALITY, config.quality.into()];
        let config = config.to_bytes()?;
        let mut cap = VideoCapture::new(0, videoio::CAP_ANY)?;
        let opened = VideoCapture::is_opened(&cap)?;
        if !opened {
            is_end.store(true, Ordering::Release);
            panic!("Unable to open default camera!");
        }

        // Send ConfigData
        let data = config.len() as u16;
        let size = data.to_be_bytes();
        stream.write_all(&size).await?;
        stream.write_all(&config.as_slice()).await?;

        loop {
            if !is_end.load(Ordering::Acquire){
                cap.read(&mut frame)?;

                // Encode the `JPEG` image format.
                let mut buffer = Vector::new();
                imencode(".jpg", &frame, &mut buffer, &Vector::from(params.clone()))?;

                // First, send the image size.
                let buffer_size = buffer.len() as u32;
                let size_bytes = buffer_size.to_be_bytes(); // 转换为大端字节序

                if let Err(e) = stream.write_all(&size_bytes).await {
                    is_end.store(true, Ordering::Release);
                    error!("Error occured: {}", e);
                    break;
                }

                // Second, send real image data.
                if let Err(e) = stream.write_all(buffer.as_slice()).await {
                    is_end.store(true, Ordering::Release);
                    error!("Error occured: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }

    pub async fn send_to_web(
        &self,
        mut stream: &mut TcpStream,
        label_flag_map: Arc<LabelFlagMap>,
        label_receiver_map: Arc<LabelReceiverMap>,
    ) -> Result<()> {
        let mut peek_buf = [0u8; 1000];
        peek_stream_data(&mut stream, &mut peek_buf).await?;
        let s: String = String::from_utf8_lossy(&peek_buf).to_string();
        let label = parse_label_data(&s)?;
        if !label_flag_map.is_labeled(&label).await {
            return Err(CaptureError::InvalidLabel("the label is not registered."));
        }
        label_flag_map
            .set_flag(&label, true, DeviceEnum::Video)
            .await?;
        stream.write_all(BASE_RESPONSE).await?;
        {
            let label_receiver_map = label_receiver_map.0.read().await;
            let receiver = label_receiver_map
                .get(&label)
                .ok_or(CaptureError::EmptyLabelName)?;
            match &receiver.video_receiver {
                Some(receiver) => {
                    while let Ok(msg) = receiver.recv().await {
                        let header = format!(
                            "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
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
                    error!("Video identifier is closed.")
                }
            }
        }

        Ok(())
    }

    pub async fn response_error(&self, stream: &mut TcpStream, err: &CaptureError) -> Result<()> {
        error!("【Response Error】{:?}", err);
        let error_json_string = err.to_json_string();
        let header = format!("Content-Length: {}\r\n\r\n", error_json_string.len());
        stream
            .write_all(
                [
                    ERROR_RESPONSE,
                    header.as_bytes(),
                    error_json_string.as_bytes(),
                ]
                .concat()
                .as_slice(),
            )
            .await?;
        Ok(())
    }
}

pub async fn peek_stream_data(stream: &mut TcpStream, buf: &mut [u8; 1000]) -> Result<()> {
    stream.peek(buf).await?;
    Ok(())
}

pub fn parse_label_data(s: &str) -> Result<Label> {
    let mut request = http_parse::Request::new();
    request.parse_from_str(s);
    let querys = request.query();
    let headers = request.headers();
    if request.path() == "/favicon.ico" {
        return Err(CaptureError::InvalidLabel("favicon.ico request."));
    }
    let mut label: Option<Label> = None;
    querys.iter().for_each( |el|{
        if el.name() == "label" {
            let lab: Label = el.value().as_bytes().into();
            label = Some(lab)
        }
    });
    if label.is_none() {
        headers.iter().for_each( |el|{
            if el.name() == "label" {
                let lab: Label = el.value().as_bytes().into();
                label = Some(lab)
            }
        });
    };

    label.ok_or(CaptureError::InvalidLabel("no label in request."))
}

impl ResponseError for VideoParse {}

#[test]
pub fn test_parse_label_data() -> Result<()> {
    let s1 = "GET /?a=b&label=127.0.0.1 HTTP/1.1\r\nHost: localhost:54322\r\nConnection: keep-alive\r\nCache-Control: max-age=0\r\nsec-ch-ua: \"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"\r\nsec-ch-ua-mobile: ?0\r\nsec-ch-ua-platform: \"macOS\"\r\nUpgrade-Insecure-Requests: 1\r\nUser-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7\r\nSec-Fetch-Site: none\r\nSec-Fetch-Mode: navigate\r\nSec-Fetch-User: ?1\r\nSec-Fetch-Dest: document\r\nAccept-Encoding: gzip, deflate, br, zstd\r\nAccept-Language: zh-CN,zh;q=0.9\r\n\r\n\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let s2 = parse_label_data(s1)?;
    assert_eq!(<&[u8] as Into<Label>>::into("127.0.0.1".as_bytes()), s2);
    let s3 = "GET /?a=b HTTP/1.1\r\nHost: localhost:54322\r\nlabel: 127.0.0.1\r\nConnection: keep-alive\r\nCache-Control: max-age=0\r\nsec-ch-ua: \"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"\r\nsec-ch-ua-mobile: ?0\r\nsec-ch-ua-platform: \"macOS\"\r\nUpgrade-Insecure-Requests: 1\r\nUser-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7\r\nSec-Fetch-Site: none\r\nSec-Fetch-Mode: navigate\r\nSec-Fetch-User: ?1\r\nSec-Fetch-Dest: document\r\nAccept-Encoding: gzip, deflate, br, zstd\r\nAccept-Language: zh-CN,zh;q=0.9\r\n\r\n\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let s4 = parse_label_data(s3)?;
    assert_eq!(<&[u8] as Into<Label>>::into("127.0.0.1".as_bytes()), s4);

    let s5 = "GET /?a=b&label=127.0.0.2 HTTP/1.1\r\nHost: localhost:54322\r\nlabel: 127.0.0.1\r\nConnection: keep-alive\r\nCache-Control: max-age=0\r\nsec-ch-ua: \"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"\r\nsec-ch-ua-mobile: ?0\r\nsec-ch-ua-platform: \"macOS\"\r\nUpgrade-Insecure-Requests: 1\r\nUser-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7\r\nSec-Fetch-Site: none\r\nSec-Fetch-Mode: navigate\r\nSec-Fetch-User: ?1\r\nSec-Fetch-Dest: document\r\nAccept-Encoding: gzip, deflate, br, zstd\r\nAccept-Language: zh-CN,zh;q=0.9\r\n\r\n\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let s6 = parse_label_data(s5)?;
    assert_eq!(<&[u8] as Into<Label>>::into("127.0.0.2".as_bytes()), s6);
    Ok(())
}
