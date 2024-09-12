#ifndef LIBXDMA_H
#define LIBXDMA_H

#include "dtld_verbs.h"
#include <linux/version.h>
#include <linux/types.h>
#include <linux/spinlock_types.h>
#include <linux/list.h>
#include <linux/workqueue.h>
#include <linux/interrupt.h>
#include <linux/dma-direction.h>
#include <linux/dma-mapping.h>

#define HAS_SWAKE_UP_ONE (LINUX_VERSION_CODE >= KERNEL_VERSION(4, 19, 0))
#define HAS_SWAKE_UP (LINUX_VERSION_CODE >= KERNEL_VERSION(4, 6, 0))
#if HAS_SWAKE_UP
#include <linux/swait.h>
#endif

#define XDMA_CHANNEL_NUM_MAX (4)
#define XDMA_BAR_NUM (2)
#define XDMA_CONFIG_BAR_IDX (1)
#define RDMA_CONFIG_BAR_IDX (0)

#define MAGIC_DEVICE 0xDDDDDDDDUL
#define MAX_USER_IRQ 16
#define XDMA_BAR_SIZE (0x8000UL)
#define XDMA_OFS_INT_CTRL (0x2000UL)
#define XDMA_OFS_CONFIG (0x3000UL)
#define MAGIC_ENGINE 0xEEEEEEEEUL

#define IRQ_BLOCK_ID 0x1fc20000UL
#define CONFIG_BLOCK_ID 0x1fc30000U

#define CHANNEL_SPACING 0x100
#define XDMA_ID_H2C 0x1fc0U
#define XDMA_ID_C2H 0x1fc1U
#define H2C_CHANNEL_OFFSET 0x1000
/* maximum number of desc per transfer request */
#define XDMA_ENGINE_XFER_MAX_DESC 0x800
#define XDMA_ENG_IRQ_NUM (1)
#define BYPASS_MODE_SPACING 0x0100

/* bits of the SG DMA control register */
#define XDMA_CTRL_RUN_STOP (1UL << 0)
#define XDMA_CTRL_IE_DESC_STOPPED (1UL << 1)
#define XDMA_CTRL_IE_DESC_COMPLETED (1UL << 2)
#define XDMA_CTRL_IE_DESC_ALIGN_MISMATCH (1UL << 3)
#define XDMA_CTRL_IE_MAGIC_STOPPED (1UL << 4)
#define XDMA_CTRL_IE_IDLE_STOPPED (1UL << 6)
#define XDMA_CTRL_IE_READ_ERROR (0x1FUL << 9)
#define XDMA_CTRL_IE_DESC_ERROR (0x1FUL << 19)
#define XDMA_CTRL_NON_INCR_ADDR (1UL << 25)
#define XDMA_CTRL_POLL_MODE_WB (1UL << 26)
#define XDMA_CTRL_STM_MODE_WB (1UL << 27)

/* obtain the 32 most significant (high) bits of a 32-bit or 64-bit address */
#define PCI_DMA_H(addr) ((addr >> 16) >> 16)
/* obtain the 32 least significant (low) bits of a 32-bit or 64-bit address */
#define PCI_DMA_L(addr) (addr & 0xffffffffUL)

#define TARGET_SPACING 0x1000

#ifdef __LIBXDMA_DEBUG__
#define dbg_io pr_err
#define dbg_fops pr_err
#define dbg_perf pr_err
#define dbg_sg pr_err
#define dbg_tfr pr_err
#define dbg_irq pr_err
#define dbg_init pr_err
#define dbg_desc pr_err
#else
/* disable debugging */
#define dbg_io(...)
#define dbg_fops(...)
#define dbg_perf(...)
#define dbg_sg(...)
#define dbg_tfr(...)
#define dbg_irq(...)
#define dbg_init(...)
#define dbg_desc(...)
#endif

enum dev_capabilities {
    CAP_64BIT_DMA = 2,
    CAP_64BIT_DESC = 4,
    CAP_ENGINE_WRITE = 8,
    CAP_ENGINE_READ = 16
};

enum shutdown_state {
    ENGINE_SHUTDOWN_NONE = 0, /* No shutdown in progress */
    ENGINE_SHUTDOWN_REQUEST = 1, /* engine requested to shutdown */
    ENGINE_SHUTDOWN_IDLE = 2 /* engine has shutdown and is idle */
};

/* SECTION: Enum definitions */
enum transfer_state {
    TRANSFER_STATE_NEW = 0,
    TRANSFER_STATE_SUBMITTED,
    TRANSFER_STATE_COMPLETED,
    TRANSFER_STATE_FAILED,
    TRANSFER_STATE_ABORTED
};

struct config_regs {
    u32 identifier;
    u32 reserved_1[4];
    u32 msi_enable;
};

/**
 * SG DMA Controller status and control registers
 *
 * These registers make the control interface for DMA transfers.
 *
 * It sits in End Point (FPGA) memory BAR[0] for 32-bit or BAR[0:1] for 64-bit.
 * It references the first descriptor which exists in Root Complex (PC) memory.
 *
 * @note The registers must be accessed using 32-bit (PCI DWORD) read/writes,
 * and their values are in little-endian byte ordering.
 */
struct engine_regs {
    u32 identifier;
    u32 control;
    u32 control_w1s;
    u32 control_w1c;
    u32 reserved_1[12]; /* padding */

    u32 status;
    u32 status_rc;
    u32 completed_desc_count;
    u32 alignments;
    u32 reserved_2[14]; /* padding */

    u32 poll_mode_wb_lo;
    u32 poll_mode_wb_hi;
    u32 interrupt_enable_mask;
    u32 interrupt_enable_mask_w1s;
    u32 interrupt_enable_mask_w1c;
    u32 reserved_3[9]; /* padding */

    u32 perf_ctrl;
    u32 perf_cyc_lo;
    u32 perf_cyc_hi;
    u32 perf_dat_lo;
    u32 perf_dat_hi;
    u32 perf_pnd_lo;
    u32 perf_pnd_hi;
} __packed;

struct interrupt_regs {
    u32 identifier;
    u32 user_int_enable;
    u32 user_int_enable_w1s;
    u32 user_int_enable_w1c;
    u32 channel_int_enable;
    u32 channel_int_enable_w1s;
    u32 channel_int_enable_w1c;
    u32 reserved_1[9]; /* padding */

    u32 user_int_request;
    u32 channel_int_request;
    u32 user_int_pending;
    u32 channel_int_pending;
    u32 reserved_2[12]; /* padding */

    u32 user_msi_vector[8];
    u32 channel_msi_vector[8];
} __packed;

struct xdma_dev;

#define XDEV_FLAG_OFFLINE 0x1
struct xdma_dev {
    struct list_head list_head;
    struct list_head rcu_node;

    unsigned long magic; /* structure ID for sanity checks */
    struct pci_dev *pdev; /* pci device struct from probe() */
    struct dtld_dev *dtld;
    int idx; /* dev index */

    const char *mod_name; /* name of module owning the dev */

    spinlock_t lock; /* protects concurrent access */
    unsigned int flags;

    /* PCIe BAR management */
    void __iomem *bar[XDMA_BAR_NUM]; /* addresses for mapped BARs */
    int regions_in_use; /* flag if dev was in use during probe() */
    int got_regions; /* flag if probe() obtained the regions */
};

static inline void xdma_device_flag_set(struct xdma_dev *xdev, unsigned int f)
{
    unsigned long flags;

    spin_lock_irqsave(&xdev->lock, flags);
    xdev->flags |= f;
    spin_unlock_irqrestore(&xdev->lock, flags);
}

static inline void xdma_device_flag_clear(struct xdma_dev *xdev, unsigned int f)
{
    unsigned long flags;

    spin_lock_irqsave(&xdev->lock, flags);
    xdev->flags &= ~f;
    spin_unlock_irqrestore(&xdev->lock, flags);
}

struct xdma_dev *xdev_find_by_pdev(struct pci_dev *pdev);

void *xdma_device_open(const char *mname, struct pci_dev *pdev);
void xdma_device_close(struct pci_dev *pdev, void *dev_hndl);

#endif
