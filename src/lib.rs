use async_channel::{Receiver, Sender};
use error::{CaptureError, Result};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicPtr, Ordering},
        Arc,
    },
};

pub mod config;
pub mod error;
pub mod parse;

pub mod task;

pub type Auth = [u8; 100];

pub struct Queue<T> {
    pub flag: AtomicBool,
    pub sender: AtomicPtr<Sender<T>>,
    pub receiver: AtomicPtr<Receiver<T>>,
}

impl<T> Queue<T> {
    pub fn new(flag: AtomicBool, sender: *mut Sender<T>, receiver: *mut Receiver<T>) -> Self {
        Self {
            flag,
            sender: AtomicPtr::new(sender),
            receiver: AtomicPtr::new(receiver),
        }
    }
}

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
                    .and_modify(|queue| (**queue.get_mut()).flag = AtomicBool::new(true));
            },
        }

        Ok(())
    }

    pub fn get_queue(&self, label: &str) -> Result<&Queue<T>> {
        let queue = self
            .0
            .get(label)
            .ok_or(error::CaptureError::EmptyLabelName)?;
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
        let flag = self.flag.load(Ordering::Acquire);
        println!("{:?}", flag);
        if flag {
            log::info!("Sending...");
            unsafe {
                (*self.sender.load(Ordering::Acquire)).send(data).await?;
            }
            log::info!("Sending successfully...");
        }

        Ok(())
    }

    async fn recv(&self) -> Result<Self::Item> {
        log::info!("Recv...");
        unsafe {
            let item = (*self.receiver.load(Ordering::Acquire)).recv().await?;
            log::info!("Recv successfully...");
            Ok(item)
        }
    }
}
