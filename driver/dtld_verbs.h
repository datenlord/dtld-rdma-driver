#ifndef DTLD_VERBS_H
#define DTLD_VERBS_H

#include "dtld_pool.h"
#include "dtld_queue.h"
#include "linux/types.h"
#include "rdma/ib_verbs.h"
#include "dtld_param.h"
#include "libxdma.h"

#include "linux/xarray.h"
struct dtld_port {
    struct ib_port_attr attr;
    __be64 port_guid;
    __be64 subnet_prefix;
    spinlock_t port_lock; /* guard port */
    unsigned int mtu_cap;
};

struct dtld_dev {
    struct ib_device ib_dev;
    struct ib_device_attr attr;
    struct dtld_port port;

    int max_inline_data;

    struct xdma_dev *xdev;
    u8* csr;
    resource_size_t csr_addr;
    resource_size_t csr_length;

    dma_addr_t cmdq_sq;
    void * cmdq_sq_buf;
    dma_addr_t cmdq_rq;
    void * cmdq_rq_buf;
    dma_addr_t workq_sq;
    void * workq_sq_buf;
    dma_addr_t workq_rq;
    void * workq_rq_buf;

};

enum dtld_qp_state {
    QP_STATE_RESET,
    QP_STATE_INIT,
    QP_STATE_READY,
    QP_STATE_DRAIN, /* req only */
    QP_STATE_DRAINED, /* req only */
    QP_STATE_ERROR
};

struct dtld_req_info {
    enum dtld_qp_state state;
};

struct dtld_resp_info {
    enum dtld_qp_state state;
    enum ib_wc_status status;
};

struct dtld_ucontext {
    struct ib_ucontext ibuc;
    struct dtld_pool_elem elem;
    struct rdma_user_mmap_entry* csr_entry;
    struct rdma_user_mmap_entry* cmdq_sq_entry;
    struct rdma_user_mmap_entry* cmdq_rq_entry;
    struct rdma_user_mmap_entry* workq_sq_entry;
    struct rdma_user_mmap_entry* workq_rq_entry;
};

struct dtld_pd {
    struct ib_pd ibpd;
    struct dtld_pool_elem elem;
};

struct dtld_ah {
    struct ib_ah ibah;
    struct dtld_pool_elem elem;
    struct dtld_av av;
    // bool			is_user;    // TODO delete me
    int ah_num;
};

enum wqe_state {
    wqe_state_posted,
    wqe_state_processing,
    wqe_state_pending,
    wqe_state_done,
    wqe_state_error,
};

struct dtld_rdma_user_mmap_entry{
	struct rdma_user_mmap_entry rdma_entry;
    u64 address;
    u32 length;
    u8 mmap_flag;
};

struct dtld_cqe {
    union {
        struct ib_wc ibwc;
        struct ib_uverbs_wc uibwc;
    };
};

struct dtld_cq {
    struct ib_cq ibcq;
    struct dtld_pool_elem elem;
    struct dtld_rdma_user_mmap_entry *ummap_ent;

    // TODO try if we can remove the following field
    struct dtld_queue *queue;
    spinlock_t cq_lock;
    u8 notify;
    struct tasklet_struct comp_task;
    atomic_t num_wq;
};

struct dtld_sq {
    int max_wr;
    int max_sge;
    int max_inline;

    struct dtld_rdma_user_mmap_entry *ummap_ent;

    spinlock_t sq_lock; /* guard queue */
    struct dtld_queue *queue;
};

struct dtld_rq {
    int max_wr;
    int max_sge;

    struct dtld_rdma_user_mmap_entry *ummap_ent;

    spinlock_t producer_lock; /* guard queue producer */
    spinlock_t consumer_lock; /* guard queue consumer */
    struct dtld_queue *queue;
};

struct dtld_srq {
    struct ib_srq ibsrq;
    struct dtld_pool_elem elem;
    struct dtld_pd *pd;
    struct dtld_rq rq;
    u32 srq_num;

    int limit;
    int error;
};

struct dtld_qp {
    struct ib_qp ibqp;
    struct dtld_pool_elem elem;
    struct ib_qp_attr attr;
    unsigned int valid;
    unsigned int mtu;

    struct dtld_pd *pd;
    struct dtld_srq *srq;
    struct dtld_cq *scq;
    struct dtld_cq *rcq;

    enum ib_sig_type sq_sig_type;

    struct dtld_sq sq;
    struct dtld_rq rq;

    struct dtld_req_info req;
    struct dtld_resp_info resp;

    atomic_t ssn;
};

#define DTLD_BUF_PER_MAP (PAGE_SIZE / sizeof(struct dtld_phys_buf))

struct dtld_phys_buf {
    u64 addr;
    u64 size;
};

struct dtld_map {
    struct dtld_phys_buf buf[DTLD_BUF_PER_MAP];
};

enum dtld_mr_state {
    DTLD_MR_STATE_INVALID,
    DTLD_MR_STATE_FREE,
    DTLD_MR_STATE_VALID,
};

struct dtld_mr {
    struct dtld_pool_elem elem;
    struct ib_mr ibmr;

    struct ib_umem *umem;

    u32 lkey;
    u32 rkey;
    enum dtld_mr_state state;
    enum ib_mr_type type;
    int access;

    struct xarray *page_table;
};

static inline struct dtld_ucontext *to_dtld_uc(struct ib_ucontext *uc)
{
    return uc ? container_of(uc, struct dtld_ucontext, ibuc) : NULL;
}

static inline struct dtld_ah *to_dtld_ah(struct ib_ah *ah)
{
    return ah ? container_of(ah, struct dtld_ah, ibah) : NULL;
}

static inline struct dtld_qp *to_dtld_qp(struct ib_qp *qp)
{
    return qp ? container_of(qp, struct dtld_qp, ibqp) : NULL;
}

static inline struct dtld_srq *to_dtld_srq(struct ib_srq *srq)
{
    return srq ? container_of(srq, struct dtld_srq, ibsrq) : NULL;
}

static inline struct dtld_cq *to_dtld_cq(struct ib_cq *cq)
{
    return cq ? container_of(cq, struct dtld_cq, ibcq) : NULL;
}

static inline struct dtld_mr *to_dtld_mr(struct ib_mr *mr)
{
    return mr ? container_of(mr, struct dtld_mr, ibmr) : NULL;
}

static inline struct dtld_pd *to_dtld_pd(struct ib_pd *pd)
{
    return pd ? container_of(pd, struct dtld_pd, ibpd) : NULL;
}

static inline struct dtld_rdma_user_mmap_entry *
to_dtld_mmap_entry(struct rdma_user_mmap_entry *ent)
{
    return ent ? container_of(ent, struct dtld_rdma_user_mmap_entry,
                              rdma_entry) :
                 NULL;
}

static inline struct dtld_dev *dtld_from_ibdev(struct ib_device *dev)
{
    return dev ? container_of(dev, struct dtld_dev, ib_dev) : NULL;
}

static inline struct dtld_pd *dtld_mr_pd(struct dtld_mr *mr)
{
    return to_dtld_pd(mr->ibmr.pd);
}

static inline struct dtld_pd *dtld_ah_pd(struct dtld_ah *ah)
{
    return to_dtld_pd(ah->ibah.pd);
}

int dtld_register_device(struct dtld_dev *dtld, const char *ibdev_name);
void dtld_unregister_device(struct dtld_dev *dtld);

#endif