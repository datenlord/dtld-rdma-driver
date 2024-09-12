/* SPDX-License-Identifier: GPL-2.0 OR Linux-OpenIB */
/*
 * Copyright (c) 2016 Mellanox Technologies Ltd. All rights reserved.
 * Copyright (c) 2015 System Fabric Works, Inc. All rights reserved.
 */

#ifndef DTLD_POOL_H
#define DTLD_POOL_H

#include "linux/kref.h"
#include "linux/xarray.h"
enum dtld_elem_type {
    DTLD_TYPE_UC,
    DTLD_TYPE_PD,
    DTLD_TYPE_AH,
    DTLD_TYPE_SRQ,
    DTLD_TYPE_QP,
    DTLD_TYPE_CQ,
    DTLD_TYPE_MR,
    DTLD_TYPE_MW,
    DTLD_NUM_TYPES, /* keep me last */
};

struct dtld_pool_elem {
    struct dtld_pool *pool;
    void *obj;
    struct kref ref_cnt;
    struct list_head list;
    u32 index;
};

struct dtld_pool {
    struct dtld_dev *dtld;
    const char *name;
    void (*cleanup)(struct dtld_pool_elem *elem);
    enum dtld_elem_type type;

    unsigned int max_elem;
    atomic_t num_elem;
    size_t elem_size;
    size_t elem_offset;

    struct xarray xa;
    struct xa_limit limit;
    u32 next;
};

/* initialize a pool of objects with given limit on
 * number of elements. gets parameters from dtld_type_info
 * pool elements will be allocated out of a slab cache
 */
void dtld_pool_init(struct dtld_dev *dtld, struct dtld_pool *pool,
                    enum dtld_elem_type type);

/* free resources from object pool */
void dtld_pool_cleanup(struct dtld_pool *pool);

/* allocate an object from pool */
void *dtld_alloc(struct dtld_pool *pool);

/* connect already allocated object to pool */
int __dtld_add_to_pool(struct dtld_pool *pool, struct dtld_pool_elem *elem);

#define dtld_add_to_pool(pool, obj) __dtld_add_to_pool(pool, &(obj)->elem)

/* lookup an indexed object from index. takes a reference on object */
void *dtld_pool_get_index(struct dtld_pool *pool, u32 index);

int __dtld_get(struct dtld_pool_elem *elem);

#define dtld_get(obj) __dtld_get(&(obj)->elem)

int __dtld_put(struct dtld_pool_elem *elem);

#define dtld_put(obj) __dtld_put(&(obj)->elem)

#define dtld_read(obj) kref_read(&(obj)->elem.ref_cnt)

#endif /* DTLD_POOL_H */
