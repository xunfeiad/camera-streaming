use crate::error::{CaptureError, Result};
use amqprs::callbacks::{DefaultChannelCallback, DefaultConnectionCallback};
use amqprs::channel::{
    BasicConsumeArguments, BasicPublishArguments, Channel, ConsumerMessage, QueueDeclareArguments,
    QueuePurgeArguments,
};
use amqprs::connection::{Connection, OpenConnectionArguments};
use amqprs::BasicProperties;
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Clone)]
pub struct RabbitmqConn {
    host: String,
    port: u16,
    username: String,
    password: String,
}

impl RabbitmqConn {
    pub fn new(host: String, port: u16, username: String, password: String) -> Self {
        Self {
            host,
            port,
            username,
            password,
        }
    }
    /// 创建连接和注册回调
    pub async fn open_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&OpenConnectionArguments::new(
            self.host.as_str(),
            self.port,
            self.username.as_str(),
            self.password.as_str(),
        ))
        .await?;

        conn.register_callback(DefaultConnectionCallback).await?;
        Ok(conn)
    }

    /// 创建并返回一个 channel
    async fn open_channel(&self, conn: &Connection) -> Result<Channel> {
        let ch = conn.open_channel(None).await?;
        ch.register_callback(DefaultChannelCallback).await?;
        Ok(ch)
    }

    /// 获取连接和频道
    pub async fn get_channel(&mut self) -> Result<Channel> {
        let conn = self.open_connection().await?;
        self.open_channel(&conn).await
    }

    /// 发送数据
    pub async fn send(
        &self,
        ch: &Channel,
        publish_args: BasicPublishArguments,
        props: BasicProperties,
        data: Vec<u8>,
    ) -> Result<()> {
        ch.basic_publish(props, data, publish_args).await?;
        // log::info!("send data successfully.");
        Ok(())
    }

    /// 声明队列
    pub async fn declare_queue(
        &self,
        ch: &Channel,
        queue_name: &str,
        durable: bool,
    ) -> Result<(BasicPublishArguments, BasicProperties)> {
        let q_args = QueueDeclareArguments::default()
            .queue(queue_name.to_string())
            .durable(durable)
            .finish();

        ch.queue_declare(q_args).await?.ok_or_else(|| {
            CaptureError::RabbitmqError(amqprs::error::Error::InternalChannelError(String::from(
                "queue_declare_failed",
            )))
        })?;
        let publish_args = BasicPublishArguments::new("", queue_name);
        let props = BasicProperties::default().with_delivery_mode(2).finish();
        Ok((publish_args, props))
    }

    /// 关闭频道
    pub async fn close_channel(&self, ch: Channel) -> Result<()> {
        ch.close().await?;
        log::info!("Channel closed successfully.");
        Ok(())
    }

    pub async fn get_rx(
        &self,
        ch: &Channel,
        queue_name: String,
    ) -> Result<UnboundedReceiver<ConsumerMessage>> {
        let q_args = QueueDeclareArguments::default()
            .queue(queue_name)
            .durable(true)
            .finish();
        let (queue_name, _, _) = ch.queue_declare(q_args).await.unwrap().unwrap();
        let consumer_args = BasicConsumeArguments::new(&queue_name, "rabbitmq-rust");

        let (_ctag, rx) = ch.basic_consume_rx(consumer_args).await.unwrap();

        Ok(rx)
    }
    pub async fn clear_spec_queue(&self, ch: &Channel, queue_name: &str) -> Result<()> {
        let purge_args = QueuePurgeArguments::new(queue_name);

        // 清空队列
        let _ = ch.queue_purge(purge_args).await?;
        Ok(())
    }
}
