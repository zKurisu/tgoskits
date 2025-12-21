[根目录](../../CLAUDE.md) > [crates](../) > **dma-api**

# DMA API - DMA 操作抽象接口

## 模块职责

DMA API 提供了统一的 DMA（Direct Memory Access）操作抽象接口，支持不同架构和平台的 DMA 内存管理、缓存同步和流传输操作，为内核和驱动开发提供标准化的 DMA 操作接口。

## 入口与启动

### 主要入口点
- **`src/lib.rs`**: 库入口，导出核心接口和类型
- **抽象层**: 提供跨平台的 DMA 操作统一接口

## 对外接口

### 核心 Trait

#### `Osal` - 操作系统抽象层
```rust
pub trait Osal {
    /// DMA 内存分配
    fn alloc_coherent(&self, size: usize, align: usize) -> (*mut u8, PhysAddr);

    /// DMA 内存释放
    fn free_coherent(&self, vaddr: *mut u8, size: usize);

    /// 缓存同步
    fn sync(&self, vaddr: *mut u8, size: usize, dir: Direction);
}
```

### DMA 方向 (`stream.rs`)
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    /// 设备到内存
    DeviceToMemory,
    /// 内存到设备
    MemoryToDevice,
    /// 双向（内存到设备再到内存）
    Bidirectional,
}
```

### 流管理器
```rust
pub struct StreamManager<O: Osal> {
    osal: O,
}

impl<O: Osal> StreamManager<O> {
    pub fn new(osal: O) -> Self;
    pub fn alloc_coherent(&self, size: usize) -> Result<DmaBuffer>;
    pub fn create_stream(&self) -> Stream;
    pub fn sync(&self, buffer: &DmaBuffer, dir: Direction);
}
```

### DMA 缓冲区
```rust
pub struct DmaBuffer {
    vaddr: *mut u8,
    paddr: PhysAddr,
    size: usize,
}

impl DmaBuffer {
    pub fn vaddr(&self) -> *mut u8;
    pub fn paddr(&self) -> PhysAddr;
    pub fn size(&self) -> usize;
    pub fn as_slice(&self) -> &[u8];
    pub fn as_mut_slice(&mut self) -> &mut [u8];
}
```

## 关键依赖与配置

### 核心依赖
```toml
[dependencies]
# 无外部依赖，仅使用 core
```

### 特性配置
- **`alloc`**: 启用内存分配支持（默认）
- **`no_std`**: 不依赖标准库

## 平台实现

### AArch64 实现 (`osal/aarch64.rs`)
- 使用 ARMv8 的缓存维护指令
- 支持非一致内存架构 (NUMA)
- 实现高效的缓存同步

### NOP 实现 (`osal/nop.rs`)
- 空实现，用于测试和一致性内存架构
- 所有操作都是空操作，无实际开销

### 平台选择
通过 feature flags 或编译时配置选择具体实现：
```rust
#[cfg(target_arch = "aarch64")]
type OsalImpl = aarch64::Osal;

#[cfg(not(target_arch = "aarch64"))]
type OsalImpl = nop::Osal;
```

## 数据模型

### DMA 传输类型
1. **一致性 DMA** (`alloc_coherent`)
   - CPU 和设备始终看到相同的数据
   - 适用于小数据量或频繁访问的缓冲区
   - 性能开销较大

2. **流式 DMA** (`create_stream`)
   - 显式的缓存同步
   - 适用于大数据量传输
   - 性能更好但需要手动管理缓存

### 缓存同步策略
- **预取**: 从设备到内存传输前的缓存失效
- **回写**: 从内存到设备传输前的缓存写回
- **双向**: 完整的双向同步

### 对齐要求
- 缓存行对齐（通常是 64 字节）
- 页对齐用于大缓冲区
- DMA 控制器特定的对齐要求

## 使用场景

### 基本使用示例
```rust
use dma_api::{StreamManager, Direction};

// 创建流管理器
let manager = StreamManager::new(osal);

// 分配一致性 DMA 内存
let buffer = manager.alloc_coherent(1024)?;
unsafe {
    // 写入数据
    buffer.as_mut_slice().fill(0);

    // 同步到设备
    manager.sync(&buffer, Direction::MemoryToDevice);
}

// 传输完成后同步回 CPU
manager.sync(&buffer, Direction::DeviceToMemory);
```

### 流式 DMA 示例
```rust
use dma_api::Stream;

// 创建 DMA 流
let stream = manager.create_stream();

// 大数据传输
let mut buffer = manager.alloc_coherent(64 * 1024)?;
stream.prepare(&mut buffer, Direction::Bidirectional)?;

// 启动 DMA 传输
stream.start_transfer();

// 等待完成
while !stream.is_complete() {
    // 可以做其他工作
}

// 同步缓存
stream.complete(Direction::DeviceToMemory);
```

## 测试与质量

### 当前测试状态
- ❌ **单元测试**: 缺少正式的单元测试
- ⚠️ **集成测试**: 依赖硬件平台测试
- ❌ **性能测试**: 缺少性能基准测试

### 建议的测试策略
1. **模拟测试**: 使用 NOP 实现进行基本功能测试
2. **缓存一致性**: 验证各种缓存同步场景
3. **对齐测试**: 验证不同对齐要求的处理
4. **并发测试**: 多线程环境下的安全性测试
5. **性能基准**: 与直接内存访问的性能对比

## 架构考虑

### 内存屏障
- 确保内存访问顺序
- 防止编译器重排
- 处理弱一致性内存模型

### 中断安全
- DMA 完成中断的处理
- 原子操作的考虑
- 与其他内核子系统的交互

### 错误处理
- 内存分配失败
- 传输超时
- 硬件错误报告

## 常见问题 (FAQ)

### Q: 一致性 DMA 和流式 DMA 有什么区别？
A: 一致性 DMA 始终保持缓存一致性，但性能开销大；流式 DMA 需要手动同步缓存，但性能更好。

### Q: 如何选择 DMA 方向？
A: 根据数据流向选择：设备到内存（读取）、内存到设备（写入）、双向（既读又写）。

### Q: DMA 缓冲区需要对齐吗？
A: 是的，通常需要缓存行对齐（64字节）或页对齐，取决于具体硬件要求。

### Q: 如何处理 DMA 传输错误？
A: 检查硬件状态寄存器，实现重试机制，并提供错误回调。

## 相关文件清单

### 核心文件
- `src/lib.rs` - 库入口和核心接口
- `src/stream.rs` - DMA 流管理实现
- `src/osal/mod.rs` - OS 抽象层接口
- `src/osal/aarch64.rs` - AArch64 平台实现
- `src/osal/nop.rs` - NOP 实现用于测试
- `README.md` - 项目说明

### 配置文件
- `Cargo.toml` - 项目配置

---

## 变更记录 (Changelog)

### 2025-12-21 21:10:20
- 初始化 dma-api 模块文档
- 完成接口分析和使用示例
- 识别平台实现和测试策略
- 建立常见问题解答