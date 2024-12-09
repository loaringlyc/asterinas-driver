// SPDX-License-Identifier: MPL-2.0

use alloc::{boxed::Box, sync::Arc};
use log::debug;
use ostd::sync::SpinLock;

use crate::{
    config::{SoundFeatures, VirtioSoundConfig},
    queue::VirtQueue,
    transport::{ConfigManager, VirtioTransport},
    VirtioDeviceError,
};

pub struct SoundDevice {
    config_manager: ConfigManager<VirtioSoundConfig>,
    transport: SpinLock<Box<dyn VirtioTransport>>,

    /// 0: The control queue is used for sending control messages from the driver to the device.
    /// 1: The event queue is used for sending notifications from the device to the driver.
    /// 2: The tx queue is used to send PCM frames for output streams.
    /// 3: The rx queue is used to receive PCM frames from input streams.
    control_queue: SpinLock<VirtQueue>,
    event_queue: SpinLock<VirtQueue>,
    tx_queue: SpinLock<VirtQueue>,
    rx_queue: SpinLock<VirtQueue>,
}

impl SoundDevice {
    pub fn negotiate_features(features: u64) -> u64 {
        let features = SoundFeatures::from_bits_truncate(features);
        // 
        features.bits()
    }

    pub fn init(mut transport: Box<dyn VirtioTransport>) -> Result<(), VirtioDeviceError> {
        let offered_features = transport.read_device_features();
        let negotiated_features = Self::negotiate_features(offered_features);
        // transport.ack_features(negotiated_features);

        let ctls_negotiated = (negotiated_features & SoundFeatures::VIRTIO_SND_F_CTLS.bits()) != 0;

        let config_manager = VirtioSoundConfig::new_manager(transport.as_ref());
        let sound_config = config_manager.read_config(ctls_negotiated);

        debug!("virtio_sound_config = {:?}", sound_config);

        const CONTROLQ_INDEX: u16 = 0;
        const EVENTQ_INDEX: u16 = 1;
        const TXQ_INDEX: u16 = 2;
        const RXQ_INDEX: u16 = 3;

        let control_queue = SpinLock::new(VirtQueue::new(CONTROLQ_INDEX, 2, transport.as_mut())?);
        let event_queue = SpinLock::new(VirtQueue::new(EVENTQ_INDEX, 2, transport.as_mut())?);
        let tx_queue = SpinLock::new(VirtQueue::new(TXQ_INDEX, 2, transport.as_mut())?);
        let rx_queue = SpinLock::new(VirtQueue::new(RXQ_INDEX, 2, transport.as_mut())?);

        let device = Arc::new(SoundDevice {
            config_manager,
            transport: SpinLock::new(transport),
            control_queue,
            event_queue,
            tx_queue,
            rx_queue,
        });


        {
            let mut transport = device.transport.lock();
            transport.finish_init();
        }

        Ok(())
    }

}
