use crate::error::Result;
use crate::rabbitmq::RabbitmqConn;
use amqprs::{
    channel::{BasicPublishArguments, Channel, ConsumerMessage},
    BasicProperties,
};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Mutex;

pub async fn get_channel(rabbitmq_conn: Arc<Mutex<RabbitmqConn>>) -> crate::error::Result<Channel> {
    let mut rabbitmq_conn = rabbitmq_conn.lock().await;
    let channel = rabbitmq_conn.get_channel().await?;
    Ok(channel)
}

pub async fn declare_queue(
    rabbitmq_conn: Arc<Mutex<RabbitmqConn>>,
    ch: &Channel,
    queue_name: &str,
    durable: bool,
) -> Result<(BasicPublishArguments, BasicProperties)> {
    let rabbitmq_conn = rabbitmq_conn.lock().await;
    println!("{:?}", ch.is_open());
    rabbitmq_conn.declare_queue(&ch, queue_name, durable).await
}

pub async fn get_rx(
    rabbitmq_conn: Arc<Mutex<RabbitmqConn>>,
    ch: &Channel,
    queue_name: String,
) -> Result<UnboundedReceiver<ConsumerMessage>> {
    let rabbitmq_conn = rabbitmq_conn.lock().await;
    let consumer_message = rabbitmq_conn.get_rx(ch, queue_name).await?;
    Ok(consumer_message)
}
