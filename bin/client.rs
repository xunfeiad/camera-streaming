use ::capture::error::Result;
use ::capture::parse::video_parse::VideoParse;
use capture::config::Configuration;
use clap::Parser;
use opencv::{prelude::*, videoio};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<()> {
    let cli: Configuration = Configuration::parse();
    let cap = videoio::VideoCapture::new(0, videoio::CAP_ANY)?;
    let opened = videoio::VideoCapture::is_opened(&cap)?;
    if !opened {
        panic!("Unable to open default camera!");
    }

    // Connect to `Server`

    let stream = TcpStream::connect((cli.host.clone(), cli.send_port)).await?;
    let video = VideoParse::new(None, None, None);
    video.encode(stream, cap, cli).await?;
    Ok(())
}
