use serde::{Deserialize, Serialize};
use std::{
    io::Error as IoError,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
};

use crate::device::{
    constants::{
        CSR_ADDR_CMD_REQ_QUEUE_HEAD, CSR_ADDR_CMD_REQ_QUEUE_TAIL, CSR_ADDR_CMD_RESP_QUEUE_HEAD,
        CSR_ADDR_CMD_RESP_QUEUE_TAIL, CSR_ADDR_META_REPORT_QUEUE_HEAD,
        CSR_ADDR_META_REPORT_QUEUE_TAIL, CSR_ADDR_SEND_QUEUE_HEAD, CSR_ADDR_SEND_QUEUE_TAIL,
    },
    ringbuf::{CsrReaderAdaptor, CsrWriterAdaptor},
    DeviceError,
};

#[derive(Debug, Clone)]
pub(super) struct RpcClient(Arc<UdpSocket>);

#[derive(Serialize, Deserialize)]
struct CsrAccessRpcMessage {
    is_write: bool,
    addr: usize,
    value: u32,
}

impl RpcClient {
    pub(super) fn new(server_addr: SocketAddr) -> Result<Self, IoError> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect(server_addr)?;
        Ok(Self(socket.into()))
    }

    pub(super) fn read_csr(&self, addr: usize) -> Result<u32, DeviceError> {
        let msg = CsrAccessRpcMessage {
            is_write: false,
            addr,
            value: 0,
        };

        let send_buf = serde_json::to_vec(&msg)?;
        let _: usize = self.0.send(&send_buf)?;

        let mut recv_buf = [0; 128];
        let recv_cnt = self.0.recv(&mut recv_buf)?;
        // the length of CsrAccessRpcMessage is fixed,
        #[allow(clippy::indexing_slicing)]
        let response = serde_json::from_slice::<CsrAccessRpcMessage>(&recv_buf[..recv_cnt])?;

        Ok(response.value)
    }

    pub(super) fn write_csr(&self, addr: usize, data: u32) -> Result<(), DeviceError> {
        let msg = CsrAccessRpcMessage {
            is_write: true,
            addr,
            value: data,
        };

        let send_buf = serde_json::to_vec(&msg).map_err(|e| DeviceError::Device(e.to_string()))?;
        let _: usize = self
            .0
            .send(&send_buf)
            .map_err(|e| DeviceError::Device(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct ToCardCtrlRbCsrProxy(RpcClient);
impl ToCardCtrlRbCsrProxy {
    const HEAD_CSR: usize = CSR_ADDR_CMD_REQ_QUEUE_HEAD;
    const TAIL_CSR: usize = CSR_ADDR_CMD_REQ_QUEUE_TAIL;
    pub(crate) fn new(client: RpcClient) -> Self {
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
pub(crate) struct ToHostCtrlRbCsrProxy(RpcClient);

impl ToHostCtrlRbCsrProxy {
    const HEAD_CSR: usize = CSR_ADDR_CMD_RESP_QUEUE_HEAD;
    const TAIL_CSR: usize = CSR_ADDR_CMD_RESP_QUEUE_TAIL;
    pub(crate) fn new(client: RpcClient) -> Self {
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
pub(crate) struct ToCardWorkRbCsrProxy(RpcClient);

impl ToCardWorkRbCsrProxy {
    const HEAD_CSR: usize = CSR_ADDR_SEND_QUEUE_HEAD;
    const TAIL_CSR: usize = CSR_ADDR_SEND_QUEUE_TAIL;
    pub(crate) fn new(client: RpcClient) -> Self {
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
pub(crate) struct ToHostWorkRbCsrProxy(RpcClient);

impl ToHostWorkRbCsrProxy {
    const HEAD_CSR: usize = CSR_ADDR_META_REPORT_QUEUE_HEAD;
    const TAIL_CSR: usize = CSR_ADDR_META_REPORT_QUEUE_TAIL;
    pub(crate) fn new(client: RpcClient) -> Self {
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

// 为DeviceError实现From<std::io::Error>
impl From<std::io::Error> for DeviceError {
    fn from(err: std::io::Error) -> Self {
        DeviceError::Device(err.to_string())
    }
}

impl From<serde_json::Error> for DeviceError {
    fn from(err: serde_json::Error) -> Self {
        DeviceError::Device(err.to_string())
    }
}
