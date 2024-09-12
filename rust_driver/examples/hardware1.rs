#![allow(warnings)] // FIXME: debug only
use eui48::MacAddress;
use log::{debug, info};
use open_rdma_driver::{
    qp::QpManager,
    types::{
        Key, MemAccessTypeFlag, Pmtu, QpBuilder, QpType, Qpn, RdmaDeviceNetworkParam,
        RdmaDeviceNetworkParamBuilder, Sge, WorkReqSendFlag, PAGE_SIZE,
    },
    Device, DeviceConfigBuilder, DeviceType, MmapMemory, Mr, Pd, RetryConfig, RoundRobinStrategy,
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    io::{self, BufRead},
    net::Ipv4Addr,
    thread::sleep,
    time::{Duration, Instant},
};

use crate::common::init_logging;

const BUFFER_LENGTH: usize = 1024 * 1024 * 4;
const SEND_CNT: usize = 1024 * 1024 * 4;
const PMTU: Pmtu = Pmtu::Mtu256;
const RAND_SEED: [u8; 32] = [
    0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef,
    0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef, 0xbe, 0xef,
];
mod common;

fn create_and_init_card<'a>(
    card_id: usize,
    qpn: Qpn,
    local_network: RdmaDeviceNetworkParam,
    remote_network: &RdmaDeviceNetworkParam,
) -> (Device, Pd, Mr, MmapMemory) {
    let config = DeviceConfigBuilder::default()
        .network_config(local_network)
        .device_type(DeviceType::Hardware {
            device_path: "/dev/infiniband/uverbs0".to_owned(),
        })
        .strategy(RoundRobinStrategy::new())
        .retry_config(RetryConfig::new(
            true,
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

    let mr_buffer = MmapMemory::new(BUFFER_LENGTH).unwrap();

    let access_flag = MemAccessTypeFlag::IbvAccessRemoteRead
        | MemAccessTypeFlag::IbvAccessRemoteWrite
        | MemAccessTypeFlag::IbvAccessLocalWrite;
    let mr = dev
        .reg_mr(
            pd,
            mr_buffer.as_ptr() as u64,
            mr_buffer.size() as u32,
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
        .pmtu(PMTU)
        .dqp_ip(remote_network.ipaddr)
        .dqp_mac(remote_network.macaddr)
        .peer_qpn(qpn)
        .build()
        .unwrap();
    dev.create_qp(&qp).unwrap();
    info!("[{}] QP created", card_id);

    (dev, pd, mr, mr_buffer)
}
fn main() {
    init_logging("log.txt").unwrap();

    let a_network = RdmaDeviceNetworkParamBuilder::default()
        .gateway(Ipv4Addr::new(127, 0, 0, 0x1))
        .netmask(Ipv4Addr::new(255, 0, 0, 0))
        .ipaddr(Ipv4Addr::new(127, 0, 0, 2))
        .macaddr(MacAddress::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFE]))
        .build()
        .unwrap();
    // let dev = Device::new_hardware(&a_network, "/dev/infiniband/uverbs0".to_owned()).unwrap();

    debug!("===========1====================");

    let b_network = RdmaDeviceNetworkParamBuilder::default()
        .gateway(Ipv4Addr::new(127, 0, 0, 0x1))
        .netmask(Ipv4Addr::new(255, 0, 0, 0))
        .ipaddr(Ipv4Addr::new(127, 0, 0, 3))
        .macaddr(MacAddress::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]))
        .build()
        .unwrap();
    debug!("===========2====================");

    let qp_manager = QpManager::new();
    debug!("===========3====================");
    let qpn = qp_manager.alloc().unwrap();
    debug!("===========4====================");
    let (dev_a, _pd_a, mr_a, mut mr_buffer_a) = create_and_init_card(0, qpn, a_network, &b_network);
    debug!("===========5====================");
    // let (_dev_b, _pd_b, mr_b, mut mr_buffer_b) =
    //     create_and_init_card(1, qpn, &b_network, &a_network);
    let dpqn = qpn;
    let mut rng = StdRng::from_seed(RAND_SEED);
    for item in mr_buffer_a.as_mut().iter_mut() {
        *item = rng.gen();
    }
    let sge0 = Sge::new(
        mr_buffer_a.as_ref().as_ptr() as usize as u64,
        SEND_CNT.try_into().unwrap(),
        mr_a.get_key(),
    );

    eprintln!("please input peer memory info:");
    let stdin = io::stdin();
    let mut peer_mem_info_str = String::new();
    let _ = stdin
        .lock()
        .read_line(&mut peer_mem_info_str)
        .expect("wait input error");
    let splited_params_strs = peer_mem_info_str.trim().split(",").collect::<Vec<_>>();
    let raddr = u64::from_str_radix(splited_params_strs[0], 16).unwrap();
    let rkey = Key::new(u32::from_str_radix(splited_params_strs[1], 16).unwrap());
    let pmtu = u32::from(&PMTU);

    // // test write
    // for i in 0..10 {
    let write_start = Instant::now();
    let ctx1 = dev_a
        .write(dpqn, raddr, rkey, WorkReqSendFlag::IbvSendSignaled, sge0)
        .unwrap();
    ctx1.wait_result().unwrap();
    // let _ = ctx1.wait();
    // log::info!(
    //     "{},{},{},{}",
    //     256,
    //     SEND_CNT / 1024 / 1024,
    //     pmtu,
    //     write_start.elapsed().as_micros()
    // );
    // }
}
