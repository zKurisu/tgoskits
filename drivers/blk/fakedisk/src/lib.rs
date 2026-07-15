//! Minimal fake block device — read returns zeros, write is a no-op.
//! Demonstrates the rdif_block trait implementation with zero OS dependencies.

#![no_std]
extern crate alloc;

use alloc::boxed::Box;
use rdif_block::{
    BlkError, DeviceInfo, DriverGeneric, IQueue, Interface, QueueInfo, QueueLimits,
    Request, RequestId, RequestOp, RequestStatus, validate_request,
};

const FAKEDISK_NAME: &str = "fakedisk";

pub struct FakeDisk {
    block_size: usize,
    num_blocks: usize,
}

impl FakeDisk {
    pub fn new(block_size: usize, num_blocks: usize) -> Self {
        Self { block_size, num_blocks }
    }
}

impl DriverGeneric for FakeDisk {
    fn name(&self) -> &str { FAKEDISK_NAME }
    fn raw_any(&self) -> Option<&dyn core::any::Any> { Some(self) }
    fn raw_any_mut(&mut self) -> Option<&mut dyn core::any::Any> { Some(self) }
}

impl Interface for FakeDisk {
    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            name: Some(FAKEDISK_NAME),
            ..DeviceInfo::new(self.num_blocks as u64, self.block_size)
        }
    }

    fn queue_limits(&self) -> QueueLimits {
        QueueLimits::simple(self.block_size, u64::MAX)
    }

    fn create_queue(&mut self) -> Option<Box<dyn IQueue>> {
        Some(Box::new(FakeQueue {
            id: 0,
            device: self.device_info(),
            limits: self.queue_limits(),
            next_id: 0,
        }))
    }
}

struct FakeQueue {
    id: usize,
    device: DeviceInfo,
    limits: QueueLimits,
    next_id: usize,
}

unsafe impl IQueue for FakeQueue {
    fn id(&self) -> usize { self.id }

    fn info(&self) -> QueueInfo {
        QueueInfo { id: self.id, device: self.device, limits: self.limits }
    }

    fn submit_request(&mut self, request: Request<'_>) -> Result<RequestId, BlkError> {
        validate_request(self.info(), &request)?;
        let req_id = RequestId::new(self.next_id);
        self.next_id += 1;

        match request.op {
            RequestOp::Read => {
                for seg in request.segments.iter() {
                    unsafe { core::ptr::write_bytes(seg.virt, 0, seg.len); }
                }
            }
            RequestOp::Write | RequestOp::Flush => {}
            _ => return Err(BlkError::NotSupported),
        }
        Ok(req_id)
    }

    fn poll_request(&mut self, _: RequestId) -> Result<RequestStatus, BlkError> {
        Ok(RequestStatus::Complete)
    }
}

// ── Unit tests: run with `cargo test -p fakedisk` ──

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use rdif_block::{RequestFlags, Segment};

    /// Heap-allocate a buffer and return a Segment pointing into it.
    /// The Vec must stay alive for the request's lifetime.
    unsafe fn make_segment(buf: &mut Vec<u8>) -> Segment<'_> {
        unsafe { Segment::from_raw_parts(buf.as_mut_ptr(), 0, buf.len()) }
    }

    fn make_request<'a>(
        op: RequestOp, lba: u64, block_count: u32, segments: &'a mut [Segment<'a>],
    ) -> Request<'a> {
        Request { op, lba, block_count, segments, flags: RequestFlags::NONE }
    }

    // ── Basic device properties ──

    #[test]
    fn device_has_correct_size() {
        let disk = FakeDisk::new(512, 32768);
        let info = disk.device_info();
        assert_eq!(info.num_blocks, 32768);
        assert_eq!(info.logical_block_size, 512);
    }

    #[test]
    fn device_has_correct_name() {
        let disk = FakeDisk::new(512, 1);
        assert_eq!(disk.name(), "fakedisk");
        assert_eq!(disk.device_info().name, Some("fakedisk"));
    }

    #[test]
    fn raw_any_downcasts_back_to_self() {
        let disk = FakeDisk::new(512, 1);
        let any: &dyn core::any::Any = disk.raw_any().unwrap();
        assert!(any.downcast_ref::<FakeDisk>().is_some());
    }

    #[test]
    fn create_queue_returns_some() {
        let mut disk = FakeDisk::new(512, 1);
        assert!(disk.create_queue().is_some());
    }

    // ── Queue info ──

    #[test]
    fn queue_info_matches_device() {
        let mut disk = FakeDisk::new(512, 8);
        let queue = disk.create_queue().unwrap();
        let info = queue.info();
        assert_eq!(info.device.logical_block_size, 512);
        assert_eq!(info.device.num_blocks, 8);
    }

    // ── Read operations ──

    #[test]
    fn read_block_zero_returns_zeros() {
        let mut disk = FakeDisk::new(512, 1);
        let mut queue = disk.create_queue().unwrap();

        let mut buf = vec![0xFFu8; 512];
        let seg = unsafe { make_segment(&mut buf) };
        let mut segments = [seg];
        let req = make_request(RequestOp::Read, 0, 1, &mut segments[..]);

        let id = queue.submit_request(req).unwrap();
        assert_eq!(queue.poll_request(id).unwrap(), RequestStatus::Complete);

        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn read_single_block_returns_zeros() {
        let mut disk = FakeDisk::new(512, 1);
        let mut queue = disk.create_queue().unwrap();

        let mut buf = vec![0xAAu8; 512];
        let seg = unsafe { make_segment(&mut buf) };
        let mut segments = [seg];
        let req = make_request(RequestOp::Read, 0, 1, &mut segments[..]);

        let id = queue.submit_request(req).unwrap();
        assert_eq!(queue.poll_request(id).unwrap(), RequestStatus::Complete);

        assert!(buf.iter().all(|&b| b == 0));
    }

    // ── Write ──

    #[test]
    fn write_is_noop_and_succeeds() {
        let mut disk = FakeDisk::new(512, 1);
        let mut queue = disk.create_queue().unwrap();

        let mut buf = vec![0xCCu8; 512];
        let seg = unsafe { make_segment(&mut buf) };
        let mut segments = [seg];
        let req = make_request(RequestOp::Write, 0, 1, &mut segments[..]);

        let id = queue.submit_request(req).unwrap();
        assert_eq!(queue.poll_request(id).unwrap(), RequestStatus::Complete);
    }

    // ── Flush ──

    #[test]
    fn flush_is_not_supported() {
        let mut disk = FakeDisk::new(512, 1);
        let mut queue = disk.create_queue().unwrap();

        let segments = &mut [];
        let req = Request {
            op: RequestOp::Flush,
            lba: 0,
            block_count: 0,
            segments,
            flags: RequestFlags::NONE,
        };

        let err = queue.submit_request(req).unwrap_err();
        assert_eq!(err, BlkError::NotSupported);
    }

    // ── Unsupported operations ──

    #[test]
    fn discard_returns_not_supported() {
        let mut disk = FakeDisk::new(512, 1);
        let mut queue = disk.create_queue().unwrap();

        let segments = &mut [];
        let req = Request {
            op: RequestOp::Discard,
            lba: 0,
            block_count: 1,
            segments,
            flags: RequestFlags::NONE,
        };

        let err = queue.submit_request(req).unwrap_err();
        assert_eq!(err, BlkError::NotSupported);
    }

    #[test]
    fn write_zeroes_returns_not_supported() {
        let mut disk = FakeDisk::new(512, 1);
        let mut queue = disk.create_queue().unwrap();

        let segments = &mut [];
        let req = Request {
            op: RequestOp::WriteZeroes,
            lba: 0,
            block_count: 1,
            segments,
            flags: RequestFlags::NONE,
        };

        let err = queue.submit_request(req).unwrap_err();
        assert_eq!(err, BlkError::NotSupported);
    }

    // ── Validation ──

    #[test]
    fn out_of_range_lba_is_rejected() {
        let mut disk = FakeDisk::new(512, 4); // blocks 0-3 are valid
        let mut queue = disk.create_queue().unwrap();

        let mut buf = vec![0u8; 512];
        let seg = unsafe { make_segment(&mut buf) };
        let mut segments = [seg];
        let req = make_request(RequestOp::Read, 4, 1, &mut segments[..]); // LBA 4 → out of range

        assert!(queue.submit_request(req).is_err());
    }

    #[test]
    fn multi_block_exceeds_limit() {
        // QueueLimits::simple() sets max_blocks_per_request=1
        let mut disk = FakeDisk::new(512, 8);
        let mut queue = disk.create_queue().unwrap();

        let mut buf = vec![0u8; 512 * 2];
        let seg = unsafe { make_segment(&mut buf) };
        let mut segments = [seg];
        let req = make_request(RequestOp::Read, 0, 2, &mut segments[..]);

        assert!(queue.submit_request(req).is_err());
    }

    // ── Request ID increments ──

    #[test]
    fn request_id_increments() {
        let mut disk = FakeDisk::new(512, 1);
        let mut queue = disk.create_queue().unwrap();

        let mut buf = vec![0u8; 512];

        let id1 = {
            let seg = unsafe { make_segment(&mut buf) };
            let mut segments = [seg];
            let req = make_request(RequestOp::Read, 0, 1, &mut segments[..]);
            queue.submit_request(req).unwrap()
        };
        let id2 = {
            let seg = unsafe { make_segment(&mut buf) };
            let mut segments = [seg];
            let req = make_request(RequestOp::Read, 0, 1, &mut segments[..]);
            queue.submit_request(req).unwrap()
        };

        assert!(id2 > id1);
    }
}
