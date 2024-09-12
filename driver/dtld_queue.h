/* SPDX-License-Identifier: GPL-2.0 OR Linux-OpenIB */
/*
 * Copyright (c) 2016 Mellanox Technologies Ltd. All rights reserved.
 * Copyright (c) 2015 System Fabric Works, Inc. All rights reserved.
 */

#ifndef DTLD_QUEUE_H
#define DTLD_QUEUE_H
#include <rdma/ib_verbs.h>
/* for definition of shared struct dtld_queue_buf */
#include "rdma/rdma_user_dtld.h"

/* Implements a simple circular buffer that is shared between user
 * and the driver and can be resized. The requested element size is
 * rounded up to a power of 2 and the number of elements in the buffer
 * is also rounded up to a power of 2. Since the queue is empty when
 * the producer and consumer indices match the maximum capacity of the
 * queue is one less than the number of element slots.
 *
 * Notes:
 *   - The driver indices are always masked off to q->index_mask
 *     before storing so do not need to be checked on reads.
 *   - The user whether user space or kernel is generally
 *     not trusted so its parameters are masked to make sure
 *     they do not access the queue out of bounds on reads.
 *   - The driver indices for queues must not be written
 *     by user so a local copy is used and a shared copy is
 *     stored when the local copy is changed.
 *   - By passing the type in the parameter list separate from q
 *     the compiler can eliminate the switch statement when the
 *     actual queue type is known when the function is called at
 *     compile time.
 *   - These queues are lock free. The user and driver must protect
 *     changes to their end of the queues with locks if more than one
 *     CPU can be accessing it at the same time.
 */

/**
 * enum queue_type - type of queue
 * @QUEUE_TYPE_TO_CLIENT:	Queue is written by dtld driver and
 *				read by client. Used by dtld driver only.
 * @QUEUE_TYPE_FROM_CLIENT:	Queue is written by client and
 *				read by dtld driver. Used by dtld driver only.
 * @QUEUE_TYPE_TO_DRIVER:	Queue is written by client and
 *				read by dtld driver. Used by kernel client only.
 * @QUEUE_TYPE_FROM_DRIVER:	Queue is written by dtld driver and
 *				read by client. Used by kernel client only.
 */
enum queue_type {
    QUEUE_TYPE_TO_CLIENT,
    QUEUE_TYPE_FROM_CLIENT,
    QUEUE_TYPE_TO_DRIVER,
    QUEUE_TYPE_FROM_DRIVER,
};

struct dtld_queue {
    struct dtld_dev *dtld;
    struct dtld_queue_buf *buf;
    struct dtld_mmap_info *ip;
    size_t buf_size;
    size_t elem_size;
    unsigned int log2_elem_size;
    u32 index_mask;
    enum queue_type type;
    /* private copy of index for shared queues between
   * kernel space and user space. Kernel reads and writes
   * this copy and then replicates to dtld_queue_buf
   * for read access by user space.
   */
    u32 index;
};

int do_mmap_info(struct dtld_dev *dtld, struct mminfo __user *outbuf,
                 struct ib_udata *udata, struct dtld_queue_buf *buf,
                 size_t buf_size, struct dtld_mmap_info **ip_p);

void dtld_queue_reset(struct dtld_queue *q);

struct dtld_queue *dtld_queue_init(struct dtld_dev *dtld, int *num_elem,
                                   unsigned int elem_size,
                                   enum queue_type type);

int dtld_queue_resize(struct dtld_queue *q, unsigned int *num_elem_p,
                      unsigned int elem_size, struct ib_udata *udata,
                      struct mminfo __user *outbuf, spinlock_t *producer_lock,
                      spinlock_t *consumer_lock);

void dtld_queue_cleanup(struct dtld_queue *queue);

static inline u32 queue_next_index(struct dtld_queue *q, int index)
{
    return (index + 1) & q->index_mask;
}

static inline u32 queue_get_producer(const struct dtld_queue *q,
                                     enum queue_type type)
{
    u32 prod;

    switch (type) {
    case QUEUE_TYPE_FROM_CLIENT:
        /* protect user index */
        prod = smp_load_acquire(&q->buf->producer_index);
        break;
    case QUEUE_TYPE_TO_CLIENT:
        prod = q->index;
        break;
    case QUEUE_TYPE_FROM_DRIVER:
        /* protect driver index */
        prod = smp_load_acquire(&q->buf->producer_index);
        break;
    case QUEUE_TYPE_TO_DRIVER:
        prod = q->buf->producer_index;
        break;
    }

    return prod;
}

static inline u32 queue_get_consumer(const struct dtld_queue *q,
                                     enum queue_type type)
{
    u32 cons;

    switch (type) {
    case QUEUE_TYPE_FROM_CLIENT:
        cons = q->index;
        break;
    case QUEUE_TYPE_TO_CLIENT:
        /* protect user index */
        cons = smp_load_acquire(&q->buf->consumer_index);
        break;
    case QUEUE_TYPE_FROM_DRIVER:
        cons = q->buf->consumer_index;
        break;
    case QUEUE_TYPE_TO_DRIVER:
        /* protect driver index */
        cons = smp_load_acquire(&q->buf->consumer_index);
        break;
    }

    return cons;
}

static inline int queue_empty(struct dtld_queue *q, enum queue_type type)
{
    u32 prod = queue_get_producer(q, type);
    u32 cons = queue_get_consumer(q, type);

    return ((prod - cons) & q->index_mask) == 0;
}

static inline int queue_full(struct dtld_queue *q, enum queue_type type)
{
    u32 prod = queue_get_producer(q, type);
    u32 cons = queue_get_consumer(q, type);

    return ((prod + 1 - cons) & q->index_mask) == 0;
}

static inline u32 queue_count(const struct dtld_queue *q, enum queue_type type)
{
    u32 prod = queue_get_producer(q, type);
    u32 cons = queue_get_consumer(q, type);

    return (prod - cons) & q->index_mask;
}

static inline void queue_advance_producer(struct dtld_queue *q,
                                          enum queue_type type)
{
    u32 prod;

    switch (type) {
    case QUEUE_TYPE_FROM_CLIENT:
        pr_warn("%s: attempt to advance client index\n", __func__);
        break;
    case QUEUE_TYPE_TO_CLIENT:
        prod = q->index;
        prod = (prod + 1) & q->index_mask;
        q->index = prod;
        /* protect user index */
        smp_store_release(&q->buf->producer_index, prod);
        break;
    case QUEUE_TYPE_FROM_DRIVER:
        pr_warn("%s: attempt to advance driver index\n", __func__);
        break;
    case QUEUE_TYPE_TO_DRIVER:
        prod = q->buf->producer_index;
        prod = (prod + 1) & q->index_mask;
        q->buf->producer_index = prod;
        break;
    }
}

static inline void queue_advance_consumer(struct dtld_queue *q,
                                          enum queue_type type)
{
    u32 cons;

    switch (type) {
    case QUEUE_TYPE_FROM_CLIENT:
        cons = q->index;
        cons = (cons + 1) & q->index_mask;
        q->index = cons;
        /* protect user index */
        smp_store_release(&q->buf->consumer_index, cons);
        break;
    case QUEUE_TYPE_TO_CLIENT:
        pr_warn("%s: attempt to advance client index\n", __func__);
        break;
    case QUEUE_TYPE_FROM_DRIVER:
        cons = q->buf->consumer_index;
        cons = (cons + 1) & q->index_mask;
        q->buf->consumer_index = cons;
        break;
    case QUEUE_TYPE_TO_DRIVER:
        pr_warn("%s: attempt to advance driver index\n", __func__);
        break;
    }
}

static inline void *queue_producer_addr(struct dtld_queue *q,
                                        enum queue_type type)
{
    u32 prod = queue_get_producer(q, type);

    return q->buf->data + (prod << q->log2_elem_size);
}

static inline void *queue_consumer_addr(struct dtld_queue *q,
                                        enum queue_type type)
{
    u32 cons = queue_get_consumer(q, type);

    return q->buf->data + (cons << q->log2_elem_size);
}

static inline void *queue_addr_from_index(struct dtld_queue *q, u32 index)
{
    return q->buf->data + ((index & q->index_mask) << q->log2_elem_size);
}

static inline u32 queue_index_from_addr(const struct dtld_queue *q,
                                        const void *addr)
{
    return (((u8 *)addr - q->buf->data) >> q->log2_elem_size) & q->index_mask;
}

static inline void *queue_head(struct dtld_queue *q, enum queue_type type)
{
    return queue_empty(q, type) ? NULL : queue_consumer_addr(q, type);
}

#endif /* DTLD_QUEUE_H */
