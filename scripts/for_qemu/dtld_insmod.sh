#!/bin/env bash

# make -C /host/linux-src INSTALL_MOD_PATH=/ modules_install

modprobe ib_core
modprobe ib_uverbs
insmod /workspaces/open-rdma-driver/driver/dtld_ib.ko
sysctl -w vm.nr_hugepages=1024