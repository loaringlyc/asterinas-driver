// SPDX-License-Identifier: MPL-2.0

//! The audio devices of Asterinas.
#![no_std]
#![deny(unsafe_code)]
#![feature(fn_traits)]

extern crate alloc;

use alloc::{collections::BTreeMap, fmt::Debug, string::String, sync::Arc, vec::Vec};
use core::any::Any;

use component::{init_component, ComponentInitError};
use ostd::{
    // mm::{Infallible, VmReader},
    mm::{Infallible, VmReader},
    sync::SpinLock,
};
use spin::Once;

pub type SoundCallback = dyn Fn(VmReader<Infallible>) + Send + Sync;

pub trait AnySoundDevice: Send + Sync + Any + Debug {
    /// 播放音频数据
    // fn play(&mut self, data: &[u8]);

    /// 录制音频数据
    fn record(&mut self, buffer: &mut [u8]);

    /// 注册播放回调
    // fn register_playback_callback(&self, callback: &'static SoundCallback);

    /// 注册录制回调
    fn register_callback(&self, callback: &'static SoundCallback);
}

pub fn register_device(name: String, device: Arc<SpinLock<dyn AnySoundDevice>>) {
    COMPONENT
        .get()
        .unwrap()
        .audio_device_table
        .lock()
        .insert(name, device);
}

pub fn all_devices() -> Vec<(String, Arc<SpinLock<dyn AnySoundDevice>>)> {
    let audio_devs = COMPONENT.get().unwrap().audio_device_table.lock();
    audio_devs
        .iter()
        .map(|(name, device)| (name.clone(), device.clone()))
        .collect()
}

static COMPONENT: Once<Component> = Once::new();

#[init_component]
fn component_init() -> Result<(), ComponentInitError> {
    let component = Component::init()?;
    COMPONENT.call_once(|| component);
    Ok(())
}

#[derive(Debug)]
struct Component {
    audio_device_table: SpinLock<BTreeMap<String, Arc<SpinLock<dyn AnySoundDevice>>>>,
}

impl Component {
    /// 初始化组件
    pub fn init() -> Result<Self, ComponentInitError> {
        Ok(Self {
            audio_device_table: SpinLock::new(BTreeMap::new()),
        })
    }
}