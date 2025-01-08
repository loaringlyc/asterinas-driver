use core::mem::offset_of;

use aster_util::safe_ptr::SafePtr;
use ostd::Pod;

use crate::transport::{ConfigManager, VirtioTransport};
bitflags::bitflags! {
    pub struct SoundFeatures: u64 {
        //Device supports control elements.
        const VIRTIO_SND_F_CTLS = 1 << 0;
    }
}

#[derive(Debug, Pod, Clone, Copy)]
#[repr(C)]
pub struct VirtioSoundConfig {
    pub jacks: u32, // (driver-read-only) indicates a total number of all available jacks.
    pub streams: u32, // (driver-read-only) indicates a total number of all available PCM streams.
    pub chmaps: u32, // (driver-read-only) indicates a total number of all available channel maps.
    pub controls: u32, // (driver-read-only) indicates a total number of all available control elements if VIRTIO_SND_F_CTLS has been negotiated.
}

impl VirtioSoundConfig {
    pub(super) fn new_manager(transport: &dyn VirtioTransport) -> ConfigManager<Self> {
        let safe_ptr = transport
            .device_config_mem()
            .map(|mem| SafePtr::new(mem, 0));
        let bar_space = transport.device_config_bar();
        ConfigManager::new(safe_ptr, bar_space)
    }
}

impl ConfigManager<VirtioSoundConfig> {
    pub(super) fn read_config(&self) -> VirtioSoundConfig {
        let mut sound_config = VirtioSoundConfig::new_uninit();
        sound_config.jacks = self
            .read_once::<u32>(offset_of!(VirtioSoundConfig, jacks))
            .unwrap_or(0);
        sound_config.streams = self
            .read_once::<u32>(offset_of!(VirtioSoundConfig, streams))
            .unwrap_or(0);
        sound_config.chmaps = self
            .read_once::<u32>(offset_of!(VirtioSoundConfig, chmaps))
            .unwrap_or(0);
        // if ctls_negotiated {
        //     sound_config.controls = self
        //         .read_once::<u32>(offset_of!(VirtioSoundConfig, controls))
        //         .unwrap_or(0);
        // } else {
        //     sound_config.controls = 0;
        // }
        sound_config
    }
}
