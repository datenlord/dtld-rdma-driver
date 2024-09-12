enum CsrIndex {
    BaseAddrLow = 0x0,
    BaseAddrHigh = 0x1,
    Head = 0x2,
    Tail = 0x3,
}

const fn generate_csr_addr(is_h2c: bool, queue_index: usize, reg_index: CsrIndex) -> usize {
    let mut a = if is_h2c { 1 } else { 0 };
    a <<= 3_i32;
    a |= queue_index & 0b111;
    a <<= 10_i32;
    a |= reg_index as usize & 0x3FF;
    a <<= 2_i32;
    a
}

pub(super) const CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW: usize =
    generate_csr_addr(true, 0, CsrIndex::BaseAddrLow);
pub(super) const CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH: usize =
    generate_csr_addr(true, 0, CsrIndex::BaseAddrHigh);
pub(super) const CSR_ADDR_CMD_REQ_QUEUE_HEAD: usize = generate_csr_addr(true, 0, CsrIndex::Head);
pub(super) const CSR_ADDR_CMD_REQ_QUEUE_TAIL: usize = generate_csr_addr(true, 0, CsrIndex::Tail);
pub(super) const CSR_ADDR_CMD_RESP_QUEUE_HEAD: usize = generate_csr_addr(false, 0, CsrIndex::Head);
pub(super) const CSR_ADDR_CMD_RESP_QUEUE_TAIL: usize = generate_csr_addr(false, 0, CsrIndex::Tail);
pub(super) const CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW: usize =
    generate_csr_addr(false, 0, CsrIndex::BaseAddrLow);
pub(super) const CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH: usize =
    generate_csr_addr(false, 0, CsrIndex::BaseAddrHigh);
pub(super) const CSR_ADDR_SEND_QUEUE_HEAD: usize = generate_csr_addr(true, 1, CsrIndex::Head);
pub(super) const CSR_ADDR_SEND_QUEUE_TAIL: usize = generate_csr_addr(true, 1, CsrIndex::Tail);
pub(super) const CSR_ADDR_SEND_QUEUE_ADDR_LOW: usize =
    generate_csr_addr(true, 1, CsrIndex::BaseAddrLow);
pub(super) const CSR_ADDR_SEND_QUEUE_ADDR_HIGH: usize =
    generate_csr_addr(true, 1, CsrIndex::BaseAddrHigh);
pub(super) const CSR_ADDR_META_REPORT_QUEUE_HEAD: usize =
    generate_csr_addr(false, 1, CsrIndex::Head);
pub(super) const CSR_ADDR_META_REPORT_QUEUE_TAIL: usize =
    generate_csr_addr(false, 1, CsrIndex::Tail);
pub(super) const CSR_ADDR_META_REPORT_QUEUE_ADDR_LOW: usize =
    generate_csr_addr(false, 1, CsrIndex::BaseAddrLow);
pub(super) const CSR_ADDR_META_REPORT_QUEUE_ADDR_HIGH: usize =
    generate_csr_addr(false, 1, CsrIndex::BaseAddrHigh);

pub(super) const RINGBUF_DEPTH: usize = 128;
pub(super) const RINGBUF_ELEM_SIZE: usize = 32;
pub(super) const RINGBUF_PAGE_SIZE: usize = 4096;
