use crate::{
    buf::PacketBuf,
    device::{
        ToCardCtrlRbDesc, ToCardCtrlRbDescCommon, ToCardCtrlRbDescUpdateMrTable,
        ToCardCtrlRbDescUpdatePageTable,
    },
    types::{Key, MemAccessTypeFlag, PAGE_SIZE},
    utils::Buffer,
    Device, Error, Pd, MR_PGT_ENTRY_SIZE,
};
use rand::RngCore as _;
use std::{
    hash::{Hash, Hasher},
    mem, ptr,
};

pub(crate) const ACKNOWLEDGE_BUFFER_SIZE: usize = PAGE_SIZE;
pub(crate) const NIC_BUFFER_SIZE: usize = PAGE_SIZE;

/// Memory Region
///
/// User use `Device::alloc_mr(..)` to allocate a `Mr` and use `Device::dereg_mr(..)` to deallocate a `Mr`.
#[derive(Debug, Clone, Copy)]
pub struct Mr {
    pub(crate) key: Key,
}

impl Mr {
    /// Get the key of the Mr
    #[must_use]
    pub fn get_key(&self) -> Key {
        self.key
    }
}

#[derive(Debug)]
pub(crate) struct MrCtx {
    pub(crate) pd: Pd,
    pub(crate) len: u32,
    pub(crate) pgt_offset: usize,
    pub(crate) pg_size: u32,
}

#[derive(Debug)]
pub(crate) struct MrPgt {
    table: Buffer,
    free_blk_list: *mut MrPgtFreeBlk,
}

struct MrPgtFreeBlk {
    idx: usize,
    len: usize,
    prev: *mut Self,
    next: *mut Self,
}

impl Device {
    fn register_page_table(&self, addr: u64, length: u32, pg_size: u32) -> Result<usize, Error> {
        let mut mr_pgt = self.0.mr_pgt.lock();
        let pgte_cnt = length.div_ceil(pg_size) as usize;
        let pgt_offset = mr_pgt.alloc(pgte_cnt)?;
        for pgt_idx in 0..pgte_cnt {
            let va = addr.wrapping_add(((pg_size as usize).wrapping_mul(pgt_idx)) as u64);
            // Should we support 32 bit system?
            let va_in_usize =
                usize::try_from(va).map_err(|_| Error::NotSupport("32 bit System"))?;
            let pa = self
                .0
                .adaptor
                .get_phys_addr(va_in_usize)
                .map_err(|e| Error::GetPhysAddrFailed(e.to_string()))?;
            // If we run with hardware DMA,
            // we must make sure va and pa are all allign to pg_size
            if va_in_usize & (PAGE_SIZE - 1) != 0 {
                return Err(Error::AddressNotAlign("va", va_in_usize));
            }
            if pa & (PAGE_SIZE - 1) != 0 {
                return Err(Error::AddressNotAlign("pa", pa));
            }
            // `mr_pgt.alloc(pgte_cnt)` has already checked that `pgt_offset + pgt_idx` is in range
            #[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
            {
                mr_pgt.table[pgt_idx] = pa as u64;
            }
        }

        let update_pgt_op_id = self.get_ctrl_op_id();
        // `pgt_offset` and `pg_size` are both derived from pg_size, which is a u32. So it's safe to covert
        #[allow(clippy::cast_possible_truncation, clippy::arithmetic_side_effects)]
        let update_pgt_desc = ToCardCtrlRbDesc::UpdatePageTable(ToCardCtrlRbDescUpdatePageTable {
            common: ToCardCtrlRbDescCommon {
                op_id: update_pgt_op_id,
            },
            start_addr: self
                .0
                .adaptor
                .get_phys_addr(mr_pgt.table.as_ref().as_ptr() as usize)
                .map_err(|e| Error::GetPhysAddrFailed(e.to_string()))? as u64,
            pgt_idx: pgt_offset as u32,
            pgte_cnt: pgte_cnt as u32,
        });

        let update_pgt_ctx = self.do_ctrl_op(update_pgt_op_id, update_pgt_desc)?;

        let update_pgt_result = update_pgt_ctx
            .wait_result()?
            .ok_or(Error::SetCtxResultFailed)?;

        if !update_pgt_result {
            mr_pgt.dealloc(pgt_offset, pgte_cnt);
            return Err(Error::DeviceReturnFailed("update page table"));
        }
        Ok(pgt_offset)
    }

    fn deregister_page_table(&self, pgt_offset: usize, length: u32) -> Result<(), Error> {
        let mut mr_pgt = self.0.mr_pgt.lock();
        mr_pgt.dealloc(
            pgt_offset,
            length
                .try_into()
                .map_err(|_| Error::NotSupport("Not 64 bit System"))?,
        );
        Ok(())
    }

    /// Register a Mr
    ///
    /// # Errors
    ///
    /// Will return `Err` if:
    /// * lock poisoned
    /// * not have enough resouce to allocate a new pagetable
    /// * invalid pd
    /// * failed to communicate with card(including creating page table and creating mr)
    pub fn reg_mr(
        &self,
        pd: Pd,
        addr: u64,
        len: u32,
        pg_size: u32,
        acc_flags: MemAccessTypeFlag,
    ) -> Result<Mr, Error> {
        let mut mr_table = self.0.mr_table.lock();

        let mut pd_pool = self.0.pd.lock();

        let Some(mr_idx) = mr_table
            .iter()
            .enumerate()
            .find_map(|(idx, ctx)| ctx.is_none().then_some(idx))
        else {
            return Err(Error::ResourceNoAvailable("MR".to_owned()));
        };

        let pd_ctx = pd_pool
            .get_mut(&pd)
            .ok_or(Error::Invalid(format!("PD :{pd:?}")))?;

        let pgt_offset = self.register_page_table(addr, len, pg_size)?;

        // mr_idx is smaller than `MR_TABLE_SIZE`. Currently, it's a relatively small number.
        // And it's expected to smaller than 2^32 during transimission
        #[allow(clippy::cast_possible_truncation, clippy::arithmetic_side_effects)]
        let key_idx = (mr_idx as u32) << (mem::size_of::<u32>() * 8 - crate::MR_KEY_IDX_BIT_CNT);
        let key_secret = rand::thread_rng().next_u32() >> crate::MR_KEY_IDX_BIT_CNT;
        let key = Key::new(key_idx | key_secret);

        let mr = Mr { key };
        let mr_ctx = MrCtx {
            pd,
            len,
            pgt_offset,
            pg_size,
        };

        let update_mr_op_id = self.get_ctrl_op_id();

        #[allow(clippy::cast_possible_truncation)]
        let update_mr_desc = ToCardCtrlRbDesc::UpdateMrTable(ToCardCtrlRbDescUpdateMrTable {
            common: ToCardCtrlRbDescCommon {
                op_id: update_mr_op_id,
            },
            addr,
            len,
            key,
            pd_hdl: mr_ctx.pd.handle,
            acc_flags,
            pgt_offset: pgt_offset as u32,
        });

        let update_mr_ctx = self.do_ctrl_op(update_mr_op_id, update_mr_desc)?;

        let update_mr_result = update_mr_ctx
            .wait_result()?
            .ok_or(Error::SetCtxResultFailed)?;

        if !update_mr_result {
            return Err(Error::DeviceReturnFailed("register mr table"));
        }

        #[allow(clippy::indexing_slicing)]
        // `mr_idx` is allocated by `find_map` above, so it's safe to index
        {
            mr_table[mr_idx] = Some(mr_ctx);
        }

        if !pd_ctx.mr.insert(mr) {
            return Err(Error::Invalid(format!("mr :{mr:?}")));
        }

        Ok(mr)
    }

    pub(crate) fn init_buf<const SLOT_SIZE: usize>(
        &self,
        buffer: &mut Buffer,
        buffer_size: usize,
    ) -> Result<PacketBuf<SLOT_SIZE>, Error> {
        let buffer_addr = buffer.as_ref().as_ptr() as usize;
        let pd = self.alloc_pd()?;

        // the `PAGE_SIZE` and `ACKNOWLEDGE_BUFFER_SIZE` is guaranteed to smaller than u32
        #[allow(clippy::cast_possible_truncation)]
        let create_mr_result = self.reg_mr(
            pd,
            u64::try_from(buffer_addr).map_err(|_| Error::NotSupport("Not 64 bit System"))?,
            buffer_size as u32,
            PAGE_SIZE as u32, // 2MB
            MemAccessTypeFlag::IbvAccessLocalWrite
                | MemAccessTypeFlag::IbvAccessRemoteRead
                | MemAccessTypeFlag::IbvAccessRemoteWrite,
        );
        create_mr_result.map(|mr| PacketBuf::new(buffer_addr, buffer_size, mr.get_key()))
    }

    /// Remove a Mr
    ///
    /// # Errors
    ///
    /// Will return `Err` if:
    /// * lock poisoned
    /// * failed to communicate with card(including remove page table and remove mr)
    /// * Operating system not support
    /// * Setted context result failed
    pub fn dereg_mr(&self, mr: Mr) -> Result<(), Error> {
        let mut mr_table = self.0.mr_table.lock();
        let mut pd_pool = self.0.pd.lock();
        #[allow(clippy::arithmetic_side_effects)]
        let mr_idx = mr.key.get() >> (mem::size_of::<u32>() * 8 - crate::MR_KEY_IDX_BIT_CNT);
        let ctx_option = mr_table
            .get_mut(mr_idx as usize)
            .ok_or(Error::Invalid(format!("MR :{mr_idx}")))?;
        let Some(mr_ctx) = ctx_option else {
            return Err(Error::Invalid(format!("MR :{mr_idx}")));
        };

        let pd_ctx = pd_pool
            .get_mut(&mr_ctx.pd)
            .ok_or(Error::Invalid(format!("PD :{:?}", &mr_ctx.pd)))?;

        let op_id = self.get_ctrl_op_id();

        let desc = ToCardCtrlRbDesc::UpdateMrTable(ToCardCtrlRbDescUpdateMrTable {
            common: ToCardCtrlRbDescCommon { op_id },
            addr: 0,
            len: 0,
            key: mr.key,
            pd_hdl: 0,
            acc_flags: MemAccessTypeFlag::IbvAccessNoFlags,
            pgt_offset: 0,
        });

        let ctx = self.do_ctrl_op(op_id, desc)?;

        let res = ctx.wait_result()?.ok_or(Error::SetCtxResultFailed)?;

        if !res {
            return Err(Error::DeviceReturnFailed("deregister mr table"));
        }

        self.deregister_page_table(mr_ctx.pgt_offset, mr_ctx.len.div_ceil(mr_ctx.pg_size))?;

        if !pd_ctx.mr.remove(&mr) {
            return Err(Error::Invalid(format!("MR :{mr_idx}")));
        }
        *ctx_option = None;

        Ok(())
    }
}

impl MrPgt {
    pub(crate) fn new(buffer: Buffer) -> Self {
        let len = buffer.size() / MR_PGT_ENTRY_SIZE;
        let free_blk = Box::into_raw(Box::new(MrPgtFreeBlk {
            idx: 0,
            len,
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
        }));
        assert!(
            buffer.as_ref().as_ptr() as usize % PAGE_SIZE == 0,
            "buffer should be aligned to PAGE_SIZE"
        );
        Self {
            table: buffer,
            free_blk_list: free_blk,
        }
    }

    #[allow(clippy::arithmetic_side_effects)]
    fn alloc(&mut self, len: usize) -> Result<usize, Error> {
        let mut ptr = self.free_blk_list;

        while !ptr.is_null() {
            let blk = unsafe { ptr.as_mut() };
            let blk = unsafe { blk.unwrap_unchecked() };

            if blk.len >= len {
                let idx = blk.idx;

                blk.idx += len; // idx, len are all smaller than `MR_PGT_SIZE`
                blk.len -= len;

                if blk.len == 0 {
                    if blk.prev.is_null() {
                        self.free_blk_list = blk.next;
                    } else {
                        let prev = unsafe { blk.prev.as_mut() };
                        let prev = unsafe { prev.unwrap_unchecked() };
                        prev.next = blk.next;
                    }

                    if !blk.next.is_null() {
                        let next = unsafe { blk.next.as_mut() };
                        let next = unsafe { next.unwrap_unchecked() };
                        next.prev = blk.prev;
                    }

                    drop(unsafe { Box::from_raw(ptr) });
                }

                return Ok(idx);
            }

            ptr = blk.next;
        }

        Err(Error::ResourceNoAvailable("MR page table".to_owned()))
    }

    fn dealloc(&mut self, idx: usize, len: usize) {
        let mut prev_ptr = ptr::null_mut();
        let mut ptr = self.free_blk_list;

        while !ptr.is_null() {
            let blk = unsafe { ptr.as_mut() };
            let blk = unsafe { blk.unwrap_unchecked() };

            if blk.len > len {
                break;
            }

            prev_ptr = ptr;
            ptr = blk.next;
        }

        let new_ptr = Box::into_raw(Box::new(MrPgtFreeBlk {
            idx,
            len,
            prev: prev_ptr,
            next: ptr,
        }));

        let new = unsafe { new_ptr.as_mut() };
        let new = unsafe { new.unwrap_unchecked() };

        if new.prev.is_null() {
            self.free_blk_list = new_ptr;
        } else {
            let new_prev = unsafe { new.prev.as_mut() };
            let new_prev = unsafe { new_prev.unwrap_unchecked() };
            new_prev.next = new_ptr;
        }

        if !new.next.is_null() {
            let new_next = unsafe { new.next.as_mut() };
            let new_next = unsafe { new_next.unwrap_unchecked() };
            new_next.prev = new_ptr;
        }

        while !new.prev.is_null() {
            let new_prev = unsafe { new.prev.as_mut() };
            let new_prev = unsafe { new_prev.unwrap_unchecked() };

            if new_prev.idx.wrapping_add(new_prev.len) != new.len {
                break;
            }

            new.idx = new_prev.idx;
            new.len = new.len.wrapping_add(new_prev.len);

            let new_prev_prev_ptr = new_prev.prev;
            drop(unsafe { Box::from_raw(new.prev) });

            if new_prev_prev_ptr.is_null() {
                self.free_blk_list = new_ptr;
            } else {
                let new_prev_prev = unsafe { new_prev_prev_ptr.as_mut() };
                let new_prev_prev = unsafe { new_prev_prev.unwrap_unchecked() };
                new_prev_prev.next = new_ptr;
            }

            new.prev = new_prev_prev_ptr;
        }

        while !new.next.is_null() {
            let new_next = unsafe { new.next.as_mut() };
            let new_next = unsafe { new_next.unwrap_unchecked() };

            if new_next.idx != new.idx.wrapping_add(new.len) {
                break;
            }

            new.len = new.len.wrapping_add(new_next.len);

            let new_next_next_ptr = new_next.next;
            drop(unsafe { Box::from_raw(new.next) });

            if !new_next_next_ptr.is_null() {
                let new_next_next = unsafe { new_next_next_ptr.as_mut() };
                let new_next_next = unsafe { new_next_next.unwrap_unchecked() };
                new_next_next.prev = new_ptr;
            }

            new.next = new_next_next_ptr;
        }
    }
}

unsafe impl Send for MrPgt {}
unsafe impl Sync for MrPgt {}

impl Hash for Mr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl PartialEq for Mr {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for Mr {}
