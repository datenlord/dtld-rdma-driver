use std::net::Ipv4Addr;

use thiserror::Error;

use super::{packet::PacketError, types::{RdmaMessage, PayloadInfo}, packet_processor::PacketProcessorError};
use std::io;

pub mod udp_agent;

pub trait NetReceiveLogic<'a>: Send + Sync {
    fn recv(&self, message: &mut RdmaMessage);
}

pub trait NetSendAgent {
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
        payload : &PayloadInfo
    ) -> Result<(), NetAgentError>;
}

#[derive(Error, Debug)]
#[allow(clippy::module_name_repetitions)]
pub enum NetAgentError {
    #[error("packet process error")]
    Packet(#[from] PacketError),
    #[error("io error")]
    Io(#[from] io::Error),
    #[error("packet process error")]
    PacketProcess(#[from] PacketProcessorError),
    #[error("setsockopt failed, errno: {0}")]
    SetSockOptFailed(i32),
    #[error("Expected {0} bytes, but sended {1} bytes")]
    WrongBytesSending(usize,usize),
}
