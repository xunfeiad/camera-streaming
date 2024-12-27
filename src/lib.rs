use crate::error::{CaptureError, Result};
use async_channel::Receiver;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
pub mod config;
pub mod error;
pub mod parse;

pub mod task;

fn copy(arr: &mut [u8; 100], bytes: &[u8]) {
    if bytes.len() > 100 {
        arr.copy_from_slice(&[1u8; 100]);
    } else {
        arr[..bytes.len()].copy_from_slice(bytes);
    }
}

#[derive(Clone, Eq, Hash, PartialEq, Debug, Copy)]
pub struct Label([u8; 100]);

impl<'a> From<&'a str> for Label {
    fn from(value: &'a str) -> Self {
        let mut arr = [0u8; 100];
        let bytes = value.as_bytes();
        copy(&mut arr, bytes);
        Label(arr)
    }
}

impl From<String> for Label {
    fn from(value: String) -> Self {
        let mut arr = [0u8; 100];
        let bytes = value.as_bytes();
        copy(&mut arr, bytes);
        Label(arr)
    }
}
impl<'a> From<&'a [u8]> for Label {
    fn from(value: &[u8]) -> Label {
        let mut arr = [0u8; 100];
        if value.len() > 100 {
            arr.copy_from_slice(&[2u8; 100]);
        } else {
            arr[..value.len()].copy_from_slice(value);
        }
        Label(arr)
    }
}

#[derive(Default)]
pub struct LabelFlagMap(RwLock<HashMap<Label, AtomicBool>>);

impl LabelFlagMap {
    pub async fn insert(&self, k: Label, v: AtomicBool) {
        let mut label_flag_map = self.0.write().await;
        log::info!("add label:{:?} to server successfully.", k);
        label_flag_map.insert(k, v);
    }
    pub async fn get_flag(&self, label: &Label) -> bool {
        let label_flag_map = self.0.write().await;
        let flag = label_flag_map.get(label);
        if flag.is_some() && flag.unwrap().load(Ordering::Acquire) {
            return true;
        }
        false
    }

    pub async fn set_flag_to_true(&self, label: &Label) -> Result<()> {
        let label_flag_map = self.0.write().await;

        let flag = label_flag_map.get(label);
        if let Some(flag) = flag {
            flag.store(true, Ordering::Release);
            Ok(())
        } else {
            Err(CaptureError::EmptyLabelName)
        }
    }
}

#[derive(Default)]
pub struct LabelReceiverMap<T>(HashMap<Label, Receiver<T>>);

impl<T> LabelReceiverMap<T> {
    pub fn get_receiver(&self, label: &Label) -> Result<&Receiver<T>> {
        let receiver = self.0.get(&label);
        match receiver {
            Some(receiver) => Ok(receiver),
            None => Err(CaptureError::EmptyLabelName),
        }
    }
}
