use core::hint::spin_loop;

use alloc::{boxed::Box, sync::Arc};

use config::{SoundFeatures, VirtioSoundConfig};
use log::debug;
use ostd::{early_println, mm::{DmaDirection, DmaStream, DmaStreamSlice, FrameAllocOptions, HasDaddr, VmIo}, sync::SpinLock};

use super::*;
use crate::{
    device::VirtioDeviceError, queue::{QueueError, VirtQueue}, transport::{ConfigManager, VirtioTransport}
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
    // Send buffer for queue.
    snd_req: DmaStream,     
    // Recv buffer for queue.
    snd_resp: DmaStream,    
}

impl SoundDevice {
    const QUEUE_SIZE: u16 = 2;
    pub fn negotiate_features(features: u64) -> u64 {
        let features = SoundFeatures::from_bits_truncate(features);
        // TODO: Implement negotiate!
        features.bits()
    }

    pub fn init(mut transport: Box<dyn VirtioTransport>) -> Result<(), VirtioDeviceError> {
        // let offered_features = transport.read_device_features();
        // let negotiated_features = Self::negotiate_features(offered_features);
        // transport.ack_features(negotiated_features);

        // if add ctls_negotiated, the number of control would be 0?
        // negotiate part is done in virtio_component_init() in src/lib.rs together
        // let ctls_negotiated = (negotiated_features & SoundFeatures::VIRTIO_SND_F_CTLS.bits()) != 0;

        let config_manager = VirtioSoundConfig::new_manager(transport.as_ref());
        let sound_config = config_manager.read_config();

        debug!("virtio_sound_config = {:?}", sound_config);
        early_println!(
            "Load virtio-sound successfully. Config = {:?}",
            sound_config
        );

        const CONTROLQ_INDEX: u16 = 0;
        const EVENTQ_INDEX: u16 = 1;
        const TXQ_INDEX: u16 = 2;
        const RXQ_INDEX: u16 = 3;
        let control_queue = 
            SpinLock::new(VirtQueue::new(CONTROLQ_INDEX, Self::QUEUE_SIZE, transport.as_mut())?);
        let event_queue = 
            SpinLock::new(VirtQueue::new(EVENTQ_INDEX, Self::QUEUE_SIZE, transport.as_mut())?);
        let tx_queue = 
            SpinLock::new(VirtQueue::new(TXQ_INDEX, Self::QUEUE_SIZE, transport.as_mut())?);
        let rx_queue = 
            SpinLock::new(VirtQueue::new(RXQ_INDEX, Self::QUEUE_SIZE, transport.as_mut())?);
        
        let snd_req = {
            let segment = FrameAllocOptions::new().alloc_segment(1).unwrap();
            DmaStream::map(segment.into(), DmaDirection::Bidirectional, false).unwrap()
        };

        let snd_resp = {
            let segment = FrameAllocOptions::new().alloc_segment(1).unwrap();
            DmaStream::map(segment.into(), DmaDirection::Bidirectional, false).unwrap()
        };

        let device = Arc::new(SoundDevice {
            config_manager,
            transport: SpinLock::new(transport),
            control_queue,
            event_queue,
            tx_queue,
            rx_queue,
            snd_req,
            snd_resp,
        });

        // Register irq callbacks
        let mut transport = device.transport.disable_irq().lock();
        // TODO: callbacks for microphone input

        transport.finish_init();
        drop(transport);
        

        Ok(())
    }

    fn request<Req: Pod>(&mut self, req: Req) -> Result<VirtioSndHdr, VirtioDeviceError>{
        // 参数req表示一个request结构体，存放request信息，如VirtIOSndQueryInfo 
        // 这里的Pod trait可以保证可转换为一连串bytes，然后就可以用len的到长度了
        let req_slice = {
            let req_slice = 
                DmaStreamSlice::new(&self.snd_req, 0, req.as_bytes().len());
            req_slice.write_val(0, &req).unwrap();
            req_slice.sync().unwrap();
            req_slice
        }; // 将req写入snd_req这个DmaStream

        let resp_slice = {
            let resp_slice = 
                DmaStreamSlice::new(&self.snd_resp, 0, SND_HDR_SIZE);
            resp_slice
        }; // 希望写入snd_resp这个DmaStream的前面 （目前只预留 返回一个最基础的OK或者ERR 的长度）
        
        let mut queue = self.control_queue
            .disable_irq()
            .lock();
        let token = queue
            .add_dma_buf(&[&req_slice], &[&resp_slice])
            .expect("add queue failed");
        if queue.should_notify() {
            queue.notify();
        }
        while !queue.can_pop() {
            spin_loop();
        }
        queue.pop_used_with_token(token).expect("pop used failed");

        resp_slice.sync().unwrap();
        let resp: VirtioSndHdr = resp_slice.read_val(0).unwrap();

        Ok(resp) //没有考虑报错
    }

    /// Transfer PCM frame to device, based on the stream type(OUTPUT/INPUT).
    ///
    /// Currently supports only output stream.
    ///
    /// This is a blocking method that will not return until the audio playback is complete.
    pub fn pcm_xfer(&mut self, stream_id: u32, frames: &[u8]) -> Result<(), VirtioDeviceError> {
        const U32_SIZE: usize = size_of::<u32>();
        // set up & set params
        Ok(())
    }

}

