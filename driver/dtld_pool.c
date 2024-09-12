// SPDX-License-Identifier: GPL-2.0 OR Linux-OpenIB
/*
 * Copyright (c) 2016 Mellanox Technologies Ltd. All rights reserved.
 * Copyright (c) 2015 System Fabric Works, Inc. All rights reserved.
 */

#include "dtld.h"

#define DTLD_POOL_ALIGN (16)

static const struct dtld_type_info {
    const char *name;
    size_t size;
    size_t elem_offset;
    void (*cleanup)(struct dtld_pool_elem *elem);
    u32 min_index;
    u32 max_index;
    u32 max_elem;
} dtld_type_info[DTLD_NUM_TYPES] = {
    [DTLD_TYPE_UC] =
        {
            .name = "uc",
            .size = sizeof(struct dtld_ucontext),
            .elem_offset = offsetof(struct dtld_ucontext, elem),
            .min_index = 1,
            .max_index = UINT_MAX,
            .max_elem = UINT_MAX,
        },
    [DTLD_TYPE_PD] =
        {
            .name = "pd",
            .size = sizeof(struct dtld_pd),
            .elem_offset = offsetof(struct dtld_pd, elem),
            .min_index = 1,
            .max_index = UINT_MAX,
            .max_elem = UINT_MAX,
        },
    [DTLD_TYPE_AH] =
        {
            .name = "ah",
            .size = sizeof(struct dtld_ah),
            .elem_offset = offsetof(struct dtld_ah, elem),
            .min_index = DTLD_MIN_AH_INDEX,
            .max_index = DTLD_MAX_AH_INDEX,
            .max_elem = DTLD_MAX_AH_INDEX - DTLD_MIN_AH_INDEX + 1,
        },
    // [DTLD_TYPE_SRQ] = {
    // 	.name		= "srq",
    // 	.size		= sizeof(struct dtld_srq),
    // 	.elem_offset	= offsetof(struct dtld_srq, elem),
    // 	.cleanup	= dtld_srq_cleanup,
    // 	.min_index	= DTLD_MIN_SRQ_INDEX,
    // 	.max_index	= DTLD_MAX_SRQ_INDEX,
    // 	.max_elem	= DTLD_MAX_SRQ_INDEX - DTLD_MIN_SRQ_INDEX + 1,
    // },
    [DTLD_TYPE_QP] =
        {
            .name = "qp",
            .size = sizeof(struct dtld_qp),
            .elem_offset = offsetof(struct dtld_qp, elem),
            .cleanup = dtld_qp_cleanup,
            .min_index = DTLD_MIN_QP_INDEX,
            .max_index = DTLD_MAX_QP_INDEX,
            .max_elem = DTLD_MAX_QP_INDEX - DTLD_MIN_QP_INDEX + 1,
        },
    [DTLD_TYPE_CQ] =
        {
            .name = "cq",
            .size = sizeof(struct dtld_cq),
            .elem_offset = offsetof(struct dtld_cq, elem),
            .cleanup = dtld_cq_cleanup,
            .min_index = 1,
            .max_index = UINT_MAX,
            .max_elem = UINT_MAX,
        },
    [DTLD_TYPE_MR] =
        {
            .name = "mr",
            .size = sizeof(struct dtld_mr),
            .elem_offset = offsetof(struct dtld_mr, elem),
            .cleanup = dtld_mr_cleanup,
            .min_index = DTLD_MIN_MR_INDEX,
            .max_index = DTLD_MAX_MR_INDEX,
            .max_elem = DTLD_MAX_MR_INDEX - DTLD_MIN_MR_INDEX + 1,
        },
    // [DTLD_TYPE_MW] = {
    // 	.name		= "mw",
    // 	.size		= sizeof(struct dtld_mw),
    // 	.elem_offset	= offsetof(struct dtld_mw, elem),
    // 	.cleanup	= dtld_mw_cleanup,
    // 	.min_index	= DTLD_MIN_MW_INDEX,
    // 	.max_index	= DTLD_MAX_MW_INDEX,
    // 	.max_elem	= DTLD_MAX_MW_INDEX - DTLD_MIN_MW_INDEX + 1,
    // },
};

void dtld_pool_init(struct dtld_dev *dtld, struct dtld_pool *pool,
                    enum dtld_elem_type type)
{
    const struct dtld_type_info *info = &dtld_type_info[type];

    memset(pool, 0, sizeof(*pool));

    pool->dtld = dtld;
    pool->name = info->name;
    pool->type = type;
    pool->max_elem = info->max_elem;
    pool->elem_size = ALIGN(info->size, DTLD_POOL_ALIGN);
    pool->elem_offset = info->elem_offset;
    pool->cleanup = info->cleanup;

    atomic_set(&pool->num_elem, 0);

    xa_init_flags(&pool->xa, XA_FLAGS_ALLOC);
    pool->limit.min = info->min_index;
    pool->limit.max = info->max_index;
}

void dtld_pool_cleanup(struct dtld_pool *pool)
{
    WARN_ON(!xa_empty(&pool->xa));
}

void *dtld_alloc(struct dtld_pool *pool)
{
    struct dtld_pool_elem *elem;
    void *obj;
    int err;

    if (WARN_ON(!(pool->type == DTLD_TYPE_MR)))
        return NULL;

    if (atomic_inc_return(&pool->num_elem) > pool->max_elem)
        goto err_cnt;

    obj = kzalloc(pool->elem_size, GFP_KERNEL);
    if (!obj)
        goto err_cnt;

    elem = (struct dtld_pool_elem *)((u8 *)obj + pool->elem_offset);

    elem->pool = pool;
    elem->obj = obj;
    kref_init(&elem->ref_cnt);

    err = xa_alloc_cyclic(&pool->xa, &elem->index, elem, pool->limit,
                          &pool->next, GFP_KERNEL);
    if (err)
        goto err_free;

    return obj;

err_free:
    kfree(obj);
err_cnt:
    atomic_dec(&pool->num_elem);
    return NULL;
}

int __dtld_add_to_pool(struct dtld_pool *pool, struct dtld_pool_elem *elem)
{
    int err;

    if (WARN_ON(pool->type == DTLD_TYPE_MR))
        return -EINVAL;

    if (atomic_inc_return(&pool->num_elem) > pool->max_elem)
        goto err_cnt;

    elem->pool = pool;
    elem->obj = (u8 *)elem - pool->elem_offset;
    kref_init(&elem->ref_cnt);

    err = xa_alloc_cyclic(&pool->xa, &elem->index, elem, pool->limit,
                          &pool->next, GFP_KERNEL);
    if (err)
        goto err_cnt;

    return 0;

err_cnt:
    atomic_dec(&pool->num_elem);
    return -EINVAL;
}

void *dtld_pool_get_index(struct dtld_pool *pool, u32 index)
{
    struct dtld_pool_elem *elem;
    struct xarray *xa = &pool->xa;
    unsigned long flags;
    void *obj;

    xa_lock_irqsave(xa, flags);
    elem = xa_load(xa, index);
    if (elem && kref_get_unless_zero(&elem->ref_cnt))
        obj = elem->obj;
    else
        obj = NULL;
    xa_unlock_irqrestore(xa, flags);

    return obj;
}

static void dtld_elem_release(struct kref *kref)
{
    struct dtld_pool_elem *elem = container_of(kref, typeof(*elem), ref_cnt);
    struct dtld_pool *pool = elem->pool;

    xa_erase(&pool->xa, elem->index);

    if (pool->cleanup)
        pool->cleanup(elem);

    if (pool->type == DTLD_TYPE_MR)
        kfree(elem->obj);

    atomic_dec(&pool->num_elem);
}

int __dtld_get(struct dtld_pool_elem *elem)
{
    return kref_get_unless_zero(&elem->ref_cnt);
}

int __dtld_put(struct dtld_pool_elem *elem)
{
    return kref_put(&elem->ref_cnt, dtld_elem_release);
}
