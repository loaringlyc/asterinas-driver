use alloc::{boxed::Box, collections::btree_map::BTreeMap, sync::Arc, vec, vec::Vec};
use core::{array, hint::spin_loop};

use config::{SoundFeatures, VirtioSoundConfig};
use log::{debug, error, info, warn};
use ostd::{
    early_println,
    mm::{DmaDirection, DmaStream, DmaStreamSlice, FrameAllocOptions, VmIo, VmReader},
    sync::{Mutex, SpinLock},
    Pod,
};

use super::{config, *};
use crate::{
    device::VirtioDeviceError,
    queue::VirtQueue,
    transport::{ConfigManager, VirtioTransport},
};
// use ostd::{mm::VmReader,
//     sync::{LocalIrqDisabled, RwLock},
//     trap::TrapFrame,
// };

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

    pcm_parameters: Vec<PcmParameters>, // 是否需要考虑锁的问题？

    set_up: bool,

    token_rsp: BTreeMap<u16, u16>,

    pcm_states: Vec<PCMState>,

    token_buf: BTreeMap<u16, u16>,
}

impl SoundDevice {
    const QUEUE_SIZE: u16 = 16;
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
        debug!("virtio_sound_config={:?}", config_manager.read_config());
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
        let control_queue = SpinLock::new(VirtQueue::new(
            CONTROLQ_INDEX,
            Self::QUEUE_SIZE,
            transport.as_mut(),
        )?);
        let event_queue = SpinLock::new(VirtQueue::new(
            EVENTQ_INDEX,
            Self::QUEUE_SIZE,
            transport.as_mut(),
        )?);
        let tx_queue = SpinLock::new(VirtQueue::new(
            TXQ_INDEX,
            Self::QUEUE_SIZE,
            transport.as_mut(),
        )?);
        let rx_queue = SpinLock::new(VirtQueue::new(
            RXQ_INDEX,
            Self::QUEUE_SIZE,
            transport.as_mut(),
        )?);

        let send_buffer = {
            let segment = FrameAllocOptions::new().alloc_segment(1).unwrap();
            DmaStream::map(segment.into(), DmaDirection::ToDevice, false).unwrap()
        };

        let receive_buffer = {
            let segment = FrameAllocOptions::new().alloc_segment(1).unwrap();
            DmaStream::map(segment.into(), DmaDirection::FromDevice, false).unwrap()
        };
        early_println!(
            "The length of receive_buffer is {:?}",
            receive_buffer.nbytes()
        );

        let mut pcm_parameters = vec![];
        for _ in 0..sound_config.streams {
            pcm_parameters.push(PcmParameters::default());
        }

        let device = Arc::new(Mutex::new(SoundDevice {
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
        }));
        
        let cloned_device = Arc::clone(&device);       
        test_device(device);     
        let device_lock = cloned_device.lock();
        // // Register irq callbacks
        let mut transport = device_lock
            .transport
            .disable_irq()
            .lock();
        // // TODO: callbacks for microphone input

        transport.finish_init();

        drop(transport);

        Ok(())
    }

    fn request<Req: Pod>(&mut self, req: Req) -> Result<VirtioSndHdr, VirtioDeviceError> {
        // 参数req表示一个request结构体，存放request信息，如VirtIOSndQueryInfo
        // 这里的Pod trait可以保证可转换为一连串bytes，然后就可以用len的到长度了
        let req_slice = {
            let req_slice = DmaStreamSlice::new(&self.send_buffer, 0, req.as_bytes().len());
            req_slice.write_val(0, &req).unwrap();
            req_slice.sync().unwrap();
            req_slice
        }; // 将req写入snd_req这个DmaStream

        //TODO: for different request, give different buffer length
        //      PcmInfo -> SND_HDR_SIZE + stream_size * PCM_INFO_SIZE

        let resp_slice = {
            let resp_slice =
                DmaStreamSlice::new(&self.receive_buffer, 0, SND_HDR_SIZE + 2 * PCM_INFO_SIZE);
            resp_slice
        }; // 希望写入snd_resp这个DmaStream的前面 （目前只预留 返回一个最基础的OK或者ERR 的长度）

        let mut queue = self.control_queue.disable_irq().lock();
        let token = queue
            .add_dma_buf(&[&req_slice], &mut [&resp_slice])
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
            early_println!("[sound device] pcm_info: {:?}", pcm_info)
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
        stream_count: u32, // The number of streams that need to be queried
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
        })?; // call self.request to send the request and get the response
        if hdr != RequestStatusCode::Ok.into() {
            // if failed(not OK) then Error
            early_println!("{:?}", hdr);
            return Err(VirtioDeviceError::IoError);
        }
        // read struct VirtIOSndPcmInfo
        let mut pcm_infos = vec![];

        for i in 0..stream_count as usize {
            const HDR_SIZE: usize = size_of::<VirtioSndHdr>();
            const PCM_INFO_SIZE: usize = size_of::<VirtioSndPcmInfo>();
            let start_byte_idx = HDR_SIZE + i * PCM_INFO_SIZE; //
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
        /*
        -------------------------------------------------------
                 offset             |         content
        -------------------------------------------------------
                   0                |          Header
        -------------------------------------------------------
                 HDR_SIZE           |     The first PCM info
        -------------------------------------------------------
          HDR_SIZE + PCM_INFO_SIZE  |     The second PCM info
        -------------------------------------------------------
         */
        Ok(pcm_infos)
    }

    /// Query information about the available chmaps.
    fn chmap_info(
        &mut self,
        chmaps_start_id: u32,
        chmaps_count: u32,
    ) -> Result<Vec<VirtioSndChmapInfo>, VirtioDeviceError> {
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
        features: PcmFeatures,
        channels: u8,
        format: PcmFormat,
        rate: PcmRate,
    ) -> Result<(), VirtioDeviceError> {
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
    pub fn pcm_prepare(&mut self, stream_id: u32) -> Result<(), VirtioDeviceError> {
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
    pub fn pcm_release(&mut self, stream_id: u32) -> Result<(), VirtioDeviceError> {
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
    pub fn pcm_start(&mut self, stream_id: u32) -> Result<(), VirtioDeviceError> {
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
    pub fn pcm_stop(&mut self, stream_id: u32) -> Result<(), VirtioDeviceError> {
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

    /// Transfer PCM frame to device, based on the stream type(OUTPUT/INPUT).
    ///
    /// Currently supports only output stream.
    ///
    /// This is a blocking method that will not return until the audio playback is complete.
    pub fn pcm_xfer(&mut self, stream_id: u32, frames: &[u8]) -> Result<(), VirtioDeviceError> {
        const U32_SIZE: usize = size_of::<u32>();
        if !self.set_up {
            self.set_up()?;
            self.set_up = true;
        }
        if !self.pcm_parameters[stream_id as usize].setup {
            warn!("Please set parameters for a stream before using it!");
            return Err(VirtioDeviceError::IoError);
        }
        let stream_id_bytes = stream_id.to_le_bytes();
        let period_size = self.pcm_parameters[stream_id as usize].period_bytes as usize;

        // 将 frames 字节数组按照 period_size 分割成多个小块
        let mut remaining_buffers = frames.chunks(period_size);
        // 初始化一个 Option 类型的缓冲区数组，存储当前可用的缓冲区
        let mut buffers: [Option<&[u8]>; Self::QUEUE_SIZE as usize] =
            [None; Self::QUEUE_SIZE as usize];
        // 初始化 VirtIOSndPcmStatus 数组，用于存储每个缓冲区的状态
        let mut statuses: [VirtioSndPcmStatus; Self::QUEUE_SIZE as usize] =
            array::from_fn(|_| Default::default());
        // 每个缓冲区的标识符（token），用于标识和管理缓冲区
        let mut tokens = [0; Self::QUEUE_SIZE as usize];
        // 缓冲区的头部与尾部
        let mut head = 0;
        let mut tail = 0;

        let stream_id_stream = {
            let segment = FrameAllocOptions::new()
                .zeroed(false)
                .alloc_segment(1)
                .unwrap();
            DmaStream::map(segment.into(), DmaDirection::ToDevice, false).unwrap()
        };
        stream_id_stream
            .writer()
            .unwrap()
            .write_once(&stream_id_bytes)
            .unwrap();

        loop {
            let mut queue = self.tx_queue.disable_irq().lock();
            early_println!("queue has {:?} available descriptor", queue.available_desc());
            if queue.available_desc() >= 3 {
                // 为什么是3？
                if let Some(buffer) = remaining_buffers.next() {
                    early_println!("buffer is {:?}", buffer);
                    let resp_slice = {
                        let resp_slice = DmaStreamSlice::new(&self.receive_buffer, 0, 8);
                        resp_slice
                    };
                    tokens[head] = {
                        // 为什么用unsafe
                        // 要用remain>0吗
                        let mut reader = VmReader::from(buffer);
                        let mut writer = self.send_buffer.writer().unwrap();
                        let len = writer.write(&mut reader);
                        self.send_buffer.sync(0..len).unwrap();

                        let pcm_data_slice: DmaStreamSlice<&DmaStream> =
                            DmaStreamSlice::new(&self.send_buffer, 0, len);

                        let device_id_slice = DmaStreamSlice::new(&stream_id_stream, 0, 4);
                        let inputs = vec![&device_id_slice, &pcm_data_slice]; //为什么需要两个分开？能并一起传吗

                        queue
                            .add_dma_buf(inputs.as_slice(), &mut [&resp_slice])
                            .unwrap()
                    };
                    // read from resp_slice
                    resp_slice.sync().unwrap();
                    statuses[head] = resp_slice.read_val(0).unwrap();
                    if queue.should_notify() {
                        queue.notify();
                    }
                    buffers[head] = Some(buffer);
                    head += 1;
                    if head >= usize::from(Self::QUEUE_SIZE) {
                        head = 0;
                    }
                } else if head == tail {
                    //都已经使用过，tail追赶上head
                    break;
                }
            }
            if queue.can_pop() {
                early_println!("tail is {:?}", tail);
                // pop以后改变tail的值
                queue.pop_used_with_token(tokens[tail])?;
                if statuses[tail].status != u32::from(CommandCode::SOk) {
                    return Err(VirtioDeviceError::IoError);
                }
                tail += 1;
                if tail >= usize::from(Self::QUEUE_SIZE) {
                    tail = 0;
                }
            }
            spin_loop();
        }

        Ok(())
    }

    /// Transfer PCM frame to device, based on the stream type(OUTPUT/INPUT).
    ///
    /// Currently supports only output stream.
    ///
    /// This is a non-blocking method that returns a token.
    ///
    /// The length of the `frames` must be equal to the buffer size set for the stream corresponding to the `stream_id`.
    pub fn pcm_xfer_nb(&mut self, stream_id: u32, frames: &[u8]) -> Result<u16, VirtioDeviceError> {
        const U32_SIZE: usize = size_of::<u32>();
        if !self.set_up {
            self.set_up()?;
            self.set_up = true;
        }
        if !self.pcm_parameters[stream_id as usize].setup {
            warn!("Please set parameters for a stream before using it!");
            return Err(VirtioDeviceError::IoError);
        }
        let period_size: usize = self.pcm_parameters[stream_id as usize].period_bytes as usize;
        assert_eq!(period_size, frames.len());

        let id_stream = {
            let segment = FrameAllocOptions::new()
                .zeroed(false)
                .alloc_segment(1)
                .unwrap();
            DmaStream::map(segment.into(), DmaDirection::Bidirectional, false).unwrap()
        };
        let stream_id_bytes = stream_id.to_le_bytes();
        id_stream
            .writer()
            .unwrap()
            .write_once(&stream_id_bytes)
            .unwrap();
        let id_stream_slice = DmaStreamSlice::new(&id_stream, 0, 4);
        let mut reader = VmReader::from(frames);
        let mut writer = self.send_buffer.writer().unwrap();
        let len = writer.write(&mut reader);
        self.send_buffer.sync(0..len).unwrap();

        let frame_slice = DmaStreamSlice::new(&self.send_buffer, 0, period_size);
        let inputs = vec![&id_stream_slice, &frame_slice];
        let rsp = VirtioSndPcmStatus::new_zeroed();
        let rsp_slice = {
            let rsp_slice = DmaStreamSlice::new(&self.receive_buffer, 0, rsp.as_bytes().len());
            rsp_slice
        };
        let mut queue = self.tx_queue.disable_irq().lock();
        let token = queue
            .add_dma_buf(inputs.as_slice(), &mut [&rsp_slice])
            .expect("add tx queue failed");
        if queue.should_notify() {
            queue.notify();
        }
        self.token_buf.insert(token, token);
        self.token_rsp.insert(token, token);
        Ok(token)
    }

    /// The PCM frame transmission corresponding to the given token has been completed.
    pub fn pcm_xfer_ok(&mut self, token: u16) -> Result<(), VirtioDeviceError> {
        assert!(self.token_buf.contains_key(&token));
        assert!(self.token_rsp.contains_key(&token));
        let mut queue = self.tx_queue.disable_irq().lock();
        queue
            .pop_used_with_token(token)
            .expect("pop used failed during pcm transfer ack");

        self.token_buf.remove(&token);
        self.token_rsp.remove(&token);
        Ok(())
    }

    fn handle_recv_irq(&self) {
        let mut eventq_lock = self.event_queue.disable_irq().lock();
        let Ok((_, len)) = eventq_lock.pop_used() else {
            return;
        };
        self.receive_buffer.sync(0..len as usize).unwrap();
        
    }
}

/// test the freaking Virtio sound device
fn test_device(device: Arc<Mutex<SoundDevice>>) {
    let cloned_device = Arc::clone(&device);
    let mut device = cloned_device.lock();
    early_println!("Config is {:?}", device.config_manager.read_config()); //Config is VirtioSoundConfig { jacks: 0, streams: 2, chmaps: 0, controls: 4294967295 }
    device.set_up().unwrap();
    const STREAMID: u32 = 0;
    const BUFFER_BYTES: u32 = 100;
    const PERIOD_BYTES: u32 = 100;
    const FEATURES: PcmFeatures = PcmFeatures::empty();
    const CHANNELS: u8 = 1;
    const FORMAT: PcmFormat = PcmFormat::U8;
    const PCMRATE: PcmRate = PcmRate::Rate8000;

    // A PCM stream has the following command lifecycle:
    //
    // - `SET PARAMETERS`
    //
    //   The driver negotiates the stream parameters (format, transport, etc) with
    //   the device.
    //
    //   Possible valid transitions: `SET PARAMETERS`, `PREPARE`.
    //
    // - `PREPARE`
    //
    //   The device prepares the stream (allocates resources, etc).
    //
    //   Possible valid transitions: `SET PARAMETERS`, `PREPARE`, `START`,
    //   `RELEASE`.   Output only: the driver transfers data for pre-buffing.
    //
    // - `START`
    //
    //   The device starts the stream (unmute, putting into running state, etc).
    //
    //   Possible valid transitions: `STOP`.
    //   The driver transfers data to/from the stream.
    //
    // - `STOP`
    //
    //   The device stops the stream (mute, putting into non-running state, etc).
    //
    //   Possible valid transitions: `START`, `RELEASE`.
    //
    // - `RELEASE`
    //
    //   The device releases the stream (frees resources, etc).
    //
    //   Possible valid transitions: `SET PARAMETERS`, `PREPARE`.
    //
    // ```text
    // +---------------+ +---------+ +---------+ +-------+ +-------+
    // | SetParameters | | Prepare | | Release | | Start | | Stop  |
    // +---------------+ +---------+ +---------+ +-------+ +-------+
    //         |              |           |          |         |
    //         |-             |           |          |         |
    //         ||             |           |          |         |
    //         |<             |           |          |         |
    //         |              |           |          |         |
    //         |------------->|           |          |         |
    //         |              |           |          |         |
    //         |<-------------|           |          |         |
    //         |              |           |          |         |
    //         |              |-          |          |         |
    //         |              ||          |          |         |
    //         |              |<          |          |         |
    //         |              |           |          |         |
    //         |              |--------------------->|         |
    //         |              |           |          |         |
    //         |              |---------->|          |         |
    //         |              |           |          |         |
    //         |              |           |          |-------->|
    //         |              |           |          |         |
    //         |              |           |          |<--------|
    //         |              |           |          |         |
    //         |              |           |<-------------------|
    //         |              |           |          |         |
    //         |<-------------------------|          |         |
    //         |              |           |          |         |
    //         |              |<----------|          |         |
    // ```
    let set_params_result = device.pcm_set_params(
        STREAMID,
        BUFFER_BYTES,
        PERIOD_BYTES,
        FEATURES,
        CHANNELS,
        FORMAT,
        PCMRATE,
    );
    let frames: [u8; 100] = [0; 100];
    match set_params_result {
        Ok(()) => {
            early_println!("Set Parameters for stream {:?} completed!", STREAMID);
        }
        Err(e) => {
            early_println!(
                "Set Parameters for stream {:?} wrong due to {:?}!",
                STREAMID,
                e
            );
        }
    }

    let pcm_prepare_result = device.pcm_prepare(STREAMID);
    match pcm_prepare_result {
        Ok(()) => {
            early_println!("Preparation for stream {:?} completed!", STREAMID);
        }
        Err(e) => {
            early_println!(
                "Preparation for stream {:?} wrong due to {:?}!",
                STREAMID,
                e
            );
        }
    }

    let pcm_start_result = device.pcm_start(STREAMID);
    match pcm_start_result {
        Ok(()) => {
            early_println!("Start for stream {:?} completed!", STREAMID);
        }
        Err(e) => {
            early_println!("Start for stream {:?} wrong due to {:?}!", STREAMID, e);
        }
    }

    // let pcm_xfer_nb_result = device.pcm_xfer_nb(STREAMID, &frames);
    // match pcm_xfer_nb_result {
    //     Ok(token) => {
    //         early_println!("Token {:?} is returned", token);
    // let pcm_xfer_nb_result = device.pcm_xfer_nb(STREAMID, &frames);
    // match pcm_xfer_nb_result {
    //     Ok(token) => {
    //         early_println!("Token {:?} is returned", token);
    //     }
    //     Err(e) => {
    //         early_println!(
    //             "Transfer pcm data in non-blokcing mode error for stream {:?} due to {:?}",
    //             STREAMID,
    //             e
    //         );
    //         early_println!(
    //             "Transfer pcm data in non-blokcing mode error for stream {:?} due to {:?}",
    //             STREAMID,
    //             e
    //         );
    //     }
    // }

    let pcm_xfer_result = device.pcm_xfer(STREAMID, &frames);
    match pcm_xfer_result {
        Ok(()) => {
            early_println!("Transfer for stream {:?} completed!", STREAMID);
        }
        Err(e) => {
            early_println!("Transfer for stream {:?} wrong due to {:?}!", STREAMID, e);
        }
    }

    let pcm_xfer_result = device.pcm_xfer(STREAMID, &frames);
    match pcm_xfer_result {
        Ok(()) => {
            early_println!("Transfer for stream {:?} completed!", STREAMID);
        }
        Err(e) => {
            early_println!("Transfer for stream {:?} wrong due to {:?}!", STREAMID, e);
        }
    }

    let pcm_stop_result = device.pcm_stop(STREAMID);
    match pcm_stop_result {
        Ok(()) => {
            early_println!("Stop for stream {:?} completed!", STREAMID);
        }
        Err(e) => {
            early_println!("Stop for stream {:?} wrong due to {:?}!", STREAMID, e);
        }
    }

    let pcm_release_result = device.pcm_release(STREAMID);
    match pcm_release_result {
        Ok(()) => {
            early_println!("Release for stream {:?} completed!", STREAMID);
        }
        Err(e) => {
            early_println!("Release for stream {:?} wrong due to {:?}!", STREAMID, e);
        }
    }
}
