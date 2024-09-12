#ifndef __XDMA_H__
#define __XDMA_H__

#include <linux/types.h>
#include <linux/cdev.h>
#include <linux/spinlock_types.h>
#include "libxdma.h"

struct xdma_pci_dev;

/* XDMA PCIe device specific book-keeping */
struct xdma_pci_dev {
    unsigned long magic; /* structure ID for sanity checks */
    struct pci_dev *pdev; /* pci device struct from probe() */
    struct xdma_dev *xdev;
    int major; /* major number */
    int instance; /* instance number */

    unsigned int flags;

    void *data;
};

struct xdma_pci_dev *xpdev_alloc(struct pci_dev *pdev);
void xpdev_free(struct xdma_pci_dev *xpdev);
void *xdma_device_open(const char *mname, struct pci_dev *pdev);

#endif