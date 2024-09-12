use std::{net::Ipv4Addr, time::Duration};

use buddy_system_allocator::LockedHeap;
use common::init_logging;
use eui48::MacAddress;
use libc::c_void;
use log::info;
use open_rdma_driver::{
    qp::QpManager,
    types::{
        MemAccessTypeFlag, Pmtu, QpBuilder, QpType, Qpn, RdmaDeviceNetworkParam,
        RdmaDeviceNetworkParamBuilder, Sge, WorkReqSendFlag, PAGE_SIZE,
    },
    AlignedMemory, Device, DeviceConfigBuilder, DeviceType, Mr, Pd, RetryConfig,
    RoundRobinStrategy,
};

mod common;

#[macro_use]
extern crate ctor;

const ORDER: usize = 32;
const SHM_PATH: &str = "/bluesim1\0";

/// Use `LockedHeap` as global allocator
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<ORDER> = LockedHeap::<ORDER>::new();
const HEAP_BLOCK_SIZE: usize = 1024 * 1024 * 64;
static mut HEAP_START_ADDR: usize = 0;
const BUFFER_LENGTH: usize = 1024 * 128;

#[ctor]
fn init_global_allocator() {
    unsafe {
        let shm_fd = libc::shm_open(
            SHM_PATH.as_ptr() as *const libc::c_char,
            libc::O_RDWR,
            0o600,
        );

        let heap = libc::mmap(
            0x7f7e8e600000 as *mut c_void,
            HEAP_BLOCK_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            shm_fd,
            0,
        );

        // let align_addr = (heap as usize + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        // let padding = align_addr - heap as usize;
        let addr = heap as usize;
        let size = HEAP_BLOCK_SIZE;
        HEAP_START_ADDR = addr;

        HEAP_ALLOCATOR.lock().init(addr, size);
    }
}

fn create_and_init_emulated_card<'a>(
    card_id: usize,
    mock_server_addr: &str,
    qpn: Qpn,
    local_network: RdmaDeviceNetworkParam,
    remote_network: &RdmaDeviceNetworkParam,
) -> (Device, Pd, Mr, AlignedMemory) {
    let head_start_addr = unsafe { HEAP_START_ADDR };
    let config = DeviceConfigBuilder::default()
        .network_config(local_network)
        .device_type(DeviceType::Emulated {
            rpc_server_addr: mock_server_addr.parse().unwrap(),
            heap_mem_start_addr: head_start_addr,
        })
        .strategy(RoundRobinStrategy::new())
        .retry_config(RetryConfig::new(
            false,
            100,
            Duration::from_secs(5),
            Duration::from_millis(10),
        ))
        .scheduler_size(1024 * 32)
        .build()
        .unwrap();
    let dev = Device::new(config).unwrap();
    info!("[{}] Device created", card_id);

    let pd = dev.alloc_pd().unwrap();
    info!("[{}] PD allocated", card_id);

    let mut mr_buffer = AlignedMemory::new(BUFFER_LENGTH).unwrap();

    unsafe {
        info!(
            "[{}] MR's PA_START={:X}",
            card_id,
            mr_buffer.as_mut().as_mut_ptr() as usize - HEAP_START_ADDR
        );
    }

    let access_flag = MemAccessTypeFlag::IbvAccessRemoteRead
        | MemAccessTypeFlag::IbvAccessRemoteWrite
        | MemAccessTypeFlag::IbvAccessLocalWrite;
    let mr = dev
        .reg_mr(
            pd,
            mr_buffer.as_mut().as_mut_ptr() as u64,
            mr_buffer.len() as u32,
            PAGE_SIZE as u32,
            access_flag,
        )
        .unwrap();
    info!("[{}] MR registered", card_id);
    let qp = QpBuilder::default()
        .pd(pd)
        .qpn(qpn)
        .qp_type(QpType::Rc)
        .rq_acc_flags(access_flag)
        .pmtu(Pmtu::Mtu4096)
        .dqp_ip(remote_network.ipaddr)
        .dqp_mac(remote_network.macaddr)
        .peer_qpn(qpn)
        .build()
        .unwrap();
    dev.create_qp(&qp).unwrap();
    info!("[{}] QP created", card_id);

    (dev, pd, mr, mr_buffer)
}

fn create_and_init_software_card<'a>(
    card_id: usize,
    qpn: Qpn,
    local_network: RdmaDeviceNetworkParam,
    remote_network: &RdmaDeviceNetworkParam,
) -> (Device, Pd, Mr, AlignedMemory) {
    let config = DeviceConfigBuilder::default()
        .network_config(local_network)
        .device_type(DeviceType::Software)
        .strategy(RoundRobinStrategy::new())
        .retry_config(RetryConfig::new(
            false,
            100,
            Duration::from_secs(5),
            Duration::from_millis(10),
        ))
        .scheduler_size(1024 * 32)
        .build()
        .unwrap();
    let dev = Device::new(config).unwrap();
    info!("[{}] Device created", card_id);

    let pd = dev.alloc_pd().unwrap();
    info!("[{}] PD allocated", card_id);

    let mut mr_buffer = AlignedMemory::new(BUFFER_LENGTH).unwrap();

    let access_flag = MemAccessTypeFlag::IbvAccessRemoteRead
        | MemAccessTypeFlag::IbvAccessRemoteWrite
        | MemAccessTypeFlag::IbvAccessLocalWrite;
    let mr = dev
        .reg_mr(
            pd,
            mr_buffer.as_mut().as_mut_ptr() as u64,
            mr_buffer.len() as u32,
            PAGE_SIZE as u32,
            access_flag,
        )
        .unwrap();
    info!("[{}] MR registered", card_id);
    let qp = QpBuilder::default()
        .pd(pd)
        .qpn(qpn)
        .qp_type(QpType::Rc)
        .rq_acc_flags(access_flag)
        .pmtu(Pmtu::Mtu4096)
        .dqp_ip(remote_network.ipaddr)
        .dqp_mac(remote_network.macaddr)
        .peer_qpn(qpn)
        .build()
        .unwrap();
    dev.create_qp(&qp).unwrap();
    info!("[{}] QP created", card_id);

    (dev, pd, mr, mr_buffer)
}

const SEND_CNT: usize = 1024 * 12;

fn main() {
    init_logging("log.txt").unwrap();

    let qp_manager = QpManager::new();
    let qpn = qp_manager.alloc().unwrap();

    let a_network = RdmaDeviceNetworkParamBuilder::default()
        .gateway(Ipv4Addr::new(127, 0, 0, 1))
        .netmask(Ipv4Addr::new(255, 255, 255, 0))
        .ipaddr(Ipv4Addr::new(127, 0, 0, 2))
        .macaddr(MacAddress::new([0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB]))
        .build()
        .unwrap();
    let b_network = RdmaDeviceNetworkParamBuilder::default()
        .gateway(Ipv4Addr::new(127, 0, 0, 1))
        .netmask(Ipv4Addr::new(255, 255, 255, 0))
        .ipaddr(Ipv4Addr::new(127, 0, 0, 3))
        .macaddr(MacAddress::new([0xCD, 0xCD, 0xCD, 0xCD, 0xCD, 0xCD]))
        .build()
        .unwrap();
    let (dev_a, _pd_a, mr_a, mut mr_buffer_a) =
        create_and_init_emulated_card(0, "0.0.0.0:9873", qpn, a_network, &b_network);
    let (dev_b, _pd_b, mr_b, mut mr_buffer_b) =
        create_and_init_software_card(1, qpn, b_network, &a_network);
    let dpqn = qpn;

    // emulator write to software
    {
        for (idx, item) in mr_buffer_a.as_mut().iter_mut().enumerate() {
            *item = idx as u8;
        }
        for item in mr_buffer_b.as_mut().iter_mut() {
            *item = 0
        }

        // emulator write to software
        let sge0 = Sge::new(
            mr_buffer_a.as_ref().as_ptr() as usize as u64,
            SEND_CNT.try_into().unwrap(),
            mr_a.get_key(),
        );
        let ctx1 = dev_a
            .write(
                dpqn,
                mr_buffer_b.as_ref().as_ptr() as usize as u64,
                mr_b.get_key(),
                WorkReqSendFlag::IbvSendSignaled,
                sge0,
            )
            .unwrap();

        let _ = ctx1.wait();
        assert_eq!(
            mr_buffer_a.as_ref()[0..SEND_CNT],
            mr_buffer_b.as_ref()[0..SEND_CNT]
        );
        info!("Emulator write to software success");
    }

    // emulator read from software
    {
        for item in mr_buffer_a.as_mut().iter_mut() {
            *item = 0
        }

        for (idx, item) in mr_buffer_b.as_mut().iter_mut().enumerate() {
            *item = idx as u8;
        }

        let sge0 = Sge::new(
            mr_buffer_a.as_ref().as_ptr() as usize as u64,
            SEND_CNT.try_into().unwrap(),
            mr_a.get_key(),
        );
        let ctx1 = dev_a
            .read(
                dpqn,
                mr_buffer_b.as_ref().as_ptr() as usize as u64,
                mr_b.get_key(),
                WorkReqSendFlag::IbvSendSignaled,
                sge0,
            )
            .unwrap();

        let _ = ctx1.wait();
        assert_eq!(
            mr_buffer_a.as_ref()[0..SEND_CNT],
            mr_buffer_b.as_ref()[0..SEND_CNT]
        );
        info!("Emulator read from software success");
    }

    // Software write to emulator
    {
        for (idx, item) in mr_buffer_b.as_mut().iter_mut().enumerate() {
            *item = idx as u8;
        }
        for item in mr_buffer_a.as_mut().iter_mut() {
            *item = 0
        }

        let sge0 = Sge::new(
            mr_buffer_b.as_ref().as_ptr() as usize as u64,
            SEND_CNT.try_into().unwrap(),
            mr_b.get_key(),
        );
        let ctx1 = dev_b
            .write(
                dpqn,
                mr_buffer_a.as_ref().as_ptr() as usize as u64,
                mr_a.get_key(),
                WorkReqSendFlag::IbvSendSignaled,
                sge0,
            )
            .unwrap();

        let _ = ctx1.wait();
        assert_eq!(
            mr_buffer_a.as_ref()[0..SEND_CNT],
            mr_buffer_b.as_ref()[0..SEND_CNT]
        );
        info!("Software write to emulator success");
    }

    // Software read from emulator
    {
        for (idx, item) in mr_buffer_a.as_mut().iter_mut().enumerate() {
            *item = idx as u8;
        }
        for item in mr_buffer_b.as_mut().iter_mut() {
            *item = 0
        }

        let sge0 = Sge::new(
            mr_buffer_b.as_ref().as_ptr() as usize as u64,
            SEND_CNT.try_into().unwrap(),
            mr_b.get_key(),
        );
        let ctx1 = dev_b
            .read(
                dpqn,
                mr_buffer_a.as_ref().as_ptr() as usize as u64,
                mr_a.get_key(),
                WorkReqSendFlag::IbvSendSignaled,
                sge0,
            )
            .unwrap();

        let _ = ctx1.wait();
        assert_eq!(
            mr_buffer_a.as_ref()[0..SEND_CNT],
            mr_buffer_b.as_ref()[0..SEND_CNT]
        );
        info!("Software read from emulator success");
    }
}
