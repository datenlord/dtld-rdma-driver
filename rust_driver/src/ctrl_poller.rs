use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

use core_affinity::CoreId;
use log::{error,info};

use crate::{
    device::{
        CtrlRbDescOpcode, ToHostCtrlRbDesc, ToHostRb
    },
    op_ctx::CtrlOpCtx, ThreadSafeHashmap,
};

#[derive(Debug)]
pub(crate) struct ControlPoller {
    thread: Option<std::thread::JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

pub(crate) struct ControlPollerContext {
    pub(crate) to_host_ctrl_rb: Arc<dyn ToHostRb<ToHostCtrlRbDesc>>,
    pub(crate) ctrl_op_ctx_map: ThreadSafeHashmap<u32, CtrlOpCtx>,
}

unsafe impl Send for ControlPollerContext {}

impl ControlPoller {
    pub(crate) fn new(ctx: ControlPollerContext,core_id: Option<CoreId>) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let thread_stop_flag = Arc::clone(&stop_flag);
        let thread = std::thread::spawn(move || {
            if let Some(core_id) = core_id{
                if !core_affinity::set_for_current(core_id) {
                    log::error!("failed to set core_affinity {:?} in cmd queue poller", core_id);
                }else{
                    log::info!("set core_affinity in cmd queue poller successfully");
                }
            }
            ControlPollerContext::poll_ctrl_thread(&ctx, &thread_stop_flag);
        });
        Self {
            thread: Some(thread),
            stop_flag,
        }
    }
}

impl ControlPollerContext {
    pub(crate) fn poll_ctrl_thread(ctx: &Self, stop_flag: &AtomicBool) {
        while !stop_flag.load(Ordering::Relaxed) {
            let desc = match ctx.to_host_ctrl_rb.pop() {
                Ok(desc) => desc,
                Err(e) => {
                    error!("failed to fetch descriptor from ctrl rb : {:?}", e);
                    return;
                }
            };
            if matches!(desc.common.opcode,CtrlRbDescOpcode::UpdateErrorPsnRecoverPoint){
                
            }else{
                ctx.handle_ctrl_desc_resp(&desc);
            }
        }
    }

    fn handle_ctrl_desc_resp(&self, desc: &ToHostCtrlRbDesc) {
        let ctx_map = self.ctrl_op_ctx_map.read();

        if let Some(ctx) = ctx_map.get(&desc.common.op_id) {
            if let Some(handler) = ctx.take_handler(){
                handler(desc.common.is_success);
            }
            if let Err(e) = ctx.set_result(desc.common.is_success) {
                error!("Set result failed {:?}", e);
            }
        } else {
            error!("no ctrl cmd ctx found");
        }
    }
}

impl Drop for ControlPoller {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            if let Err(e) = thread.join() {
                panic!("{}", format!("ControlPoller thread join failed: {e:?}"));
            }
            info!("ControlPoller thread is normally stopped");
        }
    }
}
