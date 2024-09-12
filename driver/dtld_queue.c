// SPDX-License-Identifier: GPL-2.0 OR Linux-OpenIB
/*
 * Copyright (c) 2016 Mellanox Technologies Ltd. All rights reserved.
 * Copyright (c) 2015 System Fabric Works, Inc. All rights reserved.
 */

#include "dtld_queue.h"
#include "dtld.h"
#include "dtld_loc.h"
#include <linux/vmalloc.h>

inline void dtld_queue_reset(struct dtld_queue *q)
{
    // TODO: should communicate with HW and reset the queue
}

struct dtld_queue *dtld_queue_init(struct dtld_dev *dtld, int *num_elem,
                                   unsigned int elem_size, enum queue_type type)
{
    // TODO: should communicate with HW and create the queue
    return NULL;
}

void dtld_queue_cleanup(struct dtld_queue *q)
{
    // TODO: should communicate with HW and cleanup the queue
    kfree(q);
}
