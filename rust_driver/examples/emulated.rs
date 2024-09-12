use buddy_system_allocator::LockedHeap;

use eui48::MacAddress;
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
use std::{
    ffi::{c_void, CStr},
    net::Ipv4Addr,
    thread::sleep,
    time::Duration,
};

use crate::common::init_logging;

const ORDER: usize = 32;
const SHM_PATH: &str = "/bluesim1\0";

#[macro_use]
extern crate ctor;

/// Use `LockedHeap` as global allocator
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<ORDER> = LockedHeap::<ORDER>::new();
const HEAP_BLOCK_SIZE: usize = 1024 * 1024 * 64;
const BUFFER_LENGTH: usize = 1024 * 128;
const SEND_CNT: usize = 1024 * 6;
static mut HEAP_START_ADDR: usize = 0;

mod common;

#[ctor]
fn init_global_allocator() {
    unsafe {
        let pid = libc::fork();
        match pid.cmp(&0) {
            std::cmp::Ordering::Equal => {
                libc::chdir("../blue-rdma\0".as_bytes().as_ptr() as *const i8);
                let script = CStr::from_bytes_with_nul_unchecked("run_system_test.sh\0".as_bytes());

                let args = [script.as_ptr(), core::ptr::null()];

                libc::execvp(script.as_ptr(), args.as_ptr());
                std::process::exit(1);
            }
            std::cmp::Ordering::Greater => {
                let mut status = 0;
                libc::waitpid(pid, &mut status, 0);
            }
            _ => {
                panic!("fork failed");
            }
        }
    }
    unsafe {
        let shm_fd = libc::shm_open(
            SHM_PATH.as_ptr() as *const libc::c_char,
            libc::O_RDWR,
            0o600,
        );
        if shm_fd == -1{
            libc::exit(shm_fd);
        }
        assert!(shm_fd != -1, "shm_open failed");

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

fn create_and_init_card<'a>(
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
            1,
            Duration::from_secs(100),
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
        .peer_qpn(qpn)
        .qp_type(QpType::Rc)
        .rq_acc_flags(access_flag)
        .pmtu(Pmtu::Mtu4096)
        .dqp_ip(remote_network.ipaddr)
        .dqp_mac(remote_network.macaddr)
        .build()
        .unwrap();
    dev.create_qp(&qp).unwrap();
    info!("[{}] QP created", card_id);

    (dev, pd, mr, mr_buffer)
}
fn main() {
    init_logging("log.txt").unwrap();
    let qp_manager = QpManager::new();
    let qpn = qp_manager.alloc().unwrap();
    let a_network = RdmaDeviceNetworkParamBuilder::default()
        .gateway(Ipv4Addr::new(192, 168, 0, 0x1))
        .netmask(Ipv4Addr::new(255, 255, 255, 0))
        .ipaddr(Ipv4Addr::new(192, 168, 0, 2))
        .macaddr(MacAddress::new([0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xFE]))
        .build()
        .unwrap();
    let b_network = RdmaDeviceNetworkParamBuilder::default()
        .gateway(Ipv4Addr::new(192, 168, 0, 0x1))
        .netmask(Ipv4Addr::new(255, 255, 255, 0))
        .ipaddr(Ipv4Addr::new(192, 168, 0, 3))
        .macaddr(MacAddress::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]))
        .build()
        .unwrap();
    let (dev_a, _pd_a, mr_a, mut mr_buffer_a) =
        create_and_init_card(0, "0.0.0.0:9873", qpn, a_network, &b_network);
    let (_dev_b, _pd_b, mr_b, mut mr_buffer_b) =
        create_and_init_card(1, "0.0.0.0:9875", qpn, b_network, &a_network);

    let dpqn = qpn;
    for (idx, item) in mr_buffer_a.as_mut().iter_mut().enumerate() {
        *item = idx as u8;
    }
    for item in mr_buffer_b.as_mut()[0..].iter_mut() {
        *item = 0
    }

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
    sleep(Duration::from_secs(1));
    assert_eq!(
        mr_buffer_a.as_ref()[0..SEND_CNT],
        mr_buffer_b.as_ref()[0..SEND_CNT]
    );
    info!("write success");

    // // test read
    // for (idx, item) in mr_buffer_b.iter_mut().enumerate() {
    //     *item = idx as u8;
    // }
    // for item in mr_buffer_a[0..].iter_mut() {
    //     *item = 0;
    // }

    // let sge0 = Sge::new(
    //     &mr_buffer_a[0] as *const u8 as u64,
    //     SEND_CNT.try_into().unwrap(),
    //     mr_a.get_key(),
    // );
    // let ctx2 = dev_a
    //     .read(
    //         dpqn,
    //         &mr_buffer_b[0] as *const u8 as u64,
    //         mr_b.get_key(),
    //         WorkReqSendFlag::empty(),
    //         sge0,
    //     )
    //     .unwrap();
    // let _ = ctx2.wait();

    // assert_eq!(mr_buffer_a[0..SEND_CNT], mr_buffer_b[0..SEND_CNT]);
    // info!("read success");

    // for (idx, item) in mr_buffer_a.iter_mut().enumerate() {
    //     *item = idx as u8;
    // }
    // for item in mr_buffer_b[0..].iter_mut() {
    //     *item = 0
    // }

    // let sge3 = Sge::new(
    //     &mr_buffer_a[0] as *const u8 as u64,
    //     SEND_CNT.try_into().unwrap(),
    //     mr_a.get_key(),
    // );

    // let ctx3 = dev_a
    //     .write(
    //         dpqn,
    //         &mr_buffer_b[0] as *const u8 as u64,
    //         mr_b.get_key(),
    //         WorkReqSendFlag::empty(),
    //         sge3,
    //     )
    //     .unwrap();

    // let _ = ctx3.wait();
    // assert_eq!(mr_buffer_a[0..SEND_CNT], mr_buffer_b[0..SEND_CNT]);
    // info!("write without flag success");
}
