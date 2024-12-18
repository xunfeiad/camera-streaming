use opencv::Error as opencvError;
use serde_yaml::Error;
use std::io::Error as stdError;
use thiserror::Error;
use tokio::task::JoinError;

pub type Result<T, E = CaptureError> = std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("Connect failed")]
    ConnectFailed(#[from] stdError),
    #[error("Opencv Error")]
    OpencvError(#[from] opencvError),
    #[error("Tokio Error")]
    TokioError(#[from] JoinError),
    #[error("Parsing yaml file failed.")]
    ParseYamlFileFailed(#[from] Error),
    #[error("Incorrect password.")]
    ValidateFailed,
    #[error("NotTranscribe")]
    NotTranscribe,
    #[error("SerdeJsonError")]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("Utf8Error")]
    Utf8Error(#[from] std::str::Utf8Error),
    #[error("IP not in server database")]
    IPNotInServerDatabase,
    #[error("Send stream failed")]
    SendStreamFailed(#[from] async_channel::SendError<Vec<u8>>),
    #[error("Rabbitmq error:{0}")]
    RabbitmqError(#[from] amqprs::error::Error),
    #[error("Rabbitmq Channel Not initialized.")]
    NotInitializedRabbitmqChannel,
}
