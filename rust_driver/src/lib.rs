//! open-rdma-driver
#![deny(
    // The following are allowed by default lints according to
    // https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html
    absolute_paths_not_starting_with_crate,
    explicit_outlives_requirements,
    // elided_lifetimes_in_paths,  // allow anonymous lifetime
    explicit_outlives_requirements,
    keyword_idents,
    macro_use_extern_crate,
    meta_variable_misuse,
    missing_abi,
    missing_copy_implementations,
    missing_debug_implementations, 
    // must_not_suspend, unstable
    missing_docs,
    non_ascii_idents,
    // non_exhaustive_omitted_patterns, unstable
    noop_method_call,
    pointer_structural_match,
    rust_2021_incompatible_closure_captures,
    rust_2021_incompatible_or_patterns,
    rust_2021_prefixes_incompatible_syntax,
    rust_2021_prelude_collisions,
    single_use_lifetimes,
    // trivial_casts, // We allow trivial_casts for casting a pointer
    trivial_numeric_casts,
    unreachable_pub,
    // unsafe_code, // we need unsafe when managing memory
    unsafe_op_in_unsafe_fn,
    unstable_features,
    unused_extern_crates,
    unused_import_braces,
    unused_lifetimes,
    unused_qualifications,
    unused_results,
    variant_size_differences, 

    clippy::all,
    // clippy::pedantic,
    clippy::cargo,

    // The followings are selected restriction lints for rust 1.57
    // clippy::as_conversions, // we allow lossless "as" conversion, it has checked by clippy::cast_possible_truncation
    clippy::clone_on_ref_ptr,
    clippy::create_dir,
    clippy::dbg_macro,
    clippy::decimal_literal_representation,
    clippy::default_numeric_fallback,
    clippy::disallowed_script_idents,
    clippy::else_if_without_else,
    clippy::exhaustive_enums,
    clippy::exhaustive_structs,
    clippy::exit,
    clippy::expect_used,
    clippy::filetype_is_file,
    clippy::float_arithmetic,
    clippy::float_cmp_const,
    clippy::get_unwrap,
    clippy::if_then_some_else_none,
    // clippy::implicit_return, it's idiomatic Rust code.
    clippy::indexing_slicing,
    clippy::inline_asm_x86_intel_syntax,
    clippy::arithmetic_side_effects,
 
    // clippy::pattern_type_mismatch, // cause some false postive and unneeded copy
    // clippy::print_stderr,
    clippy::print_stdout,
    clippy::rc_buffer,
    clippy::rc_mutex,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::same_name_method,
    clippy::self_named_module_files,
    // clippy::shadow_reuse, it’s a common pattern in Rust code
    // clippy::shadow_same, it’s a common pattern in Rust code
    clippy::shadow_unrelated,
    clippy::str_to_string,
    clippy::string_add,
    clippy::string_to_string,
    // clippy::todo, // TODO: We still have some unclear todos
    // clippy::unimplemented, // TODO: We still have some unimplemented functions
    clippy::unnecessary_self_imports,
    clippy::unneeded_field_pattern,
    // clippy::unreachable, // the unreachable code should unreachable otherwise it's a bug
    clippy::unwrap_in_result,
    clippy::unwrap_used, 
    clippy::use_debug,
    clippy::verbose_file_reads,
    clippy::wildcard_enum_match_arm,

    // // The followings are selected lints from 1.61.0 to 1.67.1
    clippy::as_ptr_cast_mut,
    clippy::derive_partial_eq_without_eq,
    clippy::empty_drop,
    clippy::empty_structs_with_brackets,
    clippy::format_push_string,
    clippy::iter_on_empty_collections,
    clippy::iter_on_single_items,
    clippy::large_include_file,
    clippy::manual_clamp,
    clippy::suspicious_xor_used_as_pow,
    clippy::unnecessary_safety_comment,
    clippy::unnecessary_safety_doc,
    clippy::unused_peekable,
    clippy::unused_rounding,    

    // The followings are selected restriction lints from rust 1.68.0 to 1.71.0
    // clippy::allow_attributes, still unstable
    clippy::impl_trait_in_params,
    clippy::let_underscore_untyped,
    clippy::missing_assert_message,
    clippy::multiple_unsafe_ops_per_block,
    clippy::semicolon_inside_block,
    // clippy::semicolon_outside_block, already used `semicolon_inside_block`
    clippy::tests_outside_test_module,
    // 1.71.0
    clippy::default_constructed_unit_structs,
    clippy::items_after_test_module,
    clippy::manual_next_back,
    clippy::manual_while_let_some,
    clippy::needless_bool_assign,
    clippy::non_minimal_cfg,
)]

#![cfg_attr(
    test,
    allow(
        clippy::indexing_slicing,
        unused_results,
        clippy::unwrap_used,
        clippy::unwrap_in_result,
        clippy::expect_used,
        clippy::as_conversions,
        clippy::shadow_unrelated,
        clippy::arithmetic_side_effects,
        clippy::let_underscore_untyped,
        clippy::pedantic, 
        clippy::default_numeric_fallback,
        clippy::print_stderr,
    )
)]
use crate::{
    device::{
        DeviceAdaptor, EmulatedDevice, HardwareDevice, SoftwareDevice, ToCardCtrlRbDesc,
        ToCardWorkRbDescCommon,
    },
    mr::{MrCtx, MrPgt,ACKNOWLEDGE_BUFFER_SIZE,NIC_BUFFER_SIZE},
    pd::PdCtx,
};
use buf::{PacketBuf,NIC_PACKET_BUFFER_SLOT_SIZE};
use core_affinity::CoreId;
use derive_builder::Builder;
use device::{
    ToCardCtrlRbDescCommon, ToCardCtrlRbDescSetNetworkParam, ToCardCtrlRbDescSetRawPacketReceiveMeta, ToCardWorkRbDesc, ToCardWorkRbDescBuilder, ToCardWorkRbDescOpcode
};
use eui48::MacAddress;
use flume::unbounded;
use nic::NicInterface;
use op_ctx::{CtrlOpCtx, OpCtx};
use checker::{PacketChecker, PacketCheckerContext, RecvContextMap};
use ctrl_poller::{ControlPoller, ControlPollerContext};
use work_poller::{WorkDescPoller, WorkDescPollerContext};
use qp::QpContext;
use retry::{RetryMap, RetryMonitor, RetryMonitorContext, RetryRecord};
use std::{
    collections::HashMap, fmt::Debug, net::{Ipv4Addr, SocketAddr}, sync::{
        atomic::{AtomicU32, Ordering},
        Arc, OnceLock,
    }
};
use thiserror::Error;
use types::{Key, Msn, Psn, Qpn, RdmaDeviceNetworkParam, Sge, WorkReqSendFlag};
use utils::{calculate_packet_cnt, Buffer};
use parking_lot::{Mutex,RwLock};

/// memory region
pub mod mr;
/// op context for user to track the status of the write/read/control operation
pub mod op_ctx;
/// protection domain
pub mod pd;
/// queue pair related structs and functions
pub mod qp;
/// types exported to user
pub mod types;

/// adaptor device: hardware, software, emulated
mod device;
/// pakcet check thread: checking if the packet is received correctly
mod checker;
/// ctrl poll thread: polling the ctrl descriptor
mod ctrl_poller;
/// work poll thread: polling the work descriptor
mod work_poller;
/// responser thread: sending the response(read resp or ack) to the device
mod responser;
/// A simple buffer allocator
mod buf;
/// basic nic functions
mod nic;
/// retry monitor
mod retry;
/// utility functions
mod utils;

/// unit test
#[cfg(test)]
mod tests;

pub use crate::{mr::Mr, pd::Pd};
pub use device::scheduler::{SchedulerStrategy,SealedDesc,POP_BATCH_SIZE,BatchDescs};
pub use device::scheduler::{round_robin::RoundRobinStrategy,testing::{TestingStrategy,TestingHandler}};
pub use types::Error;
pub use retry::RetryConfig;
pub use utils::{MmapMemory,AlignedMemory};

const MR_KEY_IDX_BIT_CNT: usize = 8;
const MR_TABLE_SIZE: usize = 64;
const MR_PGT_LENGTH: usize = 1024;
const MR_PGT_ENTRY_SIZE: usize = 8;
const DEFAULT_RMDA_PORT : u16 = 4791;

type ThreadSafeHashmap<K,V> = Arc<RwLock<HashMap<K,V>>>;

/// A user space RDMA device.
/// 
/// The device provides a general interface, like `write`, `read`, `register_mr/qp/pd`, etc.
/// 
/// The device has an adaptor, which can be hardware, software, or emulated. 
#[derive(Clone,Debug)]
pub struct Device(Arc<DeviceInner<dyn DeviceAdaptor>>);

struct DeviceInner<D: ?Sized> {
    pd: Mutex<HashMap<Pd, PdCtx>>,
    mr_table: Mutex<[Option<MrCtx>; MR_TABLE_SIZE]>,
    qp_table: ThreadSafeHashmap<Qpn, QpContext>,
    mr_pgt: Mutex<MrPgt>,
    user_op_ctx_map: ThreadSafeHashmap<(Qpn,Msn), OpCtx<()>>,
    ctrl_op_ctx_map: ThreadSafeHashmap<u32, CtrlOpCtx>,
    next_ctrl_op_id: AtomicU32,
    work_desc_poller: OnceLock<WorkDescPoller>,
    pkt_checker_thread: OnceLock<PacketChecker>,
    retry_monitor: OnceLock<RetryMonitor>,
    ctrl_desc_poller : OnceLock<ControlPoller>,
    local_network : RdmaDeviceNetworkParam,
    nic_device : Mutex<Option<NicInterface>>,
    buffer_keeper : Mutex<Vec<Buffer>>,
    retry_map: RetryMap,
    adaptor: D,
}

impl<D: ?Sized> Debug for DeviceInner<D>{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceInner").field("pd", &self.pd).field("mr_table", &self.mr_table).field("qp_table", &self.qp_table).field("mr_pgt", &self.mr_pgt).field("user_op_ctx_map", &self.user_op_ctx_map).field("ctrl_op_ctx_map", &self.ctrl_op_ctx_map).field("next_ctrl_op_id", &self.next_ctrl_op_id).field("work_desc_poller", &self.work_desc_poller).field("pkt_checker_thread", &self.pkt_checker_thread).field("retry_monitor", &self.retry_monitor).field("ctrl_desc_poller", &self.ctrl_desc_poller).field("local_network", &self.local_network).field("nic_device", &self.nic_device).field("buffer_keeper", &self.buffer_keeper).finish()
    }
}

/// The type of the device adaptor
#[derive(Debug,Clone)]
#[non_exhaustive]
pub enum DeviceType {
    /// A real hardware device
    Hardware{
        /// The character device that open-rdma-kernel driver created
        device_path : String
    },

    /// An emulated device to run hardware code but runs in software framework
    Emulated{
        /// The address of the RPC server created by software framework
        rpc_server_addr: SocketAddr,

        /// The start address of the heap memory
        /// The framework provides a heap memory, which needs to be shared with the driver.
        heap_mem_start_addr: usize,
    },

    /// Pure software device, might be different from the hardware device
    Software
}

/// Configuration of the device
#[derive(Debug,Builder)]
#[non_exhaustive]
pub struct DeviceConfig<Strat:SchedulerStrategy>{
    /// The network configuration of the device
    network_config : RdmaDeviceNetworkParam,

    /// Retry config
    retry_config : RetryConfig,

    /// The type of the device: hardware, software, or emulated
    device_type : DeviceType,

    /// The scheduler strategy
    strategy : Strat,

    /// The scheduler chunk size
    scheduler_size: u32,
}

impl Device {
    const MR_TABLE_EMPTY_ELEM: Option<MrCtx> = None;

    /// # Errors
    ///
    /// Will return `Err` if the device failed to create the `adaptor` or the device failed to init.
    pub fn new<Strat:SchedulerStrategy>(config : DeviceConfig<Strat>) -> Result<Self, Error> {
        let mut core_ids = core_affinity::get_core_ids();
        let scheduler_core = core_ids.as_mut().and_then(|v|v.pop());
        let dev  = match config.device_type{
            DeviceType::Hardware{device_path} => {
                let adaptor = HardwareDevice::new(device_path,config.strategy,scheduler_core,config.scheduler_size).map_err(|e| Error::Device(Box::new(e)))?;
                    let use_hugepage =  adaptor.use_hugepage();
                let pg_table_buf = Buffer::new(MR_PGT_LENGTH * MR_PGT_ENTRY_SIZE,use_hugepage).map_err(|e| Error::ResourceNoAvailable(format!("hugepage {e}")))?;
                Self(Arc::new(DeviceInner {
                    pd: Mutex::new(HashMap::new()),
                    mr_table: Mutex::new([Self::MR_TABLE_EMPTY_ELEM; MR_TABLE_SIZE]),
                    qp_table:  Arc::new(RwLock::new(HashMap::new())),
                    mr_pgt: Mutex::new(MrPgt::new(pg_table_buf)),
                    user_op_ctx_map: Arc::new(RwLock::new(HashMap::new())),
                    ctrl_op_ctx_map: Arc::new(RwLock::new(HashMap::new())),
                    next_ctrl_op_id: AtomicU32::new(0),
                    adaptor,
                    retry_monitor: OnceLock::new(),
                    pkt_checker_thread: OnceLock::new(),
                    work_desc_poller: OnceLock::new(),
                    ctrl_desc_poller : OnceLock::new(),
                    nic_device : Mutex::new(None),
                    buffer_keeper : Vec::new().into(),
                    local_network : config.network_config,
                    retry_map: RetryMap::new(config.retry_config.max_retry, config.retry_config.retry_timeout)
                }))
            },
            DeviceType::Emulated{rpc_server_addr,heap_mem_start_addr} => {
                let adaptor = EmulatedDevice::new(rpc_server_addr, heap_mem_start_addr,config.strategy,config.scheduler_size).map_err(|e| Error::Device(Box::new(e)))?;
                let use_hugepage =  adaptor.use_hugepage();
                let pg_table_buf = Buffer::new(MR_PGT_LENGTH * MR_PGT_ENTRY_SIZE,use_hugepage).map_err(|e| Error::ResourceNoAvailable(format!("hugepage {e}")))?;
                Self(Arc::new(DeviceInner {
                    pd: Mutex::new(HashMap::new()),
                    mr_table: Mutex::new([Self::MR_TABLE_EMPTY_ELEM; MR_TABLE_SIZE]),
                    qp_table:  Arc::new(RwLock::new(HashMap::new())),
                    mr_pgt: Mutex::new(MrPgt::new(pg_table_buf)),
                    user_op_ctx_map: Arc::new(RwLock::new(HashMap::new())),
                    ctrl_op_ctx_map: Arc::new(RwLock::new(HashMap::new())),
                    next_ctrl_op_id: AtomicU32::new(0),
                    adaptor,
                    retry_monitor: OnceLock::new(),
                    pkt_checker_thread: OnceLock::new(),
                    work_desc_poller: OnceLock::new(),
                    ctrl_desc_poller : OnceLock::new(),
                    nic_device : Mutex::new(None),
                    buffer_keeper : Vec::new().into(),
                    local_network : config.network_config,
                    retry_map: RetryMap::new(config.retry_config.max_retry, config.retry_config.retry_timeout)
                }))
            }
            DeviceType::Software => {
                let adaptor = SoftwareDevice::new(config.network_config.ipaddr,DEFAULT_RMDA_PORT,config.strategy,config.scheduler_size).map_err(Error::Device)?;
                let use_hugepage =  adaptor.use_hugepage();
                let pg_table_buf = Buffer::new(MR_PGT_LENGTH * MR_PGT_ENTRY_SIZE,use_hugepage).map_err(|e| Error::ResourceNoAvailable(format!("hugepage {e}")))?;
                Self(Arc::new(DeviceInner {
                    pd: Mutex::new(HashMap::new()),
                    mr_table: Mutex::new([Self::MR_TABLE_EMPTY_ELEM; MR_TABLE_SIZE]),
                    qp_table:  Arc::new(RwLock::new(HashMap::new())),
                    mr_pgt: Mutex::new(MrPgt::new(pg_table_buf)),
                    user_op_ctx_map: Arc::new(RwLock::new(HashMap::new())),
                    ctrl_op_ctx_map: Arc::new(RwLock::new(HashMap::new())),
                    next_ctrl_op_id: AtomicU32::new(0),
                    adaptor,
                    retry_monitor: OnceLock::new(),
                    pkt_checker_thread: OnceLock::new(),
                    work_desc_poller: OnceLock::new(),
                    ctrl_desc_poller : OnceLock::new(),
                    nic_device : Mutex::new(None),
                    buffer_keeper : Vec::new().into(),
                    local_network : config.network_config,
                    retry_map: RetryMap::new(config.retry_config.max_retry, config.retry_config.retry_timeout)
                }))
            }
        };
        dev.init(config.retry_config,core_ids)?;

        Ok(dev)
    }

    fn write_or_read(
        &self,
        dqpn: Qpn,
        raddr: u64,
        rkey: Key,
        flags: WorkReqSendFlag,
        sge0: Sge,
        is_read:bool)-> Result<OpCtx<()>, Error>{
            let (common,key) = {
                let total_len = sge0.len;
                let qp_guard = self.0.qp_table.read();
                let qp = qp_guard.get(&dqpn).ok_or(Error::Invalid(format!("Qpn :{dqpn:?}")))?;
                let msn = qp.next_msn();
                let mut common = ToCardWorkRbDescCommon {
                    total_len,
                    raddr,
                    rkey,
                    dqp_ip: qp.dqp_ip,
                    dqpn: qp.qpn,
                    mac_addr: qp.dqp_mac_addr,
                    pmtu: qp.pmtu,
                    flags,
                    qp_type: qp.qp_type,
                    psn: Psn::default(),
                    msn,
                };
                let packet_cnt = if !is_read{
                    calculate_packet_cnt(qp.pmtu, raddr, total_len)
                }else{
                    1
                };
                let first_pkt_psn = {
                    let mut send_psn = qp.sending_psn.lock();
                    let first_pkt_psn = *send_psn;
                    *send_psn = send_psn.wrapping_add(packet_cnt);
                    first_pkt_psn
                };
                common.psn = first_pkt_psn;
                let key = (common.dqpn,msn);
                (common, key)    
            };
            let opcode = if !is_read{
                ToCardWorkRbDescOpcode::Write
            }else{
                ToCardWorkRbDescOpcode::Read
            };
            let desc = ToCardWorkRbDescBuilder::new(opcode)
                .with_common(common)
                .with_sge(sge0)
                .build()?;
            let clone_desc = desc.clone();
            self.send_work_desc(desc)?;
    
            let ctx = OpCtx::new_running();
    
            self.0
                .user_op_ctx_map
                .write()
                .insert(key, ctx.clone()).map_or_else(||Ok(()),|_|Err(Error::CreateOpCtxFailed))?;
            let _ignore = self.0.retry_map.add((dqpn, key.1), clone_desc, !is_read);
            Ok(ctx)
    }
    
    /// RDMA write operation
    /// 
    /// # Errors
    ///
    /// Will return `Err` if:
    /// * lock poisoned
    /// * failed to create a descriptor
    /// * failed to send a descriptor
    /// * failed to create a operation context
    pub fn write(
        &self,
        dqpn: Qpn,
        raddr: u64,
        rkey: Key,
        flags: WorkReqSendFlag,
        sge0: Sge
    ) -> Result<OpCtx<()>, Error> {
        self.write_or_read(dqpn,raddr,rkey,flags,sge0,false)
    }

    /// RDMA read operation
    /// 
    /// # Errors
    ///
    /// Will return `Err` if:
    /// * lock poisoned
    /// * failed to create a read descriptor
    /// * failed to send a read descriptor
    /// * failed to create a operation context
    pub fn read(
        &self,
        dqpn: Qpn,
        raddr: u64,
        rkey: Key,
        flags: WorkReqSendFlag,
        sge: Sge,
    ) -> Result<OpCtx<()>, Error> {
        self.write_or_read(dqpn,raddr,rkey,flags,sge,true)
    }

    /// # Errors
    pub fn query_mac_address(&self, ip:Ipv4Addr) -> Result<MacAddress,Error> {
        let guard = self.0.nic_device.lock();
        if let Some(nic) =  guard.as_ref(){
            nic.query_mac_addr(ip).ok_or(Error::ResourceNoAvailable("query mac address failed".to_owned()))
        }else{
            Err(Error::ResourceNoAvailable("nic device not ready".to_owned()))
        }
    }

    fn do_ctrl_op(&self, id: u32, desc: ToCardCtrlRbDesc) -> Result<CtrlOpCtx, Error> {
        // save operation context for unparking
        let ctrl_ctx = {
            let mut ctx = self.0.ctrl_op_ctx_map.write();
            let ctrl_ctx = CtrlOpCtx::new_running();

            if ctx.insert(id, ctrl_ctx.clone()).is_some() {
                return Err(Error::CreateOpCtxFailed);
            }

            ctrl_ctx
        };

        // send desc to device
        self.0
            .adaptor
            .to_card_ctrl_rb()
            .push(desc)
            .map_err(|_| Error::DeviceBusy)?;

        Ok(ctrl_ctx)
    }

    fn get_ctrl_op_id(&self) -> u32 {
        self.0.next_ctrl_op_id.fetch_add(1, Ordering::AcqRel)
    }

    #[allow(clippy::expect_used,clippy::unwrap_in_result)]
    fn init(&self,retry_config:RetryConfig,mut core_ids : Option<Vec<CoreId>>) -> Result<(), Error> {
        // enable ctrl desc poller module
        let ctrl_thread_ctx = ControlPollerContext{
            to_host_ctrl_rb: self.0.adaptor.to_host_ctrl_rb(),
            ctrl_op_ctx_map: Arc::<RwLock<HashMap<u32, CtrlOpCtx>>>::clone(&self.0.ctrl_op_ctx_map)
        };
        let ctrl_queue_core = core_ids.as_mut().and_then(|v|v.pop());
        let ctrl_desc_poller = ControlPoller::new(ctrl_thread_ctx,ctrl_queue_core);
        self.0.ctrl_desc_poller.set(ctrl_desc_poller).expect("ctrl_desc_poller has been set");

        let use_hugepage = self.0.adaptor.use_hugepage();
        let mut buf = Buffer::new(ACKNOWLEDGE_BUFFER_SIZE, use_hugepage).map_err(|e| Error::ResourceNoAvailable(format!("hugepage {e}")))?;
        let ack_buf = self.init_buf(&mut buf,ACKNOWLEDGE_BUFFER_SIZE)?;
        self.0.buffer_keeper.lock().push(buf);

        // enable work desc poller module.
        let (nic_notify_send_queue, nic_notify_recv_queue) = unbounded();
        let (checker_send_queue,checker_recv_queue) = unbounded();
        let work_desc_poller_ctx = WorkDescPollerContext{
            work_rb : self.0.adaptor.to_host_work_rb(),
            nic_channel : nic_notify_send_queue,
            checker_channel: checker_send_queue,
        };

        let work_queue_core = core_ids.as_mut().and_then(|v|v.pop());
        let work_desc_poller = WorkDescPoller::new(work_desc_poller_ctx,work_queue_core);
        self.0.work_desc_poller.set(work_desc_poller).expect("work descriptor poller has been set");

        // create nic send device, but we don't prepare receive buffer. So it won't work now.
        let mut tx_slot_buf = Buffer::new(NIC_BUFFER_SIZE, use_hugepage).map_err(|e| Error::ResourceNoAvailable(format!("hugepage {e}")))?;
        let tx_buf = self.init_buf(&mut tx_slot_buf,NIC_BUFFER_SIZE)?;
        let self_device = self.clone();
        let nic_interface = NicInterface::new(self_device, tx_buf, nic_notify_recv_queue,self.0.local_network.macaddr);
        let mut guard = self.0.nic_device.lock();
        *guard = Some(nic_interface);  

        // enable packet checker module
        let packet_checker_ctx = PacketCheckerContext{
            desc_poller_channel: checker_recv_queue,
            user_op_ctx_map: Arc::clone(&self.0.user_op_ctx_map),
            qp_table : Arc::clone(&self.0.qp_table),
            recv_ctx_map : RecvContextMap::new(),
            ctrl_desc_sender: Arc::new(self.clone()),
            work_desc_sender: Arc::new(self.clone()),
            ack_buffers: ack_buf,
            retry_map: self.0.retry_map.clone()
        };
        let pkt_checker_thread = PacketChecker::new(packet_checker_ctx);
        self.0.pkt_checker_thread.set(pkt_checker_thread).expect("pkt_checker_thread has been set");

        // install retry monitor
        let retry_context = RetryMonitorContext{
            map: self.0.retry_map.clone(),
            config: retry_config,
            user_op_ctx_map: Arc::clone(&self.0.user_op_ctx_map),
            device: Arc::new(self.clone()),
        };  
        let retry_monitor = retry::RetryMonitor::new(retry_context);
        self.0.retry_monitor.set(retry_monitor).expect("double init");

        // set card network
        self.set_network(&self.0.local_network)?;

        Ok(())
    }

    /// Enable the NIC interface so that hardware can send Arp, ICMP, etc.  
    pub fn enable_nic_interface(&self) -> Result<(),Error> {
        let mut guard = self.0.nic_device.lock();
        if let Some(nic) =  guard.as_mut(){
            nic.start();
            let use_hugepage = self.0.adaptor.use_hugepage();
            self.prepare_nic_recv_buf(use_hugepage)?;
            Ok(())
        }else{
            Err(Error::ResourceNoAvailable("nic device not ready".to_owned()))
        }
        
    }

    #[allow(clippy::unwrap_in_result,clippy::unwrap_used)]
    fn prepare_nic_recv_buf(&self,use_huge_page:bool) -> Result<(), Error> {
        // configure basic nic recv buffer
        let op_id = self.get_ctrl_op_id();
        let mut buf = Buffer::new(NIC_BUFFER_SIZE, use_huge_page).map_err(|e| Error::ResourceNoAvailable(format!("hugepage {e}")))?;
        let recv_buf: PacketBuf<NIC_PACKET_BUFFER_SLOT_SIZE> = self.init_buf(&mut buf,NIC_BUFFER_SIZE)?;
        self.0.buffer_keeper.lock().push(buf);

        let (start_va,lkey) = recv_buf.get_register_params();
        let set_raw_desc = ToCardCtrlRbDesc::SetRawPacketReceiveMeta(ToCardCtrlRbDescSetRawPacketReceiveMeta {
            common: ToCardCtrlRbDescCommon { op_id },
            base_write_addr : start_va as u64,
            key: lkey,
        });
        let set_raw_ctx = self.do_ctrl_op(op_id, set_raw_desc)?;
        let is_set_raw_success = set_raw_ctx.wait_result()?.ok_or_else(||Error::SetCtxResultFailed)?;
        if !is_set_raw_success {
            return Err(Error::DeviceReturnFailed("Network param"));
        };
        Ok(())
    }

    fn set_network(&self, network: &RdmaDeviceNetworkParam) -> Result<(), Error> {
        let op_id = self.get_ctrl_op_id();
        let desc = ToCardCtrlRbDesc::SetNetworkParam(ToCardCtrlRbDescSetNetworkParam {
            common: ToCardCtrlRbDescCommon { op_id },
            gateway: network.gateway,
            netmask: network.netmask,
            ipaddr: network.ipaddr,
            macaddr: network.macaddr,
        });
        let ctx = self.send_ctrl_desc(desc)?;
        let is_success = ctx.wait_result()?.ok_or_else(||Error::SetCtxResultFailed)?;
        if !is_success {
            return Err(Error::DeviceReturnFailed("Network param"));
        };
        Ok(())
    }
}

/// A interface that allows `DescResponser` to push the work descriptor to the device
pub(crate) trait WorkDescriptorSender: Send + Sync {
    fn send_work_desc(&self, desc_builder: Box<ToCardWorkRbDesc>) -> Result<(), Error>;
}

pub(crate) trait CtrlDescriptorSender: Send + Sync {
    fn send_ctrl_desc(&self, desc_builder: ToCardCtrlRbDesc) -> Result<CtrlOpCtx, Error>;
}

impl WorkDescriptorSender for Device {
    fn send_work_desc(&self, desc: Box<ToCardWorkRbDesc>) -> Result<(), Error> {
        self.0
            .adaptor
            .to_card_work_rb()
            .push(desc)
            .map_err(|_| Error::DeviceBusy)?;
        Ok(())
    }
}

impl CtrlDescriptorSender for Device {
    fn send_ctrl_desc(&self, mut desc: ToCardCtrlRbDesc) -> Result<CtrlOpCtx, Error> {
        let id = self.get_ctrl_op_id();
        desc.set_id(id);
        self.0
            .adaptor
            .to_card_ctrl_rb()
            .push(desc)
            .map_err(|_| Error::DeviceBusy)?;
        let mut ctx = self.0.ctrl_op_ctx_map.write();
        let ctrl_ctx = CtrlOpCtx::new_running();

        if ctx.insert(id, ctrl_ctx.clone()).is_some() {
            return Err(Error::CreateOpCtxFailed);
        }
        Ok(ctrl_ctx)
    }
}

