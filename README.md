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
