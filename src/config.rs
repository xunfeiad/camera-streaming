use crate::error::{CaptureError, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, path::Path};

#[derive(Parser, Serialize, Deserialize, Debug, Clone)]
#[command(version, about, long_about = None)]
#[command(name = "Client")]
#[command(version = "0.0.1")]
#[non_exhaustive]
pub struct Configuration {
    #[arg(short = 'u', long, help = "The username of authentication")]
    pub username: Option<String>,
    #[arg(short = 'p', long, help = "The password of authentication")]
    pub password: Option<String>,
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    pub host: String,
    #[arg(long="vrp", value_parser = clap::value_parser!(u16).range(1..), default_value_t = 54321)]
    pub video_receiver_port: u16,
    #[arg(long="vsp", value_parser = clap::value_parser!(u16).range(1..), default_value_t = 54322)]
    pub video_server_port: u16,
    #[arg(long="arp", value_parser = clap::value_parser!(u16).range(1..), default_value_t = 54323)]
    pub audio_receiver_port: u16,
    #[arg(long="asp", value_parser = clap::value_parser!(u16).range(1..), default_value_t = 54324)]
    pub audio_server_port: u16,
    #[arg(
        short = 't',
        long,
        default_value = "true",
        help = "The field for whether to transcribe"
    )]
    pub is_transcribe: String,
    #[arg(
        short = 'f',
        long,
        default_value_t = 60.0,
        long,
        help = "The fps of the video"
    )]
    pub fps: f64,
    #[arg(
        short = 'q',
        long,
        default_value_t = 90,
        long,
        help = "The quality of the video"
    )]
    pub quality: i32,

    #[arg(short = 'l', long, help = "The identification of the camera")]
    pub label: String,
}

impl TryFrom<String> for Configuration {
    type Error = CaptureError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let deserialized: Configuration = serde_yaml::from_str(&value)?;
        Ok(deserialized)
    }
}

impl Configuration {
    pub fn parse_yaml<T: AsRef<Path>>(path: T) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        Configuration::try_from(buf)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let binding = serde_json::to_string(self)?;
        let b = binding.as_bytes();
        Ok(b.to_vec())
    }
}

#[test]
pub fn test_parse_yaml_file() {
    let configuration: Configuration = Configuration::parse_yaml("config.yaml").unwrap();
    println!("{:?}", configuration);
    assert_eq!(configuration.username.unwrap(), String::from("xunfei"));
    assert_eq!(configuration.password.unwrap(), String::from("xunfei"));
    assert_eq!(configuration.host, String::from("localhost"));
    assert_eq!(configuration.video_receiver_port, 54321);
}
