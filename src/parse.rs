use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::error;
use crate::constant::ERROR_RESPONSE;
use crate::error::CaptureError;

pub mod audio_parse;
pub mod video_parse;

#[async_trait::async_trait]
pub trait ResponseError{
    async fn response_error(&self, stream: &mut TcpStream, err: &CaptureError) -> crate::error::Result<()> {
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
