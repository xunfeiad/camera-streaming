use std::sync::Arc;
use tokio::sync::RwLock;

pub mod config;
pub mod error;
pub mod parse;

pub mod rabbitmq;
pub mod task;

pub type Auth = [u8; 100];

pub type IPMapFlag = std::collections::HashMap<String, bool>;

pub async fn set_ip_map_true(ip_map: Arc<RwLock<IPMapFlag>>, ip_addr: String) -> error::Result<()> {
    let mut ip_map = ip_map.write().await;
    let _ = *ip_map
        .entry(ip_addr)
        .and_modify(|x| *x = true)
        .or_insert(true);
    Ok(())
}
