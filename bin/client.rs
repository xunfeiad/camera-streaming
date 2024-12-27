use ::camera_streaming::error::Result;
use ::camera_streaming::parse::video_parse::VideoParse;
use camera_streaming::config::Configuration;
use clap::Parser;
use opencv::{prelude::*, videoio};
use tokio::net::TcpStream;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_line_number(true).init();
    info!(
        server = "Camera-Stream-Client",
        "Preparing to start server."
    );
    let cli: Configuration = Configuration::parse();
    let cap = videoio::VideoCapture::new(0, videoio::CAP_ANY)?;
    let opened = videoio::VideoCapture::is_opened(&cap)?;
    if !opened {
        panic!("Unable to open default camera!");
    }

    let stream = TcpStream::connect((cli.host.clone(), cli.send_port)).await?;
    let video = VideoParse::new(None, None, None);
    video.encode(stream, cap, cli).await?;
    Ok(())
}
