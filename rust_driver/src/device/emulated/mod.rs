use log::debug;
use parking_lot::Mutex;

use crate::{utils::Buffer, AlignedMemory, SchedulerStrategy};

use self::rpc_cli::{
    RpcClient, ToCardCtrlRbCsrProxy, ToCardWorkRbCsrProxy, ToHostCtrlRbCsrProxy,
    ToHostWorkRbCsrProxy,
};
use super::{
    constants, ringbuf::Ringbuf, DeviceAdaptor, DeviceError, ToCardCtrlRbDesc, ToCardRb,
    ToCardWorkRbDesc, ToHostCtrlRbDesc, ToHostRb, ToHostWorkRbDesc, ToHostWorkRbDescError,
};
use std::{fmt::Debug, net::SocketAddr, sync::Arc};

use super::scheduler::DescriptorScheduler;

mod rpc_cli;

type ToCardCtrlRb = Ringbuf<
    ToCardCtrlRbCsrProxy,
    Buffer,
    { constants::RINGBUF_DEPTH },
    { constants::RINGBUF_ELEM_SIZE },
    { constants::RINGBUF_PAGE_SIZE },
>;

type ToHostCtrlRb = Ringbuf<
    ToHostCtrlRbCsrProxy,
    Buffer,
    { constants::RINGBUF_DEPTH },
    { constants::RINGBUF_ELEM_SIZE },
    { constants::RINGBUF_PAGE_SIZE },
>;

type ToCardWorkRb = Ringbuf<
    ToCardWorkRbCsrProxy,
    Buffer,
    { constants::RINGBUF_DEPTH },
    { constants::RINGBUF_ELEM_SIZE },
    { constants::RINGBUF_PAGE_SIZE },
>;

type ToHostWorkRb = Ringbuf<
    ToHostWorkRbCsrProxy,
    Buffer,
    { constants::RINGBUF_DEPTH },
    { constants::RINGBUF_ELEM_SIZE },
    { constants::RINGBUF_PAGE_SIZE },
>;

/// An emulated device implementation of the device.
#[derive(Debug)]
pub(crate) struct EmulatedDevice<Strat: SchedulerStrategy> {
    // FIXME: Temporarily ,we use Mutex to make the Rb imuumtable as well as thread safe
    to_card_ctrl_rb: Mutex<ToCardCtrlRb>,
    to_host_ctrl_rb: Mutex<ToHostCtrlRb>,
    to_host_work_rb: Mutex<ToHostWorkRb>,
    heap_mem_start_addr: usize,
    rpc_cli: RpcClient,
    scheduler: Arc<DescriptorScheduler<Strat>>,
}

impl<Strat: SchedulerStrategy> EmulatedDevice<Strat> {
    /// Initializing an emulated device.
    /// This function needs to be synchronized.
    ///
    /// Here we allow the `cast_possible_truncation` lint because before every transcation
    /// we used an "AND" mask to perform truncation.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn new(
        rpc_server_addr: SocketAddr,
        heap_mem_start_addr: usize,
        strategy: Strat,
        scheduler_size: u32,
    ) -> Result<Arc<Self>, DeviceError> {
        let rpc_cli =
            RpcClient::new(rpc_server_addr).map_err(|e| DeviceError::Device(e.to_string()))?;

        let to_card_ctrl_rb_buffer = Buffer::new(constants::RINGBUF_PAGE_SIZE, false)
            .map_err(|e| DeviceError::Device(e.to_string()))?;
        let to_card_ctrl_rb_addr = to_card_ctrl_rb_buffer.as_ref().as_ptr() as usize;
        let to_card_ctrl_rb = ToCardCtrlRb::new(
            ToCardCtrlRbCsrProxy::new(rpc_cli.clone()),
            to_card_ctrl_rb_buffer,
        );

        let to_host_ctrl_rb = Buffer::new(constants::RINGBUF_PAGE_SIZE, false)
            .map_err(|e| DeviceError::Device(e.to_string()))?;
        let to_host_ctrl_rb_addr = to_host_ctrl_rb.as_ref().as_ptr() as usize;
        let to_host_ctrl_rb =
            ToHostCtrlRb::new(ToHostCtrlRbCsrProxy::new(rpc_cli.clone()), to_host_ctrl_rb);

        let to_card_work_rb_buffer = Buffer::new(constants::RINGBUF_PAGE_SIZE, false)
            .map_err(|e| DeviceError::Device(e.to_string()))?;
        let to_card_work_rb_addr = to_card_work_rb_buffer.as_ref().as_ptr() as usize;
        let to_card_work_rb = ToCardWorkRb::new(
            ToCardWorkRbCsrProxy::new(rpc_cli.clone()),
            to_card_work_rb_buffer,
        );

        let to_host_work_rb_buffer = Buffer::new(constants::RINGBUF_PAGE_SIZE, false)
            .map_err(|e| DeviceError::Device(e.to_string()))?;
        let to_host_work_rb_addr = to_host_work_rb_buffer.as_ref().as_ptr() as usize;
        let to_host_work_rb = ToHostWorkRb::new(
            ToHostWorkRbCsrProxy::new(rpc_cli.clone()),
            to_host_work_rb_buffer,
        );

        let scheduler = Arc::new(DescriptorScheduler::new(
            strategy,
            Mutex::new(to_card_work_rb),
            None,
            scheduler_size,
        ));
        let dev = Arc::new(Self {
            to_card_ctrl_rb: Mutex::new(to_card_ctrl_rb),
            to_host_ctrl_rb: Mutex::new(to_host_ctrl_rb),
            to_host_work_rb: Mutex::new(to_host_work_rb),
            heap_mem_start_addr,
            rpc_cli,
            scheduler: Arc::<DescriptorScheduler<Strat>>::clone(&scheduler),
        });

        let pa_of_to_card_ctrl_rb_addr = dev.get_phys_addr(to_card_ctrl_rb_addr)?;
        dev.rpc_cli.write_csr(
            constants::CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW,
            (pa_of_to_card_ctrl_rb_addr & 0xFFFF_FFFF) as u32,
        )?;
        dev.rpc_cli.write_csr(
            constants::CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH,
            (pa_of_to_card_ctrl_rb_addr >> 32) as u32,
        )?;

        let pa_of_to_host_ctrl_rb_addr = dev.get_phys_addr(to_host_ctrl_rb_addr)?;
        dev.rpc_cli.write_csr(
            constants::CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW,
            (pa_of_to_host_ctrl_rb_addr & 0xFFFF_FFFF) as u32,
        )?;
        dev.rpc_cli.write_csr(
            constants::CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH,
            (pa_of_to_host_ctrl_rb_addr >> 32) as u32,
        )?;

        let pa_of_to_card_work_rb_addr = dev.get_phys_addr(to_card_work_rb_addr)?;
        dev.rpc_cli.write_csr(
            constants::CSR_ADDR_SEND_QUEUE_ADDR_LOW,
            (pa_of_to_card_work_rb_addr & 0xFFFF_FFFF) as u32,
        )?;
        dev.rpc_cli.write_csr(
            constants::CSR_ADDR_SEND_QUEUE_ADDR_HIGH,
            (pa_of_to_card_work_rb_addr >> 32) as u32,
        )?;

        let pa_of_to_host_work_rb_addr = dev.get_phys_addr(to_host_work_rb_addr)?;
        dev.rpc_cli.write_csr(
            constants::CSR_ADDR_META_REPORT_QUEUE_ADDR_LOW,
            (pa_of_to_host_work_rb_addr & 0xFFFF_FFFF) as u32,
        )?;
        dev.rpc_cli.write_csr(
            constants::CSR_ADDR_META_REPORT_QUEUE_ADDR_HIGH,
            (pa_of_to_host_work_rb_addr >> 32) as u32,
        )?;

        // let _: std::thread::JoinHandle<_> = spawn(move || {
        //     let rb = Mutex::new(to_card_work_rb);
        //     loop {
        //         match scheduler.pop_batch() {
        //             Ok((result, n)) => {
        //                 if n == 0{
        //                     continue;
        //                 }
        //                 if let Err(e) = push_to_card_work_rb_desc(&rb, result) {
        //                     error!("push to to_card_work_rb failed: {:?}", e);
        //                 }
        //             }
        //             Err(e) => {
        //                 error!("scheduler pop failed:{:?}", e);
        //             }
        //         }
        //     }
        // });

        Ok(dev)
    }
}

impl<Strat: SchedulerStrategy> DeviceAdaptor for Arc<EmulatedDevice<Strat>> {
    fn to_card_ctrl_rb(&self) -> Arc<dyn ToCardRb<ToCardCtrlRbDesc>> {
        Arc::<EmulatedDevice<Strat>>::clone(self)
    }

    fn to_host_ctrl_rb(&self) -> Arc<dyn ToHostRb<ToHostCtrlRbDesc>> {
        Arc::<EmulatedDevice<Strat>>::clone(self)
    }

    fn to_card_work_rb(&self) -> Arc<dyn ToCardRb<Box<ToCardWorkRbDesc>>> {
        Arc::<DescriptorScheduler<Strat>>::clone(&self.scheduler)
    }

    fn to_host_work_rb(&self) -> Arc<dyn ToHostRb<ToHostWorkRbDesc>> {
        Arc::<EmulatedDevice<Strat>>::clone(self)
    }

    fn read_csr(&self, addr: usize) -> Result<u32, DeviceError> {
        self.rpc_cli.read_csr(addr)
    }

    fn write_csr(&self, addr: usize, data: u32) -> Result<(), DeviceError> {
        self.rpc_cli.write_csr(addr, data)
    }

    fn get_phys_addr(&self, virt_addr: usize) -> Result<usize, DeviceError> {
        if virt_addr < self.heap_mem_start_addr {
            return Err(DeviceError::Device(format!(
                "virt_addr is less than heap_mem_start_addr: {} < {}",
                virt_addr, self.heap_mem_start_addr
            )));
        }
        // this will never do downflow
        #[allow(clippy::arithmetic_side_effects)]
        Ok(virt_addr - self.heap_mem_start_addr)
    }

    fn use_hugepage(&self) -> bool {
        false
    }
}

#[allow(clippy::unwrap_used, clippy::unwrap_in_result)]
impl<Strat: SchedulerStrategy> ToCardRb<ToCardCtrlRbDesc> for EmulatedDevice<Strat> {
    fn push(&self, desc: ToCardCtrlRbDesc) -> Result<(), DeviceError> {
        let mut guard = self.to_card_ctrl_rb.lock();
        let mut writer = guard.write();

        let mem = writer.next().unwrap(); // If block write fail, it should panic
        debug!("{:?}", &desc);
        desc.write(mem);

        Ok(())
    }
}

impl<Strat: SchedulerStrategy> ToHostRb<ToHostCtrlRbDesc> for EmulatedDevice<Strat> {
    fn pop(&self) -> Result<ToHostCtrlRbDesc, DeviceError> {
        let mut guard = self.to_host_ctrl_rb.lock();
        let mut reader = guard.read();
        let mem = reader.next()?;
        let desc = ToHostCtrlRbDesc::read(mem)?;
        debug!("{:?}", &desc);
        Ok(desc)
    }
}

// fn push_to_card_work_rb_desc(
//     rb: &Mutex<ToCardWorkRb>,
//     descs: [Option<SealedDesc>; POP_BATCH_SIZE],
// ) -> Result<(), DeviceError> {
//     let mut guard = rb.lock();
//     let mut writer = guard.write();
//     for desc in descs.into_iter().flatten() {
//         let desc = desc.into_desc();
//         debug!("driver send to card SQ: {:?}", &desc);

//         let desc_cnt = desc.serialized_desc_cnt();
//         desc.write_0(writer.next().ok_or(DeviceError::Overflow)?);
//         desc.write_1(writer.next().ok_or(DeviceError::Overflow)?);
//         desc.write_2(writer.next().ok_or(DeviceError::Overflow)?);

//         if desc_cnt == 4 {
//             desc.write_3(writer.next().ok_or(DeviceError::Overflow)?);
//         }
//     }
//     Ok(())
// }

impl<Strat: SchedulerStrategy> ToHostRb<ToHostWorkRbDesc> for EmulatedDevice<Strat> {
    fn pop(&self) -> Result<ToHostWorkRbDesc, DeviceError> {
        let mut guard = self.to_host_work_rb.lock();
        let mut reader = guard.read();

        let mem = reader.next()?;
        let mut read_res = ToHostWorkRbDesc::read(mem);

        loop {
            match read_res {
                Ok(desc) => break Ok(desc),
                Err(ToHostWorkRbDescError::DeviceError(e)) => {
                    return Err(e);
                }
                Err(ToHostWorkRbDescError::Incomplete(incomplete_desc)) => {
                    let next_mem = reader.next()?;
                    read_res = incomplete_desc.read(next_mem);
                }
            }
        }
    }
}
