use crate::error::{CaptureError, Result};
use async_channel::Receiver;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::sync::RwLock;
use tracing::info;

pub mod config;
pub mod error;
pub mod parse;

mod constant;
pub mod task;

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
pub struct DeviceFlag {
    video_flag: AtomicBool,
    audio_flag: AtomicBool,
}

impl DeviceFlag {
    fn new(video_flag: bool, audio_flag: bool) -> Self {
        Self {
            video_flag: AtomicBool::new(video_flag),
            audio_flag: AtomicBool::new(audio_flag),
        }
    }
}

pub enum DeviceEnum {
    Video,
    Audio,
}

#[derive(Default)]
pub struct LabelFlagMap(RwLock<HashMap<Label, DeviceFlag>>);

impl LabelFlagMap {
    pub async fn insert(&self, k: Label, v: DeviceFlag) -> Result<()> {
        if !self.is_labeled(&k).await {
            self.0.write().await.insert(k, v);
        } else {
            if v.video_flag.load(Ordering::Acquire).eq(&true) {
                self.set_flag(&k, true, DeviceEnum::Video).await?;
            }

            if v.audio_flag.load(Ordering::Acquire).eq(&true) {
                self.set_flag(&k, true, DeviceEnum::Audio).await?;
            }
        }
        Ok(())
    }
    pub async fn get_flag(&self, label: &Label) -> Result<(bool, bool)> {
        let label_flag_map = self.0.read().await;
        let flag = label_flag_map
            .get(label)
            .ok_or(CaptureError::EmptyLabelName)?;
        Ok((
            flag.video_flag.load(Ordering::Acquire),
            flag.audio_flag.load(Ordering::Acquire),
        ))
    }

    pub async fn set_flag(&self, label: &Label, f: bool, device_enum: DeviceEnum) -> Result<()> {
        let label_flag_map = self.0.write().await;

        let flag = label_flag_map
            .get(label)
            .ok_or(CaptureError::EmptyLabelName)?;

        match device_enum {
            DeviceEnum::Video => flag.video_flag.store(f, Ordering::Release),
            DeviceEnum::Audio => flag.audio_flag.store(f, Ordering::Release),
        }
        Ok(())
    }

    pub async fn is_labeled(&self, lab: &Label) -> bool {
        let label_flag_map = self.0.write().await;
        label_flag_map.contains_key(lab)
    }
}

#[derive(Default, Debug)]
pub struct DeviceReceiver {
    video_receiver: Option<Receiver<Vec<u8>>>,
    audio_receiver: Option<Receiver<Vec<u8>>>,
}

impl DeviceReceiver {
    pub fn new(
        video_receiver: Option<Receiver<Vec<u8>>>,
        audio_receiver: Option<Receiver<Vec<u8>>>,
    ) -> Self {
        Self {
            video_receiver,
            audio_receiver,
        }
    }
}

#[derive(Default)]
pub struct LabelReceiverMap(RwLock<HashMap<Label, DeviceReceiver>>);

impl LabelReceiverMap {
    pub async fn insert(
        &self,
        k: Label,
        v: Receiver<Vec<u8>>,
        device_enum: DeviceEnum,
    ) -> Result<()> {
        let mut label_receiver_map = self.0.write().await;

        match device_enum {
            DeviceEnum::Video => {
                label_receiver_map
                    .entry(k)
                    .and_modify(|receiver| receiver.video_receiver = Some(v.clone()))
                    .or_insert(DeviceReceiver::new(Some(v), None));
            }
            DeviceEnum::Audio => {
                label_receiver_map
                    .entry(k)
                    .and_modify(|receiver| receiver.audio_receiver = Some(v.clone()))
                    .or_insert(DeviceReceiver::new(None, Some(v)));
            }
        }
        info!("{:?}", label_receiver_map);
        Ok(())
    }

    pub async fn remove(&self, k: Label) -> Result<()> {
        let mut label_receiver_map = self.0.write().await;
        label_receiver_map
            .remove(&k)
            .ok_or(CaptureError::EmptyLabelName)?;
        Ok(())
    }
}

fn copy(arr: &mut [u8; 100], bytes: &[u8]) {
    if bytes.len() > 100 {
        arr.copy_from_slice(&[1u8; 100]);
    } else {
        arr[..bytes.len()].copy_from_slice(bytes);
    }
}

pub type IsEnd = AtomicBool;
