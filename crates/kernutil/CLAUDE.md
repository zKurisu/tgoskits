[根目录](../../CLAUDE.md) > [crates](../) > **kernutil**

# KernUtil - 内核实用工具库

## 模块职责

KernUtil 提供内核开发中常用的实用工具和抽象，包括地址管理、ID 生成、静态变量初始化等功能，简化内核开发并提高代码复用性。

## 入口与启动

### 主要入口点

- **`src/lib.rs`**: 库入口，导出所有实用工具
- **核心模块**: `address`, `id`, `staticcell`

## 对外接口

### 地址管理 (`address.rs`)

#### 物理地址抽象

```rust
pub struct PhysAddr {
    addr: usize,
}

impl PhysAddr {
    pub fn new(addr: usize) -> Self;
    pub fn get(&self) -> usize;
    pub fn align_down(&self, align: usize) -> Self;
    pub fn align_up(&self, align: usize) -> Self;
    pub fn is_aligned(&self, align: usize) -> bool;
}
```

#### 虚拟地址抽象

```rust
pub struct VirtAddr {
    addr: usize,
}

impl VirtAddr {
    pub fn new(addr: usize) -> Self;
    pub fn get(&self) -> usize;
    pub fn as_ptr<T>(&self) -> *const T;
    pub fn as_mut_ptr<T>(&self) -> *mut T;
    // 更多对齐和转换方法...
}
```

#### 地址转换

- 支持物理地址和虚拟地址之间的转换
- 提供便捷的地址对齐和操作方法

### ID 管理 (`id.rs`)

#### ID 生成器

```rust
pub struct IdGenerator<T> {
    next: T,
    phantom: PhantomData<T>,
}

impl<T: Id> IdGenerator<T> {
    pub fn new() -> Self;
    pub fn generate(&mut self) -> T;
    pub fn reset(&mut self);
}
```

#### ID Trait

```rust
pub trait Id: Copy + Clone + Debug + Eq + Ord + 'static {
    fn new(id: usize) -> Self;
    fn as_usize(&self) -> usize;
}
```

### 静态变量管理 (`staticcell.rs`)

#### 静态单元

```rust
pub struct StaticCell<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    init: AtomicBool,
}

impl<T> StaticCell<T> {
    pub const fn uninit() -> Self;
    pub fn init_or<F>(&self, f: F) -> &mut T where F: FnOnce() -> T;
    pub fn get(&self) -> Option<&T>;
    pub fn get_mut(&self) -> Option<&mut T>;
}
```

## 关键依赖与配置

### 核心依赖

```toml
[dependencies]
# 仅使用 core，无外部依赖
```

### 特性配置

- **`no_std`**: 完全不依赖标准库
- **零成本抽象**: 编译时优化，无运行时开销

## 数据模型

### 地址类型

- **`PhysAddr`**: 物理内存地址，用于硬件相关的地址操作
- **`VirtAddr`**: 虚拟内存地址，可转换为指针
- 地址对齐和边界检查
- 地址范围和偏移计算

### ID 系统

- **泛型 ID**: 支持任意类型的 ID 实现
- **唯一性保证**: 自动递增生成器
- **类型安全**: 编译时类型检查

### 静态初始化

- **延迟初始化**: 运行时一次性初始化
- **线程安全**: 使用原子操作确保安全性
- **零成本**: 初始化完成后无额外开销

## 使用场景

### 地址管理示例

```rust
use kernutil::{PhysAddr, VirtAddr};

// 物理地址操作
let paddr = PhysAddr::new(0x1000);
let aligned_paddr = paddr.align_down(0x1000);

// 虚拟地址操作
let vaddr = VirtAddr::new(0x8000_0000);
let ptr = vaddr.as_mut_ptr::<u32>();
```

### ID 生成示例

```rust
use kernutil::{IdGenerator, Id};

#[derive(Debug, Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct TaskId(u64);

impl Id for TaskId {
    fn new(id: usize) -> Self { TaskId(id as u64) }
    fn as_usize(&self) -> usize { self.0 as usize }
}

let mut id_gen = IdGenerator::new();
let task_id = id_gen.generate();
```

### 静态初始化示例

```rust
use kernutil::StaticCell;

static GLOBAL_DATA: StaticCell<MyStruct> = StaticCell::uninit();

fn get_global_data() -> &'static mut MyStruct {
    GLOBAL_DATA.init_or(|| MyStruct::new())
}
```

## 测试与质量

### 当前测试状态

- ❌ **单元测试**: 缺少正式的单元测试
- ⚠️ **集成测试**: 在其他 crate 中间接测试
- ✅ **编译测试**: 通过编译验证正确性

### 建议的测试策略

1. **地址操作测试**: 验证地址对齐、转换和边界检查
2. **ID 生成测试**: 验证唯一性和溢出处理
3. **并发测试**: 验证静态单元在多线程环境下的安全性
4. **性能测试**: 确保零成本抽象

## 常见问题 (FAQ)

### Q: PhysAddr 和 VirtAddr 有什么区别？

A: PhysAddr 用于物理内存地址操作，VirtAddr 可以转换为指针直接访问内存。

### Q: IdGenerator 如何处理溢出？

A: 当前实现会在溢出时 panic，可以根据需要实现循环或其他策略。

### Q: StaticCell 和 OnceCell 有什么不同？

A: StaticCell 专为内核环境设计，支持 no_std 并提供原子初始化保证。

### Q: 地址对齐的性能影响？

A: 所有对齐操作都是编译时优化的位运算，运行时无额外开销。

## 相关文件清单

### 核心文件

- `src/lib.rs` - 库入口和模块导出
- `src/address.rs` - 地址抽象实现
- `src/id.rs` - ID 生成器实现
- `src/staticcell.rs` - 静态单元实现
- `Cargo.toml` - 项目配置

### 文档文件

- `CLAUDE.md` - 本文档

---

## 变更记录 (Changelog)

### 2025-12-21 21:10:20

- 初始化 kernutil 模块文档
- 完成核心功能分析和使用示例
- 识别测试缺口和建议
- 建立常见问题解答
