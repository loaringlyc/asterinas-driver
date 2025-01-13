use alloc::collections::btree_map::BTreeMap;
use alloc::{vec::Vec,vec};
use alloc::{boxed::Box, sync::Arc};
use core::hint::spin_loop;

use config::{SoundFeatures, VirtioSoundConfig};
use log::{debug, error, info, warn};
use ostd::mm::VmIo;
use ostd::Pod;
use ostd::{early_println, sync::SpinLock};
use crate::{
    device::VirtioDeviceError, queue::{QueueError, VirtQueue}, transport::{ConfigManager, VirtioTransport}
};
use super::config;
use super::*;
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


    fn request<Req: Pod>(&mut self, req: Req) -> Result<VirtioSndHdr, VirtioDeviceError>{
        // 参数req表示一个request结构体，存放request信息，如VirtIOSndQueryInfo 
        // 这里的Pod trait可以保证可转换为一连串bytes，然后就可以用len的到长度了
        let req_slice = {
            let req_slice = 
                DmaStreamSlice::new(&self.send_buffer, 0, req.as_bytes().len());
            req_slice.write_val(0, &req).unwrap();
            req_slice.sync().unwrap();
            req_slice
        }; // 将req写入snd_req这个DmaStream

        let resp_slice = {
            let resp_slice = 
                DmaStreamSlice::new(&self.receive_buffer, 0, SND_HDR_SIZE);
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
        stream_count: u32,  // The number of streams that need to be queried
    ) -> Result<Vec<VirtioSndPcmInfo>, VirtioDeviceError> {
        // Check if stream_dart_id+stream_comnt exceeds the number of streams supported by the device. If exceeded, return an error.
        if stream_start_id + stream_count > self.config_manager.read_config().streams {
            error!("stream_start_id + stream_count > streams! There are not enough streams to be queried!");
            return Err(VirtioDeviceError::IoError);
        }

        // Construct a request header
        let request_hdr = VirtioSndHdr::from(ItemInformationRequestType::RPcmInfo);
        let hdr = self.request(VirtioSndQueryInfo {
            hdr: request_hdr,
            start_id: stream_start_id,
            count: stream_count,
            size: size_of::<VirtioSndPcmInfo>() as u32,
        })?;// call self.request to send the request and get the response
        if hdr != RequestStatusCode::Ok.into() { // if failed(not OK) then Error
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

        //
        if chmaps_start_id + chmaps_count > self.config_manager.read_config().streams {
            error!("chmaps_start_id + chmaps_count > self.chmaps");
            return Err(VirtioDeviceError::IoError);
        }

        // Construct a request header
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

    pub fn pcm_set_params(
        &mut self,
        stream_id: u32,
        buffer_bytes: u32,
        period_bytes: u32,
        features: PCM_FEATURES,
        channels: u8,
        format: PCM_FORMAT,
        rate: PCM_RATE,
    ) -> Result<(),VirtioDeviceError> {
        if !self.set_up {
            self.set_up()?;
            self.set_up = true;
        }
        if period_bytes == 0 || period_bytes > buffer_bytes || buffer_bytes % period_bytes != 0 {
            return Err(VirtioDeviceError::InvalidParam);
        }
        let request_hdr = VirtioSndHdr::from(CommandCode::RPcmSetParams);
        let rsp = self.request(VirtioSndPcmSetParams {
            hdr: VirtioSndPcmHdr {
                hdr: request_hdr,
                stream_id,
            },
            buffer_bytes,
            period_bytes,
            features: features.bits(),
            channels,
            format: format.into(),
            rate: rate.into(),
            padding: 0,
        })?;
        // rsp is just a header, so it can be compared with VirtIOSndHdr
        if rsp == VirtioSndHdr::from(RequestStatusCode::Ok) {
            self.pcm_parameters[stream_id as usize] = PcmParameters {
                setup: true,
                buffer_bytes,
                period_bytes,
                features,
                channels,
                format,
                rate,
            };
            Ok(())
        } else {
            Err(VirtioDeviceError::IoError)
        }
    }

    /// Prepare a stream with specified stream ID.
    pub fn pcm_prepare(&mut self, stream_id: u32) -> Result<(),VirtioDeviceError>  {
        if !self.set_up {
            self.set_up()?;
            self.set_up = true;
        }
        let request_hdr = VirtioSndHdr::from(CommandCode::RPcmPrepare);
        let rsp = self.request(VirtioSndPcmHdr {
            hdr: request_hdr,
            stream_id,
        })?;
        // rsp is just a header, so it can be compared with VirtIOSndHdr
        if rsp == VirtioSndHdr::from(RequestStatusCode::Ok) {
            Ok(())
        } else {
            Err(VirtioDeviceError::IoError)
        }
    }

    /// Release a stream with specified stream ID.
    pub fn pcm_release(&mut self, stream_id: u32) -> Result<(),VirtioDeviceError> {
        if !self.set_up {
            self.set_up()?;
            self.set_up = true;
        }
        let request_hdr = VirtioSndHdr::from(CommandCode::RPcmRelease);
        let rsp = self.request(VirtioSndPcmHdr {
            hdr: request_hdr,
            stream_id,
        })?;
        // rsp is just a header, so it can be compared with VirtIOSndHdr
        if rsp == VirtioSndHdr::from(RequestStatusCode::Ok) {
            Ok(())
        } else {
            Err(VirtioDeviceError::IoError)
        }
    }

    /// Start a stream with specified stream ID.
    pub fn pcm_start(&mut self, stream_id: u32) -> Result<(),VirtioDeviceError> {
        if !self.set_up {
            self.set_up()?;
            self.set_up = true;
        }
        let request_hdr = VirtioSndHdr::from(CommandCode::RPcmStart);
        let rsp = self.request(VirtioSndPcmHdr {
            hdr: request_hdr,
            stream_id,
        })?;
        // rsp is just a header, so it can be compared with VirtIOSndHdr
        if rsp == VirtioSndHdr::from(RequestStatusCode::Ok) {
            Ok(())
        } else {
            Err(VirtioDeviceError::IoError)
        }
    }

    /// Stop a stream with specified stream ID.
    pub fn pcm_stop(&mut self, stream_id: u32) -> Result<(),VirtioDeviceError> {
        if !self.set_up {
            self.set_up()?;
            self.set_up = true;
        }
        let request_hdr = VirtioSndHdr::from(CommandCode::RPcmStop);
        let rsp = self.request(VirtioSndPcmHdr {
            hdr: request_hdr,
            stream_id,
        })?;
        // rsp is just a header, so it can be compared with VirtIOSndHdr
        if rsp == VirtioSndHdr::from(RequestStatusCode::Ok) {
            Ok(())
        } else {
            Err(VirtioDeviceError::IoError)
        }
    }

}
