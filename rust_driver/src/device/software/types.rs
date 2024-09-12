use crate::{
    device::{
        DescSge, ToCardWorkRbDesc, ToCardWorkRbDescCommon, ToCardWorkRbDescOpcode,
        ToHostWorkRbDescAethCode, ToHostWorkRbDescOpcode, ToHostWorkRbDescTransType,
    },
    types::{MemAccessTypeFlag, Psn, QpType},
};

use super::{
    logic::BlueRdmaLogicError,
    packet::{Immediate, PacketError, AETH, BTH, RDMA_PAYLOAD_ALIGNMENT, RETH},
};

/// Queue-pair number
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Qpn(u32);

impl Qpn {
    pub(crate) fn new(qpn: u32) -> Self {
        Qpn(qpn)
    }

    pub(crate) fn get(self) -> u32 {
        self.0
    }
}

/// Protection Domain handle
#[derive(Debug, Clone, Copy)]
pub(crate) struct PDHandle(u32);

impl PDHandle {
    pub(crate) fn new(handle: u32) -> Self {
        PDHandle(handle)
    }

    #[cfg(test)]
    pub(crate) fn get(&self) -> u32 {
        self.0
    }
}

/// The general key type, like `RKey`, `Lkey`
#[derive(Default, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) struct Key(u32);

impl Key {
    pub(crate) fn new(key: u32) -> Self {
        Key(key)
    }

    pub(crate) fn get(self) -> u32 {
        self.0
    }
}

impl From<crate::Key> for Key {
    fn from(key: crate::Key) -> Self {
        Key::new(key.get())
    }
}

impl From<Key> for crate::Key {
    fn from(key: Key) -> Self {
        Self::new(key.get())
    }
}

/// Partition Key
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PKey(u16);
impl PKey {
    pub(crate) fn new(key: u16) -> Self {
        Self(key)
    }

    pub(crate) fn get(self) -> u16 {
        self.0
    }
}

/// State of the queue pair
#[allow(dead_code)]
pub(crate) enum StateQP {
    Reset,
    Init,
    Rtr,
    Rts,
    Sqd,
    Sqe,
    Err,
    Unknown,
    Create, // Not defined in rdma-core
}

/// A abstraction of a RDMA message.
#[derive(Debug, Clone)]
pub(crate) enum Metadata {
    /// RDMA write, read request and response
    General(RdmaGeneralMeta),

    /// Acknowledge message
    Acknowledge(AethHeader),
}

impl Metadata {
    pub(crate) fn get_opcode(&self) -> ToHostWorkRbDescOpcode {
        match self {
            Metadata::General(header) => header.common_meta.opcode.clone(),
            Metadata::Acknowledge(header) => header.common_meta.opcode.clone(),
        }
    }

    pub(crate) fn common_meta(&self) -> &RdmaMessageMetaCommon {
        match self {
            Metadata::General(header) => &header.common_meta,
            Metadata::Acknowledge(header) => &header.common_meta,
        }
    }
}

/// A scatter-gather list element.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SGListElement {
    pub(crate) data: *const u8,
    pub(crate) len: usize,
}

/// A payload info, which contains the scatter-gather list and the total length of the payload.
#[derive(Debug, Clone)]
pub(crate) struct PayloadInfo {
    sg_list: Vec<SGListElement>,
    total_len: usize,
}

impl PayloadInfo {
    pub(crate) fn new() -> Self {
        PayloadInfo {
            sg_list: Vec::new(),
            total_len: 0,
        }
    }

    pub(crate) fn new_with_data(data: *const u8, len: usize) -> Self {
        PayloadInfo {
            sg_list: vec![SGListElement { data, len }],
            total_len: len,
        }
    }

    #[cfg(test)]
    pub(crate) fn get_length(&self) -> usize {
        self.total_len
    }

    pub(crate) fn get_pad_cnt(&self) -> usize {
        #[allow(clippy::arithmetic_side_effects)]
        let mut pad_cnt = RDMA_PAYLOAD_ALIGNMENT - self.total_len % RDMA_PAYLOAD_ALIGNMENT;
        if pad_cnt == RDMA_PAYLOAD_ALIGNMENT {
            pad_cnt = 0;
        }
        pad_cnt
    }

    pub(crate) fn with_pad_length(&self) -> usize {
        self.total_len.wrapping_add(self.get_pad_cnt())
    }

    pub(crate) fn add(&mut self, data: *const u8, len: usize) {
        self.sg_list.push(SGListElement { data, len });
        self.total_len = self.total_len.wrapping_add(len);
    }

    #[cfg(test)]
    pub(crate) fn get_sg_list(&self) -> &Vec<SGListElement> {
        &self.sg_list
    }

    pub(crate) fn copy_to(&self, mut dst: *mut u8) {
        for element in &self.sg_list {
            unsafe {
                std::ptr::copy_nonoverlapping(element.data, dst, element.len);
            }
            unsafe {
                dst = dst.add(element.len);
            }
        }
    }

    /// Get the first and only element of the scatter-gather list.
    /// Note that you should only use this function when you are sure that the payload only contains one element.
    /// 
    /// If `skip_eth` is `true`, it will skip the first 14 bytes of the payload, which is the Ethernet header.
    pub(crate) fn direct_data_ptr(&self,skip_eth:bool) -> Option<&[u8]> {
        let buf = self.sg_list.first();
        buf.map(|first|{
            let data = unsafe { std::slice::from_raw_parts(first.data, first.len)};
            #[allow(clippy::indexing_slicing)]
            if skip_eth{
                &data[14..]
            } else{
                data
            }
        })
        
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RdmaMessage {
    pub(crate) meta_data: Metadata,
    pub(crate) payload: PayloadInfo,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RethHeader {
    pub(crate) va: u64,
    pub(crate) rkey: Key,
    pub(crate) len: u32,
}

impl From<&RETH> for RethHeader {
    fn from(reth: &RETH) -> Self {
        RethHeader {
            va: reth.get_va(),
            rkey: Key::new(reth.get_rkey()),
            len: reth.get_dlen(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RdmaMessageMetaCommon {
    pub(crate) tran_type: ToHostWorkRbDescTransType,
    pub(crate) opcode: ToHostWorkRbDescOpcode,
    pub(crate) solicited: bool,
    pub(crate) pkey: PKey,
    pub(crate) dqpn: Qpn,
    pub(crate) ack_req: bool,
    pub(crate) psn: Psn,
}

impl TryFrom<&BTH> for RdmaMessageMetaCommon {
    type Error = PacketError;
    fn try_from(bth: &BTH) -> Result<Self, PacketError> {
        Ok(Self {
            tran_type: ToHostWorkRbDescTransType::try_from(bth.get_transaction_type())
                .map_err(|_| PacketError::FailedToConvertTransType)?,
            opcode: ToHostWorkRbDescOpcode::try_from(bth.get_opcode())?,
            solicited: bth.get_solicited(),
            pkey: PKey::new(bth.get_pkey()),
            dqpn: Qpn(bth.get_destination_qpn()),
            ack_req: bth.get_ack_req(),
            psn: Psn::new(bth.get_psn()),
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RdmaGeneralMeta {
    pub(crate) common_meta: RdmaMessageMetaCommon,
    pub(crate) reth: RethHeader,
    pub(crate) imm: Option<u32>,
    pub(crate) secondary_reth: Option<RethHeader>,
}

impl RdmaGeneralMeta {
    pub(crate) fn new_from_packet(
        bth: &BTH,
        reth: &RETH,
        imm: Option<&Immediate>,
        secondary_reth: Option<&RETH>,
    ) -> Result<Self, PacketError> {
        Ok(RdmaGeneralMeta {
            common_meta: RdmaMessageMetaCommon::try_from(bth)?,
            reth: RethHeader::from(reth),
            imm: imm.map(Immediate::get),
            secondary_reth: secondary_reth.map(RethHeader::from),
        })
    }

    pub(crate) fn is_read_request(&self) -> bool {
        matches!(
            self.common_meta.opcode,
            ToHostWorkRbDescOpcode::RdmaReadRequest
        )
    }

    pub(crate) fn has_payload(&self) -> bool {
        matches!(
            self.common_meta.opcode,
            ToHostWorkRbDescOpcode::RdmaWriteFirst
                | ToHostWorkRbDescOpcode::RdmaWriteMiddle
                | ToHostWorkRbDescOpcode::RdmaWriteLast
                | ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate
                | ToHostWorkRbDescOpcode::RdmaWriteOnly
                | ToHostWorkRbDescOpcode::RdmaWriteOnlyWithImmediate
                | ToHostWorkRbDescOpcode::RdmaReadResponseFirst
                | ToHostWorkRbDescOpcode::RdmaReadResponseMiddle
                | ToHostWorkRbDescOpcode::RdmaReadResponseLast
                | ToHostWorkRbDescOpcode::RdmaReadResponseOnly
        )
    }

    pub(crate) fn needed_permissions(&self) -> MemAccessTypeFlag {
        if self.has_payload() {
            MemAccessTypeFlag::IbvAccessRemoteWrite
        } else if self.is_read_request() {
            MemAccessTypeFlag::IbvAccessRemoteRead
        } else {
            MemAccessTypeFlag::IbvAccessNoFlags
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) struct AethHeader {
    pub(crate) common_meta: RdmaMessageMetaCommon,
    pub(crate) aeth_code: ToHostWorkRbDescAethCode,
    pub(crate) aeth_value: u8,
    pub(crate) msn: u32,
}

impl AethHeader {
    pub(crate) fn new_from_packet(bth: &BTH, aeth: &AETH) -> Result<Self, PacketError> {
        let aeth_code = ToHostWorkRbDescAethCode::try_from(aeth.get_aeth_code())?;
        let aeth_value = aeth.get_aeth_value();
        let msn = aeth.get_msn();

        Ok(AethHeader {
            common_meta: RdmaMessageMetaCommon::try_from(bth)?,
            aeth_code,
            aeth_value,
            msn,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SGListElementWithKey {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) key: Key,
}

impl Default for SGListElementWithKey {
    fn default() -> Self {
        SGListElementWithKey {
            addr: 0,
            len: 0,
            key: Key::new(0),
        }
    }
}

// impl SGListElementWithKey {
//     /// Cut a buffer of length from a scatter-gather element
//     pub(crate) fn cut(&mut self, length: u32) -> Result<PayloadInfo, BlueRdmaLogicError> {
//         let mut payload = PayloadInfo::new();
//         if self.len >= length {
//             let addr = self.addr as *mut u8;
//             payload.add(addr, length as usize);
//             self.addr += length as u64;
//             self.len -= length;
//             return Ok(payload);
//         }
//         Err(BlueRdmaLogicError::Unreachable)
//     }
// }

impl From<DescSge> for SGListElementWithKey {
    fn from(sge: DescSge) -> Self {
        SGListElementWithKey {
            addr: sge.addr,
            len: sge.len,
            key: Key::new(sge.key.get()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SGList {
    pub(crate) data: [SGListElementWithKey; 4],
    pub(crate) cur_level: u32,
    pub(crate) len: u32,
}

impl SGList {
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        SGList {
            data: [SGListElementWithKey::default(); 4],
            cur_level: 0,
            len: 0,
        }
    }

    pub(crate) fn new_with_sge(sge: DescSge) -> Self {
        SGList {
            data: [
                SGListElementWithKey::from(sge),
                SGListElementWithKey::default(),
                SGListElementWithKey::default(),
                SGListElementWithKey::default(),
            ],
            cur_level: 0,
            len: 1,
        }
    }

    pub(crate) fn get_total_length(&self) -> u32 {
        self.data.iter().map(|sge| sge.len).sum()
    }

    fn get_sge_from_option(sge: Option<DescSge>) -> (SGListElementWithKey, u32) {
        match sge {
            Some(sge) => (SGListElementWithKey::from(sge), 1),
            None => (SGListElementWithKey::default(), 0),
        }
    }

    #[allow(clippy::arithmetic_side_effects)] //sge_counter is either 0 or 1
    pub(crate) fn new_with_sge_list(
        sge0: DescSge,
        sge1: Option<DescSge>,
        sge2: Option<DescSge>,
        sge3: Option<DescSge>,
    ) -> Self {
        let sge0 = SGListElementWithKey::from(sge0);
        let mut counter = 1;
        let (sge1, sge1_counter) = Self::get_sge_from_option(sge1);
        counter += sge1_counter;
        let (sge2, sge2_counter) = Self::get_sge_from_option(sge2);
        counter += sge2_counter;
        let (sge3, sge3_counter) = Self::get_sge_from_option(sge3);
        counter += sge3_counter;
        SGList {
            data: [sge0, sge1, sge2, sge3],
            cur_level: 0,
            len: counter,
        }
    }

    /// Cut a buffer of length from the scatter-gather list
    ///
    /// The function iterate from `cur_level` of the scatter-gather list and cut the buffer of `length` from the list.
    /// If current level is not enough, it will move to the next level.
    /// All the slice will be added to the `payload`.
    #[allow(clippy::indexing_slicing,clippy::arithmetic_side_effects)]
    pub(crate) fn cut(&mut self, mut length: u32) -> Result<PayloadInfo, BlueRdmaLogicError> {
        let mut current_level = self.cur_level as usize;
        let mut payload = PayloadInfo::new();
        // here the current level should be a very small number, so it is safe to cast it to u32
        #[allow(clippy::cast_possible_truncation)]
        while (current_level as u32) < self.len {
            if self.data[current_level].len >= length {
                let addr = self.data[current_level].addr as *mut u8;
                payload.add(addr, length as usize);
                self.data[current_level].addr = self.data[current_level].addr.wrapping_add(u64::from(length));
                self.data[current_level].len -= length;
                if self.data[current_level].len == 0 {
                    current_level += 1;
                    self.cur_level = current_level as u32;
                }
                return Ok(payload);
            }
            // check next level
            let addr = self.data[current_level].addr as *mut u8;
            payload.add(addr, self.data[current_level].len as usize);
            length -= self.data[current_level].len;
            self.data[current_level].len = 0;
            current_level += 1;
        }
        Err(BlueRdmaLogicError::Unreachable)
    }

    pub(crate) fn cut_all_levels(&mut self) -> PayloadInfo {
        let mut payload = PayloadInfo::new();
        for data in &mut self.data {
            let addr = data.addr as *mut u8;
            let length = data.len as usize;
            payload.add(addr, length);
            data.len = 0;
        }
        payload
    }

    #[cfg(test)]
    pub(crate) fn into_four_sges(
        self,
    ) -> (
        DescSge,
        Option<DescSge>,
        Option<DescSge>,
        Option<DescSge>,
    ) {
        use crate::types::Key;

        let sge1 = (self.len > 1).then(|| DescSge {
            addr: self.data[1].addr,
            len: self.data[1].len,
            key: Key::new(self.data[1].key.get()),
        });

        let sge2 = (self.len > 2).then(|| DescSge {
            addr: self.data[2].addr,
            len: self.data[2].len,
            key: Key::new(self.data[2].key.get()),
        });

        let sge3 = (self.len > 3).then(|| DescSge {
            addr: self.data[3].addr,
            len: self.data[3].len,
            key: Key::new(self.data[3].key.get()),
        });
        (
            DescSge {
                addr: self.data[0].addr,
                len: self.data[0].len,
                key: Key::new(self.data[0].key.get()),
            },
            sge1,
            sge2,
            sge3,
        )
    }
}

#[derive(Debug)]
pub(crate) enum ToCardDescriptor {
    Write(ToCardWriteDescriptor),
    Read(ToCardReadDescriptor),
}

impl ToCardDescriptor {
    pub(crate) fn is_raw_packet(&self) -> bool {
        match self {
            ToCardDescriptor::Write(desc) => {
                matches!(desc.opcode, ToCardWorkRbDescOpcode::WriteWithImm)
                    && matches!(desc.common.qp_type, QpType::RawPacket)
            }
            ToCardDescriptor::Read(_) => false,
        }
    }

    pub(crate) fn common(&self) -> &ToCardWorkRbDescCommon {
        match self {
            ToCardDescriptor::Write(desc) => &desc.common,
            ToCardDescriptor::Read(desc) => &desc.common,
        }
    }

    pub(crate) fn first_sge_mut(&mut self) -> &mut SGList {
        match self {
            ToCardDescriptor::Write(desc) => &mut desc.sg_list,
            ToCardDescriptor::Read(desc) => &mut desc.sge,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ToCardWriteDescriptor {
    pub(crate) opcode: ToCardWorkRbDescOpcode,
    pub(crate) common: ToCardWorkRbDescCommon,
    pub(crate) imm: Option<u32>,
    pub(crate) is_first: bool,
    pub(crate) is_last: bool,
    pub(crate) sg_list: SGList,
}

impl ToCardWriteDescriptor {
    pub(crate) fn write_only_opcode_with_imm(&self) -> (ToHostWorkRbDescOpcode, Option<u32>) {
        if self.is_first && self.is_last {
            // is_first = True and is_last = True, means only one packet
            match (self.is_resp(), self.has_imm()) {
                (true, _) => (ToHostWorkRbDescOpcode::RdmaReadResponseOnly, None),
                (false, true) => (ToHostWorkRbDescOpcode::RdmaWriteOnlyWithImmediate, self.imm),
                (false, false) => (ToHostWorkRbDescOpcode::RdmaWriteOnly, None),
            }
        } else if self.is_first {
            // self.is_last = False
            if self.is_resp() {
                (ToHostWorkRbDescOpcode::RdmaReadResponseFirst, None)
            } else {
                (ToHostWorkRbDescOpcode::RdmaWriteFirst, None)
            }
        } else {
            // self.is_last = True
            match (self.is_resp(), self.has_imm()) {
                (true, _) => (ToHostWorkRbDescOpcode::RdmaReadResponseLast, None),
                (false, true) => (ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate, self.imm),
                (false, false) => (ToHostWorkRbDescOpcode::RdmaWriteLast, None),
            }
        }
    }

    pub(crate) fn write_first_opcode(&self) -> ToHostWorkRbDescOpcode {
        match (self.is_first, self.is_resp()) {
            (true, true) => ToHostWorkRbDescOpcode::RdmaReadResponseFirst,
            (true, false) => ToHostWorkRbDescOpcode::RdmaWriteFirst,
            (false, true) => ToHostWorkRbDescOpcode::RdmaReadResponseMiddle,
            (false, false) => ToHostWorkRbDescOpcode::RdmaWriteMiddle,
        }
    }

    pub(crate) fn write_middle_opcode(&self) -> ToHostWorkRbDescOpcode {
        if self.is_resp() {
            ToHostWorkRbDescOpcode::RdmaReadResponseMiddle
        } else {
            ToHostWorkRbDescOpcode::RdmaWriteMiddle
        }
    }

    pub(crate) fn write_last_opcode_with_imm(&self) -> (ToHostWorkRbDescOpcode, Option<u32>) {
        match (self.is_last, self.is_resp(), self.has_imm()) {
            (true, true, _) => (ToHostWorkRbDescOpcode::RdmaReadResponseLast, None), // ignore read response last with imm
            (true, false, true) => (ToHostWorkRbDescOpcode::RdmaWriteLastWithImmediate, self.imm),
            (true, false, false) => (ToHostWorkRbDescOpcode::RdmaWriteLast, None),
            (false, true, _) => (ToHostWorkRbDescOpcode::RdmaReadResponseMiddle, None),
            (false, false, _) => (ToHostWorkRbDescOpcode::RdmaWriteMiddle, None),
        }
    }

    pub(crate) fn is_resp(&self) -> bool {
        matches!(self.opcode, ToCardWorkRbDescOpcode::ReadResp)
    }

    pub(crate) fn has_imm(&self) -> bool {
        self.imm.is_some()
    }
}

#[derive(Debug)]
pub(crate) struct ToCardReadDescriptor {
    pub(crate) common: ToCardWorkRbDescCommon,
    pub(crate) sge: SGList,
}

impl From<Box<ToCardWorkRbDesc>> for ToCardDescriptor {
    fn from(desc: Box<ToCardWorkRbDesc>) -> Self {
        match *desc {
            ToCardWorkRbDesc::Write(desc) => ToCardDescriptor::Write(ToCardWriteDescriptor {
                opcode: ToCardWorkRbDescOpcode::Write,
                common: desc.common,
                is_first: desc.is_first,
                is_last: desc.is_last,
                imm: None,
                sg_list: SGList::new_with_sge_list(desc.sge0, desc.sge1, desc.sge2, desc.sge3),
            }),
            ToCardWorkRbDesc::Read(desc) => ToCardDescriptor::Read(ToCardReadDescriptor {
                common: desc.common,
                sge: SGList::new_with_sge(desc.sge),
            }),
            ToCardWorkRbDesc::WriteWithImm(desc) => {
                ToCardDescriptor::Write(ToCardWriteDescriptor {
                    opcode: ToCardWorkRbDescOpcode::WriteWithImm,
                    common: desc.common,
                    is_first: desc.is_first,
                    is_last: desc.is_last,
                    imm: Some(desc.imm),
                    sg_list: SGList::new_with_sge_list(desc.sge0, desc.sge1, desc.sge2, desc.sge3),
                })
            }
            ToCardWorkRbDesc::ReadResp(desc) => ToCardDescriptor::Write(ToCardWriteDescriptor {
                opcode: ToCardWorkRbDescOpcode::ReadResp,
                common: desc.common,
                is_first: desc.is_first,
                is_last: desc.is_last,
                imm: None,
                sg_list: SGList::new_with_sge_list(desc.sge0, desc.sge1, desc.sge2, desc.sge3),
            }),
        }
    }
}

impl From<&QpType> for ToHostWorkRbDescTransType {
    fn from(value: &QpType) -> Self {
        match value {
            QpType::Rc | QpType::RawPacket => ToHostWorkRbDescTransType::Rc,
            QpType::Ud => ToHostWorkRbDescTransType::Ud,
            QpType::Uc => ToHostWorkRbDescTransType::Uc,
            QpType::XrcRecv | QpType::XrcSend => ToHostWorkRbDescTransType::Xrc,
        }
    }
}
