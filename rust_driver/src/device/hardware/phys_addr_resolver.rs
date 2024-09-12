use std::{fs::File, io, mem::size_of, os::fd::AsRawFd, process};

use log::error;

const PAGE_SHIFT: u64 = 12; // Typical page size shift for 4KB pages
const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

#[derive(Debug)]
pub(crate) struct PhysAddrResolver {
    _pagemap: File,
    pagemap_fd: i32,
}

impl PhysAddrResolver {
    pub(crate) fn new() -> io::Result<PhysAddrResolver> {
        let pagemap_file = format!("/proc/{}/pagemap", process::id());
        let pagemap = File::open(pagemap_file)?;
        let pagemap_fd = pagemap.as_raw_fd();
        Ok(PhysAddrResolver {
            _pagemap: pagemap,
            pagemap_fd,
        })
    }

    #[allow(
        clippy::borrow_as_ptr,
        clippy::arithmetic_side_effects,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    pub(crate) fn query(&self, vaddr: usize) -> Option<usize> {
        let vpn = vaddr >> PAGE_SHIFT;
        let offset = vaddr % PAGE_SIZE;
        let pagemap_fd = self.pagemap_fd.as_raw_fd();
        let data: u64 = 0;
        log::info!("query {:x}",vaddr);
        let ret = unsafe{libc::lseek(pagemap_fd, (vpn * 8) as i64, libc::SEEK_SET)};
        if ret < 0{
            error!("lseek failed :{:?}",io::Error::last_os_error());
        }
        if unsafe {
            libc::read(
                pagemap_fd,
                &data as *const _ as *mut libc::c_void,
                size_of::<u64>(),
            )
        } != 8
        {
            error!("read failed");
            return None;
        }
        let pfn = data & ((1 << 55_i32) - 1);
        #[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)]
        // copy from C code
        let phsy_addr = (pfn << PAGE_SHIFT | (offset as u64)) as usize;
        log::debug!("phsy_addr: {phsy_addr}");
        Some(phsy_addr)
    }
}
