use std::{
    io,
    sync::Arc,
};

use parking_lot::Mutex;

use crate::{
    device::{
        constants::{
            CSR_ADDR_CMD_REQ_QUEUE_HEAD, CSR_ADDR_CMD_REQ_QUEUE_TAIL, CSR_ADDR_CMD_RESP_QUEUE_HEAD,
            CSR_ADDR_CMD_RESP_QUEUE_TAIL, CSR_ADDR_META_REPORT_QUEUE_HEAD,
            CSR_ADDR_META_REPORT_QUEUE_TAIL, CSR_ADDR_SEND_QUEUE_HEAD, CSR_ADDR_SEND_QUEUE_TAIL,
        },
        ringbuf::{CsrReaderAdaptor, CsrWriterAdaptor},
        DeviceError,
    },
    MmapMemory,
};

pub(crate) const CSR_LENGTH: usize = 0x0010_0000;

#[derive(Debug, Clone)]
pub(crate) struct CsrClient(Arc<CsrClientInner>);

#[derive(Debug)]
struct CsrClientInner {
    mapping: Mutex<MmapMemory>,
}

impl CsrClient {
    pub(crate) fn new(csr_buf: MmapMemory) -> io::Result<Self> {
        let mapping = Mutex::new(csr_buf);

        Ok(Self(Arc::new(CsrClientInner { mapping })))
    }

    pub(crate) fn read_csr(&self, addr: usize) -> Result<u32, DeviceError> {
        if addr >= CSR_LENGTH {
            return Err(DeviceError::Device(format!("csr overflow :{addr}")));
        }
        let buf = self.0.mapping.lock();
        #[allow(clippy::ptr_offset_with_cast)]
        let offset = unsafe { buf.as_ptr().offset(addr as isize) as *const u32 };
        // read only 4 bytes. Because it is a hardware resouce.
        let val = unsafe { offset.read_volatile() };
        Ok(val)
    }

    pub(crate) fn write_csr(&self, addr: usize, data: u32) -> Result<(), DeviceError> {
        if addr >= CSR_LENGTH {
            return Err(DeviceError::Device(format!("csr overflow :{addr}")));
        }
        let mut buf = self.0.mapping.lock();
        #[allow(clippy::ptr_offset_with_cast)]
        let offset = unsafe { buf.as_ptr().offset(addr as isize) as *mut u32 };
        unsafe { offset.write_volatile(data) }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct ToCardCtrlRbCsrProxy(CsrClient);
impl ToCardCtrlRbCsrProxy {
    const HEAD_CSR: usize = CSR_ADDR_CMD_REQ_QUEUE_HEAD;
    const TAIL_CSR: usize = CSR_ADDR_CMD_REQ_QUEUE_TAIL;
    pub(crate) fn new(client: CsrClient) -> Self {
        Self(client)
    }
}
impl CsrWriterAdaptor for ToCardCtrlRbCsrProxy {
    fn write_head(&self, data: u32) -> Result<(), DeviceError> {
        self.0.write_csr(Self::HEAD_CSR, data)
    }
    fn read_tail(&self) -> Result<u32, DeviceError> {
        self.0.read_csr(Self::TAIL_CSR)
    }
}

#[derive(Debug)]
pub(crate) struct ToHostCtrlRbCsrProxy(CsrClient);

impl ToHostCtrlRbCsrProxy {
    const HEAD_CSR: usize = CSR_ADDR_CMD_RESP_QUEUE_HEAD;
    const TAIL_CSR: usize = CSR_ADDR_CMD_RESP_QUEUE_TAIL;
    pub(crate) fn new(client: CsrClient) -> Self {
        Self(client)
    }
}

impl CsrReaderAdaptor for ToHostCtrlRbCsrProxy {
    fn write_tail(&self, data: u32) -> Result<(), DeviceError> {
        self.0.write_csr(Self::TAIL_CSR, data)
    }
    fn read_head(&self) -> Result<u32, DeviceError> {
        self.0.read_csr(Self::HEAD_CSR)
    }
}

#[derive(Debug)]
pub(crate) struct ToCardWorkRbCsrProxy(CsrClient);

impl ToCardWorkRbCsrProxy {
    const HEAD_CSR: usize = CSR_ADDR_SEND_QUEUE_HEAD;
    const TAIL_CSR: usize = CSR_ADDR_SEND_QUEUE_TAIL;
    pub(crate) fn new(client: CsrClient) -> Self {
        Self(client)
    }
}

impl CsrWriterAdaptor for ToCardWorkRbCsrProxy {
    fn write_head(&self, data: u32) -> Result<(), DeviceError> {
        self.0.write_csr(Self::HEAD_CSR, data)
    }
    fn read_tail(&self) -> Result<u32, DeviceError> {
        self.0.read_csr(Self::TAIL_CSR)
    }
}

#[derive(Debug)]
pub(crate) struct ToHostWorkRbCsrProxy(CsrClient);

impl ToHostWorkRbCsrProxy {
    const HEAD_CSR: usize = CSR_ADDR_META_REPORT_QUEUE_HEAD;
    const TAIL_CSR: usize = CSR_ADDR_META_REPORT_QUEUE_TAIL;
    pub(crate) fn new(client: CsrClient) -> Self {
        Self(client)
    }
}

impl CsrReaderAdaptor for ToHostWorkRbCsrProxy {
    fn write_tail(&self, data: u32) -> Result<(), DeviceError> {
        self.0.write_csr(Self::TAIL_CSR, data)
    }
    fn read_head(&self) -> Result<u32, DeviceError> {
        self.0.read_csr(Self::HEAD_CSR)
    }
}
