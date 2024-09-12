### How to develop

This project recommends to use the devcontainer. The corresponsding devcontainer provides:
* a prebuild qemu VM to test the driver.
* a copy of kernel source code and a prebuild kernel image with debug info.
* some useful scripts to help the development.
* gdb debug scripts and configs for both vscode based GUI debug and pure cli debug.


suppose you will clone this project and it's submodules, now you will have a top folder named `dtld-rdma-driver`, make sure your folder use this name, since when using devcontainer, this folder will be mounted as `/workspaces/dtld-rdma-driver` in the container, and this path is hardcoded in many scripts. So, ***don't change this name***.


#### Dev Container Structure
after open this git repo in devcontainer, you will see the following folders, before we go on, let's keep one thing in mind:

> our code should only rely on the kernel, but not modify the kernel. So, any change that be made should go into the `/workspaces/dtld-rdma-driver` folder，**ALL THE OTHER FOLDERS WILL BE DROPPED WHEN REBUILDING THE DEV CONTAINER**. for example, you should not save anything in the QEMU VM, since the qemu's disk is not stored in `/workspaces/dtld-rdma-driver`. The dev container and QEMU VM is only useful for testing and debugging, so it may be dropped and recreated very often. All the changes that you want to keep between drop and recreate should go into `/workspaces/dtld-rdma-driver`.

* `/linux-src`: the linux kernel source and prebuild binarys.
* `/vm`: this is the rootfs disk image for qemu
* `/workspaces/dtld-rdma-driver/driver`: this is the kernel driver, run in kernel space
    * you can run `make driver` to build it
* `/workspaces/dtld-rdma-driver/rdma-code`: this is a submodule for RDMA user space driver
    * you can run `make user` to build the user-space code
* `/workspaces/dtld-rdma-driver/scripts` this provides many useful scripts.
    * `/workspaces/dtld-rdma-driver/scripts/for_qemu` this floder will be mounted into the QEMU VM and be added to the `PATH` in the QEMU's guest os, so you can run the scripts under this folder easily in QEMU.


#### QEMU Guest OS Structure
you can run `make qemu` to launch the qemu, the user and pass for this guest os is both `root`

in the qemu, there are some pre-mounted folders:
* `/host`: this is the `/` in the dev container, so you can use this window to access the whole filesystem in the dev contianer.
* `/workspaces`: this is a softlink to `/host/workspaces`, with this trick, the binary in qemu can see the same path as it was built in the devcontainer. 
    * we need this trick because the built binary for `rdma-core` must be run *in-place*, i.e., you can't build the binary, copy it to some other path, and then run it. because we build it in devcontainer, and run it in qemu, so we must make the path in qemu looks like where it was built in the dev container. 

* the following path is added to the `PATH` in the qemu's guest OS, so we can run scripts or RDMA user space tools easily.
    * `/host/workspaces/dtld-rdma-driver/scripts/for_qemu`
    * `/host/workspaces/dtld-rdma-driver/rdma-core/build/bin`


#### How to debug the kernel?
there is out-of-the-box support for debugging. the gdb config has been set for you, you need to first run `make qemu` and then:
* you can run `make gdb` to launch a gdb in commandline and auto connect to qemu
* you can press F5 in vscode, the `launch.json` is already written for you to attach to the qemu.
    * it seems that you should start the debubgger first, then insert kernel module. otherwise, it seems the debugger won't find the kernel module.


#### language server?
we use `clangd`, you can run `make lsp` to generate `compile_commands.json` for clangd to use.
you may need to use `ctrl + shift + p` to show the vscode's command panel and select `clangd: download language server` to make the clangd extension work.