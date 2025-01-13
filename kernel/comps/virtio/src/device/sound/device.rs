use alloc::collections::btree_map::BTreeMap;
use alloc::{vec::Vec,vec};
use alloc::{boxed::Box, sync::Arc};

use config::{SoundFeatures, VirtioSoundConfig};
use log::{debug, error, info, warn};
use ostd::Pod;
use ostd::{early_println, sync::SpinLock};

use super::config;
use super::*;
use crate::{
    device::VirtioDeviceError,
    queue::VirtQueue,
    transport::{ConfigManager, VirtioTransport},
};
use ostd::{
    mm::{DmaDirection, DmaStream, DmaStreamSlice, FrameAllocOptions, VmReader},
    sync::{LocalIrqDisabled, RwLock},
    trap::TrapFrame,
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
    send_buffer: DmaStream,
    receive_buffer: DmaStream,

    pcm_infos: Option<Vec<VirtioSndPcmInfo>>,
    // jack_infos: Option<Vec<VirtIOSndJackInfo>>,
    chmap_infos: Option<Vec<VirtioSndChmapInfo>>,

    pcm_parameters:Vec<PcmParameters>,

    set_up: bool,

    token_rsp: BTreeMap<u16, Box<VirtioSndPcmStatus>>,

    pcm_states: Vec<PCMState>,

    token_buf: BTreeMap<u16, Vec<u8>>
}


impl SoundDevice {
    pub fn negotiate_features(features: u64) -> u64 {
        let features = SoundFeatures::from_bits_truncate(features);
        //
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
        debug!("virtio_sound_config={:?}",config_manager.read_config());
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
            SpinLock::new(VirtQueue::new(CONTROLQ_INDEX, 2, transport.as_mut())?);
        let event_queue = 
            SpinLock::new(VirtQueue::new(EVENTQ_INDEX, 2, transport.as_mut())?);
        let tx_queue = 
            SpinLock::new(VirtQueue::new(TXQ_INDEX, 2, transport.as_mut())?);
        let rx_queue = 
            SpinLock::new(VirtQueue::new(RXQ_INDEX, 2, transport.as_mut())?);

        
        let send_buffer = {
            let segment = FrameAllocOptions::new().alloc_segment(1).unwrap();
            DmaStream::map(segment.into(), DmaDirection::ToDevice, false).unwrap()
        };

        let receive_buffer = {
            let segment = FrameAllocOptions::new().alloc_segment(1).unwrap();
            DmaStream::map(segment.into(), DmaDirection::FromDevice, false).unwrap()
        };

        let mut pcm_parameters = vec![];
        for _ in 0..sound_config.streams {
            pcm_parameters.push(PcmParameters::default());
        }

        let device = Arc::new(SoundDevice {
            config_manager,
            transport: SpinLock::new(transport),
            control_queue,
            event_queue,
            tx_queue,
            rx_queue,
            pcm_infos: None,
            chmap_infos: None,
            send_buffer,
            receive_buffer,
            pcm_parameters,
            set_up: false,
            token_rsp: BTreeMap::new(),
            pcm_states: vec![],
            token_buf: BTreeMap::new(),
        });

        // Register irq callbacks
        let mut transport = device.transport.disable_irq().lock();
        todo!();// Register irq callbacks

        transport.finish_init();

        let e_queue=event_queue.disable_irq().lock();
        if e_queue.should_notify() {
            debug!("notify event queue");
            e_queue.notify();
        }

        drop(transport);
        

        Ok(())
    }


    // fn request<Req: IntoBytes + Immutable>(&mut self, req: Req) -> Result<VirtioSndHdr,VirtioDeviceError> {
    //     self.control_queue.add_notify_wait_pop(
    //         &[req.as_bytes()],
    //         &mut [self.queue_buf_recv.as_mut_bytes()],
    //         &mut self.transport,
    //     )?;
    //     Ok(VirtioSndHdr::read_from_prefix(&self.queue_buf_recv)
    //         .unwrap()
    //         .0)
    // }


    fn request<Req>(&mut self, req: Req) -> Result<VirtioSndHdr,VirtioDeviceError>{
        todo!()
    }

    fn set_up(&mut self) -> Result<(), VirtioDeviceError> {

        // init pcm info
        let pcm_infos = self.pcm_info(0, self.config_manager.read_config().streams)?;
        for pcm_info in &pcm_infos {
            info!("[sound device] pcm_info: {}", pcm_info);
        }
        self.pcm_infos = Some(pcm_infos);

        // init chmap info
        if let Ok(chmap_infos) = self.chmap_info(0, self.config_manager.read_config().chmaps) {
            for chmap_info in &chmap_infos {
                info!("[sound device] chmap_info: {}", chmap_info);
            }
            self.chmap_infos = Some(chmap_infos);
        } else {
            self.chmap_infos = Some(vec![]);
            warn!("[sound device] Error getting chmap infos");
        }

        // set pcm state to default
        for _ in 0..self.config_manager.read_config().streams {
            self.pcm_states.push(PCMState::default());
        }
        Ok(())
    }


    fn pcm_info(
        &mut self,
        stream_start_id: u32,
        stream_count: u32,
    ) -> Result<Vec<VirtioSndPcmInfo>, VirtioDeviceError> {
        if stream_start_id + stream_count > self.config_manager.read_config().streams {
            error!("stream_start_id + stream_count > streams! There are not enough streams to be queried!");
            return Err(VirtioDeviceError::IoError);
        }
        let request_hdr = VirtioSndHdr::from(ItemInformationRequestType::RPcmInfo);
        let hdr = self.request(VirtioSndQueryInfo {
            hdr: request_hdr,
            start_id: stream_start_id,
            count: stream_count,
            size: size_of::<VirtioSndPcmInfo>() as u32,
        })?;
        if hdr != RequestStatusCode::Ok.into() {
            return Err(VirtioDeviceError::IoError);
        }
        // read struct VirtIOSndPcmInfo
        let mut pcm_infos = vec![];
        
        for i in 0..stream_count as usize {
            const HDR_SIZE: usize = size_of::<VirtioSndHdr>();
            const PCM_INFO_SIZE: usize = size_of::<VirtioSndPcmInfo>();
            let start_byte_idx = HDR_SIZE + i * PCM_INFO_SIZE;
            let end_byte_idx = HDR_SIZE + (i + 1) * PCM_INFO_SIZE;
            if end_byte_idx > self.receive_buffer.nbytes() {
                return Err(VirtioDeviceError::BufferOverflow);
            }
            let reader = self.receive_buffer.reader().unwrap();
            let mut reader = reader.skip(start_byte_idx).limit(PCM_INFO_SIZE);
            // let pcm_info = VirtioSndPcmInfo::from_bytes(
            //     &self.receive_buffer[start_byte_idx..end_byte_idx],
            // )
            // .unwrap();
            let mut buffer = [0u8; size_of::<VirtioSndPcmInfo>()];
            reader.read(&mut buffer.as_mut_slice().into()); // 读取数据到缓冲区
            let pcm_info = VirtioSndPcmInfo::from_bytes(&buffer); // 解析数据
            pcm_infos.push(pcm_info);
        }
        Ok(pcm_infos)
    }

    /// Query information about the available chmaps.
    fn chmap_info(
        &mut self,
        chmaps_start_id: u32,
        chmaps_count: u32,
    ) -> Result<Vec<VirtioSndChmapInfo>,VirtioDeviceError> {
        if chmaps_start_id + chmaps_count > self.config_manager.read_config().streams {
            error!("chmaps_start_id + chmaps_count > self.chmaps");
            return Err(VirtioDeviceError::IoError);
        }
        let hdr = self.request(VirtioSndQueryInfo {
            hdr: ItemInformationRequestType::RChmapInfo.into(),
            start_id: chmaps_start_id,
            count: chmaps_count,
            size: size_of::<VirtioSndQueryInfo>() as u32,
        })?;
        if hdr != RequestStatusCode::Ok.into() {
            return Err(VirtioDeviceError::IoError);
        }
        let mut chmap_infos = vec![];
        for i in 0..chmaps_count as usize {
            const OFFSET: usize = size_of::<VirtioSndHdr>();
            const CHAMP_INFO_SIZE: usize = size_of::<VirtioSndQueryInfo>();
            let start_byte = OFFSET + i * CHAMP_INFO_SIZE;
            let end_byte = OFFSET + (i + 1) * CHAMP_INFO_SIZE;
            if end_byte > self.receive_buffer.nbytes() {
                return Err(VirtioDeviceError::BufferOverflow);
            }
            let reader = self.receive_buffer.reader().unwrap();
            let mut reader = reader.skip(start_byte).limit(CHAMP_INFO_SIZE);
            // let chmap_info =
            //     VirtioSndChmapInfo::read_from_bytes(&self.queue_buf_recv[start_byte..end_byte])
            //         .unwrap();
            let mut buffer = [0u8; size_of::<VirtioSndPcmInfo>()];
            reader.read(&mut buffer.as_mut_slice().into()); // 读取数据到缓冲区
            let chmap_info = VirtioSndChmapInfo::from_bytes(&buffer); // 解析数据
            chmap_infos.push(chmap_info);
        }
        Ok(chmap_infos)
    }
}
