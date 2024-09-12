use std::{
    collections::HashMap,
    net::Ipv4Addr,
    sync::{atomic::AtomicBool, Arc},
    thread::{self, sleep, JoinHandle, Thread},
};

use crate::{
    buf::{PacketBuf, Slot, NIC_PACKET_BUFFER_SLOT_SIZE},
    device::{ToCardWorkRbDescBuilder, ToCardWorkRbDescCommon, ToCardWorkRbDescOpcode},
    types::QpType,
    Device as BlueRdmaDevice, WorkDescriptorSender,
};
use eui48::MacAddress;
use flume::{Receiver, Sender, TryRecvError};
use log::debug;
use parking_lot::Mutex;
use smoltcp::{
    iface::{Config, Interface, SocketSet},
    phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
    socket::{dhcpv4, icmp},
    time::Instant,
    wire::{EthernetAddress, Icmpv4Packet, Icmpv4Repr, IpAddress, IpCidr, Ipv4Cidr},
};

// the first 6 bytes of the ethernet frame is the destination mac address
const ETH_SRC_POS: std::ops::Range<usize> = 6..12;
const ETH_TYPE_START: usize = 12;
const ETH_TYPE_IP: u16 = 0x0800;
const IPV4_SRC_START: usize = 26;

pub(crate) struct NicRecvNotification {
    pub(crate) buf: Slot<NIC_PACKET_BUFFER_SLOT_SIZE>,
    pub(crate) len: u32,
}

unsafe impl Send for NicRecvNotification {}
unsafe impl Sync for NicRecvNotification {}

unsafe impl Send for BasicNicDeivce {}
unsafe impl Sync for BasicNicDeivce {}

#[derive(Debug)]
pub(crate) struct BasicNicDeivce {
    device: BlueRdmaDevice,
    tx_buf: PacketBuf<NIC_PACKET_BUFFER_SLOT_SIZE>,
    receiver: Receiver<NicRecvNotification>,
    neighbor_cache: Arc<Mutex<HashMap<Ipv4Addr, MacAddress>>>,
}

#[derive(Debug)]
pub(crate) struct NicInterface {
    icmp_queries_sender: Sender<(Ipv4Addr, Thread)>,
    neighbor_cache: Arc<Mutex<HashMap<Ipv4Addr, MacAddress>>>,
    stop_flag: Arc<AtomicBool>,
    handler: Option<JoinHandle<()>>,
    context : Option<NicWorkingContext>,
}

#[derive(Debug)]
struct NicWorkingContext{
    self_mac_addr: MacAddress,
    icmp_queries_receiver: Receiver<(Ipv4Addr, Thread)>,
    device : BasicNicDeivce,
}

impl NicInterface {
    pub(crate) fn new(
        device: BlueRdmaDevice,
        tx_buf: PacketBuf<NIC_PACKET_BUFFER_SLOT_SIZE>,
        receiver: Receiver<NicRecvNotification>,
        self_mac_addr: MacAddress,
    ) -> Self {
        let (icmp_queries_sender, icmp_queries_receiver) = flume::unbounded();
        let cache = Arc::new(Mutex::new(HashMap::new()));
        #[allow(clippy::clone_on_ref_ptr)]
        let device = BasicNicDeivce {
            device,
            tx_buf,
            receiver,
            neighbor_cache: cache.clone(),
        };
        let stop_flag = Arc::new(AtomicBool::new(false));
        let context = NicWorkingContext{
            self_mac_addr,
            icmp_queries_receiver,
            device,
        };
        NicInterface {
            icmp_queries_sender,
            neighbor_cache: cache,
            stop_flag,
            handler: None,
            context : Some(context),
        }
    }

    #[allow(clippy::unwrap_used)]
    pub(crate) fn start(&mut self){
        if let Some(mut context) = self.context.take(){
            let stop_flag_clone = Arc::<AtomicBool>::clone(&self.stop_flag);
            let handler = thread::spawn(move || {
                working_thread(
                    &stop_flag_clone,
                    &mut context
                );
            });
            self.handler = Some(handler);
        }
    }

    pub(crate) fn query_mac_addr(&self, ip: Ipv4Addr) -> Option<MacAddress> {
        let cache = self.neighbor_cache.lock();
        let mac = cache.get(&ip);
        if mac.is_some() {
            return mac.copied();
        }
        drop(cache);
        if self
            .icmp_queries_sender
            .send((ip, thread::current()))
            .is_err()
        {
            return None;
        }
        thread::park();
        self.neighbor_cache.lock().get(&ip).copied()
    }
}

impl Drop for NicInterface {
    fn drop(&mut self) {
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let handler = self.handler.take();
        if let Some(handler) = handler {
            if let Err(e) = handler.join() {
                panic!("{e:?}");
            }
        }
    }
}

impl Device for BasicNicDeivce {
    type RxToken<'a> = NicRxToken<'a> where Self: 'a;
    type TxToken<'a> = NicTxToken<'a> where Self: 'a;

    #[allow(clippy::indexing_slicing)]
    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        match self.receiver.try_recv() {
            Ok(mut notification) => {
                let buf = notification.buf.as_mut_slice();
                let len = notification.len as usize;
                return Some((
                    NicRxToken(&mut buf[..len], self), // the length is guaranteed to be less than the buffer size
                    NicTxToken(&self.device, &self.tx_buf),
                ));
            }
            Err(TryRecvError::Disconnected) => {
                panic!("The receiver is disconnected");
            }
            Err(TryRecvError::Empty) => None,
        }
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(NicTxToken(&self.device, &self.tx_buf))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = NIC_PACKET_BUFFER_SLOT_SIZE;
        caps.max_burst_size = None;
        caps.medium = Medium::Ethernet;
        caps
    }
}

pub(crate) struct NicRxToken<'a>(&'static mut [u8], &'a BasicNicDeivce);
pub(crate) struct NicTxToken<'a>(
    &'a BlueRdmaDevice,
    &'a PacketBuf<NIC_PACKET_BUFFER_SLOT_SIZE>,
);

impl RxToken for NicRxToken<'_> {
    #[allow(clippy::indexing_slicing, clippy::unwrap_used)]
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        // some hacks:
        // 1. First check if it's an Ethernet frame, and the upper layer is IP.
        // 2. we distract the src IP and src MAC, store them into our cache.
        log::info!("Received packet: {:?}", self.0);
        let type_ =
            u16::from(self.0[ETH_TYPE_START]) << 8_i32 | u16::from(self.0[ETH_TYPE_START + 1]);
        if type_ == ETH_TYPE_IP {
            let src_mac_addr = MacAddress::from_bytes(&self.0[ETH_SRC_POS]).unwrap();
            let src_ip_addr = Ipv4Addr::new(
                self.0[IPV4_SRC_START],
                self.0[IPV4_SRC_START + 1],
                self.0[IPV4_SRC_START + 2],
                self.0[IPV4_SRC_START + 3],
            );
            let _: &'_ mut MacAddress = self
                .1
                .neighbor_cache
                .lock()
                .entry(src_ip_addr)
                .and_modify(|e| *e = src_mac_addr)
                .or_insert(src_mac_addr);
        }
        f(self.0)
    }
}

impl TxToken for NicTxToken<'_> {
    #[allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::cast_possible_truncation,
        clippy::arithmetic_side_effects
    )]
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = self.1.recycle_buf();

        let ret = f(buf.as_mut_slice()[0..len].as_mut());

        log::debug!(
            "Sending packet to buffer: {:?}",
            &buf.as_mut_slice()[0..len]
        );
        let sge = buf.into_sge(len as u32);
        let total_len = 64.min(len);
        let common = ToCardWorkRbDescCommon {
            qp_type: QpType::RawPacket,
            total_len: total_len as u32,
            pmtu: crate::types::Pmtu::Mtu1024,
            ..Default::default()
        };
        // we don't miss any field, so it's impossible to panic
        // `WriteWithImm` with `Qptype == raw` means raw packet
        let desc = ToCardWorkRbDescBuilder::new(ToCardWorkRbDescOpcode::WriteWithImm)
            .with_common(common)
            .with_sge(sge)
            .build()
            .unwrap();
        if let Err(e) = self.0.send_work_desc(desc) {
            log::error!("Failed to send work desc: {:?}", e);
        }
        ret
    }
}

// TODO: we may separate the ICMP and DHCP into different functions
#[allow(
    clippy::similar_names,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::arithmetic_side_effects
)]
fn working_thread(
    stop_flag: &AtomicBool,
    context : &mut NicWorkingContext,
) {
    // Create interface
    let mut config = Config::new(EthernetAddress(context.self_mac_addr.to_array()).into());
    config.random_seed = rand::random();
    let mut iface = Interface::new(config, &mut context.device, Instant::now());

    // Create sockets
    let icmp_rx_buffer = icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY], vec![0; 256]);
    let icmp_tx_buffer = icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY], vec![0; 256]);
    let dhcp_socket = dhcpv4::Socket::new();
    let icmp_socket = icmp::Socket::new(icmp_rx_buffer, icmp_tx_buffer);

    let mut sockets = SocketSet::new(vec![]);
    let dhcp_handle = sockets.add(dhcp_socket);
    let icmp_handle = sockets.add(icmp_socket);
    let icmp_ident = 0x22b;
    let echo_payload = [0x0u8; 40];
    let mut icmp_query_map = HashMap::new();
    while !stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
        let timestamp = Instant::now();
        let _is_any_packet_proceed = iface.poll(timestamp, &mut context.device, &mut sockets);

        // handle ICMP packet here
        let socket = sockets.get_mut::<icmp::Socket>(icmp_handle);
        if !socket.is_open() {
            socket.bind(icmp::Endpoint::Ident(icmp_ident)).unwrap();
        }

        if socket.can_send() {
            match context.icmp_queries_receiver.try_recv() {
                Ok((addr, thread)) => {
                    log::info!("Querying mac address for {:?}", addr);
                    let addr: IpAddress = addr.into();
                    let timeout = timestamp + smoltcp::time::Duration::from_secs(5);
                    if icmp_query_map.insert(addr, (thread, timeout)).is_none() {
                        let icmp_repr = Icmpv4Repr::EchoRequest {
                            ident: icmp_ident,
                            seq_no: 0,
                            data: &echo_payload,
                        };
                        let icmp_payload = socket.send(icmp_repr.buffer_len(), addr).unwrap();
                        let mut icmp_packet = Icmpv4Packet::new_unchecked(icmp_payload);
                        icmp_repr.emit(&mut icmp_packet, &context.device.capabilities().checksum);
                        log::info!("sending ICMP\n");
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    log::error!("The nic worker thread receiver is disconnected");
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        if socket.can_recv() {
            let (payload, addr) = socket.recv().unwrap();
            let icmp_packet = Icmpv4Packet::new_checked(payload).unwrap();
            let icmp_repr =
                Icmpv4Repr::parse(&icmp_packet, &context.device.capabilities().checksum).unwrap();
            match icmp_repr {
                Icmpv4Repr::EchoRequest { .. } => {
                    if socket.can_send() {
                        let icmp_reply_repr = Icmpv4Repr::EchoRequest {
                            ident: icmp_ident,
                            seq_no: 0,
                            data: &echo_payload,
                        };
                        let icmp_payload = socket.send(icmp_reply_repr.buffer_len(), addr).unwrap();
                        let mut icmp_reply_packet = Icmpv4Packet::new_unchecked(icmp_payload);
                        icmp_reply_repr
                            .emit(&mut icmp_reply_packet, &context.device.capabilities().checksum);
                    }
                }
                Icmpv4Repr::EchoReply { .. }
                | Icmpv4Repr::DstUnreachable { .. }
                | Icmpv4Repr::TimeExceeded { .. } => {}
                _ => unreachable!(),
            }
            // If we receive the message, the we must have sent the message, and got its mac address(suppose we are in LAN).
            if let Some((thread, _timeout)) = icmp_query_map.get(&addr) {
                thread.unpark();
            }
        }

        // // handle DHCP packet here
        let event = sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).poll();
        match event {
            None => {}
            Some(dhcpv4::Event::Configured(dhcp_config)) => {
                debug!("DHCP config acquired!");
                debug!("IP address:      {}", dhcp_config.address);
                set_ipv4_addr(&mut iface, dhcp_config.address);

                if let Some(router) = dhcp_config.router {
                    debug!("Default gateway: {}", router);
                    // unless OOM, this should never fail
                    let _route: Option<smoltcp::iface::Route> =
                        iface.routes_mut().add_default_ipv4_route(router).unwrap();
                } else {
                    debug!("Default gateway: None");
                    let _route: Option<smoltcp::iface::Route> =
                        iface.routes_mut().remove_default_ipv4_route();
                }
            }
            Some(dhcpv4::Event::Deconfigured) => {
                debug!("DHCP lost config!");
                #[allow(clippy::redundant_closure_for_method_calls)]
                iface.update_ip_addrs(|addrs| addrs.clear());
                let _route: Option<smoltcp::iface::Route> =
                    iface.routes_mut().remove_default_ipv4_route();
            }
        }

        // iterate ICMP map
        icmp_query_map.retain(|_addr, (thread, timeout)| {
            if timestamp >= *timeout {
                thread.unpark();
                false
            } else {
                true
            }
        });
        sleep(std::time::Duration::from_millis(1));
    }
}

#[allow(clippy::unwrap_used)]
fn set_ipv4_addr(iface: &mut Interface, cidr: Ipv4Cidr) {
    iface.update_ip_addrs(|addrs| {
        addrs.clear();
        // unless OOM, this should never fail
        addrs.push(IpCidr::Ipv4(cidr)).unwrap();
    });
}
