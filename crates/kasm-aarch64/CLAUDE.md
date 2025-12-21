[根目录](../../CLAUDE.md) > [crates](../) > **kasm-aarch64**

# KASM AArch64 - AArch64 内核汇编支持库

## 模块职责

KASM AArch64 为 Sparreal OS 内核提供 AArch64 架构的底层汇编操作支持，包括异常处理、上下文切换、系统调用等关键操作的汇编代码封装，实现安全的内联汇编和架构特定功能。

## 入口与启动

### 主要入口点
- **`src/lib.rs`**: 库入口，导出核心汇编宏和函数
- **核心模块**:
  - 内联汇编宏定义
  - 异常处理汇编
  - 上下文切换例程

## 对外接口

### 内联汇编宏

#### 异常返回宏
```rust
use kasm_aarch64::eret;

// 异常返回
eret!();
```

#### 上下文切换宏
```rust
use kasm_aarch64::context_switch;

// 切换到新上下文
context_switch!(old_context, new_context);
```

#### 栈操作宏
```rust
use kasm_aarch64::{push_registers, pop_registers};

// 保存所有通用寄存器
push_registers!();

// 恢复所有通用寄存器
pop_registers!();
```

### 异常处理支持 (`trap.rs`)

#### 异常入口包装器
```rust
use kasm_aarch64::trap_entry;

#[trap_entry]
fn sync_exception_handler(esr: u64, far: u64) {
    // 同步异常处理
}
```

#### 上下文保存/恢复
```rust
// 保存异常上下文
#[macro_export]
macro_rules! save_exception_context {
    () => {
        asm!(
            "stp x0, x1, [sp, #-16]!",
            "stp x2, x3, [sp, #-16]!",
            // ... 保存所有寄存器
        )
    };
}
```

## 关键依赖与配置

### 核心依赖
```rust
// 无外部依赖，仅使用 core
// 依赖编译器内建功能
```

### 编译器要求
- Rust 支持内联汇编
- AArch64 目标支持
- `global_asm!` 宏支持（可能需要 nightly）

## 实现细节

### 寄存器操作
提供安全的寄存器访问：
```rust
/// 读取当前异常级别
pub fn current_el() -> u32 {
    let mut el: u32;
    unsafe {
        asm!("mrs {}, CurrentEL", out(reg) el);
    }
    el >> 2
}

/// 读取 TPIDR_EL0 (线程指针)
pub fn read_tpidr_el0() -> u64 {
    let mut tpidr: u64;
    unsafe {
        asm!("mrs {}, TPIDR_EL0", out(reg) tpidr);
    }
    tpidr
}
```

### 内存屏障
提供内存排序保证：
```rust
/// 数据内存屏障
#[inline]
pub fn dmb(isy: bool) {
    unsafe {
        if isy {
            asm!("dmb ish");
        } else {
            asm!("dmb ishld");
        }
    }
}

/// 指令同步屏障
#[inline]
pub fn isb() {
    unsafe {
        asm!("isb");
    }
}
```

### 系统寄存器访问
安全访问系统控制寄存器：
```rust
/// 读取 SCTLR_EL1 (系统控制寄存器)
pub fn read_sctlr_el1() -> u64 {
    let mut sctlr: u64;
    unsafe {
        asm!("mrs {}, SCTLR_EL1", out(reg) sctlr);
    }
    sctlr
}

/// 写入 SCTLR_EL1
pub unsafe fn write_sctlr_el1(sctlr: u64) {
    asm!("msr SCTLR_EL1, {}", in(reg) sctlr);
}
```

## AArch64 特定功能

### 异常级别管理
- **EL0**: 用户模式
- **EL1**: 内核模式（标准）
- **EL2**: 虚拟化扩展
- **EL3**: 安全监控器

### 页表操作
```rust
/// 读取 TTBR0_EL1 (转换表基址寄存器 0)
pub fn read_ttbr0_el1() -> u64 {
    let mut ttbr0: u64;
    unsafe {
        asm!("mrs {}, TTBR0_EL1", out(reg) ttbr0);
    }
    ttbr0
}

/// 设置 TTBR0_EL1
pub unsafe fn write_ttbr0_el1(ttbr0: u64) {
    asm!("msr TTBR0_EL1, {}", in(reg) ttbr0);
    // 需要 TLB 刷新
    asm!("tlbi vmalle1is");
    isb();
}
```

### 中断控制
```rust
/// 启用/禁用中断
pub fn set_daif(daif: u32) {
    unsafe {
        asm!("msr DAIFset, {}", in(reg) daif);
    }
}

/// 清除中断掩码
pub fn clear_daif(daif: u32) {
    unsafe {
        asm!("msr DAIFclr, {}", in(reg) daif);
    }
}
```

## 使用场景

### 异常处理实现
```rust
use kasm_aarch64::*;

#[no_mangle]
pub extern "C" fn sync_exception_entry() {
    // 保存上下文
    save_exception_context!();

    // 获取异常信息
    let esr = read_esr_el1();
    let far = read_far_el1();

    // 调用 Rust 处理函数
    handle_sync_exception(esr, far);

    // 恢复上下文
    restore_exception_context!();

    // 异常返回
    eret!();
}
```

### 任务切换
```rust
use kasm_aarch64::*;

/// 任务上下文结构
#[repr(C)]
pub struct TaskContext {
    x19: u64,
    x20: u64,
    // ... 其他寄存器
    sp_el0: u64,
}

pub fn context_switch(old: &mut TaskContext, new: &TaskContext) {
    unsafe {
        // 保存当前任务状态
        push_context!(old);

        // 恢复新任务状态
        pop_context!(new);
    }
}
```

## 测试与质量

### 当前测试状态
- ❌ **单元测试**: 汇编代码难以直接测试
- ⚠️ **集成测试**: 通过系统启动验证
- ⚠️ **模拟测试**: QEMU 环境测试

### 测试策略
1. **功能测试**
   - 寄存器读写操作
   - 内存屏障效果
   - 异常处理流程

2. **性能测试**
   - 关键路径的执行时间
   - 缓存影响分析

3. **安全测试**
   - 权限检查
   - 异常处理安全性

## 安全考虑

### 内联汇编安全
- 使用正确的约束
- 避免未定义行为
- 保持内存一致性

### 寄存器约定
- 遵循 AArch64 ABI
- 正确处理调用者/被调用者保存寄存器
- 避免意外修改系统寄存器

### 异常安全
- 确保异常时系统状态一致
- 正确的栈管理
- 防止特权提升

## 最佳实践

### 编码规范
1. **保持简单**
   - 汇编代码应尽可能简单
   - 使用宏封装复杂操作

2. **充分注释**
   - 解释汇编指令的目的
   - 说明副作用和约束

3. **错误处理**
   - 检查操作结果
   - 提供清晰的错误路径

### 性能优化
1. **避免不必要的屏障**
   - 只在需要时使用
   - 选择合适的屏障类型

2. **优化热路径**
   - 关键代码路径优化
   - 考虑缓存友好性

## 常见问题 (FAQ)

### Q: 为什么需要专门的汇编库？
A: 某些底层操作只能通过汇编实现，如上下文切换、异常处理、系统寄存器访问等。

### Q: 内联汇编安全吗？
A: 只要正确使用约束并遵循 AArch64 规范，内联汇编是安全的。需要特别注意副作用。

### Q: 如何调试汇编代码？
A: 使用 GDB 的汇编调试功能，查看寄存器状态和内存内容。可以使用 `objdump` 查看生成的汇编。

### Q: 可以在其他 AArch64 项目中使用吗？
A: 是的，这个库提供通用的 AArch64 内核汇编支持，可以用于其他操作系统内核项目。

## 相关文件清单

### 核心文件
- `src/lib.rs` - 库入口和宏导出
- `src/trap.rs` - 异常处理汇编支持
- `CHANGELOG.md` - 变更日志
- `.gitignore` - Git 忽略文件

### 配置文件
- `Cargo.toml` - 项目配置

### 文档文件
- `CLAUDE.md` - 本文档

---

## 变更记录 (Changelog)

### 2025-12-21 21:10:20
- 初始化 kasm-aarch64 模块文档
- 完成核心汇编宏和功能分析
- 识别使用场景和安全考虑
- 建立最佳实践和常见问题解答