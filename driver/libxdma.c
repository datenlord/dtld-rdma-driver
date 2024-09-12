#include <linux/kernel.h>
#include <linux/device.h>
#include <linux/pci.h>
#include <linux/pci_regs.h>
#include "libxdma.h"

#define XDMA_H2C_CTRL_OFFSET 0x4
#define XDMA_C2H_CTRL_OFFSET 0x1004
#define XDMA_START_DMA_ENGINE 0x1

/*
 * xdma device management
 * maintains a list of the xdma devices
 */
static LIST_HEAD(xdev_list);
static DEFINE_MUTEX(xdev_mutex);
static LIST_HEAD(xdev_rcu_list);
static DEFINE_SPINLOCK(xdev_rcu_lock);

inline u32 read_register(void *iomem)
{
    return ioread32(iomem);
}
#define write_register(v, mem, off) iowrite32(v,(mem+off))

static inline u32 build_u32(u32 hi, u32 lo)
{
    return ((hi & 0xFFFFUL) << 16) | (lo & 0xFFFFUL);
}

/// alloc a dummpy xdma_dev device
static struct xdma_dev *alloc_dev_instance(struct pci_dev *pdev)
{
    struct xdma_dev *xdev;

    if (!pdev) {
        pr_err("Invalid pdev\n");
        return NULL;
    }

    /* allocate zeroed device book keeping structure */
    xdev = kzalloc(sizeof(struct xdma_dev), GFP_KERNEL);
    if (!xdev) {
        pr_info("OOM, xdma_dev.\n");
        return NULL;
    }
    spin_lock_init(&xdev->lock);

    xdev->magic = MAGIC_DEVICE;

    /* create a driver to device reference */
    xdev->pdev = pdev;
    dbg_init("xdev = 0x%p\n", xdev);

    return xdev;
}

static inline int xdev_list_add(struct xdma_dev *xdev)
{
    mutex_lock(&xdev_mutex);
    if (list_empty(&xdev_list)) {
        xdev->idx = 0;
    } else {
        struct xdma_dev *last;

        last = list_last_entry(&xdev_list, struct xdma_dev, list_head);
        xdev->idx = last->idx + 1;
    }
    list_add_tail(&xdev->list_head, &xdev_list);
    mutex_unlock(&xdev_mutex);

    dbg_init("dev %s, xdev 0x%p, xdma idx %d.\n", dev_name(&xdev->pdev->dev),
             xdev, xdev->idx);

    spin_lock(&xdev_rcu_lock);
    list_add_tail_rcu(&xdev->rcu_node, &xdev_rcu_list);
    spin_unlock(&xdev_rcu_lock);

    return 0;
}

static inline void xdev_list_remove(struct xdma_dev *xdev)
{
    mutex_lock(&xdev_mutex);
    list_del(&xdev->list_head);
    mutex_unlock(&xdev_mutex);

    spin_lock(&xdev_rcu_lock);
    list_del_rcu(&xdev->rcu_node);
    spin_unlock(&xdev_rcu_lock);
    synchronize_rcu();
}

struct xdma_dev *xdev_find_by_pdev(struct pci_dev *pdev)
{
    struct xdma_dev *xdev, *tmp;

    mutex_lock(&xdev_mutex);
    list_for_each_entry_safe(xdev, tmp, &xdev_list, list_head)
    {
        if (xdev->pdev == pdev) {
            mutex_unlock(&xdev_mutex);
            return xdev;
        }
    }
    mutex_unlock(&xdev_mutex);
    return NULL;
}

static int request_regions(struct xdma_dev *xdev, struct pci_dev *pdev)
{
    int rv;

    if (!xdev) {
        pr_err("Invalid xdev\n");
        return -EINVAL;
    }

    if (!pdev) {
        pr_err("Invalid pdev\n");
        return -EINVAL;
    }

    dbg_init("pci_request_regions()\n");
    rv = pci_request_regions(pdev, xdev->mod_name);
    /* could not request all regions? */
    if (rv) {
        dbg_init("pci_request_regions() = %d, device in use?\n", rv);
        /* assume device is in use so do not disable it later */
        xdev->regions_in_use = 1;
    } else {
        xdev->got_regions = 1;
    }

    return rv;
}

static int map_single_bar(struct xdma_dev *xdev, struct pci_dev *dev, int idx)
{
    resource_size_t bar_start;
    resource_size_t bar_len;
    resource_size_t map_len;

    bar_start = pci_resource_start(dev, idx);
    bar_len = pci_resource_len(dev, idx);
    map_len = bar_len;

    xdev->bar[idx] = NULL;

    /* do not map BARs with length 0. Note that start MAY be 0! */
    if (!bar_len) {
        pr_info("BAR #%d is not present - skipping\n", idx);
        return 0;
    }

    /* BAR size exceeds maximum desired mapping? */
    if (bar_len > INT_MAX) {
        pr_info("Limit BAR %d mapping from %llu to %d bytes\n", idx,
                (u64)bar_len, INT_MAX);
        map_len = (resource_size_t)INT_MAX;
    }
    /*
	 * map the full device memory or IO region into kernel virtual
	 * address space
	 */
    dbg_init("BAR%d: %llu bytes to be mapped.\n", idx, (u64)map_len);
    xdev->bar[idx] = pci_iomap(dev, idx, map_len);

    if (!xdev->bar[idx]) {
        pr_info("Could not map BAR %d.\n", idx);
        return -1;
    }

    pr_info("BAR%d at 0x%llx mapped at 0x%p, length=%llu(/%llu)\n", idx,
            (u64)bar_start, xdev->bar[idx], (u64)map_len, (u64)bar_len);

    return (int)map_len;
}

static int is_config_bar(struct xdma_dev *xdev, int idx)
{
    u32 irq_id = 0;
    u32 cfg_id = 0;
    int flag = 0;
    u32 mask = 0xffff0000; /* Compare only XDMA ID's not Version number */
    struct interrupt_regs *irq_regs =
            (struct interrupt_regs *)(xdev->bar[idx] + XDMA_OFS_INT_CTRL);
    struct config_regs *cfg_regs =
            (struct config_regs *)(xdev->bar[idx] + XDMA_OFS_CONFIG);

    irq_id = read_register(&irq_regs->identifier);
    cfg_id = read_register(&cfg_regs->identifier);

    if (((irq_id & mask) == IRQ_BLOCK_ID) &&
        ((cfg_id & mask) == CONFIG_BLOCK_ID)) {
        dbg_init("BAR %d is the XDMA config BAR\n", idx);
        flag = 1;
    } else {
        dbg_init("BAR %d is NOT the XDMA config BAR: 0x%x, 0x%x.\n", idx,
                 irq_id, cfg_id);
        flag = 0;
    }

    return flag;
}

/*
 * Unmap the BAR regions that had been mapped earlier using map_bars()
 */
static void unmap_bars(struct xdma_dev *xdev, struct pci_dev *dev)
{
    pci_iounmap(dev, xdev->bar[XDMA_CONFIG_BAR_IDX]);
    xdev->bar[XDMA_CONFIG_BAR_IDX] = NULL;

    pci_iounmap(dev, xdev->bar[RDMA_CONFIG_BAR_IDX]);
    xdev->bar[RDMA_CONFIG_BAR_IDX] = NULL;
}

/* map_bars() -- map device regions into kernel virtual address space */
static int map_bars(struct xdma_dev *xdev, struct pci_dev *dev)
{
    int rv = -EINVAL;

    // map config bar
    if (map_single_bar(xdev, dev, XDMA_CONFIG_BAR_IDX) == 0) {
        goto fail;
    }

    // check if xdma config bar exist
    if (!is_config_bar(xdev, XDMA_CONFIG_BAR_IDX)) {
        goto fail;
    }

    if (map_single_bar(xdev, dev, RDMA_CONFIG_BAR_IDX) == 0) {
        goto fail;
    }


    /* successfully mapped all required BAR regions */
    return 0;

fail:
    /* unwind; unmap any BARs that we did map */
    unmap_bars(xdev, dev);
    return rv;
}

static int set_dma_mask(struct pci_dev *pdev)
{
    int ret;
    if (!pdev) {
        pr_err("Invalid pdev\n");
        return -EINVAL;
    }

    dbg_init("sizeof(dma_addr_t) == %ld\n", sizeof(dma_addr_t));
    /* 64-bit addressing capability for XDMA? */
    ret = dma_set_mask_and_coherent(&pdev->dev, DMA_BIT_MASK(64));
    if (ret) {
        ret = dma_set_mask_and_coherent(&pdev->dev, DMA_BIT_MASK(32));
    }
    if (ret) {
        dbg_init("No suitable DMA possible.\n");
        return -EINVAL;
    }
    return 0;
}

static void pci_enable_capability(struct pci_dev *pdev, int cap)
{
    u16 v;
    int pos;

    pos = pci_pcie_cap(pdev);
    if (pos > 0) {
        pci_read_config_word(pdev, pos + PCI_EXP_DEVCTL, &v);
        v |= cap;
        pci_write_config_word(pdev, pos + PCI_EXP_DEVCTL, v);
    }
}

void *xdma_device_open(const char *mname, struct pci_dev *pdev)
{
    struct xdma_dev *xdev = NULL;
    int rv = 0;

    pr_info("%s device %s, 0x%p.\n", mname, dev_name(&pdev->dev), pdev);

    /* allocate zeroed device book keeping structure */
    xdev = alloc_dev_instance(pdev);
    if (!xdev)
        return NULL;
    xdev->mod_name = mname;

    xdma_device_flag_set(xdev, XDEV_FLAG_OFFLINE);

    rv = xdev_list_add(xdev);
    if (rv < 0)
        goto free_xdev;

    rv = pci_enable_device(pdev);
    if (rv) {
        dbg_init("pci_enable_device() failed, %d.\n", rv);
        goto err_enable;
    }

    /* enable relaxed ordering */
    pci_enable_capability(pdev, PCI_EXP_DEVCTL_RELAX_EN);
    pci_enable_capability(pdev, PCI_EXP_DEVCTL_EXT_TAG);

    /* force MRRS to be 512 */
    rv = pcie_set_readrq(pdev, 512);
    if (rv)
        pr_info("device %s, error set PCI_EXP_DEVCTL_READRQ: %d.\n",
                dev_name(&pdev->dev), rv);

    /* enable bus master capability */
    pci_set_master(pdev);

    rv = request_regions(xdev, pdev);
    if (rv)
        goto err_regions;

    rv = map_bars(xdev, pdev);
    if (rv)
        goto err_map;

    rv = set_dma_mask(pdev);
    if (rv)
        goto err_mask;

    xdma_device_flag_clear(xdev, XDEV_FLAG_OFFLINE);

    // start XDMA engine
    write_register(XDMA_START_DMA_ENGINE, xdev->bar[XDMA_CONFIG_BAR_IDX], XDMA_H2C_CTRL_OFFSET);
    write_register(XDMA_START_DMA_ENGINE, xdev->bar[XDMA_CONFIG_BAR_IDX], XDMA_C2H_CTRL_OFFSET);

    return (void *)xdev;

err_mask:
    unmap_bars(xdev, pdev);
err_map:
    if (xdev->got_regions)
        pci_release_regions(pdev);
err_regions:
    if (!xdev->regions_in_use)
        pci_disable_device(pdev);
err_enable:
    xdev_list_remove(xdev);
free_xdev:
    kfree(xdev);
    return NULL;
}

void xdma_device_close(struct pci_dev *pdev, void *dev_hndl)
{
    struct xdma_dev *xdev = (struct xdma_dev *)dev_hndl;

    if (!dev_hndl)
        return;

    dbg_sg("remove(dev = 0x%p) where pdev->dev.driver_data = 0x%p\n", pdev,
           xdev);
    if (xdev->pdev != pdev) {
        dbg_sg("pci_dev(0x%lx) != pdev(0x%lx)\n", (unsigned long)xdev->pdev,
               (unsigned long)pdev);
    }

    unmap_bars(xdev, pdev);

    if (xdev->got_regions) {
        dbg_init("pci_release_regions 0x%p.\n", pdev);
        pci_release_regions(pdev);
    }

    if (!xdev->regions_in_use) {
        dbg_init("pci_disable_device 0x%p.\n", pdev);
        pci_disable_device(pdev);
    }

    xdev_list_remove(xdev);

    kfree(xdev);
}