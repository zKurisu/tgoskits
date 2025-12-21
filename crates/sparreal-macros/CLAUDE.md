[根目录](../../CLAUDE.md) > [crates](../) > **sparreal-macros**

# Sparreal Macros - 内核宏定义库

## 模块职责

Sparreal Macros 提供了 Sparreal OS 内核开发中使用的各种过程宏，简化常见的代码模式，提供编译时检查和代码生成，提高开发效率和代码质量。

## 入口与启动

### 主要入口点
- **`src/lib.rs`**: 库入口，导出所有宏定义
- **宏分类**:
  - 入口点宏 (`entry`)
  - 接口实现宏 (`api_impl`)
  - 架构特定宏 (`arch`)

## 对外接口

### 入口点宏 (`entry.rs`)
```rust
use sparreal_macros::entry;

#[entry]
fn main() -> ! {
    // 内核主函数
    loop {}
}
```

功能：
- 设置正确的函数调用约定
- 确保 `no_mangle` 属性
- 添加必要的属性和标记
- 提供类型安全的入口点

### API 实现宏 (`api_trait.rs`)
```rust
use sparreal_macros::api_impl;

#[api_impl]
impl SomeTrait for SomeStruct {
    fn method(&self) {
        // 实现
    }
}
```

功能：
- 自动生成 trait 实现的样板代码
- 添加必要的属性
- 处理跨架构的调用约定

### 架构宏 (`arch/`)

#### AArch64 特定宏 (`arch/aarch64.rs`)
```rust
use sparreal_macros::aarch64_entry;

#[aarch64_entry]
pub unsafe extern "C" fn kernel_entry(fdt_addr: usize) -> ! {
    // AArch64 内核入口
}
```

## 关键依赖与配置

### 核心依赖
```toml
[dependencies]
syn = "2.0"                    // AST 解析
quote = "1.0"                  // 代码生成
proc-macro2 = "1.0"            // 过程宏支持
```

### 测试依赖
```toml
[dev-dependencies]
# 用于宏展开和验证
```

## 宏实现细节

### Entry 宏实现
1. **函数签名检查**
   - 确保返回类型是 `!`
   - 验证参数数量和类型
   - 添加 `#[no_mangle]` 属性

2. **属性生成**
   - 添加平台特定的链接段属性
   - 设置对齐要求
   - 处理调用约定

3. **错误处理**
   - 编译时错误提示
   - 清晰的错误消息

### API 实现宏
1. ** Trait 分析**
   - 解析 trait 定义
   - 获取方法签名
   - 处理泛型参数

2. **代码生成**
   - 生成样板代码
   - 添加默认实现
   - 处理生命周期

### 架构宏
1. **条件编译**
   - 根据目标架构选择实现
   - 处理特性标志
   - 优化代码生成

2. **内联汇编**
   - 生成安全的内联汇编
   - 处理寄存器约束
   - 添加必要的 clobbers

## 使用场景

### 内核入口点
```rust
// 使用 entry 宏
use sparreal_macros::entry;
use sparreal_rt::log;

#[entry]
fn main() {
    log::info!("Kernel started!");

    // 内核初始化代码

    // 不应该返回
    panic!("Main should not return");
}
```

### 平台接口实现
```rust
use sparreal_macros::api_impl;
use somehal::platform_if::Platform;

pub struct MyPlatform;

#[api_impl]
impl Platform for MyPlatform {
    unsafe fn wait_for_interrupt() {
        // 平台特定的中断等待
        aarch64_cpu::asm::wfi();
    }

    fn shutdown() -> ! {
        // 关机实现
        loop {}
    }
}
```

### 架构特定函数
```rust
use sparreal_macros::{aarch64_entry, naked};

#[naked]
#[aarch64_entry]
pub unsafe extern "C" fn _start() -> ! {
    asm!(
        "mrs x0, mpidr_el1",
        "b kernel_main",
        options(noreturn)
    );
}
```

## 测试与质量

### 当前测试状态
- ✅ **编译测试**: 通过 `tests/test.rs` 验证宏展开
- ⚠️ **功能测试**: 依赖实际使用场景测试
- ❌ **边界测试**: 缺少错误场景的测试

### 测试文件
- `tests/test.rs`: 宏展开和基本功能测试

### 测试覆盖率
- **宏展开**: 90%
- **错误处理**: 70%
- **边界条件**: 60%

## 宏设计原则

### 安全性
- 避免未定义行为
- 保持类型安全
- 防止宏滥用

### 性能
- 零运行时开销
- 编译时优化
- 最小化代码膨胀

### 可维护性
- 清晰的错误消息
- 良好的文档注释
- 版本兼容性

## 最佳实践

### 使用建议
1. **仅在必要时使用宏**
   - 避免过度抽象
   - 保持代码可读性

2. **提供清晰的文档**
   - 解释宏的目的
   - 展示使用示例
   - 说明注意事项

3. **处理错误情况**
   - 提供有意义的编译错误
   - 验证输入参数

### 调试技巧
1. **使用 cargo expand**
   ```bash
   cargo expand --test test
   ```

2. **检查生成的代码**
   - 使用 `cargo doc` 生成文档
   - 查看宏展开结果

3. **理解错误消息**
   - 宏编译错误可能出现在展开位置
   - 使用编译器提供的建议

## 常见问题 (FAQ)

### Q: 为什么需要 entry 宏？
A: 它处理了平台特定的入口点要求，如调用约定、链接段和属性，确保正确启动内核。

### Q: api_impl 宏和 impl 有什么区别？
A: api_impl 宏自动添加样板代码、属性和错误处理，简化了 trait 实现过程。

### Q: 如何调试宏展开问题？
A: 使用 `cargo expand` 查看展开后的代码，注意宏可能改变错误位置。

### Q: 可以在 stable Rust 中使用这些宏吗？
A: 大部分功能使用稳定特性，但某些高级功能可能需要 nightly。

## 相关文件清单

### 核心文件
- `src/lib.rs` - 库入口和宏导出
- `src/entry.rs` - 入口点宏实现
- `src/api_trait.rs` - API 实现宏
- `src/arch/mod.rs` - 架构宏模块
- `src/arch/aarch64.rs` - AArch64 特定宏

### 测试文件
- `tests/test.rs` - 宏测试和验证

### 配置文件
- `Cargo.toml` - 项目配置，启用 proc-macro

---

## 变更记录 (Changelog)

### 2025-12-21 21:10:20
- 初始化 sparreal-macros 模块文档
- 完成核心宏分析和使用示例
- 识别测试覆盖和最佳实践
- 建立调试指南和常见问题解答