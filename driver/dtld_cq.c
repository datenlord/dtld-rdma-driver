// SPDX-License-Identifier: GPL-2.0 OR Linux-OpenIB
/*
 * Copyright (c) 2016 Mellanox Technologies Ltd. All rights reserved.
 * Copyright (c) 2015 System Fabric Works, Inc. All rights reserved.
 */
#include "dtld.h"
#include "dtld_loc.h"
#include "dtld_queue.h"
#include "rdma/ib_verbs.h"
#include <linux/pci.h>
#include <linux/vmalloc.h>
#include <rdma/uverbs_ioctl.h>

int dtld_cq_chk_attr(struct dtld_dev *dtld, struct dtld_cq *cq, int cqe,
                     int comp_vector)
{
    int count;

    if (cqe <= 0) {
        pr_warn("cqe(%d) <= 0\n", cqe);
        goto err1;
    }

    if (cqe > dtld->attr.max_cqe) {
        pr_warn("cqe(%d) > max_cqe(%d)\n", cqe, dtld->attr.max_cqe);
        goto err1;
    }

    if (cq) {
        count = queue_count(cq->queue, QUEUE_TYPE_TO_CLIENT);
        if (cqe < count) {
            pr_warn("cqe(%d) < current # elements in queue (%d)", cqe, count);
            goto err1;
        }
    }

    return 0;

err1:
    return -EINVAL;
}

int dtld_cq_from_init(struct dtld_dev *dtld, struct dtld_cq *cq, int cqe,
                      int comp_vector, struct ib_udata *udata,
                      struct dtld_uresp_create_cq *uresp)
{
    int err;
    struct dtld_ucontext *ctx =
            rdma_udata_to_drv_context(udata, struct dtld_ucontext, ibuc);
    struct dtld_rdma_user_mmap_entry *ummap_ent;
    u64 mmap_offset;

    ummap_ent = kzalloc(sizeof(*ummap_ent), GFP_KERNEL);
    if (!ummap_ent)
        return -ENOMEM;
    cq->ummap_ent = ummap_ent;

    ummap_ent->address =
            pci_resource_start(dtld->xdev->pdev, RDMA_CONFIG_BAR_IDX);

    err = rdma_user_mmap_entry_insert(&ctx->ibuc, &ummap_ent->rdma_entry,
                                      PAGE_SIZE);
    if (err) {
        kfree(ummap_ent);
        return err;
    }

    mmap_offset = rdma_user_mmap_get_offset(&ummap_ent->rdma_entry);

    // TODO: the hardcoded is only for test, fix me
    uresp->q_offset = mmap_offset;
    uresp->q_length = PAGE_SIZE;
    uresp->num_cqe = 2;
    uresp->cq_id = cq->elem.index;

    spin_lock_init(&cq->cq_lock);
    cq->ibcq.cqe = cqe;
    return 0;
}

void dtld_cq_disable(struct dtld_cq *cq)
{
    unsigned long flags;

    spin_lock_irqsave(&cq->cq_lock, flags);
    // TODO do something here
    spin_unlock_irqrestore(&cq->cq_lock, flags);
}

void dtld_cq_cleanup(struct dtld_pool_elem *elem)
{
    struct dtld_cq *cq = container_of(elem, typeof(*cq), elem);

    if (cq->queue)
        dtld_queue_cleanup(cq->queue);
}
