use std::{
    error::Error,
    net::Ipv4Addr,
    sync::{
        atomic::AtomicBool,
        Arc,
    },
};

use flume::{unbounded, Receiver};
use log::debug;

use crate::SchedulerStrategy;

use self::net_agent::udp_agent::{UDPReceiveAgent, UDPSendAgent};

use super::{
    scheduler::DescriptorScheduler, DeviceAdaptor, DeviceError, ToCardCtrlRbDesc, ToCardRb,
    ToCardWorkRbDesc, ToHostCtrlRbDesc, ToHostRb, ToHostWorkRbDesc,
};

mod logic;
mod net_agent;
mod packet;
mod packet_processor;
#[cfg(test)]
pub(crate) mod tests;
mod types;

pub(crate) use logic::BlueRDMALogic;

/// An software device implementation of the device.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct SoftwareDevice<Strat: SchedulerStrategy> {
    recv_agent: UDPReceiveAgent,
    device: Arc<BlueRDMALogic>,
    stop_flag: Arc<AtomicBool>,
    to_card_work_rb: ToCardWorkRb<Strat>,
    to_host_work_rb: ToHostWorkRb,
    to_host_ctrl_rb: ToHostCtrlRb,
}

#[derive(Debug, Clone)]
struct ToCardWorkRb<Strat: SchedulerStrategy>(Arc<DescriptorScheduler<Strat>>);

#[derive(Debug, Clone)]
struct ToHostWorkRb(Receiver<ToHostWorkRbDesc>);

#[derive(Debug, Clone)]
struct ToHostCtrlRb(Receiver<ToHostCtrlRbDesc>);

impl<Strat: SchedulerStrategy> SoftwareDevice<Strat> {
    /// Initializing an software device.
    pub(crate) fn new(addr: Ipv4Addr, port: u16, strategy: Strat,scheduler_size:u32) -> Result<Self, Box<dyn Error>> {
        let send_agent = UDPSendAgent::new(addr, port)?;
        let (ctrl_sender, ctrl_receiver) = unbounded();
        let (work_sender, work_receiver) = unbounded();
        let device = Arc::new(BlueRDMALogic::new(
            Arc::new(send_agent),
            ctrl_sender,
            work_sender,
        ));
        let recv_agent = UDPReceiveAgent::new(Arc::<BlueRDMALogic>::clone(&device), addr, port)?;

        let this_device = Arc::<BlueRDMALogic>::clone(&device);

        let stop_flag = Arc::new(AtomicBool::new(false));
        let scheduler = Arc::new(DescriptorScheduler::new_with_software(
            strategy,
            this_device,
            scheduler_size
        ));
        let to_card_work_rb = ToCardWorkRb(scheduler);
        Ok(Self {
            recv_agent,
            device,
            to_card_work_rb,
            to_host_work_rb: ToHostWorkRb(work_receiver),
            to_host_ctrl_rb: ToHostCtrlRb(ctrl_receiver),
            stop_flag,
        })
    }
}

impl<Strat: SchedulerStrategy> DeviceAdaptor for SoftwareDevice<Strat> {
    fn to_card_ctrl_rb(&self) -> Arc<dyn ToCardRb<ToCardCtrlRbDesc>> {
        Arc::<BlueRDMALogic>::clone(&self.device)
    }

    fn to_host_ctrl_rb(&self) -> Arc<dyn ToHostRb<ToHostCtrlRbDesc>> {
        Arc::new(self.to_host_ctrl_rb.clone())
    }

    fn to_card_work_rb(&self) -> Arc<dyn ToCardRb<Box<ToCardWorkRbDesc>>> {
        Arc::new(self.to_card_work_rb.clone())
    }

    fn to_host_work_rb(&self) -> Arc<dyn ToHostRb<ToHostWorkRbDesc>> {
        Arc::new(self.to_host_work_rb.clone())
    }

    fn read_csr(&self, _addr: usize) -> Result<u32, DeviceError> {
        todo!()
    }

    fn write_csr(&self, _addr: usize, _data: u32) -> Result<(), DeviceError> {
        todo!()
    }

    fn get_phys_addr(&self, virt_addr: usize) -> Result<usize, DeviceError> {
        Ok(virt_addr)
    }

    fn use_hugepage(&self) -> bool {
        false
    }
}

impl ToCardRb<ToCardCtrlRbDesc> for BlueRDMALogic {
    fn push(&self, desc: ToCardCtrlRbDesc) -> Result<(), DeviceError> {
        self.update(desc)
            .map_err(|e| DeviceError::Device(e.to_string()))?;
        Ok(())
    }
}

impl ToHostRb<ToHostCtrlRbDesc> for ToHostCtrlRb {
    fn pop(&self) -> Result<ToHostCtrlRbDesc, DeviceError> {
        self.0
            .recv()
            .map_err(|e| DeviceError::Device(e.to_string()))
    }
}

impl ToHostRb<ToHostWorkRbDesc> for ToHostWorkRb {
    fn pop(&self) -> Result<ToHostWorkRbDesc, DeviceError> {
        self.0
            .recv()
            .map_err(|e| DeviceError::Device(e.to_string()))
    }
}

impl<Strat: SchedulerStrategy> ToCardRb<Box<ToCardWorkRbDesc>> for ToCardWorkRb<Strat> {
    fn push(&self, desc: Box<ToCardWorkRbDesc>) -> Result<(), DeviceError> {
        debug!("driver to card SQ: {:?}", desc);
        self.0.push(desc)
    }
}
