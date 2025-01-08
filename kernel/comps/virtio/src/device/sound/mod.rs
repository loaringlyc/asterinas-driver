use ostd::Pod;

pub mod config;
pub mod device;

pub static DEVICE_NAME: &str = "Virtio-Sound";

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
pub const VIRTIO_SND_S_OK: u32 = 0x8000;        // success
pub const VIRTIO_SND_S_BAD_MSG: u32 = 0x8001;   // a control message is malformed or contains invalid parameters
pub const VIRTIO_SND_S_NOT_SUPP: u32 = 0x8002;  // requested operation or parameters are not supported
pub const VIRTIO_SND_S_IO_ERR: u32 = 0x8003;    // an I/O error occurred

// device data flow directions
pub const VIRTIO_SND_D_OUTPUT: u8 = 0;
pub const VIRTIO_SND_D_INPUT: u8 = 1;

// supported jack features
pub const VIRTIO_SND_JACK_F_REMAP: u8 = 0;

// supported PCM stream features
pub const VIRTIO_SND_PCM_F_SHMEM_HOST: u8 = 0;          // supports sharing a host memory with a guest
pub const VIRTIO_SND_PCM_F_SHMEM_GUEST: u8 = 1;         // supports sharing a guest memory with a host
pub const VIRTIO_SND_PCM_F_MSG_POLLING: u8 = 2;         // supports polling mode for message-based transport
pub const VIRTIO_SND_PCM_F_EVT_SHMEM_PERIODS: u8 = 3;   // supports elapsed period notifications for shared memory transport
pub const VIRTIO_SND_PCM_F_EVT_XRUNS: u8 = 4;           // supports underrun/overrun notifications

// supported PCM sample formats
//   analog formats (width / physical width)
pub const VIRTIO_SND_PCM_FMT_IMA_ADPCM: u8 = 0;
pub const VIRTIO_SND_PCM_FMT_MU_LAW: u8 = 1;
pub const VIRTIO_SND_PCM_FMT_A_LAW: u8 = 2;
pub const VIRTIO_SND_PCM_FMT_S8: u8 = 3;
pub const VIRTIO_SND_PCM_FMT_U8: u8 = 4;
pub const VIRTIO_SND_PCM_FMT_S16: u8 = 5;
pub const VIRTIO_SND_PCM_FMT_U16: u8 = 6;
pub const VIRTIO_SND_PCM_FMT_S18_3: u8 = 7;
pub const VIRTIO_SND_PCM_FMT_U18_3: u8 = 8;
pub const VIRTIO_SND_PCM_FMT_S20_3: u8 = 9;
pub const VIRTIO_SND_PCM_FMT_U20_3: u8 = 10;
pub const VIRTIO_SND_PCM_FMT_S24_3: u8 = 11;
pub const VIRTIO_SND_PCM_FMT_U24_3: u8 = 12;
pub const VIRTIO_SND_PCM_FMT_S20: u8 = 13;
pub const VIRTIO_SND_PCM_FMT_U20: u8 = 14;
pub const VIRTIO_SND_PCM_FMT_S24: u8 = 15;
pub const VIRTIO_SND_PCM_FMT_U24: u8 = 16;
pub const VIRTIO_SND_PCM_FMT_S32: u8 = 17;
pub const VIRTIO_SND_PCM_FMT_U32: u8 = 18;
pub const VIRTIO_SND_PCM_FMT_FLOAT: u8 = 19;
pub const VIRTIO_SND_PCM_FMT_FLOAT64: u8 = 20;
//   digital formats (width / physical width)
pub const VIRTIO_SND_PCM_FMT_DSD_U8: u8 = 21;
pub const VIRTIO_SND_PCM_FMT_DSD_U16: u8 = 22;
pub const VIRTIO_SND_PCM_FMT_DSD_U32: u8 = 23;
pub const VIRTIO_SND_PCM_FMT_IEC958_SUBFRAME: u8 = 24;
pub(crate) const _VIRTIO_SND_PCM_FMT_MAX: u8 = 25;

// supported PCM frame rates
pub const VIRTIO_SND_PCM_RATE_5512: u8 = 0;
pub const VIRTIO_SND_PCM_RATE_8000: u8 = 1;
pub const VIRTIO_SND_PCM_RATE_11025: u8 = 2;
pub const VIRTIO_SND_PCM_RATE_16000: u8 = 3;
pub const VIRTIO_SND_PCM_RATE_22050: u8 = 4;
pub const VIRTIO_SND_PCM_RATE_32000: u8 = 5;
pub const VIRTIO_SND_PCM_RATE_44100: u8 = 6;
pub const VIRTIO_SND_PCM_RATE_48000: u8 = 7;
pub const VIRTIO_SND_PCM_RATE_64000: u8 = 8;
pub const VIRTIO_SND_PCM_RATE_88200: u8 = 9;
pub const VIRTIO_SND_PCM_RATE_96000: u8 = 10;
pub const VIRTIO_SND_PCM_RATE_176400: u8 = 11;
pub const VIRTIO_SND_PCM_RATE_192000: u8 = 12;
pub const VIRTIO_SND_PCM_RATE_384000: u8 = 13;

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


/// Virtio Sound Request / Response common header
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndHdr {
    /// specifies a device request type (VIRTIO_SND_R_*) / response status (VIRTIO_SND_S_*)
    /// p.s. use u32 to represent le32
    pub code: u32 
}

/// Virtio Sound event notification
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndEvent {
    pub header: VirtioSndHdr,  // indicates an event type (VIRTIO_SND_EVT_*)
    pub data: u32              // indicates an optional event data
}

/// Virtio Sound request information about any kind of configuration item (A special control message)
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndQueryInfo {
    pub header: VirtioSndHdr,  // a particular item request type (VIRTIO_SND_R_*_INFO)
    pub start_id: u32,         // starting identifier for the item
    pub count: u32,            // number of items for which information is requested
    pub size: u32              // size of the structure containing information for one item
}

/// Virtio Sound response common information header
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndInfo {
    pub hda_fn_nid: u32   // a function group node identifier (Used to link together different types of resources)
}

/// Jack control request header / Jack common header
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndJackHdr {
    pub header: VirtioSndHdr, // request type (VIRTIO_SND_R_JACK_*)
    pub jack_id: u32          // jack identifier from 0 to jacks - 1
}

/// Jack response information about available jacks
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndJackInfo {
    pub header: VirtioSndHdr, // request type (VIRTIO_SND_R_JACK_*)
    pub features: u32,        // supported feature bit map
    pub hda_reg_defconf: u32, // a pin default configuration value
    pub hda_reg_caps: u32,    // a pin capabilities value
    pub connected: u8,        // current jack connection status (1 - connected, 0 - disconnected)
    pub padding: [u8; 7]      //
}

/// Jack Remap control request
/// If the VIRTIO_SND_JACK_F_REMAP feature bit is set in the jack information, then the driver can send a
///    control request to change the association and/or sequence number for the specified jack ID.
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSoundJackRemap {
    pub header: VirtioSndHdr, 
    pub association: u32,     // selected association number
    pub sequence: u32         // selected sequence number
}

/// PCM control request / PCM common header
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndPcmHeader {
    pub header: VirtioSndHdr, // request type (VIRTIO_SND_R_PCM_*)
    pub stream_id: u32        // PCM stream identifier from 0 to streams - 1
}

/// PCM response information
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndPcmInfo {
    pub header: VirtioSndInfo,
    pub features: u32,      // a bit map of the supported features /* 1 << VIRTIO_SND_PCM_F_XXX */
    pub formats: u64,       // supported sample format bit map /* 1 << VIRTIO_SND_PCM_FMT_XXX */
    pub rates: u64,         // supported frame rate bit map /* 1 << VIRTIO_SND_PCM_RATE_XXX */
    pub direction: u8,      // the direction of data flow (VIRTIO_SND_D_*)
    pub channels_min: u8,   // minimum number of supported channels
    pub channels_max: u8,   // maximum number of supported channels

    pub padding: [u8; 5],
}

/// Set selected stream parameters for the specified stream ID
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndPcmSetParams {
    pub header: VirtioSndPcmHeader, // 
    pub buffer_bytes: u32,   // the size of the hardware buffer used by the driver
    pub period_bytes: u32,   // the size of the hardware period used by the driver
    pub features: u32,       // specifies a selected feature bit map /* 1 << VIRTIO_SND_PCM_F_XXX */
    pub channels: u8,        // a selected number of channels
    pub format: u8,          // a selected sample format (VIRTIO_SND_PCM_FMT_*).
    pub rate: u8,            // a selected frame rate (VIRTIO_SND_PCM_RATE_*).
    pub padding: u8,
}

/// PCM I/O header
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndPcmXfer {
    pub stream_id: u32,      // a PCM stream identifier from 0 to streams - 1
}

/// PCM I/O status
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndPcmStatus {
    pub status: u32,         // contains VIRTIO_SND_S_OK if an operation is successful, and VIRTIO_SND_S_IO_ERR otherwise.
    pub latency_bytes: u32,  // indicates the current device latency
}

/// channel maps response information
#[derive(Debug, Clone, Copy, Pod)]
#[repr(C)]
pub struct VirtioSndChmapInfo {
    pub header: VirtioSndInfo,
    pub direction: u8,       // the direction of data flow (VIRTIO_SND_D_*)
    pub channels: u8,        // the number of valid channel position values
    pub positions: [u8; VIRTIO_SND_CHMAP_MAX_SIZE],  //channel position values (VIRTIO_SND_CHMAP_*)
}
