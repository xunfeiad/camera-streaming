pub mod config;
pub mod error;
pub mod parse;

pub mod rabbitmq;
pub mod task;

pub type Auth = [u8; 100];
pub type LabelMapFlag = std::collections::HashMap<String, bool>;
