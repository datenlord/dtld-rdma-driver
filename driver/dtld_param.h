/* SPDX-License-Identifier: GPL-2.0 OR Linux-OpenIB */
/*
 * Copyright (c) 2016 Mellanox Technologies Ltd. All rights reserved.
 * Copyright (c) 2015 System Fabric Works, Inc. All rights reserved.
 */

#ifndef DTLD_PARAM_H
#define DTLD_PARAM_H

#define DEFAULT_MAX_VALUE (1 << 20)

#define DTLD_RINGBUF_SIZE 4096
#include <rdma/ib_verbs.h>
// TODO: change me
#define DTLD_MAX_HDR_LENGTH 10

static inline enum ib_mtu dtld_mtu_int_to_enum(int mtu)
{
    if (mtu < 256)
        return 0;
    else if (mtu < 512)
        return IB_MTU_256;
    else if (mtu < 1024)
        return IB_MTU_512;
    else if (mtu < 2048)
        return IB_MTU_1024;
    else if (mtu < 4096)
        return IB_MTU_2048;
    else
        return IB_MTU_4096;
}

/* Find the IB mtu for a given network MTU. */
static inline enum ib_mtu eth_mtu_int_to_enum(int mtu)
{
    mtu -= DTLD_MAX_HDR_LENGTH;

    return dtld_mtu_int_to_enum(mtu);
}

/* default/initial dtld device parameter settings */
/* clang-format off */
enum dtld_device_param {
	DTLD_MAX_MR_SIZE			= -1ull,
	DTLD_PAGE_SIZE_CAP		= 0xfffff000,
	DTLD_MAX_QP_WR			= DEFAULT_MAX_VALUE,
	/* clang-format off */
	DTLD_DEVICE_CAP_FLAGS		= IB_DEVICE_BAD_PKEY_CNTR
								| IB_DEVICE_BAD_QKEY_CNTR
								| IB_DEVICE_AUTO_PATH_MIG
								| IB_DEVICE_CHANGE_PHY_PORT
								| IB_DEVICE_UD_AV_PORT_ENFORCE
								| IB_DEVICE_PORT_ACTIVE_EVENT
								| IB_DEVICE_SYS_IMAGE_GUID
								| IB_DEVICE_RC_RNR_NAK_GEN
								| IB_DEVICE_SRQ_RESIZE
								| IB_DEVICE_MEM_MGT_EXTENSIONS
								| IB_DEVICE_MEM_WINDOW
								| IB_DEVICE_MEM_WINDOW_TYPE_2B,
    /* clang-format on */

    DTLD_MAX_SGE = 32,

    // DTLD_MAX_WQE_SIZE		= sizeof(struct dtld_send_wqe) +
    // 				  sizeof(struct ib_sge) * DTLD_MAX_SGE,
    // DTLD_MAX_INLINE_DATA		= DTLD_MAX_WQE_SIZE -
    // 				  sizeof(struct dtld_send_wqe),
    DTLD_MAX_WQE_SIZE = 128 + sizeof(struct ib_sge) * DTLD_MAX_SGE,
    DTLD_MAX_INLINE_DATA = DTLD_MAX_WQE_SIZE - 128,
    DTLD_MAX_SGE_RD = 32,
    DTLD_MAX_CQ = DEFAULT_MAX_VALUE,
    DTLD_MAX_LOG_CQE = 15,
    DTLD_MAX_PD = DEFAULT_MAX_VALUE,
    DTLD_MAX_QP_RD_ATOM = 128,
    DTLD_MAX_RES_RD_ATOM = 0x3f000,
    DTLD_MAX_QP_INIT_RD_ATOM = 128,
    DTLD_MAX_MCAST_GRP = 8192,
    DTLD_MAX_MCAST_QP_ATTACH = 56,
    DTLD_MAX_TOT_MCAST_QP_ATTACH = 0x70000,
    DTLD_MAX_AH = (1 << 15) - 1, /* 32Ki - 1 */
    DTLD_MIN_AH_INDEX = 1,
    DTLD_MAX_AH_INDEX = DTLD_MAX_AH,
    DTLD_MAX_SRQ_WR = DEFAULT_MAX_VALUE,
    DTLD_MIN_SRQ_WR = 1,
    DTLD_MAX_SRQ_SGE = 27,
    DTLD_MIN_SRQ_SGE = 1,
    DTLD_MAX_FMR_PAGE_LIST_LEN = 512,
    DTLD_MAX_PKEYS = 64,
    DTLD_LOCAL_CA_ACK_DELAY = 15,

    DTLD_MAX_UCONTEXT = DEFAULT_MAX_VALUE,

    DTLD_NUM_PORT = 1,

    DTLD_MIN_QP_INDEX = 16,
    DTLD_MAX_QP_INDEX = DEFAULT_MAX_VALUE,
    DTLD_MAX_QP = DEFAULT_MAX_VALUE - DTLD_MIN_QP_INDEX,

    DTLD_MIN_SRQ_INDEX = 0x00020001,
    DTLD_MAX_SRQ_INDEX = DEFAULT_MAX_VALUE,
    DTLD_MAX_SRQ = DEFAULT_MAX_VALUE - DTLD_MIN_SRQ_INDEX,

    DTLD_MIN_MR_INDEX = 0x00000001,
    DTLD_MAX_MR_INDEX = DEFAULT_MAX_VALUE,
    DTLD_MAX_MR = DEFAULT_MAX_VALUE - DTLD_MIN_MR_INDEX,
    DTLD_MIN_MW_INDEX = 0x00010001,
    DTLD_MAX_MW_INDEX = 0x00020000,
    DTLD_MAX_MW = 0x00001000,

    DTLD_MAX_PKT_PER_ACK = 64,

    DTLD_MAX_UNACKED_PSNS = 128,

    /* Max inflight SKBs per queue pair */
    DTLD_INFLIGHT_SKBS_PER_QP_HIGH = 64,
    DTLD_INFLIGHT_SKBS_PER_QP_LOW = 16,

    /* Delay before calling arbiter timer */
    DTLD_NSEC_ARB_TIMER_DELAY = 200,

    /* IBTA v1.4 A3.3.1 VENDOR INFORMATION section */
    DTLD_VENDOR_ID = 0XFFFFFF,
};

/* default/initial dtld port parameters */
enum dtld_port_param {
    DTLD_PORT_GID_TBL_LEN = 1024,
    DTLD_PORT_MAX_MSG_SZ = 0x800000,
    DTLD_PORT_BAD_PKEY_CNTR = 0,
    DTLD_PORT_QKEY_VIOL_CNTR = 0,
    DTLD_PORT_LID = 0,
    DTLD_PORT_SM_LID = 0,
    DTLD_PORT_SM_SL = 0,
    DTLD_PORT_LMC = 0,
    DTLD_PORT_MAX_VL_NUM = 1,
    DTLD_PORT_SUBNET_TIMEOUT = 0,
    DTLD_PORT_INIT_TYPE_REPLY = 0,
    DTLD_PORT_ACTIVE_WIDTH = IB_WIDTH_1X,
    DTLD_PORT_ACTIVE_SPEED = 1,
    DTLD_PORT_PKEY_TBL_LEN = 1,
    DTLD_PORT_PHYS_STATE = IB_PORT_PHYS_STATE_POLLING,
    DTLD_PORT_SUBNET_PREFIX = 0xfe80000000000000ULL,
};

/* default/initial port info parameters */
enum dtld_port_info_param {
    DTLD_PORT_INFO_VL_CAP = 4, /* 1-8 */
    DTLD_PORT_INFO_MTU_CAP = 5, /* 4096 */
    DTLD_PORT_INFO_OPER_VL = 1, /* 1 */
};

#endif /* DTLD_PARAM_H */
