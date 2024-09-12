/* SPDX-License-Identifier: GPL-2.0 OR Linux-OpenIB */
/*
 * Copyright (c) 2016 Mellanox Technologies Ltd. All rights reserved.
 * Copyright (c) 2015 System Fabric Works, Inc. All rights reserved.
 */
#ifndef __DTLD_USER_H__
#define __DTLD_USER_H__

#include <linux/types.h>

struct dtld_uresp_alloc_ctx {
	__aligned_u64 csr;
	__aligned_u64 cmdq_sq;
	__aligned_u64 cmdq_rq;
	__aligned_u64 workq_sq;
	__aligned_u64 workq_rq;
	__aligned_u64 cmdq_sq_dma_addr;
    __aligned_u64 cmdq_rq_dma_addr;
    __aligned_u64 workq_sq_dma_addr;
    __aligned_u64 workq_rq_dma_addr;
};
#endif
