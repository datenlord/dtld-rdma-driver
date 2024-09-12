use std::{fs::{File, OpenOptions}, io, mem::size_of, os::fd::AsRawFd, path::Path};


const RDMA_IOCTL_CMD: u64 = 0xc018_1b01;
#[repr(C)]
#[derive(Debug)]
struct RDMASyscallReq {
    length: u16,
    object_id: u16,
    method_id: u16,
    num_attrs: u16,
    reserved1: u64,
    driver_id: u32,
    reserved2: u32,
}

#[repr(C)]
#[derive(Debug)]
struct RDMASyscallReqAttr {
    attr_id: u16,
    len: u16,
    flags: u16,
    reserved: u16,
    data: u64,
}

#[repr(C)]
#[derive(Debug)]
struct RDMAGetUContextSyscallReq {
    req: RDMASyscallReq,
    attr: RDMASyscallReqAttr,
}

impl RDMAGetUContextSyscallReq {
    #[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)]
    pub(crate) fn new_get_context(resp: &RDMAGetUContextSyscallResp) -> Self {
        let req = RDMASyscallReq {
            length: (size_of::<RDMASyscallReq>() + size_of::<RDMASyscallReqAttr>()) as u16, // 40, this will never overflow
            object_id: 0,
            method_id: 3, // get context
            num_attrs: 1,
            reserved1: 0,
            driver_id: 0, // RDMA Unknown
            reserved2: 0,
        };
        let attr = RDMASyscallReqAttr {
            attr_id: 4097, // out ptr attr
            len: size_of::<RDMAGetUContextSyscallResp>() as u16,
            flags: 1,
            reserved: 0,
            data: resp as *const _ as u64,
        };
        Self { req, attr }
    }
}

// should sync with dtld-abi.h
// FIXME: try bindings
#[repr(C)]
#[derive(Debug, Default)]
pub(crate)struct RDMAGetUContextSyscallResp {
    pub(crate) csr: i64,
    pub(crate) cmdq_sq: i64,
    pub(crate) cmdq_rq: i64,
    pub(crate) workq_sq: i64,
    pub(crate) workq_rq: i64,
    pub(crate) cmdq_sq_dma_addr: usize,
    pub(crate) cmdq_rq_dma_addr: usize,
    pub(crate) workq_sq_dma_addr: usize,
    pub(crate) workq_rq_dma_addr: usize,
}

/// Will be replaced by ib_verbs
pub(crate) fn new_ucontext(device: &File) -> io::Result<RDMAGetUContextSyscallResp> {
    let resp = RDMAGetUContextSyscallResp::default();
    let req = RDMAGetUContextSyscallReq::new_get_context(&resp);
    let device_file_fd = device.as_raw_fd();
    let ret_val = unsafe {
        libc::ioctl(
            device_file_fd,
            RDMA_IOCTL_CMD,
            std::ptr::addr_of!(req) as *mut u8,
        )
    };

    if ret_val != 0_i32 {
        return Err(io::Error::last_os_error());
    }
    log::info!("{:?}", resp);

    Ok(resp)
}
