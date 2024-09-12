use std::{
    mem::{size_of, MaybeUninit},
    net::{Ipv4Addr, SocketAddrV4},
    os::fd::AsRawFd,
    sync::{
        atomic::{AtomicBool, AtomicU16, Ordering},
        Arc,
    },
    thread,
};

use log::{error, info};
use socket2::{Domain, Protocol, Socket, Type};

use crate::device::software::{
    packet::{CommonPacketHeader, IpUdpHeaders, ICRC_SIZE},
    packet_processor::{is_icrc_valid, PacketProcessor, PacketWriter},
    types::{PayloadInfo, RdmaMessage},
};

use super::{NetAgentError, NetReceiveLogic, NetSendAgent};

pub(crate) const NET_SERVER_BUF_SIZE: usize = 8192;

/// A single thread udp server that listens to the corresponding port and calls the `recv` method of the receiver when a message is received.
#[derive(Debug)]
pub(crate) struct UDPReceiveAgent {
    listen_thread: Option<thread::JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

/// A udp client that sends messages to the corresponding address and port.
#[derive(Debug)]
pub(crate) struct UDPSendAgent {
    sender: Socket,
    sending_id_counter: AtomicU16,
    src_addr: Ipv4Addr,
    src_port: u16,
}

impl UDPSendAgent {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(crate) fn new(src_addr: Ipv4Addr, src_port: u16) -> Result<Self, NetAgentError> {
        let sender = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::UDP))?;
        let fd = sender.as_raw_fd();
        unsafe {
            let on = 1i32;
            let on_ref = std::ptr::addr_of!(on).cast::<libc::c_void>();
            let ret = libc::setsockopt(
                fd,
                libc::IPPROTO_IP,
                libc::IP_HDRINCL,
                on_ref,
                std::mem::size_of_val(&on) as u32, // size_of(int) is a u32 value
            );
            if ret != 0_i32 {
                return Err(NetAgentError::SetSockOptFailed(ret));
            }
        }

        // We can use the `rand` crate as well.

        let time_in_number = unsafe { libc::time(std::ptr::null_mut()) as u32 };
        unsafe {
            libc::srand(time_in_number);
        }
        let rand_val = unsafe { libc::rand() };
        // just truncation here, we don't care its exact value.
        let sending_id = AtomicU16::new(rand_val as u16);
        Ok(Self {
            sender,
            sending_id_counter: sending_id,
            src_addr,
            src_port,
        })
    }
}

impl UDPReceiveAgent {
    pub(crate) fn new(
        receiver: Arc<dyn for<'a> NetReceiveLogic<'a>>,
        addr: Ipv4Addr,
        port: u16,
    ) -> Result<Self, NetAgentError> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let thread_stop_flag = Arc::clone(&stop_flag);

        let socket = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::UDP))?;
        let addr = SocketAddrV4::new(addr, port);
        socket.bind(&addr.into())?;
        info!("UDP server started at {}:{}", addr.ip(), addr.port());
        let listen_thread = Some(thread::spawn(move || {
            let mut buf = [MaybeUninit::<u8>::uninit(); NET_SERVER_BUF_SIZE];
            while !thread_stop_flag.load(Ordering::Relaxed) {
                if let Ok((length, _src)) = socket.recv_from(&mut buf) {
                    #[allow(clippy::arithmetic_side_effects)]
                    if length < size_of::<CommonPacketHeader>() + 4 {
                        error!("Packet too short");
                        continue;
                    }
                    // SAFETY: `recv_from` ensures that the buffer is filled with `length` bytes.
                    let received_data = unsafe {
                        std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>(), length)
                    };

                    match is_icrc_valid(received_data) {
                        Ok(is_valid) =>{
                            if !is_valid {
                                error!("ICRC check failed {:?}", received_data);
                                continue;
                            }
                        }
                        Err(e) => {
                            error!("ICRC check failed {:?}", e);
                            continue;
                        }
                    }
                    // skip the ip header and udp header and the icrc
                    let offset = size_of::<IpUdpHeaders>();

                    #[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
                    // if we pass the CRC check, it should be ok
                    let received_data = &received_data[offset..length - ICRC_SIZE];
                    if let Ok(mut message) = PacketProcessor::to_rdma_message(received_data) {
                        receiver.recv(&mut message);
                    }
                }
            }
        }));
        Ok(Self {
            listen_thread,
            stop_flag,
        })
    }
}

impl Drop for UDPReceiveAgent {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        // if let Some(thread) =  self.listen_thread.take(){
        //     if let Err(e) = thread.join(){
        //         panic!("{}", format!("UDPReceiveAgent thread join failed: {e:?}"));
        //     }
        // }
    }
}

impl NetSendAgent for UDPSendAgent {
    fn send(
        &self,
        dest_addr: Ipv4Addr,
        dest_port: u16,
        message: &RdmaMessage,
    ) -> Result<(), NetAgentError> {
        let mut buf = [0u8; NET_SERVER_BUF_SIZE];
        let src_addr = self.src_addr;
        let src_port = self.src_port;
        let ip_id = self
            .sending_id_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let total_length = PacketWriter::new(&mut buf)
            .src_addr(src_addr)
            .src_port(src_port)
            .dest_addr(dest_addr)
            .dest_port(dest_port)
            .ip_id(ip_id)
            .message(message)
            .write()?;
        #[allow(clippy::indexing_slicing)]
        // We are sure that the total_length is less than the buffer size.
        let sended_size = self.sender.send_to(
            &buf[0..total_length],
            &SocketAddrV4::new(dest_addr, dest_port).into(),
        )?;
        if total_length != sended_size {
            return Err(NetAgentError::WrongBytesSending(total_length, sended_size));
        }
        Ok(())
    }

    fn send_raw(
        &self,
        dest_addr: Ipv4Addr,
        dest_port: u16,
        payload: &PayloadInfo,
    ) -> Result<(), NetAgentError> {
        let buf = payload
            .direct_data_ptr(true)
            .ok_or(NetAgentError::InvalidRdmaMessage(
                "PayloadInfo should have at least one item".to_owned(),
            ))?;
        let sended_size = self
            .sender
            .send_to(buf, &SocketAddrV4::new(dest_addr, dest_port).into())?;
        if buf.len() != sended_size {
            return Err(NetAgentError::WrongBytesSending(buf.len(), sended_size));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::device::software::{net_agent::NetReceiveLogic, types::RdmaMessage};
    #[derive(Debug)]
    struct DummyNetReceiveLogic {
        packets: Arc<Mutex<Vec<RdmaMessage>>>,
    }
    unsafe impl Sync for DummyNetReceiveLogic {}
    unsafe impl Send for DummyNetReceiveLogic {}

    impl NetReceiveLogic<'_> for DummyNetReceiveLogic {
        fn recv(&self, msg: &mut RdmaMessage) {
            let new_msg = msg.clone();
            self.packets.lock().unwrap().push(new_msg);
        }
    }
}
