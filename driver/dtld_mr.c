// SPDX-License-Identifier: GPL-2.0 OR Linux-OpenIB
/*
 * Copyright (c) 2016 Mellanox Technologies Ltd. All rights reserved.
 * Copyright (c) 2015 System Fabric Works, Inc. All rights reserved.
 */

#include "dtld.h"
#include "dtld_loc.h"

#include "linux/xarray.h"

/* Return a random 8 bit key value that is
 * different than the last_key. Set last_key to -1
 * if this is the first key for an MR or MW
 */
u8 dtld_get_next_key(u32 last_key)
{
    u8 key;

    do {
        get_random_bytes(&key, 1);
    } while (key == last_key);

    return key;
}

#define IB_ACCESS_REMOTE                                                       \
    (IB_ACCESS_REMOTE_READ | IB_ACCESS_REMOTE_WRITE | IB_ACCESS_REMOTE_ATOMIC)

static int dtld_mr_init(struct dtld_mr *mr, struct ib_umem *umem, u64 length,
                        u64 iova, int access)
{
    unsigned int pgsz = ib_umem_find_best_pgsz(umem, SZ_2G - SZ_4K, iova);
    if (!pgsz)
        return -EINVAL;

    size_t pg_cnt = ib_umem_num_dma_blocks(umem, pgsz);

    u32 lkey = mr->elem.index << 8 | dtld_get_next_key(-1);
    u32 rkey = (access & IB_ACCESS_REMOTE) ? lkey : 0;

    /* set ibmr->l/rkey and also copy into private l/rkey
   * for user MRs these will always be the same
   * for cases where caller 'owns' the key portion
   * they may be different until REG_MR WQE is executed.
   */
    mr->lkey = mr->ibmr.lkey = lkey;
    mr->rkey = mr->ibmr.rkey = rkey;

    mr->type = mr->ibmr.type = IB_MR_TYPE_USER;

    mr->ibmr.iova = iova;
    mr->ibmr.length = length;
    mr->ibmr.page_size = pgsz;

    mr->state = DTLD_MR_STATE_INVALID;
    // mr->map_shift = ilog2(DTLD_BUF_PER_MAP);

    mr->access = access;
    mr->state = DTLD_MR_STATE_VALID;

    mr->umem = umem;

    struct xarray *page_table;

    page_table = kmalloc(sizeof(struct xarray), GFP_KERNEL);
    if (!page_table) {
        return -ENOMEM;
    }

    xa_init(page_table);

    for (size_t i = 0; i < pg_cnt; i++) {
        unsigned long pg_p = umem->address + i * pgsz;
        xa_store(page_table, virt_to_phys((void *)pg_p), (void *)pg_p,
                 GFP_KERNEL);
    }

    mr->page_table = page_table;

    return 0;
}

int dtld_mr_init_user(struct dtld_pd *pd, u64 start, u64 length, u64 iova,
                      int access, struct dtld_mr *mr)
{
    int err;
    struct ib_umem *umem = ib_umem_get(pd->ibpd.device, start, length, access);

    if (IS_ERR(umem)) {
        pr_warn("%s: Unable to pin memory region err = %d\n", __func__,
                (int)PTR_ERR(umem));
        err = PTR_ERR(umem);
        goto err_out;
    }

    mr->ibmr.pd = &pd->ibpd;

    err = dtld_mr_init(mr, umem, length, iova, access);

    if (err)
        goto err_release_umem;

    return 0;

err_release_umem:
    ib_umem_release(umem);
err_out:
    return err;
}

int dtld_dereg_mr(struct ib_mr *ibmr, struct ib_udata *udata)
{
    struct dtld_mr *mr = to_dtld_mr(ibmr);

    /* See IBA 10.6.7.2.6 */
    // if (atomic_read(&mr->num_mw) > 0)
    // 	return -EINVAL;

    dtld_put(mr);

    return 0;
}

void dtld_mr_cleanup(struct dtld_pool_elem *elem)
{
    struct dtld_mr *mr = container_of(elem, typeof(*mr), elem);

    dtld_put(dtld_mr_pd(mr));

    ib_umem_release(mr->umem);

    // if (mr->cur_map_set)
    // 	dtld_mr_free_map_set(mr->num_map, mr->cur_map_set);

    // if (mr->next_map_set)
    // 	dtld_mr_free_map_set(mr->num_map, mr->next_map_set);
}
