use std::{fmt::Debug, net::Ipv4Addr};

use thiserror::Error;

use super::{
    packet::PacketError,
    packet_processor::PacketProcessorError,
    types::{PayloadInfo, RdmaMessage},
};
use std::io;

pub(crate) mod udp_agent;

pub(crate) trait NetReceiveLogic<'a>: Send + Sync + Debug {
    fn recv(&self, message: &mut RdmaMessage);
}

pub(crate) trait NetSendAgent: Debug {
    fn send(
        &self,
        dest_addr: Ipv4Addr,
        dest_port: u16,
        message: &RdmaMessage,
    ) -> Result<(), NetAgentError>;

    fn send_raw(
        &self,
        dest_addr: Ipv4Addr,
        dest_port: u16,
        payload: &PayloadInfo,
    ) -> Result<(), NetAgentError>;
}

#[derive(Error, Debug)]
#[allow(clippy::module_name_repetitions)]
pub(crate) enum NetAgentError {
    #[error("packet process error")]
    Packet(#[from] PacketError),
    #[error("io error")]
    Io(#[from] io::Error),
    #[error("packet process error")]
    PacketProcess(#[from] PacketProcessorError),
    #[error("setsockopt failed, errno: {0}")]
    SetSockOptFailed(i32),
    #[error("Expected {0} bytes, but sended {1} bytes")]
    WrongBytesSending(usize, usize),
    #[error("Invalid RDMA message :{0}")]
    InvalidRdmaMessage(String),
}
