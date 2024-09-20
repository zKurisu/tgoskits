# 雀实操作系统 Sparreal

麻雀虽小，五脏俱全的实时操作系统。

## 环境搭建

1. 安装 Rust
2. 安装依赖

```bash
cargo install cargo-binutils
rustup component add llvm-tools-preview
rustup component add rust-src
```

### Windows

安装[Qemu](https://www.qemu.org/download/#windows), 并加入环境变量。

### Linux

安装[Qemu](https://www.qemu.org/download/#linux)

## 构建

```bash
cargo xtask build
```

## Qemu 测试

```bash
cargo xtask qemu
```

## Qemu + Vscode 断点调试

```bash
cargo xtask qemu -d
```

vscode 选择调试配置 `KDebug`， 点击 `Run and Debug` 按钮。

## U-Boot 调试

需要连接开发板串口。

```bash
cargo xtask uboot
```

## 配置

首次执行 `xtask` 任务后，会在根目录生成默认配置文件 `.project.toml`。
