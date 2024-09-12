use common::init_logging;
use eui48::MacAddress;
use log::info;
use open_rdma_driver::{
    qp::QpManager, types::{
        MemAccessTypeFlag, Pmtu, QpBuilder, QpType, Qpn, RdmaDeviceNetworkParam,
        RdmaDeviceNetworkParamBuilder, Sge, WorkReqSendFlag, PAGE_SIZE,
    }, AlignedMemory, Device, DeviceConfigBuilder, DeviceType, Mr, Pd, RetryConfig, RoundRobinStrategy
};
use serial_test::serial;
use std::{net::Ipv4Addr, time::Duration};

const BUFFER_LENGTH: usize = 1024 * 128;
const SEND_CNT: usize = 1024 * 64;

mod common;

fn create_and_init_card(
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
        .pmtu(Pmtu::Mtu1024)
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
    let b_network = RdmaDeviceNetworkParamBuilder::default()
        .gateway(Ipv4Addr::new(127, 0, 0, 0x1))
        .netmask(Ipv4Addr::new(255, 0, 0, 0))
        .ipaddr(Ipv4Addr::new(127, 0, 0, 3))
        .macaddr(MacAddress::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]))
        .build()
        .unwrap();
    let qp_manager = QpManager::new();
    let qpn = qp_manager.alloc().unwrap();
    let (dev_a, _pd_a, mr_a, mut mr_buffer_a) = create_and_init_card(0, qpn, a_network, &b_network);
    let (_dev_b, _pd_b, mr_b, mut mr_buffer_b) =
        create_and_init_card(1, qpn, b_network, &a_network);
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

    let sge1 = Sge::new(
        &mr_buffer_a.as_ref()[SEND_CNT] as *const u8 as usize as u64,
        SEND_CNT.try_into().unwrap(),
        mr_a.get_key(),
    );

    // test write
    let ctx1 = dev_a
        .write(
            dpqn,
            mr_buffer_b.as_ref().as_ptr() as usize as u64,
            mr_b.get_key(),
            WorkReqSendFlag::empty(),
            sge0,
        )
        .unwrap();
    let ctx2 = dev_a
        .write(
            dpqn,
            &mr_buffer_b.as_ref()[SEND_CNT] as *const u8 as usize as u64,
            mr_b.get_key(),
            WorkReqSendFlag::empty(),
            sge1,
        )
        .unwrap();

    let _ = ctx1.wait();
    let _ = ctx2.wait();

    if mr_buffer_a.as_ref()[0..SEND_CNT * 2] != mr_buffer_b.as_ref()[0..SEND_CNT * 2] {
        for i in 0..SEND_CNT * 2 {
            if mr_buffer_a.as_ref()[i] != mr_buffer_b.as_ref()[i] {
                panic!(
                    "{}: {} != {}",
                    i,
                    mr_buffer_a.as_ref()[i],
                    mr_buffer_b.as_ref()[i]
                );
            }
        }
    }
    info!("Write succ");
    // test read

    for item in mr_buffer_a.as_mut().iter_mut() {
        *item = 0;
    }
    for (idx, item) in mr_buffer_a.as_mut().iter_mut().enumerate() {
        *item = idx as u8;
    }

    let sge_read = Sge::new(
        mr_buffer_a.as_ref().as_ptr() as usize as u64,
        SEND_CNT.try_into().unwrap(),
        mr_a.get_key(),
    );

    let ctx1 = dev_a
        .read(
            dpqn,
            mr_buffer_b.as_ref().as_ptr() as usize as u64,
            mr_b.get_key(),
            WorkReqSendFlag::empty(),
            sge_read,
        )
        .unwrap();
    let _ = ctx1.wait();
    info!("Read succ");
}
