pub mod config;
pub mod device;

pub static DEVICE_NAME: &str = "Virtio-Sound";

use alloc::fmt::Debug;
use core::fmt::{self, Display, Formatter};

use bitflags::bitflags;
use ostd::Pod;
// jack control request types
pub const VIRTIO_SND_R_JACK_INFO: u32 = 1;
pub const VIRTIO_SND_R_JACK_REMAP: u32 = 2;

// PCM control request types
pub const VIRTIO_SND_R_PCM_INFO: u32 = 0x0100;
pub const VIRTIO_SND_R_PCM_SET_PARAMS: u32 = 0x0101;
pub const VIRTIO_SND_R_PCM_PREPARE: u32 = 0x0102;
pub const VIRTIO_SND_R_PCM_RELEASE: u32 = 0x0103;
pub const VIRTIO_SND_R_PCM_START: u32 = 0x0104;
pub const VIRTIO_SND_R_PCM_STOP: u32 = 0x0105;

// channel map control request types
pub const VIRTIO_SND_R_CHMAP_INFO: u32 = 0x0200;

// control element request types
pub const VIRTIO_SND_R_CTL_INFO: u32 = 0x0300;
pub const VIRTIO_SND_R_CTL_ENUM_ITEMS: u32 = 0x0301;
pub const VIRTIO_SND_R_CTL_READ: u32 = 0x0302;
pub const VIRTIO_SND_R_CTL_WRITE: u32 = 0x0303;
pub const VIRTIO_SND_R_CTL_TLV_READ: u32 = 0x0304;
pub const VIRTIO_SND_R_CTL_TLV_WRITE: u32 = 0x0305;
pub const VIRTIO_SND_R_CTL_TLV_COMMAND: u32 = 0x0306;

// jack event types
pub const VIRTIO_SND_EVT_JACK_CONNECTED: u32 = 0x1000;
pub const VIRTIO_SND_EVT_JACK_DISCONNECTED: u32 = 0x1001;

// pcm event types
pub const VIRTIO_SND_EVT_PCM_PERIOD_ELAPSED: u32 = 0x1100;
pub const VIRTIO_SND_EVT_PCM_XRUN: u32 = 0x1101;

// control element event types
pub const VIRTIO_SND_EVT_CTL_NOTIFY: u32 = 0x1200;

// common status codes
pub const VIRTIO_SND_S_OK: u32 = 0x8000; // success
pub const VIRTIO_SND_S_BAD_MSG: u32 = 0x8001; // a control message is malformed or contains invalid parameters
pub const VIRTIO_SND_S_NOT_SUPP: u32 = 0x8002; // requested operation or parameters are not supported
pub const VIRTIO_SND_S_IO_ERR: u32 = 0x8003; // an I/O error occurred

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum RequestStatusCode {
    /* common status codes */
    Ok = 0x8000,
    BadMsg,
    NotSupp,
    IoErr,
}

impl From<RequestStatusCode> for VirtioSndHdr {
    fn from(value: RequestStatusCode) -> Self {
        VirtioSndHdr { code: value as _ }
    }
}

/// Virtio Sound Request / Response common header
#[derive(Debug, Clone, Copy, Pod, Eq, PartialEq)]
#[repr(C)]
pub struct VirtioSndHdr {
    /// specifies a device request type (VIRTIO_SND_R_*) / response status (VIRTIO_SND_S_*)
    /// p.s. use u32 to represent le32
    pub code: u32,
}

const SND_HDR_SIZE: usize = size_of::<VirtioSndHdr>();

impl From<CommandCode> for VirtioSndHdr {
    fn from(value: CommandCode) -> Self {
        VirtioSndHdr { code: value.into() }
    }
}

/// Virtio Sound event notification
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndEvent {
    pub header: VirtioSndHdr, // indicates an event type (VIRTIO_SND_EVT_*)
    pub data: u32,            // indicates an optional event data
}

/// The notification type.
#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NotificationType {
    /// An external device has been connected to the jack.
    JackConnected = 0x1000,
    /// An external device has been disconnected from the jack.
    JackDisconnected,
    /// A hardware buffer period has elapsed, the period size is controlled using the `period_bytes` field.
    PcmPeriodElapsed = 0x1100,
    /// An underflow for the output stream or an overflow for the inputstream has occurred.
    PcmXrun,
}

impl NotificationType {
    /// Converts the given value to a variant of this enum, if any matches.
    fn n(value: u32) -> Option<Self> {
        match value {
            0x1100 => Some(Self::PcmPeriodElapsed),
            0x1101 => Some(Self::PcmXrun),
            0x1000 => Some(Self::JackConnected),
            0x1001 => Some(Self::JackDisconnected),
            _ => None,
        }
    }
}

/// Notification from sound device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Notification {
    notification_type: NotificationType,
    data: u32,
}

impl Notification {
    /// Get the resource index.
    pub fn data(&self) -> u32 {
        self.data
    }

    /// Get the notification type.
    pub fn notification_type(&self) -> NotificationType {
        self.notification_type
    }
}

// device data flow directions
const VIRTIO_SND_D_OUTPUT: u8 = 0;
const VIRTIO_SND_D_INPUT: u8 = 1;

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum CommandCode {
    /* jack control request types */
    RJackInfo = 1,
    RJackRemap,

    /* PCM control request types */
    RPcmInfo = 0x0100,
    RPcmSetParams,
    RPcmPrepare,
    RPcmRelease,
    RPcmStart,
    RPcmStop,

    /* channel map control request types */
    RChmapInfo = 0x0200,

    /* jack event types */
    EvtJackConnected = 0x1000,
    EvtJackDisconnected,

    /* PCM event types */
    EvtPcmPeriodElapsed = 0x1100,
    EvtPcmXrun,

    /* common status codes */
    /// success
    SOk = 0x8000,
    /// a control message is malformed or contains invalid parameters
    SBadMsg,
    /// requested operation or parameters are not supported
    SNotSupp,
    ///  an I/O error occurred
    SIoErr,
}

impl From<CommandCode> for u32 {
    fn from(code: CommandCode) -> u32 {
        code as u32
    }
}

/// Virtio Sound request information about any kind of configuration item (A special control message)
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndQueryInfo {
    pub hdr: VirtioSndHdr, // a particular item request type (VIRTIO_SND_R_*_INFO)
    pub start_id: u32,     // starting identifier for the item
    pub count: u32,        // number of items for which information is requested
    pub size: u32,         // size of the structure containing information for one item
}

#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
struct VirtIOSndQueryInfoRsp {
    hdr: VirtioSndHdr,
    info: VirtioSndInfo,
}

/// Virtio Sound response common information header
#[derive(Debug, Clone, Copy, Pod, Eq, PartialEq)]
#[repr(C)]
pub struct VirtioSndInfo {
    pub hda_fn_nid: u32, // a function group node identifier (Used to link together different types of resources)
}

// supported PCM stream features
// #[derive(Copy, Clone, Debug, Eq, PartialEq,Default)]
// enum PcmFeatures {
//     #[default]
//     VIRTIO_SND_PCM_F_SHMEM_HOST = 0,         // supports sharing a host memory with a guest
//     VIRTIO_SND_PCM_F_SHMEM_GUEST = 1,         // supports sharing a guest memory with a host
//     VIRTIO_SND_PCM_F_MSG_POLLING= 2,         // supports polling mode for message-based transport
//     VIRTIO_SND_PCM_F_EVT_SHMEM_PERIODS= 3,   // supports elapsed period notifications for shared memory transport
//     VIRTIO_SND_PCM_F_EVT_XRUNS= 4          // supports underrun/overrun notifications
// }

bitflags! {
    /// Supported PCM stream features.
    #[derive(Default)]
    #[repr(transparent)]
    pub struct PcmFeatures: u32 {
        /// Supports sharing a host memory with a guest.
        const SHMEM_HOST = 1 << 0;
        /// Supports sharing a guest memory with a host.
        const SHMEM_GUEST = 1 << 1;
        /// Supports polling mode for message-based transport.
        const MSG_POLLING = 1 << 2;
        /// Supports elapsed period notifications for shared memory transport.
        const EVT_SHMEM_PERIODS = 1 << 3;
        /// Supports underrun/overrun notifications.
        const EVT_XRUNS = 1 << 4;
    }
}

// impl From<PcmFeatures> for u32 {
//     fn from(value: PcmFeatures) -> Self {
//         value as _
//     }
// }

// supported PCM sample formats
//   analog formats (width / physical width)
bitflags! {
    /// Supported PCM sample formats.
    #[derive(Default)]
    #[repr(transparent)]
    pub struct PcmFormats: u64 {
        /// IMA ADPCM format.
        const IMA_ADPCM = 1 << 0;
        /// Mu-law format.
        const MU_LAW = 1 << 1;
        /// A-law format.
        const A_LAW = 1 << 2;
        /// Signed 8-bit format.
        const S8 = 1 << 3;
        /// Unsigned 8-bit format.
        const U8 = 1 << 4;
        /// Signed 16-bit format.
        const S16 = 1 << 5;
        /// Unsigned 16-bit format.
        const U16 = 1 << 6;
        /// Signed 18.3-bit format.
        const S18_3 = 1 << 7;
        /// Unsigned 18.3-bit format.
        const U18_3 = 1 << 8;
        /// Signed 20.3-bit format.
        const S20_3 = 1 << 9;
        /// Unsigned 20.3-bit format.
        const U20_3 = 1 << 10;
        /// Signed 24.3-bit format.
        const S24_3 = 1 << 11;
        /// Unsigned 24.3-bit format.
        const U24_3 = 1 << 12;
        /// Signed 20-bit format.
        const S20 = 1 << 13;
        /// Unsigned 20-bit format.
        const U20 = 1 << 14;
        /// Signed 24-bit format.
        const S24 = 1 << 15;
        /// Unsigned 24-bit format.
        const U24 = 1 << 16;
        /// Signed 32-bit format.
        const S32 = 1 << 17;
        /// Unsigned 32-bit format.
        const U32 = 1 << 18;
        /// 32-bit floating-point format.
        const FLOAT = 1 << 19;
        /// 64-bit floating-point format.
        const FLOAT64 = 1 << 20;
        /// DSD unsigned 8-bit format.
        const DSD_U8 = 1 << 21;
        /// DSD unsigned 16-bit format.
        const DSD_U16 = 1 << 22;
        /// DSD unsigned 32-bit format.
        const DSD_U32 = 1 << 23;
        /// IEC958 subframe format.
        const IEC958_SUBFRAME = 1 << 24;
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum PcmFormat {
    /// IMA ADPCM format.
    #[default]
    ImaAdpcm = 0,
    /// Mu-law format.
    MuLaw = 1,
    /// A-law format.
    ALaw = 2,
    /// Signed 8-bit format.
    S8 = 3,
    /// Unsigned 8-bit format.
    U8 = 4,
    /// Signed 16-bit format.
    S16 = 5,
    /// Unsigned 16-bit format.
    U16 = 6,
    /// Signed 18.3-bit format.
    S18_3 = 7,
    /// Unsigned 18.3-bit format.
    U18_3 = 8,
    /// Signed 20.3-bit format.
    S20_3 = 9,
    /// Unsigned 20.3-bit format.
    U20_3 = 10,
    /// Signed 24.3-bit format.
    S24_3 = 11,
    /// Unsigned 24.3-bit format.
    U24_3 = 12,
    /// Signed 20-bit format.
    S20 = 13,
    /// Unsigned 20-bit format.
    U20 = 14,
    /// Signed 24-bit format.
    S24 = 15,
    /// Unsigned 24-bit format.
    U24 = 16,
    /// Signed 32-bit format.
    S32 = 17,
    /// Unsigned 32-bit format.
    U32 = 18,
    /// 32-bit floating-point format.
    FLOAT = 19,
    /// 64-bit floating-point format.
    FLOAT64 = 20,
    /// DSD unsigned 8-bit format.
    DsdU8 = 21,
    /// DSD unsigned 16-bit format.
    DsdU16 = 22,
    /// DSD unsigned 32-bit format.
    DsdU32 = 23,
    /// IEC958 subframe format.
    Iec958Subframe = 24,
}

impl From<PcmFormat> for PcmFormats {
    fn from(format: PcmFormat) -> Self {
        match format {
            PcmFormat::ImaAdpcm => PcmFormats::IMA_ADPCM,
            PcmFormat::MuLaw => PcmFormats::MU_LAW,
            PcmFormat::ALaw => PcmFormats::A_LAW,
            PcmFormat::S8 => PcmFormats::S8,
            PcmFormat::U8 => PcmFormats::U8,
            PcmFormat::S16 => PcmFormats::S16,
            PcmFormat::U16 => PcmFormats::U16,
            PcmFormat::S18_3 => PcmFormats::S18_3,
            PcmFormat::U18_3 => PcmFormats::U18_3,
            PcmFormat::S20_3 => PcmFormats::S20_3,
            PcmFormat::U20_3 => PcmFormats::U20_3,
            PcmFormat::S24_3 => PcmFormats::S24_3,
            PcmFormat::U24_3 => PcmFormats::U24_3,
            PcmFormat::S20 => PcmFormats::S20,
            PcmFormat::U20 => PcmFormats::U20,
            PcmFormat::S24 => PcmFormats::S24,
            PcmFormat::U24 => PcmFormats::U24,
            PcmFormat::S32 => PcmFormats::S32,
            PcmFormat::U32 => PcmFormats::U32,
            PcmFormat::FLOAT => PcmFormats::FLOAT,
            PcmFormat::FLOAT64 => PcmFormats::FLOAT64,
            PcmFormat::DsdU8 => PcmFormats::DSD_U8,
            PcmFormat::DsdU16 => PcmFormats::DSD_U16,
            PcmFormat::DsdU32 => PcmFormats::DSD_U32,
            PcmFormat::Iec958Subframe => PcmFormats::IEC958_SUBFRAME,
        }
    }
}

impl From<PcmFormat> for u8 {
    fn from(format: PcmFormat) -> u8 {
        format as _
    }
}

/// PCM control request / PCM common header
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndPcmHdr {
    pub hdr: VirtioSndHdr, // request type (VIRTIO_SND_R_PCM_*)
    pub stream_id: u32,    // PCM stream identifier from 0 to streams - 1
}

// supported PCM frame rates
bitflags! {
    /// Supported PCM frame rates.
    #[derive(Default)]
    #[repr(transparent)]
    pub struct PcmRates: u64 {
        /// 5512 Hz PCM rate.
        const RATE_5512 = 1 << 0;
        /// 8000 Hz PCM rate.
        const RATE_8000 = 1 << 1;
        /// 11025 Hz PCM rate.
        const RATE_11025 = 1 << 2;
        /// 16000 Hz PCM rate.
        const RATE_16000 = 1 << 3;
        /// 22050 Hz PCM rate.
        const RATE_22050 = 1 << 4;
        /// 32000 Hz PCM rate.
        const RATE_32000 = 1 << 5;
        /// 44100 Hz PCM rate.
        const RATE_44100 = 1 << 6;
        /// 48000 Hz PCM rate.
        const RATE_48000 = 1 << 7;
        /// 64000 Hz PCM rate.
        const RATE_64000 = 1 << 8;
        /// 88200 Hz PCM rate.
        const RATE_88200 = 1 << 9;
        /// 96000 Hz PCM rate.
        const RATE_96000 = 1 << 10;
        /// 176400 Hz PCM rate.
        const RATE_176400 = 1 << 11;
        /// 192000 Hz PCM rate.
        const RATE_192000 = 1 << 12;
        /// 384000 Hz PCM rate.
        const RATE_384000 = 1 << 13;
    }
}

/// A PCM frame rate.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum PcmRate {
    /// 5512 Hz PCM rate.
    #[default]
    Rate5512 = 0,
    /// 8000 Hz PCM rate.
    Rate8000 = 1,
    /// 11025 Hz PCM rate.
    Rate11025 = 2,
    /// 16000 Hz PCM rate.
    Rate16000 = 3,
    /// 22050 Hz PCM rate.
    Rate22050 = 4,
    /// 32000 Hz PCM rate.
    Rate32000 = 5,
    /// 44100 Hz PCM rate.
    Rate44100 = 6,
    /// 48000 Hz PCM rate.
    Rate48000 = 7,
    /// 64000 Hz PCM rate.
    Rate64000 = 8,
    /// 88200 Hz PCM rate.
    Rate88200 = 9,
    /// 96000 Hz PCM rate.
    Rate96000 = 10,
    /// 176400 Hz PCM rate.
    Rate176400 = 11,
    /// 192000 Hz PCM rate.
    Rate192000 = 12,
    /// 384000 Hz PCM rate.
    Rate384000 = 13,
}

impl From<PcmRate> for PcmRates {
    fn from(rate: PcmRate) -> Self {
        match rate {
            PcmRate::Rate5512 => Self::RATE_5512,
            PcmRate::Rate8000 => Self::RATE_8000,
            PcmRate::Rate11025 => Self::RATE_11025,
            PcmRate::Rate16000 => Self::RATE_16000,
            PcmRate::Rate22050 => Self::RATE_22050,
            PcmRate::Rate32000 => Self::RATE_32000,
            PcmRate::Rate44100 => Self::RATE_44100,
            PcmRate::Rate48000 => Self::RATE_48000,
            PcmRate::Rate64000 => Self::RATE_64000,
            PcmRate::Rate88200 => Self::RATE_88200,
            PcmRate::Rate96000 => Self::RATE_96000,
            PcmRate::Rate176400 => Self::RATE_176400,
            PcmRate::Rate192000 => Self::RATE_192000,
            PcmRate::Rate384000 => Self::RATE_384000,
        }
    }
}

impl From<PcmRate> for u8 {
    fn from(rate: PcmRate) -> Self {
        rate as _
    }
}

/// PCM response information
#[derive(Clone, Copy, Pod, Eq, PartialEq)]
#[repr(C)]
pub struct VirtioSndPcmInfo {
    pub hdr: VirtioSndInfo,
    pub features: u32, // a bit map of the supported features /* 1 << VIRTIO_SND_PCM_F_XXX */
    pub formats: u64,  // supported sample format bit map /* 1 << VIRTIO_SND_PCM_FMT_XXX */
    pub rates: u64,    // supported frame rate bit map /* 1 << VIRTIO_SND_PcmRate_XXX */
    pub direction: u8, // the direction of data flow (VIRTIO_SND_D_*)
    pub channels_min: u8, // minimum number of supported channels
    pub channels_max: u8, // maximum number of supported channels

    pub padding: [u8; 5],
}

const PCM_INFO_SIZE: usize = size_of::<VirtioSndPcmInfo>();

impl Debug for VirtioSndPcmInfo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("VirtIOSndPcmInfo")
            .field("hdr", &self.hdr)
            .field("features", &PcmFeatures::from_bits(self.features))
            .field("formats", &PcmFormats::from_bits(self.formats))
            .field("rates", &PcmRates::from_bits(self.rates))
            .field("direction", &self.direction)
            .field("channels_min", &self.channels_min)
            .field("channels_max", &self.channels_max)
            .field("_padding", &self.padding)
            .finish()
    }
}

impl Display for VirtioSndPcmInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let direction = if self.direction == VIRTIO_SND_D_INPUT {
            "INPUT"
        } else {
            "OUTPUT"
        };
        write!(
            f,
            "features: {:?}, rates: {:?}, formats: {:?}, direction: {}",
            PcmFeatures::from_bits(self.features),
            PcmRates::from_bits(self.rates),
            PcmFormats::from_bits(self.formats),
            direction
        )
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemInformationRequestType {
    /// Represents a jack information request.
    RJackInfo = 1,
    /// Represents a PCM information request.
    RPcmInfo = 0x0100,
    /// Represents a channel map information request.
    RChmapInfo = 0x0200,
}

impl From<ItemInformationRequestType> for VirtioSndHdr {
    fn from(value: ItemInformationRequestType) -> Self {
        VirtioSndHdr { code: value.into() }
    }
}

impl From<ItemInformationRequestType> for u32 {
    fn from(request_type: ItemInformationRequestType) -> u32 {
        request_type as _
    }
}

// standard channel position definition
pub const VIRTIO_SND_CHMAP_NONE: u8 = 0; /* undefined */
pub const VIRTIO_SND_CHMAP_NA: u8 = 1; /* silent */
pub const VIRTIO_SND_CHMAP_MONO: u8 = 2; /* mono stream */
pub const VIRTIO_SND_CHMAP_FL: u8 = 3; /* front left */
pub const VIRTIO_SND_CHMAP_FR: u8 = 4; /* front right */
pub const VIRTIO_SND_CHMAP_RL: u8 = 5; /* rear left */
pub const VIRTIO_SND_CHMAP_RR: u8 = 6; /* rear right */
pub const VIRTIO_SND_CHMAP_FC: u8 = 7; /* front center */
pub const VIRTIO_SND_CHMAP_LFE: u8 = 8; /* low frequency (LFE) */
pub const VIRTIO_SND_CHMAP_SL: u8 = 9; /* side left */
pub const VIRTIO_SND_CHMAP_SR: u8 = 10; /* side right */
pub const VIRTIO_SND_CHMAP_RC: u8 = 11; /* rear center */
pub const VIRTIO_SND_CHMAP_FLC: u8 = 12; /* front left center */
pub const VIRTIO_SND_CHMAP_FRC: u8 = 13; /* front right center */
pub const VIRTIO_SND_CHMAP_RLC: u8 = 14; /* rear left center */
pub const VIRTIO_SND_CHMAP_RRC: u8 = 15; /* rear right center */
pub const VIRTIO_SND_CHMAP_FLW: u8 = 16; /* front left wide */
pub const VIRTIO_SND_CHMAP_FRW: u8 = 17; /* front right wide */
pub const VIRTIO_SND_CHMAP_FLH: u8 = 18; /* front left high */
pub const VIRTIO_SND_CHMAP_FCH: u8 = 19; /* front center high */
pub const VIRTIO_SND_CHMAP_FRH: u8 = 20; /* front right high */
pub const VIRTIO_SND_CHMAP_TC: u8 = 21; /* top center */
pub const VIRTIO_SND_CHMAP_TFL: u8 = 22; /* top front left */
pub const VIRTIO_SND_CHMAP_TFR: u8 = 23; /* top front right */
pub const VIRTIO_SND_CHMAP_TFC: u8 = 24; /* top front center */
pub const VIRTIO_SND_CHMAP_TRL: u8 = 25; /* top rear left */
pub const VIRTIO_SND_CHMAP_TRR: u8 = 26; /* top rear right */
pub const VIRTIO_SND_CHMAP_TRC: u8 = 27; /* top rear center */
pub const VIRTIO_SND_CHMAP_TFLC: u8 = 28; /* top front left center */
pub const VIRTIO_SND_CHMAP_TFRC: u8 = 29; /* top front right center */
pub const VIRTIO_SND_CHMAP_TSL: u8 = 34; /* top side left */
pub const VIRTIO_SND_CHMAP_TSR: u8 = 35; /* top side right */
pub const VIRTIO_SND_CHMAP_LLFE: u8 = 36; /* left LFE */
pub const VIRTIO_SND_CHMAP_RLFE: u8 = 37; /* right LFE */
pub const VIRTIO_SND_CHMAP_BC: u8 = 38; /* bottom center */
pub const VIRTIO_SND_CHMAP_BLC: u8 = 39; /* bottom left center */
pub const VIRTIO_SND_CHMAP_BRC: u8 = 40; /* bottom right center */

// maximum possible number of channels
pub const VIRTIO_SND_CHMAP_MAX_SIZE: usize = 18;

/// Set selected stream parameters for the specified stream ID
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndPcmSetParams {
    pub hdr: VirtioSndPcmHdr, //
    pub buffer_bytes: u32,    // the size of the hardware buffer used by the driver
    pub period_bytes: u32,    // the size of the hardware period used by the driver
    pub features: u32, // specifies a selected feature bit map /* 1 << VIRTIO_SND_PCM_F_XXX */
    pub channels: u8,  // a selected number of channels
    pub format: u8,    // a selected sample format (VIRTIO_SND_PCM_FMT_*).
    pub rate: u8,      // a selected frame rate (VIRTIO_SND_PcmRate_*).
    pub padding: u8,
}

/// PCM I/O header
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndPcmXfer {
    pub stream_id: u32, // a PCM stream identifier from 0 to streams - 1
}

/// PCM I/O status
#[derive(Debug, Clone, Copy, Pod, Default)]
#[repr(C)]
pub struct VirtioSndPcmStatus {
    pub status: u32, // contains VIRTIO_SND_S_OK if an operation is successful, and VIRTIO_SND_S_IO_ERR otherwise.
    pub latency_bytes: u32, // indicates the current device latency
}

// channel maps response information
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndChmapInfo {
    pub hdr: VirtioSndInfo,
    pub direction: u8, // the direction of data flow (VIRTIO_SND_D_*)
    pub channels: u8,  // the number of valid channel position values
    pub positions: [u8; VIRTIO_SND_CHMAP_MAX_SIZE], //channel position values (VIRTIO_SND_CHMAP_*)
}

impl Display for VirtioSndChmapInfo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let direction = if self.direction == VIRTIO_SND_D_INPUT {
            "INPUT"
        } else {
            "OUTPUT"
        };
        write!(
            f,
            "direction: {}, channels: {}, postions: [",
            direction, self.channels
        )?;
        for i in 0..usize::from(self.channels) {
            if i != 0 {
                write!(f, ", ")?;
            }
            match ChannelPosition::try_from(self.positions[i]) {
                Ok(position) => {
                    write!(f, "{:?}", position)?;
                }
                Err(_) => {
                    write!(f, "{}", self.positions[i])?;
                }
            }
        }
        write!(f, "]")?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct PcmParameters {
    setup: bool,
    buffer_bytes: u32,
    period_bytes: u32,
    features: PcmFeatures,
    channels: u8,
    format: PcmFormat,
    rate: PcmRate,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
enum ChannelPosition {
    /// undefined
    None = 0,
    /// silent
    Na,
    /// mono stream
    Mono,
    /// front left
    Fl,
    /// front right
    Fr,
    /// rear left
    Rl,
    /// rear right
    Rr,
    /// front center
    Fc,
    /// low frequency (LFE)
    Lfe,
    /// side left
    Sl,
    /// side right
    Sr,
    /// rear center
    Rc,
    /// front left center
    Flc,
    /// front right center
    Frc,
    /// rear left center
    Rlc,
    /// rear right center
    Rrc,
    /// front left wide
    Flw,
    /// front right wide
    Frw,
    /// front left high
    Flh,
    /// front center high
    Fch,
    /// front right high
    Frh,
    /// top center
    Tc,
    /// top front left
    Tfl,
    /// top front right
    Tfr,
    /// top front center
    Tfc,
    /// top rear left
    Trl,
    /// top rear right
    Trr,
    /// top rear center
    Trc,
    /// top front left center
    Tflc,
    /// top front right center
    Tfrc,
    /// top side left
    Tsl,
    /// top side right
    Tsr,
    /// left LFE
    Llfe,
    /// right LFE
    Rlfe,
    /// bottom center
    Bc,
    /// bottom left center
    Blc,
    /// bottom right center
    Brc,
}

impl TryFrom<u8> for ChannelPosition {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ChannelPosition::None),
            1 => Ok(ChannelPosition::Na),
            2 => Ok(ChannelPosition::Mono),
            3 => Ok(ChannelPosition::Fl),
            4 => Ok(ChannelPosition::Fr),
            5 => Ok(ChannelPosition::Rl),
            6 => Ok(ChannelPosition::Rr),
            7 => Ok(ChannelPosition::Fc),
            8 => Ok(ChannelPosition::Lfe),
            9 => Ok(ChannelPosition::Sl),
            10 => Ok(ChannelPosition::Sr),
            11 => Ok(ChannelPosition::Rc),
            12 => Ok(ChannelPosition::Flc),
            13 => Ok(ChannelPosition::Frc),
            14 => Ok(ChannelPosition::Rlc),
            15 => Ok(ChannelPosition::Flw),
            16 => Ok(ChannelPosition::Frw),
            17 => Ok(ChannelPosition::Flh),
            18 => Ok(ChannelPosition::Fch),
            19 => Ok(ChannelPosition::Frh),
            20 => Ok(ChannelPosition::Tc),
            21 => Ok(ChannelPosition::Tfl),
            22 => Ok(ChannelPosition::Tfr),
            23 => Ok(ChannelPosition::Tfc),
            24 => Ok(ChannelPosition::Trl),
            25 => Ok(ChannelPosition::Trr),
            26 => Ok(ChannelPosition::Trc),
            27 => Ok(ChannelPosition::Tflc),
            28 => Ok(ChannelPosition::Tfrc),
            29 => Ok(ChannelPosition::Tsl),
            30 => Ok(ChannelPosition::Tsr),
            31 => Ok(ChannelPosition::Llfe),
            32 => Ok(ChannelPosition::Rlfe),
            33 => Ok(ChannelPosition::Bc),
            34 => Ok(ChannelPosition::Blc),
            35 => Ok(ChannelPosition::Brc),

            _ => Err(()),
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum PCMState {
    #[default]
    SetParameters,
    Prepare,
    Release,
    Start,
    Stop,
}
