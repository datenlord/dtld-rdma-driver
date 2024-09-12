use std::time::Duration;

use parking_lot::{Mutex, MutexGuard};

use super::DeviceError;

/// A trait provides the ability to poll a descriptor.
///
/// The implementor should return `true` if the descriptor is ready to be processed.
/// Note that the buffer is mutable, so the implementor can update the descriptor in the buffer.
/// For example, the implementor can erase the `ready` field so that it won't be read again.
pub(super) trait PollDescriptor {
    fn poll(&self, buf: &mut [u8]) -> Result<bool, DeviceError>;
}

/// An adaptor to read the tail pointer and write the head pointer, using by writer.
pub(super) trait CsrWriterAdaptor {
    fn write_head(&self, data: u32) -> Result<(), DeviceError>;
    fn read_tail(&self) -> Result<u32, DeviceError>;
}

/// An adaptor to read the head pointer and write the tail pointer, using by reader.
pub(super) trait CsrReaderAdaptor {
    fn write_tail(&self, data: u32) -> Result<(), DeviceError>;
    fn read_head(&self) -> Result<u32, DeviceError>;
}

/// The Ringbuf is a circular buffer used comunicate between the host and the card.
///
/// `T` is an adaptor to provide meta data of the queue(like head pointer or is it ready)
/// `BUF` is the buffer.
/// `DEPTH` is the max capacity of the ringbuf.
/// `ELEM_SIZE` is the size of each descriptor.
/// `PAGE_SIZE` is the size of the page. In real hardware, the buffer should be aligned to `PAGE_SIZE`.
#[derive(Debug)]
pub(super) struct Ringbuf<
    T,
    BUF,
    const DEPTH: usize,
    const ELEM_SIZE: usize,
    const PAGE_SIZE: usize,
> {
    buf: Mutex<BUF>,
    head: usize,
    tail: usize,
    adaptor: T,
}

/// A writer for host to write descriptors to the ring buffer.
pub(super) struct RingbufWriter<
    'a,
    'adaptor,
    T: CsrWriterAdaptor,
    BUF: AsMut<[u8]>,
    const DEPTH: usize,
    const ELEM_SIZE: usize,
> {
    buf: MutexGuard<'a, BUF>,
    head: &'a mut usize,
    tail: &'a mut usize,
    written_cnt: usize,
    adaptor: &'adaptor T,
}

impl<
        T,
        BUF: AsMut<[u8]> + AsRef<[u8]>,
        const DEPTH: usize,
        const ELEM_SIZE: usize,
        const PAGE_SIZE: usize,
    > Ringbuf<T, BUF, DEPTH, ELEM_SIZE, PAGE_SIZE>
{
    const _IS_DEPTH_POWER_OF_2: () = assert!(_is_power_of_2(DEPTH), "invalid ringbuf depth");
    const _IS_ELEM_SIZE_POWER_OF_2: () = assert!(_is_power_of_2(ELEM_SIZE), "invalid element size");
    const _IS_RINGBUF_SIZE_VALID: () =
        assert!(DEPTH * ELEM_SIZE >= PAGE_SIZE, "invalid ringbuf size");

    /// Return (ringbuf, ringbuf virtual memory address)
    #[allow(clippy::arithmetic_side_effects)] // false positive in assert
    pub(super) fn new(adaptor: T, buffer: BUF) -> Self {
        #[cfg(not(test))]
        {
            assert!(
                (buffer.as_ref().as_ptr() as usize).wrapping_rem(PAGE_SIZE) == 0,
                "buffer should be aligned to PAGE_SIZE"
            );
            assert!(
                buffer.as_ref().len().wrapping_rem(PAGE_SIZE) == 0,
                "buffer size should be multiple of PAGE_SIZE"
            );
        }
        assert!(
            buffer.as_ref().len() >= DEPTH.wrapping_mul(ELEM_SIZE),
            "buffer is too small"
        );
        Self {
            buf: Mutex::new(buffer),
            head: 0,
            tail: 0,
            adaptor,
        }
    }
}

impl<
        T: CsrWriterAdaptor,
        BUF: AsMut<[u8]>,
        const DEPTH: usize,
        const ELEM_SIZE: usize,
        const PAGE_SIZE: usize,
    > Ringbuf<T, BUF, DEPTH, ELEM_SIZE, PAGE_SIZE>
{
    /// Return a writer to write descriptors to the ring buffer.
    pub(super) fn write(&mut self) -> RingbufWriter<'_, '_, T, BUF, DEPTH, ELEM_SIZE> {
        RingbufWriter {
            buf: self.buf.lock(),
            head: &mut self.head,
            tail: &mut self.tail,
            written_cnt: 0,
            adaptor: &self.adaptor,
        }
    }
}

impl<
        T: CsrReaderAdaptor,
        BUF: AsMut<[u8]>,
        const DEPTH: usize,
        const ELEM_SIZE: usize,
        const PAGE_SIZE: usize,
    > Ringbuf<T, BUF, DEPTH, ELEM_SIZE, PAGE_SIZE>
{
    /// Get a reader to read descriptors from the ring buffer.
    pub(super) fn read(&mut self) -> RingbufReader<'_, '_, T, BUF, DEPTH, ELEM_SIZE> {
        RingbufReader {
            buf: self.buf.lock(),
            head: &mut self.head,
            tail: &mut self.tail,
            read_cnt: 0,
            adaptor: &self.adaptor,
        }
    }
}

impl<
        T: PollDescriptor,
        BUF: AsMut<[u8]>,
        const DEPTH: usize,
        const ELEM_SIZE: usize,
        const PAGE_SIZE: usize,
    > Ringbuf<T, BUF, DEPTH, ELEM_SIZE, PAGE_SIZE>
{
    pub(super) fn read_via_poll_descriptor(
        &mut self,
    ) -> RingbufPollDescriptorReader<'_, '_, T, BUF, DEPTH, ELEM_SIZE> {
        RingbufPollDescriptorReader {
            buf: self.buf.lock(),
            adaptor: &self.adaptor,
            head: &mut self.head,
            tail: &mut self.tail,
        }
    }
}

impl<'a, T: CsrWriterAdaptor, BUF: AsMut<[u8]>, const DEPTH: usize, const ELEM_SIZE: usize>
    RingbufWriter<'a, '_, T, BUF, DEPTH, ELEM_SIZE>
{
    const HARDWARE_IDX_MASK: usize = gen_hardware_idx_mask(DEPTH);
    const HARDWARE_IDX_GUARD_MASK: usize = gen_hardware_idx_guard_mask(DEPTH);
    const MEMORY_IDX_MASK: usize = gen_memory_idx_mask(DEPTH);

    /// get a buffer to write a descriptor to the ring buffer
    pub(crate) fn next(&mut self) -> Result<&'a mut [u8], DeviceError> {
        self.next_timeout(None)
    }

    /// get a buffer to write a descriptor to the ring buffer with timeout
    ///
    /// If the timeout is reached, it will return `DeviceError::Timeout`.
    /// If the timeout is `None`, it will block until a descriptor is ready.
    ///
    /// # Errors
    /// `DeviceError::Timeout`: if the timeout is reached.
    /// Other: if the underlying adaptor returns an error.
    pub(crate) fn next_timeout(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<&'a mut [u8], DeviceError> {
        let timeout_in_millis = timeout.map_or(0, |d| d.as_millis());
        let start = std::time::Instant::now();
        let idx = self.next_head_idx();
        if self.would_it_full(idx, *self.tail) {
            // write back first
            self.advance_head()?;
            loop {
                let new_tail = self.adaptor.read_tail()?;
                if !self.would_it_full(idx, new_tail as usize) {
                    *self.tail = new_tail as usize;
                    break;
                }
                if timeout_in_millis > 0 && start.elapsed().as_millis() > timeout_in_millis {
                    return Err(DeviceError::Timeout);
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        let buf =
            get_descriptor_mut_helper(self.buf.as_mut(), idx, ELEM_SIZE, Self::MEMORY_IDX_MASK);
        self.written_cnt = self.written_cnt.wrapping_add(1);
        Ok(buf)
    }

    /// Write back the head pointer to the hardware.
    pub(crate) fn flush(&mut self) -> Result<(), DeviceError> {
        self.advance_head()
    }

    #[allow(clippy::unused_self)] // Using Self may lead to lifetime issue
    fn would_it_full(&self, new_head: usize, new_tail: usize) -> bool {
        is_full_helper(
            new_head,
            new_tail,
            Self::MEMORY_IDX_MASK,         // the mask for the rest bits
            Self::HARDWARE_IDX_GUARD_MASK, // the mask for the highest bit
        )
    }

    fn next_head_idx(&self) -> usize {
        wrapping_add_helper(*self.head, self.written_cnt, Self::HARDWARE_IDX_MASK)
    }

    fn advance_head(&mut self) -> Result<(), DeviceError> {
        let new_head = self.next_head_idx();
        *self.head = new_head;
        // since the head is got by wrapping_add, it's safe to cast to u32
        #[allow(clippy::cast_possible_truncation)]
        self.adaptor.write_head(new_head as u32)?;
        self.written_cnt = 0;
        Ok(())
    }
}

/// Drop the writer to update the head pointer.
impl<T: CsrWriterAdaptor, BUF: AsMut<[u8]>, const DEPTH: usize, const ELEM_SIZE: usize> Drop
    for RingbufWriter<'_, '_, T, BUF, DEPTH, ELEM_SIZE>
{
    fn drop(&mut self) {
        if let Err(e) = self.advance_head() {
            log::error!("failed to advance head pointer: {:?}", e);
        }
    }
}

/// A reader for host to read descriptors from the ring buffer.
pub(super) struct RingbufReader<
    'a,
    'adaptor,
    T: CsrReaderAdaptor,
    BUF: AsMut<[u8]>,
    const DEPTH: usize,
    const ELEM_SIZE: usize,
> {
    buf: MutexGuard<'a, BUF>,
    head: &'a mut usize,
    tail: &'a mut usize,
    read_cnt: usize,
    adaptor: &'adaptor T,
}

impl<'a, T: CsrReaderAdaptor, BUF: AsMut<[u8]>, const DEPTH: usize, const ELEM_SIZE: usize>
    RingbufReader<'a, '_, T, BUF, DEPTH, ELEM_SIZE>
{
    const HARDWARE_IDX_MASK: usize = gen_hardware_idx_mask(DEPTH);
    const HARDWARE_IDX_GUARD_MASK: usize = gen_hardware_idx_guard_mask(DEPTH);
    const MEMORY_IDX_MASK: usize = gen_memory_idx_mask(DEPTH);

    /// read a descriptor from the ring buffer
    pub(crate) fn next(&mut self) -> Result<&'a mut [u8], DeviceError> {
        self.next_timeout(None)
    }

    /// read a descriptor from the ring buffer with timeout
    ///
    /// If the timeout is reached, it will return `DeviceError::Timeout`.
    /// If the timeout is `None`, it will block until a descriptor is ready.
    ///
    /// # Errors
    /// `DeviceError::Timeout`: if the timeout is reached.
    /// Other: if the underlying adaptor returns an error.
    pub(crate) fn next_timeout(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<&'a mut [u8], DeviceError> {
        let timeout_in_millis = timeout.map_or(0, |d| d.as_millis());
        let start = std::time::Instant::now();
        if self.is_full() {
            self.advance_tail()?;
        }
        let next_tail_idx = self.next_tail_idx();
        if Self::would_it_empty(*self.head, next_tail_idx) {
            loop {
                let new_head = self.adaptor.read_head()?;
                if !Self::would_it_empty(new_head as usize, next_tail_idx) {
                    *self.head = new_head as usize;
                    break;
                }
                if timeout_in_millis > 0 && start.elapsed().as_millis() > timeout_in_millis {
                    return Err(DeviceError::Timeout);
                }
            }
        }
        self.read_cnt = self.read_cnt.wrapping_add(1);
        let buf = get_descriptor_mut_helper(
            self.buf.as_mut(),
            next_tail_idx,
            ELEM_SIZE,
            Self::MEMORY_IDX_MASK,
        );
        Ok(buf)
    }

    /// Write back the tail pointer to the hardware.
    pub(crate) fn flush(&mut self) -> Result<(), DeviceError> {
        self.advance_tail()
    }

    fn next_tail_idx(&self) -> usize {
        wrapping_add_helper(*self.tail, self.read_cnt, Self::HARDWARE_IDX_MASK)
    }

    fn would_it_empty(new_head: usize, new_tail: usize) -> bool {
        is_empty_helper(new_head, new_tail)
    }

    fn is_full(&self) -> bool {
        is_full_helper(
            *self.head,
            *self.tail,
            Self::MEMORY_IDX_MASK,         // the mask for the rest bits
            Self::HARDWARE_IDX_GUARD_MASK, // the mask for the highest bit
        )
    }

    fn advance_tail(&mut self) -> Result<(), DeviceError> {
        let new_tail = self.next_tail_idx();
        #[allow(clippy::cast_possible_truncation)] // new_tail must be in range of `DEPTH *2`
        self.adaptor.write_tail(new_tail as u32)?;
        *self.tail = new_tail;
        self.read_cnt = 0;
        Ok(())
    }
}

impl<T: CsrReaderAdaptor, BUF: AsMut<[u8]>, const DEPTH: usize, const ELEM_SIZE: usize> Drop
    for RingbufReader<'_, '_, T, BUF, DEPTH, ELEM_SIZE>
{
    fn drop(&mut self) {
        if let Err(e) = self.advance_tail() {
            log::error!("failed to advance tail pointer: {:?}", e);
        }
    }
}

/// A polling reader for host to read descriptors from the ring buffer.
pub(super) struct RingbufPollDescriptorReader<
    'a,
    'adaptor,
    T: PollDescriptor,
    BUF: AsMut<[u8]>,
    const DEPTH: usize,
    const ELEM_SIZE: usize,
> {
    buf: MutexGuard<'a, BUF>,
    adaptor: &'adaptor T,
    head: &'a mut usize,
    tail: &'a mut usize,
}

impl<'a, T: PollDescriptor, BUF: AsMut<[u8]>, const DEPTH: usize, const ELEM_SIZE: usize>
    RingbufPollDescriptorReader<'_, '_, T, BUF, DEPTH, ELEM_SIZE>
{
    const HARDWARE_IDX_MASK: usize = gen_hardware_idx_mask(DEPTH);
    const MEMORY_IDX_MASK: usize = gen_memory_idx_mask(DEPTH);

    pub(crate) fn next(&mut self) -> Result<&'a [u8], DeviceError> {
        self.next_timeout(None)
    }

    fn next_timeout(&mut self, timeout: Option<Duration>) -> Result<&'a [u8], DeviceError> {
        let timeout_in_millis = timeout.map_or(0, |d| d.as_millis());
        let start = std::time::Instant::now();
        let current = self.current_idx();

        loop {
            let buf = get_descriptor_mut_helper(
                self.buf.as_mut(),
                current,
                ELEM_SIZE,
                Self::MEMORY_IDX_MASK,
            );
            if self.adaptor.poll(buf)? {
                self.advance_idx();
                return Ok(buf);
            }
            if timeout_in_millis > 0 && start.elapsed().as_millis() > timeout_in_millis {
                return Err(DeviceError::Timeout);
            }
        }
    }

    const fn current_idx(&self) -> usize {
        *self.head
    }

    fn advance_idx(&mut self) {
        let next_idx = wrapping_add_helper(self.current_idx(), 1, Self::HARDWARE_IDX_MASK);
        *self.head = next_idx;
        *self.tail = next_idx;
    }
}

const fn _is_power_of_2(v: usize) -> bool {
    (v & (v.wrapping_sub(1))) == 0
}

fn is_full_helper(
    head: usize,
    tail: usize,
    memory_idx_mask: usize,
    hardware_idx_guard_mask: usize,
) -> bool {
    // Since the highest bit stands for two times of the DEPTH in bineary, if the head and tail have different highest bit and the rest bits are the same,
    // it means the ringbuf is full.
    // In hardware we use like `(head.idx == tail.idx) && (head.guard != tail.guard)`
    let head_guard = head & hardware_idx_guard_mask;
    let tail_guard = tail & hardware_idx_guard_mask;
    let head_low = head & memory_idx_mask;
    let tail_low = tail & memory_idx_mask;
    (head_guard != tail_guard) && (head_low == tail_low)
}

const fn is_empty_helper(head: usize, tail: usize) -> bool {
    head == tail
}

const fn wrapping_add_helper(current: usize, cnt: usize, hardware_idx_mask: usize) -> usize {
    current.wrapping_add(cnt) & hardware_idx_mask
}

const fn gen_hardware_idx_mask(depth: usize) -> usize {
    // depth * 2 -1
    depth.wrapping_mul(2).wrapping_sub(1)
}

const fn gen_hardware_idx_guard_mask(depth: usize) -> usize {
    depth
}

const fn gen_memory_idx_mask(depth: usize) -> usize {
    // depth -1
    depth.wrapping_sub(1)
}

#[allow(unsafe_code)]
fn get_descriptor_mut_helper(
    buf: &mut [u8],
    idx: usize,
    element_size: usize,
    idx_mask: usize,
) -> &'static mut [u8] {
    let offset = (idx & idx_mask).wrapping_mul(element_size);
    let ptr = unsafe { buf.as_mut_ptr().add(offset) };
    unsafe { std::slice::from_raw_parts_mut(ptr, element_size) }
}

#[allow(unsafe_code)]
fn get_descriptor_helper(
    buf: &[u8],
    idx: usize,
    element_size: usize,
    idx_mask: usize,
) -> &'static [u8] {
    let offset = (idx & idx_mask).wrapping_mul(element_size);
    let ptr = unsafe { buf.as_ptr().add(offset) };
    unsafe { std::slice::from_raw_parts(ptr, element_size) }
}

#[cfg(test)]
mod test {
    use std::{
        slice::from_raw_parts_mut,
        sync::{
            atomic::{AtomicBool, AtomicU32, Ordering},
            Arc,
        },
        thread::{sleep, spawn},
        time::Duration,
    };

    use rand::Rng;

    use crate::{device::DeviceError, types::PAGE_SIZE};

    use super::{PollDescriptor, Ringbuf};

    #[derive(Debug, Clone)]
    struct Adaptor(Arc<AdaptorInner>);

    #[derive(Debug)]
    struct AdaptorInner {
        head: AtomicU32,
        tail: AtomicU32,
    }

    impl Adaptor {
        fn consume(&self) {
            // move the tail to the head
            let head = self.0.head.load(Ordering::Acquire);
            self.0.tail.store(head, Ordering::Release);
        }

        fn check(&self, max_value: u32) {
            // move the tail to the head
            let head = self.0.head.load(Ordering::Acquire);
            let tail = self.0.head.load(Ordering::Acquire);
            assert!(tail <= max_value && head <= max_value);
            let diff = (head as i32 - tail as i32) as u32 & max_value;
            assert!(diff <= max_value);
        }

        fn produce<const DEPTH: usize>(&self, cnt: usize) {
            // move the head to the tail
            let head = self.0.head.load(Ordering::Acquire);
            let tail = self.0.tail.load(Ordering::Acquire);
            let cnt = cnt % (DEPTH + 1);
            let is_full = ((head as i32 - tail as i32) as usize) == DEPTH;
            if !is_full {
                let new_head = (head + cnt as u32) % (DEPTH * 2) as u32;
                self.0.head.store(new_head, Ordering::Release);
            }
        }
        fn head(&self) -> u32 {
            self.0.head.load(Ordering::Acquire)
        }

        fn tail(&self) -> u32 {
            self.0.tail.load(Ordering::Acquire)
        }
    }
    impl super::CsrWriterAdaptor for Adaptor {
        fn write_head(&self, data: u32) -> Result<(), DeviceError> {
            self.0.head.store(data, Ordering::Release);
            Ok(())
        }
        fn read_tail(&self) -> Result<u32, DeviceError> {
            Ok(self.0.tail.load(Ordering::Acquire))
        }
    }
    impl super::CsrReaderAdaptor for Adaptor {
        fn write_tail(&self, data: u32) -> Result<(), DeviceError> {
            self.0.tail.store(data, Ordering::Release);
            Ok(())
        }
        fn read_head(&self) -> Result<u32, DeviceError> {
            Ok(self.0.head.load(Ordering::Acquire))
        }
    }
    #[test]
    fn test_ringbuf_writer() {
        const MAX_DEPTH: usize = 128;
        const MAX_VALUE: u32 = 255;
        let adaptor = Adaptor(Arc::new(AdaptorInner {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
        }));
        let thread_proxy = adaptor.clone();
        let _ = spawn(move || loop {
            sleep(std::time::Duration::from_millis(100));
            thread_proxy.consume();
            thread_proxy.check(MAX_VALUE);
        });
        let buffer = vec![0u8; PAGE_SIZE];
        let mut ringbuf =
            Ringbuf::<Adaptor, Vec<u8>, MAX_DEPTH, 32, 4096>::new(adaptor.clone(), buffer);
        let mut writer = ringbuf.write();

        for i in 0..128 {
            let desc = writer.next().unwrap();
            desc.fill(i);
        }
        drop(writer);
        assert!(adaptor.head() == 128);
        assert!(adaptor.tail() == 0);
        let mut writer = ringbuf.write();
        assert!(writer
            .next_timeout(Some(Duration::from_millis(10)))
            .is_err());
        assert!(writer
            .next_timeout(Some(Duration::from_millis(10)))
            .is_err());
        drop(writer);
        sleep(std::time::Duration::from_millis(100));
        assert!(adaptor.head() == 128);
        assert!(adaptor.tail() == 128);
        // test if blocking?

        let mut writer = ringbuf.write();
        for i in 0..=256 {
            let desc = writer.next().unwrap();
            desc.fill(i as u8);
        }
        drop(writer);
    }

    #[test]
    fn test_ringbuf_writer_random_write() {
        // test if we write random number of descriptors, will it work correctly
        const MAX_DEPTH: usize = 128;
        const MAX_VALUE: u32 = 255;
        let adaptor = Adaptor(Arc::new(AdaptorInner {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
        }));
        let buffer = vec![0u8; PAGE_SIZE];
        let mut ringbuf =
            Ringbuf::<Adaptor, Vec<u8>, MAX_DEPTH, 32, 4096>::new(adaptor.clone(), buffer);
        let thread_proxy = adaptor.clone();
        let _ = spawn(move || {
            let mut rng = rand::thread_rng();
            sleep(std::time::Duration::from_millis(10));
            loop {
                // periodically and randomly consume the ringbuf
                let sleep_time: u64 = rng.gen_range(1..10);
                sleep(std::time::Duration::from_millis(sleep_time));
                thread_proxy.consume();
                thread_proxy.check(MAX_VALUE);
            }
        });

        let mut rng = rand::thread_rng();
        for _ in 0..500 {
            let mut writer = ringbuf.write();
            let batch_to_write: u8 = rng.gen_range(3..200);
            for _ in 0..batch_to_write {
                let desc = writer.next().unwrap();
                desc.fill(batch_to_write);
            }
            adaptor.check(MAX_VALUE);
        }
    }

    #[test]
    fn test_ringbuf_reader() {
        const MAX_DEPTH: usize = 128;
        const MAX_VALUE: u32 = 255;
        let adaptor = Adaptor(Arc::new(AdaptorInner {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
        }));
        let thread_proxy = adaptor.clone();
        let _ = spawn(move || loop {
            thread_proxy.produce::<128>(128);
            sleep(std::time::Duration::from_millis(10));
            thread_proxy.check(MAX_VALUE);
        });
        let buffer = vec![0u8; PAGE_SIZE];
        let mut ringbuf =
            Ringbuf::<Adaptor, Vec<u8>, MAX_DEPTH, 32, 4096>::new(adaptor.clone(), buffer);
        let mut reader = ringbuf.read();
        sleep(std::time::Duration::from_millis(100));
        for _i in 0..128 {
            let _desc = reader.next().unwrap();
        }
        drop(reader);
        assert!(adaptor.tail() == 128);

        let mut reader = ringbuf.read();

        let finish_flag = Arc::new(AtomicBool::new(false));
        for _i in 0..130 {
            let _desc = reader.next().unwrap();
        }
        drop(reader);
        finish_flag.store(true, Ordering::Relaxed);
    }

    #[test]
    fn test_ringbuf_reader_random() {
        const MAX_DEPTH: usize = 128;
        const MAX_VALUE: u32 = 255;
        let adaptor = Adaptor(Arc::new(AdaptorInner {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
        }));
        let thread_proxy = adaptor.clone();
        let _ = spawn(move || {
            let mut rng = rand::thread_rng();
            loop {
                thread_proxy.check(MAX_VALUE);
                let produce: u8 = rng.gen_range(1..128);
                thread_proxy.produce::<MAX_DEPTH>(produce.into());
                sleep(std::time::Duration::from_millis(10));
            }
        });
        let mut buffer = vec![0u8; PAGE_SIZE];

        for i in 0..128 {
            for j in 0..32 {
                buffer[i * 32 + j] = i as u8;
            }
        }
        let mut ringbuf =
            Ringbuf::<Adaptor, Vec<u8>, MAX_DEPTH, 32, 4096>::new(adaptor.clone(), buffer);
        let mut reader = ringbuf.read();
        for i in 0..4096 {
            let desc = reader.next().unwrap();
            assert!(desc[0] == (i % 128) as u8);
        }
    }

    struct MockDma {
        head: u32,
        memory: &'static mut [u8],
    }
    struct PollingAdaptor;
    impl PollDescriptor for PollingAdaptor {
        fn poll(&self, buf: &mut [u8]) -> Result<bool, DeviceError> {
            if buf[0] == 1 {
                buf[0] = 0;
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }

    impl MockDma {
        fn new(memory: &'static mut [u8]) -> Self {
            Self {
                head: u32::default(),
                memory,
            }
        }

        fn move_head(&mut self, n: u32, depth: u32, elem_size: u32) {
            let head = self.head;
            let n = n % (depth + 1);
            for i in 0..n {
                let offset = (((head + i) % depth) * elem_size) as usize;
                self.memory[offset] = 1;
            }
            let next_head = (head + n) % depth;
            self.head = next_head;
        }
    }

    #[test]
    fn test_ringbuf_reader_polling() {
        const MAX_DEPTH: usize = 128;
        let mut buffer = vec![0u8; PAGE_SIZE];

        for i in 0..128 {
            for j in 0..32 {
                buffer[i * 32 + j] = i as u8;
            }
            buffer[i * 32] = 0;
        }
        let dma_buf = unsafe { from_raw_parts_mut(buffer.as_mut_ptr(), PAGE_SIZE) };

        let mut dma = MockDma::new(dma_buf);

        let adaptor = PollingAdaptor;
        let mut ringbuf =
            Ringbuf::<PollingAdaptor, Vec<u8>, MAX_DEPTH, 32, 4096>::new(adaptor, buffer);
        let mut reader = ringbuf.read_via_poll_descriptor();
        dma.move_head(64, MAX_DEPTH.try_into().unwrap(), 32);
        for i in 0..64 {
            let desc = reader.next().unwrap();
            assert_eq!(desc[1], i);
        }
        dma.move_head(128, MAX_DEPTH.try_into().unwrap(), 32);
        for i in 64..192 {
            let desc = reader.next().unwrap();
            assert_eq!(desc[1], i % 128);
        }
    }
}
