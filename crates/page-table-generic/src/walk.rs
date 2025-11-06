use crate::{
    FramAllocator, PageTable, PageTableEntry, PhysAddr, TableGeneric, VirtAddr,
};

use heapless::Vec;

/// Maximum stack depth for page table walker
const MAX_WALK_DEPTH: usize = 8;

/// 页表项信息，包含原PTE对象
#[derive(Debug, Clone, Copy)]
pub struct PteInfo<P: PageTableEntry> {
    /// 页表级别（1=叶子页表，数字越大级别越高）
    pub level: usize,
    /// 此页表项对应的虚拟地址
    pub vaddr: VirtAddr,
    /// 原页表项对象
    pub pte: P,
}

/// 页表遍历配置
#[derive(Debug, Clone, Copy)]
pub struct WalkConfig {
    /// 起始虚拟地址（包含）
    pub start_vaddr: VirtAddr,
    /// 结束虚拟地址（不包含）
    pub end_vaddr: VirtAddr,
    /// 是否访问无效的页表项
    pub visit_invalid: bool,
}

/// 页表遍历迭代器
pub struct PageTableWalker<'a, T: TableGeneric, A: FramAllocator> {
    page_table: &'a PageTable<T, A>,
    config: WalkConfig,
    // 内部状态管理 - 使用heapless::Vec
    stack: Vec<WalkState<T, A>, MAX_WALK_DEPTH>,
    current_vaddr: VirtAddr,
    finished: bool,
}

/// 遍历状态
#[derive(Clone, Copy)]
struct WalkState<T: TableGeneric, A: FramAllocator> {
    frame: Frame<T, A>,
    level: usize,
    index: usize,
    base_vaddr: VirtAddr,
}

#[derive(Clone, Copy)]
struct Frame<T: TableGeneric, A: FramAllocator> {
    paddr: PhysAddr,
    allocator: A,
    _marker: core::marker::PhantomData<T>,
}

impl<T: TableGeneric, A: FramAllocator> core::fmt::Debug for Frame<T, A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Frame")
            .field("paddr", &format_args!("{:#x}", self.paddr.raw()))
            .finish()
    }
}

impl<T, A> Frame<T, A>
where
    T: TableGeneric,
    A: FramAllocator,
{
    const INDEX_MASK: usize = (1 << T::INDEX_BITS) - 1;

    fn as_slice(&self) -> &[T::P] {
        let vaddr = self.allocator.phys_to_virt(self.paddr);
        unsafe { core::slice::from_raw_parts(vaddr as *const T::P, T::TABLE_LEN) }
    }

    /// 重建完整的虚拟地址
    fn reconstruct_vaddr(index: usize, level: usize, base_vaddr: VirtAddr) -> VirtAddr {
        // 对于页表遍历，我们需要重建完整的虚拟地址
        // 使用更精确的计算方法
        if level == 1 {
            // 叶子级别，每个条目对应一个页面
            base_vaddr + index * T::PAGE_SIZE
        } else {
            // 更高级别，每个条目对应一个子表
            let entry_size = Self::level_size(level);
            base_vaddr + index * entry_size
        }
    }

    /// 计算指定级别对应的映射大小（通用版本）
    pub fn level_size(level: usize) -> usize {
        if level == T::LEVEL {
            // 最后一级是页级别
            T::PAGE_SIZE
        } else if level > T::MAX_BLOCK_LEVEL {
            // 不支持大页的级别
            T::PAGE_SIZE
        } else {
            // 大页级别：页面大小 * 2^(索引位数 * (总级别 - 当前级别))
            T::PAGE_SIZE << (T::INDEX_BITS * (T::LEVEL - level))
        }
    }

    /// 计算指定级别的页表索引（通用版本）
    pub fn virt_to_index(vaddr: VirtAddr, level: usize) -> usize {
        if level == 0 || level > T::LEVEL {
            panic!("Invalid level: {} (valid: 1..{})", level, T::LEVEL);
        }
        // 计算当前级别的位移：页面大小的对数 + (总级别 - 当前级别) * 索引位数
        let page_shift = T::PAGE_SIZE.trailing_zeros() as usize;
        let shift = page_shift + (T::LEVEL - level) * T::INDEX_BITS;
        (vaddr.raw() >> shift) & Self::INDEX_MASK
    }
}

impl<'a, T: TableGeneric, A: FramAllocator> PageTableWalker<'a, T, A> {
    /// 创建新的页表遍历器
    pub fn new(page_table: &'a PageTable<T, A>, config: WalkConfig) -> Self {
        let mut walker = Self {
            page_table,
            config,
            stack: Vec::new(),
            current_vaddr: config.start_vaddr,
            finished: false,
        };

        // 初始化栈，从根页表开始
        if !walker.config.start_vaddr.ge(&walker.config.end_vaddr) {
            let root_state = WalkState {
                frame: Frame {
                    paddr: page_table.root.paddr,
                    allocator: page_table.root.allocator,
                    _marker: core::marker::PhantomData,
                },
                level: T::LEVEL,
                index: 0,
                base_vaddr: VirtAddr::new(0),
            };
            walker.stack.push(root_state).ok(); // 栈容量足够时一定成功
        } else {
            walker.finished = true;
        }

        walker
    }

    /// 查找下一个有效的页表项
    fn find_next_entry(&mut self) -> Option<PteInfo<T::P>> {
        loop {
            if self.finished {
                return None;
            }

            if self.stack.is_empty() {
                self.finished = true;
                return None;
            }

            let state = self.stack.last_mut().unwrap();

            // 检查当前级别是否还有更多条目
            if state.index >= T::TABLE_LEN {
                self.stack.pop();
                continue;
            }

            // 获取页表项
            let entries = state.frame.as_slice();
            let pte = entries[state.index];
            state.index += 1;

            // 获取当前条目的虚拟地址 - 重建完整的虚拟地址
            let current_vaddr = Frame::<T, A>::reconstruct_vaddr(state.index - 1, state.level, state.base_vaddr);

            // 跳过不在范围内的地址
            if current_vaddr < self.config.start_vaddr {
                continue;
            }

            if current_vaddr >= self.config.end_vaddr {
                self.finished = true;
                return None;
            }

            // 根据配置决定是否处理无效项
            if !pte.valid() && !self.config.visit_invalid {
                continue;
            }

            // 如果是有效的子页表项，需要深入下一级
            if pte.valid() && !pte.is_huge() && state.level > 1 {
                let child_frame = Frame {
                    paddr: pte.paddr(),
                    allocator: state.frame.allocator,
                    _marker: core::marker::PhantomData,
                };

                // 计算子页表的基地址：当前条目的虚拟地址就是子页表覆盖的地址范围起点
                let child_base_vaddr = current_vaddr;

                // 创建子页表状态并压入栈中，优先处理子页表
                let child_state = WalkState {
                    frame: child_frame,
                    level: state.level - 1,
                    index: 0,
                    base_vaddr: child_base_vaddr,
                };

                self.stack.push(child_state).ok(); // 栈容量足够时一定成功
                continue;
            }

            // 返回找到的页表项信息
            return Some(PteInfo {
                level: state.level,
                vaddr: current_vaddr,
                pte,
            });
        }
    }
}

impl<'a, T: TableGeneric, A: FramAllocator> Iterator for PageTableWalker<'a, T, A> {
    type Item = PteInfo<T::P>;

    fn next(&mut self) -> Option<Self::Item> {
        self.find_next_entry()
    }
}