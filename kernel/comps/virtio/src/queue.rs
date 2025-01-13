// SPDX-License-Identifier: MPL-2.0

//! Virtqueue

use alloc::vec::Vec;
use core::{
    mem::size_of,
    sync::atomic::{fence, Ordering},
};

use aster_rights::{Dup, TRightSet, TRights, Write};
use aster_util::{field_ptr, safe_ptr::SafePtr};
use bitflags::bitflags;
use log::debug;
use ostd::{
    mm::{DmaCoherent, FrameAllocOptions},
    offset_of, Pod,
};

use crate::{
    dma_buf::DmaBuf,
    transport::{pci::legacy::VirtioPciLegacyTransport, ConfigManager, VirtioTransport},
};

#[derive(Debug)]
pub enum QueueError {
    InvalidArgs,
    BufferTooSmall,
    NotReady,
    AlreadyUsed,
    WrongToken,
}

/// The mechanism for bulk data transport on virtio devices.
///
/// Each device can have zero or more virtqueues.
#[derive(Debug)]
pub struct VirtQueue {
    /// Descriptor table
    descs: Vec<SafePtr<Descriptor, DmaCoherent>>,
    /// Available ring
    avail: SafePtr<AvailRing, DmaCoherent>,
    /// Used ring
    used: SafePtr<UsedRing, DmaCoherent>,
    /// Notify configuration manager
    notify_config: ConfigManager<u32>,

    /// The index of queue
    queue_idx: u32,
    /// The size of the queue.
    ///
    /// This is both the number of descriptors, and the number of slots in the available and used
    /// rings.
    queue_size: u16,
    /// The number of used queues.
    num_used: u16,
    /// The head desc index of the free list.
    free_head: u16,
    /// the index of the next avail ring index
    avail_idx: u16,
    /// last service used index
    last_used_idx: u16,
    /// Whether the callback of this queue is enabled
    is_callback_enabled: bool,
}

impl VirtQueue {
    /// Create a new VirtQueue.
    pub(crate) fn new(
        idx: u16,
        mut size: u16,
        transport: &mut dyn VirtioTransport,
    ) -> Result<Self, QueueError> {
        if !size.is_power_of_two() {
            return Err(QueueError::InvalidArgs);
        }

        let (descriptor_ptr, avail_ring_ptr, used_ring_ptr) = if transport.is_legacy_version() {
            // Currently, we use one UFrame to place the descriptors and available rings, one UFrame to place used rings
            // because the virtio-mmio legacy required the address to be continuous. The max queue size is 128.
            if size > 128 {
                return Err(QueueError::InvalidArgs);
            }
            let queue_size = transport.max_queue_size(idx).unwrap() as usize;
            let desc_size = size_of::<Descriptor>() * queue_size;
            size = queue_size as u16;

            let (seg1, seg2) = {
                let align_size = VirtioPciLegacyTransport::QUEUE_ALIGN_SIZE;
                let total_frames =
                    VirtioPciLegacyTransport::calc_virtqueue_size_aligned(queue_size) / align_size;
                let continue_segment = FrameAllocOptions::new()
                    .alloc_segment(total_frames)
                    .unwrap();

                let avial_size = size_of::<u16>() * (3 + queue_size);
                let seg1_frames = (desc_size + avial_size).div_ceil(align_size);

                continue_segment.split(seg1_frames * align_size)
            };
            let desc_frame_ptr: SafePtr<Descriptor, DmaCoherent> =
                SafePtr::new(DmaCoherent::map(seg1.into(), true).unwrap(), 0);
            let mut avail_frame_ptr: SafePtr<AvailRing, DmaCoherent> =
                desc_frame_ptr.clone().cast();
            avail_frame_ptr.byte_add(desc_size);
            let used_frame_ptr: SafePtr<UsedRing, DmaCoherent> =
                SafePtr::new(DmaCoherent::map(seg2.into(), true).unwrap(), 0);
            (desc_frame_ptr, avail_frame_ptr, used_frame_ptr)
        } else {
            if size > 256 {
                return Err(QueueError::InvalidArgs);
            }
            (
                SafePtr::new(
                    DmaCoherent::map(
                        FrameAllocOptions::new().alloc_segment(1).unwrap().into(),
                        true,
                    )
                    .unwrap(),
                    0,
                ),
                SafePtr::new(
                    DmaCoherent::map(
                        FrameAllocOptions::new().alloc_segment(1).unwrap().into(),
                        true,
                    )
                    .unwrap(),
                    0,
                ),
                SafePtr::new(
                    DmaCoherent::map(
                        FrameAllocOptions::new().alloc_segment(1).unwrap().into(),
                        true,
                    )
                    .unwrap(),
                    0,
                ),
            )
        };
        debug!("queue_desc start paddr:{:x?}", descriptor_ptr.paddr());
        debug!("queue_driver start paddr:{:x?}", avail_ring_ptr.paddr());
        debug!("queue_device start paddr:{:x?}", used_ring_ptr.paddr());

        transport
            .set_queue(idx, size, &descriptor_ptr, &avail_ring_ptr, &used_ring_ptr)
            .unwrap();
        let mut descs = Vec::with_capacity(size as usize);
        descs.push(descriptor_ptr);
        for i in 0..size {
            let mut desc = descs.get(i as usize).unwrap().clone();
            let next_i = i + 1;
            if next_i != size {
                field_ptr!(&desc, Descriptor, next)
                    .write_once(&next_i)
                    .unwrap();
                desc.add(1);
                descs.push(desc);
            } else {
                field_ptr!(&desc, Descriptor, next)
                    .write_once(&(0u16))
                    .unwrap();
            }
        }

        let notify_config = transport.notify_config(idx as usize);
        field_ptr!(&avail_ring_ptr, AvailRing, flags)
            .write_once(&AvailFlags::empty())
            .unwrap();
        Ok(VirtQueue {
            descs,
            avail: avail_ring_ptr,
            used: used_ring_ptr,
            notify_config,
            queue_size: size,
            queue_idx: idx as u32,
            num_used: 0,
            free_head: 0,
            avail_idx: 0,
            last_used_idx: 0,
            is_callback_enabled: true,
        })
    }

    /// Add dma buffers to the virtqueue, return a token.
    ///
    /// Ref: linux virtio_ring.c virtqueue_add
    pub fn add_dma_buf<T: DmaBuf>(
        &mut self,
        inputs: &[&T],
        outputs: &[&T],
    ) -> Result<u16, QueueError> {
        if inputs.is_empty() && outputs.is_empty() {
            return Err(QueueError::InvalidArgs);
        }
        if inputs.len() + outputs.len() + self.num_used as usize > self.queue_size as usize {
            return Err(QueueError::BufferTooSmall);
        }

        // allocate descriptors from free list
        let head = self.free_head;
        let mut last = self.free_head;
        for input in inputs.iter() {
            let desc = &self.descs[self.free_head as usize];
            set_dma_buf(&desc.borrow_vm().restrict::<TRights![Write, Dup]>(), *input);
            field_ptr!(desc, Descriptor, flags)
                .write_once(&DescFlags::NEXT)
                .unwrap();
            last = self.free_head;
            self.free_head = field_ptr!(desc, Descriptor, next).read_once().unwrap();
        }
        for output in outputs.iter() {
            let desc = &mut self.descs[self.free_head as usize];
            set_dma_buf(
                &desc.borrow_vm().restrict::<TRights![Write, Dup]>(),
                *output,
            );
            field_ptr!(desc, Descriptor, flags)
                .write_once(&(DescFlags::NEXT | DescFlags::WRITE))
                .unwrap();
            last = self.free_head;
            self.free_head = field_ptr!(desc, Descriptor, next).read_once().unwrap();
        }
        // set last_elem.next = NULL
        {
            let desc = &mut self.descs[last as usize];
            let mut flags: DescFlags = field_ptr!(desc, Descriptor, flags).read_once().unwrap();
            flags.remove(DescFlags::NEXT);
            field_ptr!(desc, Descriptor, flags)
                .write_once(&flags)
                .unwrap();
        }
        self.num_used += (inputs.len() + outputs.len()) as u16;

        let avail_slot = self.avail_idx & (self.queue_size - 1);

        {
            let ring_ptr: SafePtr<[u16; 64], &DmaCoherent> =
                field_ptr!(&self.avail, AvailRing, ring);
            let mut ring_slot_ptr = ring_ptr.cast::<u16>();
            ring_slot_ptr.add(avail_slot as usize);
            ring_slot_ptr.write_once(&head).unwrap();
        }
        // write barrier
        fence(Ordering::SeqCst);

        // increase head of avail ring
        self.avail_idx = self.avail_idx.wrapping_add(1);
        field_ptr!(&self.avail, AvailRing, idx)
            .write_once(&self.avail_idx)
            .unwrap();

        fence(Ordering::SeqCst);
        Ok(head)
    }

    /// Whether there is a used element that can pop.
    pub fn can_pop(&self) -> bool {
        // read barrier
        fence(Ordering::SeqCst);

        self.last_used_idx != field_ptr!(&self.used, UsedRing, idx).read_once().unwrap()
    }

    /// The number of free descriptors.
    pub fn available_desc(&self) -> usize {
        (self.queue_size - self.num_used) as usize
    }

    /// Recycle descriptors in the list specified by head.
    ///
    /// This will push all linked descriptors at the front of the free list.
    fn recycle_descriptors(&mut self, mut head: u16) {
        let origin_free_head = self.free_head;
        self.free_head = head;
        loop {
            let desc = &mut self.descs[head as usize];
            // Sets the buffer address and length to 0
            field_ptr!(desc, Descriptor, addr)
                .write_once(&(0u64))
                .unwrap();
            field_ptr!(desc, Descriptor, len)
                .write_once(&(0u32))
                .unwrap();
            self.num_used -= 1;

            let flags: DescFlags = field_ptr!(desc, Descriptor, flags).read_once().unwrap();
            if flags.contains(DescFlags::NEXT) {
                field_ptr!(desc, Descriptor, flags)
                    .write_once(&DescFlags::empty())
                    .unwrap();
                head = field_ptr!(desc, Descriptor, next).read_once().unwrap();
            } else {
                field_ptr!(desc, Descriptor, next)
                    .write_once(&origin_free_head)
                    .unwrap();
                break;
            }
        }
    }

    /// Get a token from device used buffers, return (token, len).
    ///
    /// Ref: linux virtio_ring.c virtqueue_get_buf_ctx
    pub fn pop_used(&mut self) -> Result<(u16, u32), QueueError> {
        if !self.can_pop() {
            return Err(QueueError::NotReady);
        }

        let last_used_slot = self.last_used_idx & (self.queue_size - 1);
        let element_ptr = {
            let mut ptr = self.used.borrow_vm();
            ptr.byte_add(offset_of!(UsedRing, ring) as usize + last_used_slot as usize * 8);
            ptr.cast::<UsedElem>()
        };
        let index = field_ptr!(&element_ptr, UsedElem, id).read_once().unwrap();
        let len = field_ptr!(&element_ptr, UsedElem, len).read_once().unwrap();

        self.recycle_descriptors(index as u16);
        self.last_used_idx = self.last_used_idx.wrapping_add(1);

        Ok((index as u16, len))
    }

    /// If the given token is next on the device used queue, pops it and returns the total buffer
    /// length which was used (written) by the device.
    ///
    /// Ref: linux virtio_ring.c virtqueue_get_buf_ctx
    pub fn pop_used_with_token(&mut self, token: u16) -> Result<u32, QueueError> {
        if !self.can_pop() {
            return Err(QueueError::NotReady);
        }

        let last_used_slot = self.last_used_idx & (self.queue_size - 1);
        let element_ptr = {
            let mut ptr = self.used.borrow_vm();
            ptr.byte_add(offset_of!(UsedRing, ring) as usize + last_used_slot as usize * 8);
            ptr.cast::<UsedElem>()
        };
        let index = field_ptr!(&element_ptr, UsedElem, id).read_once().unwrap();
        let len = field_ptr!(&element_ptr, UsedElem, len).read_once().unwrap();

        if index as u16 != token {
            return Err(QueueError::WrongToken);
        }

        self.recycle_descriptors(index as u16);
        self.last_used_idx = self.last_used_idx.wrapping_add(1);

        Ok(len)
    }

    /// Return size of the queue.
    pub fn size(&self) -> u16 {
        self.queue_size
    }

    /// whether the driver should notify the device
    pub fn should_notify(&self) -> bool {
        // read barrier
        fence(Ordering::SeqCst);
        let flags = field_ptr!(&self.used, UsedRing, flags).read_once().unwrap();
        flags & 0x0001u16 == 0u16
    }

    /// notify that there are available rings
    pub fn notify(&mut self) {
        if self.notify_config.is_modern() {
            self.notify_config
                .write_once::<u32>(0, self.queue_idx)
                .unwrap();
        } else {
            self.notify_config
                .write_once::<u16>(0, self.queue_idx as u16)
                .unwrap();
        }
    }

    /// Disables registered callbacks.
    ///
    /// That is to say, the queue won't generate interrupts after calling this method.
    pub fn disable_callback(&mut self) {
        if !self.is_callback_enabled {
            return;
        }

        let flags_ptr = field_ptr!(&self.avail, AvailRing, flags);
        let mut flags: AvailFlags = flags_ptr.read_once().unwrap();
        debug_assert!(!flags.contains(AvailFlags::VIRTQ_AVAIL_F_NO_INTERRUPT));
        flags.insert(AvailFlags::VIRTQ_AVAIL_F_NO_INTERRUPT);
        flags_ptr.write_once(&flags).unwrap();

        self.is_callback_enabled = false;
    }

    /// Enables registered callbacks.
    ///
    /// The queue will generate interrupts if any event comes after calling this method.
    pub fn enable_callback(&mut self) {
        if self.is_callback_enabled {
            return;
        }

        let flags_ptr = field_ptr!(&self.avail, AvailRing, flags);
        let mut flags: AvailFlags = flags_ptr.read_once().unwrap();
        debug_assert!(flags.contains(AvailFlags::VIRTQ_AVAIL_F_NO_INTERRUPT));
        flags.remove(AvailFlags::VIRTQ_AVAIL_F_NO_INTERRUPT);
        flags_ptr.write_once(&flags).unwrap();

        self.is_callback_enabled = true;
    }
}

#[repr(C, align(16))]
#[derive(Debug, Default, Copy, Clone, Pod)]
pub struct Descriptor {
    addr: u64,
    len: u32,
    flags: DescFlags,
    next: u16,
}

type DescriptorPtr<'a> = SafePtr<Descriptor, &'a DmaCoherent, TRightSet<TRights![Dup, Write]>>;

#[inline]
fn set_dma_buf<T: DmaBuf>(desc_ptr: &DescriptorPtr, buf: &T) {
    // TODO: skip the empty dma buffer or just return error?
    debug_assert_ne!(buf.len(), 0);
    let daddr = buf.daddr();
    field_ptr!(desc_ptr, Descriptor, addr)
        .write_once(&(daddr as u64))
        .unwrap();
    field_ptr!(desc_ptr, Descriptor, len)
        .write_once(&(buf.len() as u32))
        .unwrap();
}

bitflags! {
    /// Descriptor flags
    #[derive(Pod, Default)]
    #[repr(C)]
    struct DescFlags: u16 {
        const NEXT = 1;
        const WRITE = 2;
        const INDIRECT = 4;
    }
}

/// The driver uses the available ring to offer buffers to the device:
/// each ring entry refers to the head of a descriptor chain.
/// It is only written by the driver and read by the device.
#[repr(C, align(2))]
#[derive(Debug, Copy, Clone, Pod)]
pub struct AvailRing {
    flags: AvailFlags,
    /// A driver MUST NOT decrement the idx.
    idx: u16,
    ring: [u16; 64], // actual size: queue_size
    used_event: u16, // unused
}

/// The used ring is where the device returns buffers once it is done with them:
/// it is only written to by the device, and read by the driver.
#[repr(C, align(4))]
#[derive(Debug, Copy, Clone, Pod)]
pub struct UsedRing {
    // the flag in UsedRing
    flags: u16,
    // the next index of the used element in ring array
    idx: u16,
    ring: [UsedElem; 64], // actual size: queue_size
    avail_event: u16,     // unused
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone, Pod)]
pub struct UsedElem {
    id: u32,
    len: u32,
}

bitflags! {
    /// The flags useds in [`AvailRing`]
    #[repr(C)]
    #[derive(Pod)]
    pub struct AvailFlags: u16 {
        /// The flag used to disable virt queue interrupt
        const VIRTQ_AVAIL_F_NO_INTERRUPT = 1;
    }
}



/// Simulates the device reading from a VirtIO queue and writing a response back, for use in tests.
///
/// The fake device always uses descriptors in order.
///
/// Returns true if a descriptor chain was available and processed, or false if no descriptors were
/// available.
#[cfg(test)]
pub(crate) fn fake_read_write_queue<const QUEUE_SIZE: usize>(
    descriptors: *const [Descriptor; QUEUE_SIZE],
    queue_driver_area: *const u8,
    queue_device_area: *mut u8,
    handler: impl FnOnce(Vec<u8>) -> Vec<u8>,
) -> bool {
    use core::{ops::Deref, slice};

    let available_ring = queue_driver_area as *const AvailRing<QUEUE_SIZE>;
    let used_ring = queue_device_area as *mut UsedRing<QUEUE_SIZE>;

    // Safe because the various pointers are properly aligned, dereferenceable, initialised, and
    // nothing else accesses them during this block.
    unsafe {
        // Make sure there is actually at least one descriptor available to read from.
        if (*available_ring).idx.load(Ordering::Acquire) == (*used_ring).idx.load(Ordering::Acquire)
        {
            return false;
        }
        // The fake device always uses descriptors in order, like VIRTIO_F_IN_ORDER, so
        // `used_ring.idx` marks the next descriptor we should take from the available ring.
        let next_slot = (*used_ring).idx.load(Ordering::Acquire) & (QUEUE_SIZE as u16 - 1);
        let head_descriptor_index = (*available_ring).ring[next_slot as usize];
        let mut descriptor = &(*descriptors)[head_descriptor_index as usize];

        let input_length;
        let output;
        if descriptor.flags.contains(DescFlags::INDIRECT) {
            // The descriptor shouldn't have any other flags if it is indirect.
            assert_eq!(descriptor.flags, DescFlags::INDIRECT);

            // Loop through all input descriptors in the indirect descriptor list, reading data from
            // them.
            let indirect_descriptor_list: &[Descriptor] = zerocopy::Ref::into_ref(
                zerocopy::Ref::<_, [Descriptor]>::from_bytes(slice::from_raw_parts(
                    descriptor.addr as *const u8,
                    descriptor.len as usize,
                ))
                .unwrap(),
            );
            let mut input = Vec::new();
            let mut indirect_descriptor_index = 0;
            while indirect_descriptor_index < indirect_descriptor_list.len() {
                let indirect_descriptor = &indirect_descriptor_list[indirect_descriptor_index];
                if indirect_descriptor.flags.contains(DescFlags::WRITE) {
                    break;
                }

                input.extend_from_slice(slice::from_raw_parts(
                    indirect_descriptor.addr as *const u8,
                    indirect_descriptor.len as usize,
                ));

                indirect_descriptor_index += 1;
            }
            input_length = input.len();

            // Let the test handle the request.
            output = handler(input);

            // Write the response to the remaining descriptors.
            let mut remaining_output = output.deref();
            while indirect_descriptor_index < indirect_descriptor_list.len() {
                let indirect_descriptor = &indirect_descriptor_list[indirect_descriptor_index];
                assert!(indirect_descriptor.flags.contains(DescFlags::WRITE));

                let length_to_write = min(remaining_output.len(), indirect_descriptor.len as usize);
                ptr::copy(
                    remaining_output.as_ptr(),
                    indirect_descriptor.addr as *mut u8,
                    length_to_write,
                );
                remaining_output = &remaining_output[length_to_write..];

                indirect_descriptor_index += 1;
            }
            assert_eq!(remaining_output.len(), 0);
        } else {
            // Loop through all input descriptors in the chain, reading data from them.
            let mut input = Vec::new();
            while !descriptor.flags.contains(DescFlags::WRITE) {
                input.extend_from_slice(slice::from_raw_parts(
                    descriptor.addr as *const u8,
                    descriptor.len as usize,
                ));

                if let Some(next) = descriptor.next() {
                    descriptor = &(*descriptors)[next as usize];
                } else {
                    break;
                }
            }
            input_length = input.len();

            // Let the test handle the request.
            output = handler(input);

            // Write the response to the remaining descriptors.
            let mut remaining_output = output.deref();
            if descriptor.flags.contains(DescFlags::WRITE) {
                loop {
                    assert!(descriptor.flags.contains(DescFlags::WRITE));

                    let length_to_write = min(remaining_output.len(), descriptor.len as usize);
                    ptr::copy(
                        remaining_output.as_ptr(),
                        descriptor.addr as *mut u8,
                        length_to_write,
                    );
                    remaining_output = &remaining_output[length_to_write..];

                    if let Some(next) = descriptor.next() {
                        descriptor = &(*descriptors)[next as usize];
                    } else {
                        break;
                    }
                }
            }
            assert_eq!(remaining_output.len(), 0);
        }

        // Mark the buffer as used.
        (*used_ring).ring[next_slot as usize].id = head_descriptor_index.into();
        (*used_ring).ring[next_slot as usize].len = (input_length + output.len()) as u32;
        (*used_ring).idx.fetch_add(1, Ordering::AcqRel);

        true
    }
}
