use std::sync::Arc;
use tokio::sync::RwLock;

pub mod config;
pub mod error;
pub mod parse;

pub mod rabbitmq;
pub mod task;

pub type Auth = [u8; 100];

pub type IPMapFlag = std::collections::HashMap<String, bool>;
