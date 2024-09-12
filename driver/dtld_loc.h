/* SPDX-License-Identifier: GPL-2.0 OR Linux-OpenIB */
/*
 * Copyright (c) 2016 Mellanox Technologies Ltd. All rights reserved.
 * Copyright (c) 2015 System Fabric Works, Inc. All rights reserved.
 */

#ifndef DTLD_LOC_H
#define DTLD_LOC_H

/* dtld_av.c */
// void dtld_init_av(struct rdma_ah_attr *attr, struct dtld_av *av);

// int dtld_av_chk_attr(struct dtld_dev *dtld, struct rdma_ah_attr *attr);

// void dtld_av_from_attr(u8 port_num, struct dtld_av *av,
// 		     struct rdma_ah_attr *attr);

// void dtld_av_to_attr(struct dtld_av *av, struct rdma_ah_attr *attr);

// void dtld_av_fill_ip_info(struct dtld_av *av, struct rdma_ah_attr *attr);

// struct dtld_av *dtld_get_av(struct dtld_pkt_info *pkt, struct dtld_ah **ahp);

/* dtld_cq.c */
int dtld_cq_chk_attr(struct dtld_dev *dtld, struct dtld_cq *cq, int cqe,
                     int comp_vector);

int dtld_cq_from_init(struct dtld_dev *dtld, struct dtld_cq *cq, int cqe,
                      int comp_vector, struct ib_udata *udata,
                      struct dtld_uresp_create_cq *uresp);

// int dtld_cq_resize_queue(struct dtld_cq *cq, int new_cqe,
// 			struct dtld_resize_cq_resp __user *uresp,
// 			struct ib_udata *udata);

// int dtld_cq_post(struct dtld_cq *cq, struct dtld_cqe *cqe, int solicited);

void dtld_cq_disable(struct dtld_cq *cq);

void dtld_cq_cleanup(struct dtld_pool_elem *elem);

// /* dtld_mcast.c */
// struct dtld_mcg *dtld_lookup_mcg(struct dtld_dev *dtld, union ib_gid *mgid);
// int dtld_attach_mcast(struct ib_qp *ibqp, union ib_gid *mgid, u16 mlid);
// int dtld_detach_mcast(struct ib_qp *ibqp, union ib_gid *mgid, u16 mlid);
// void dtld_cleanup_mcg(struct kref *kref);

/* dtld_mmap.c */
struct dtld_mmap_info {
    struct list_head pending_mmaps;
    struct ib_ucontext *context;
    struct kref ref;
    void *obj;

    struct mminfo info;
};

// void dtld_mmap_release(struct kref *ref);

// struct dtld_mmap_info *dtld_create_mmap_info(struct dtld_dev *dev, u32 size,
// 					   struct ib_udata *udata, void *obj);

// /* dtld_mr.c */
// u8 dtld_get_next_key(u32 last_key);
// void dtld_mr_init_dma(struct dtld_pd *pd, int access, struct dtld_mr *mr);
int dtld_mr_init_user(struct dtld_pd *pd, u64 start, u64 length, u64 iova,
                      int access, struct dtld_mr *mr);
// int dtld_mr_init_fast(struct dtld_pd *pd, int max_pages, struct dtld_mr *mr);
// int dtld_mr_copy(struct dtld_mr *mr, u64 iova, void *addr, int length,
// 		enum dtld_mr_copy_dir dir);
// int copy_data(struct dtld_pd *pd, int access, struct dtld_dma_info *dma,
// 	      void *addr, int length, enum dtld_mr_copy_dir dir);
// void *iova_to_vaddr(struct dtld_mr *mr, u64 iova, int length);
// struct dtld_mr *lookup_mr(struct dtld_pd *pd, int access, u32 key,
// 			 enum dtld_mr_lookup_type type);
// int mr_check_range(struct dtld_mr *mr, u64 iova, size_t length);
// int advance_dma_data(struct dtld_dma_info *dma, unsigned int length);
// int dtld_invalidate_mr(struct dtld_qp *qp, u32 rkey);
// int dtld_reg_fast_mr(struct dtld_qp *qp, struct dtld_send_wqe *wqe);
// int dtld_mr_set_page(struct ib_mr *ibmr, u64 addr);
int dtld_dereg_mr(struct ib_mr *ibmr, struct ib_udata *udata);
void dtld_mr_cleanup(struct dtld_pool_elem *elem);

// /* dtld_mw.c */
// int dtld_alloc_mw(struct ib_mw *ibmw, struct ib_udata *udata);
// int dtld_dealloc_mw(struct ib_mw *ibmw);
// int dtld_bind_mw(struct dtld_qp *qp, struct dtld_send_wqe *wqe);
// int dtld_invalidate_mw(struct dtld_qp *qp, u32 rkey);
// struct dtld_mw *dtld_lookup_mw(struct dtld_qp *qp, int access, u32 rkey);
// void dtld_mw_cleanup(struct dtld_pool_elem *elem);

// /* dtld_net.c */
// struct sk_buff *dtld_init_packet(struct dtld_dev *dtld, struct dtld_av *av,
// 				int paylen, struct dtld_pkt_info *pkt);
// int dtld_prepare(struct dtld_av *av, struct dtld_pkt_info *pkt,
// 		struct sk_buff *skb);
// int dtld_xmit_packet(struct dtld_qp *qp, struct dtld_pkt_info *pkt,
// 		    struct sk_buff *skb);
// const char *dtld_parent_name(struct dtld_dev *dtld, unsigned int port_num);

/* dtld_qp.c */
int dtld_qp_chk_init(struct dtld_dev *dtld, struct ib_qp_init_attr *init);
int dtld_qp_from_init(struct dtld_dev *dtld, struct dtld_qp *qp,
                      struct dtld_pd *pd, struct ib_pd *ibpd,
                      struct ib_qp_init_attr *init, struct ib_udata *udata,
                      struct dtld_uresp_create_qp *uresp);
int dtld_qp_to_init(struct dtld_qp *qp, struct ib_qp_init_attr *init);
int dtld_qp_chk_attr(struct dtld_dev *dtld, struct dtld_qp *qp,
                     struct ib_qp_attr *attr, int mask);
int dtld_qp_from_attr(struct dtld_qp *qp, struct ib_qp_attr *attr, int mask,
                      struct ib_udata *udata);
int dtld_qp_to_attr(struct dtld_qp *qp, struct ib_qp_attr *attr, int mask);
// void dtld_qp_error(struct dtld_qp *qp);
int dtld_qp_chk_destroy(struct dtld_qp *qp);
void dtld_qp_cleanup(struct dtld_pool_elem *elem);

static inline int qp_num(struct dtld_qp *qp)
{
    return qp->ibqp.qp_num;
}

static inline enum ib_qp_type qp_type(struct dtld_qp *qp)
{
    return qp->ibqp.qp_type;
}

static inline enum ib_qp_state qp_state(struct dtld_qp *qp)
{
    return qp->attr.qp_state;
}

// static inline int qp_mtu(struct dtld_qp *qp)
// {
// 	if (qp->ibqp.qp_type == IB_QPT_RC || qp->ibqp.qp_type == IB_QPT_UC)
// 		return qp->attr.path_mtu;
// 	else
// 		return IB_MTU_4096;
// }

static inline int rcv_wqe_size(int max_sge)
{
    return sizeof(struct dtld_recv_wqe) + max_sge * sizeof(struct ib_sge);
}

void free_rd_atomic_resource(struct dtld_qp *qp, struct resp_res *res);

// static inline void dtld_advance_resp_resource(struct dtld_qp *qp)
// {
// 	qp->resp.res_head++;
// 	if (unlikely(qp->resp.res_head == qp->attr.max_dest_rd_atomic))
// 		qp->resp.res_head = 0;
// }

void retransmit_timer(struct timer_list *t);
void rnr_nak_timer(struct timer_list *t);

// /* dtld_srq.c */
// int dtld_srq_chk_init(struct dtld_dev *dtld, struct ib_srq_init_attr *init);
// int dtld_srq_from_init(struct dtld_dev *dtld, struct dtld_srq *srq,
// 		      struct ib_srq_init_attr *init, struct ib_udata *udata,
// 		      struct dtld_create_srq_resp __user *uresp);
// int dtld_srq_chk_attr(struct dtld_dev *dtld, struct dtld_srq *srq,
// 		     struct ib_srq_attr *attr, enum ib_srq_attr_mask mask);
// int dtld_srq_from_attr(struct dtld_dev *dtld, struct dtld_srq *srq,
// 		      struct ib_srq_attr *attr, enum ib_srq_attr_mask mask,
// 		      struct dtld_modify_srq_cmd *ucmd, struct ib_udata *udata);
// void dtld_srq_cleanup(struct dtld_pool_elem *elem);

// void dtld_dealloc(struct ib_device *ib_dev);

// int dtld_completer(void *arg); // TODO: delete me
// int dtld_requester(void *arg); // TODO: delete me
// int dtld_responder(void *arg); // TODO: delete me

// /* dtld_icrc.c */
// int dtld_icrc_init(struct dtld_dev *dtld);
// int dtld_icrc_check(struct sk_buff *skb, struct dtld_pkt_info *pkt);
// void dtld_icrc_generate(struct sk_buff *skb, struct dtld_pkt_info *pkt);

// void dtld_resp_queue_pkt(struct dtld_qp *qp, struct sk_buff *skb);

// void dtld_comp_queue_pkt(struct dtld_qp *qp, struct sk_buff *skb);

#endif /* DTLD_LOC_H */
