# $env:KERNEL_BUILTIN_CMDLINE = "earlycon=pl011,mmio32,0x9000000"
ostool run -c build-config\loongarch64.toml qemu -q .\apps\helloworld\qemu-la64.toml