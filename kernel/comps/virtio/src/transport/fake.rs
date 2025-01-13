// Fake Virtio transport for unit test

use super::{DeviceStatus, VirtioTransport};
use crate::{
    queue::{fake_read_write_queue, Descriptor},
    Error, PhysAddr,
    VirtioDeviceType
};
use alloc::{sync::Arc, vec::Vec};
use core::{
    fmt::{self, Debug, Formatter},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use ostd::{sync::Mutex, thread , Pod}; // What is the use of thread?
// use zerocopy::{FromBytes, Immutable, IntoBytes};

#[derive(Debug)]

pub struct FakeTransport<C> {
    /// The type of device which the transport should claim to be for.
    pub device_type: VirtioDeviceType,
    /// The maximum queue size supported by the transport.
    pub max_queue_size: u32,
    /// The device features which should be reported by the transport.
    pub device_features: u64,
    /// The mutable state of the transport.
    pub state: Arc<Mutex<State<C>>>,
}

pub struct State<C> {
    /// The status of the fake device.
    pub status: DeviceStatus,
    /// The features which the driver says it supports.
    pub driver_features: u64,
    /// The guest page size set by the driver.
    pub guest_page_size: u32,
    /// Whether the transport has an interrupt pending.
    pub interrupt_pending: bool,
    /// The state of the transport's queues.
    pub queues: Vec<QueueStatus>,
    /// The config generation which the transport should report.
    pub config_generation: u32,
    /// The state of the transport's VirtIO configuration space.
    pub config_space: C,
}

impl<C> Debug for State<C> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("State")
            .field("status", &self.status)
            .field("driver_features", &self.driver_features)
            .field("guest_page_size", &self.guest_page_size)
            .field("interrupt_pending", &self.interrupt_pending)
            .field("queues", &self.queues)
            .field("config_generation", &self.config_generation)
            .field("config_space", &"...")
            .finish()
    }
}

