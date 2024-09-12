KERNEL_SRC_DIR ?= /linux-src

.PHONY: kernel driver user gdb

kernel:
	$(MAKE) -C $(KERNEL_SRC_DIR)

driver:
	$(MAKE) -C $(KERNEL_SRC_DIR) M=$(PWD)/driver

user:
	export CMAKE_EXPORT_COMPILE_COMMANDS=1 && ./rdma-core/build.sh

qemu:
	./scripts/run_qemu.sh

gdb:
	./scripts/run_gdb.sh

# language server config, e.g., clangd
lsp:
	cd $(KERNEL_SRC_DIR) && ./scripts/clang-tools/gen_compile_commands.py
	cd driver && $(KERNEL_SRC_DIR)/scripts/clang-tools/gen_compile_commands.py -d $(KERNEL_SRC_DIR) ./