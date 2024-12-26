use async_channel::{Receiver, Sender};
use error::{CaptureError, Result};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicPtr, Ordering},
        Arc,
    },
};
use tokio::sync::RwLock;

pub mod config;
pub mod error;
pub mod parse;

pub mod task;

pub type Auth = [u8; 100];

pub struct Queue<T> {
    pub flag: bool,
    pub sender: Sender<T>,
    pub receiver: Receiver<T>,
}

impl<T> Queue<T> {
    pub fn new(flag: bool, sender: Sender<T>, receiver: Receiver<T>) -> Self {
        Self {
            flag,
            sender,
            receiver,
        }
    }
}

#[derive(Debug)]
pub struct LabelMapQueue<T>(HashMap<String, AtomicPtr<Queue<T>>>);

impl<T> LabelMapQueue<T> {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub async fn join_label_to_server(
        &mut self,
        label: String,
        queue: Option<&mut Queue<T>>,
    ) -> Result<()> {
        match queue {
            Some(queue) => {
                self.0.insert(label, AtomicPtr::new(queue));
            }
            None => unsafe {
                self.0
                    .entry(label)
                    .and_modify(|queue| (**queue.get_mut()).flag = true);
            },
        }

        Ok(())
    }

    pub async fn set_flag_to_true(&mut self, label: &str) -> Result<()>{
        let queue = self.0.get_mut(label).ok_or(CaptureError::EmptyLabelName)?;
        unsafe {
            (*queue.load(Ordering::Acquire)).flag = true;
        }
        Ok(())
    }

    pub async fn remove(&mut self, label: &str) -> Result<()>{
        self.0.remove(label).ok_or(CaptureError::EmptyLabelName)?;
        Ok(())
    }

    // todo!
    pub fn get_queue(&self, label: &str) -> Result<&Queue<T>> {
        let queue = self.0
            .get(label)
            .ok_or(CaptureError::EmptyLabelName)?;
        unsafe {
            let a = queue
                .as_ptr()
                .as_ref()
                .ok_or(CaptureError::EmptyLabelName)?
                .as_ref()
                .ok_or(CaptureError::EmptyLabelName)?;
            Ok(a)
        }
    }
}

#[async_trait::async_trait]
pub trait MiddleQueue {
    type Item;
    type QueueMap;

    async fn send(&self, data: Self::Item) -> Result<()>;
    async fn recv(&self) -> Result<Self::Item>;
}

#[async_trait::async_trait]
impl MiddleQueue for Queue<Vec<u8>> {
    type Item = Vec<u8>;
    type QueueMap = LabelMapQueue<Self::Item>;

    async fn send(&self, data: Self::Item) -> Result<()> {
        if self.flag {
            self.sender.send(data).await?;
        }

        Ok(())
    }

    async fn recv(&self) -> Result<Self::Item> {
        let item = self.receiver.recv().await?;
        Ok(item)
    }
}
