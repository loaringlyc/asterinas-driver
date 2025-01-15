use aster_sound;

use super::*;
use crate::{
    events::IoEvents,
    fs::inode_handle::FileIo,
    prelude::*,
    process::signal::{PollHandle, Pollable},
};


pub struct Sound;

impl Device for Sound {
    fn type_(&self) -> DeviceType {
        DeviceType::CharDevice
    }

    fn id(&self) -> DeviceId {
        // Same value with Linux
        DeviceId::new(6, 6)
    }

    fn open(&self) -> Result<Option<Arc<dyn FileIo>>> {
        let device=&aster_sound::all_devices()[0].1;
        device.lock().test_device();
        Ok(Some(Arc::new(Sound)))
    }
}

impl Pollable for Sound {
    fn poll(&self, mask: IoEvents, _: Option<&mut PollHandle>) -> IoEvents {
        let events = IoEvents::IN | IoEvents::OUT;
        events & mask
    }
}

impl FileIo for Sound {
    fn read(&self, _writer: &mut VmWriter) -> Result<usize> {
        Ok(0)
    }

    fn write(&self, reader: &mut VmReader) -> Result<usize> {
        Ok(reader.remain())
    }
}