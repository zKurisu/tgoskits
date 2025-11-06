use crate::{FramAllocator, PageTable, PageTableEntry, PhysAddr, TableGeneric, VirtAddr};

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
    /// 此页表项是否为最终映射（叶子级别或大页）
    /// - true: 有效的叶子级别映射或大页映射
    /// - false: 无效项或中间级别的页表指针
    pub is_final_mapping: bool,
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
}

/// 页表遍历迭代器
pub struct PageTableWalker<'a, T: TableGeneric, A: FramAllocator> {
    _phantom: core::marker::PhantomData<&'a ()>,
    config: WalkConfig,
    // 内部状态管理 - 使用heapless::Vec
    stack: Vec<WalkState<T, A>, MAX_WALK_DEPTH>,
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
    fn as_slice(&self) -> &[T::P] {
        let vaddr = self.allocator.phys_to_virt(self.paddr);
        unsafe { core::slice::from_raw_parts(vaddr as *const T::P, T::TABLE_LEN) }
    }

    /// 重建完整的虚拟地址
    fn reconstruct_vaddr(index: usize, level: usize, base_vaddr: VirtAddr) -> VirtAddr {
        // 每个级别的条目覆盖的地址空间大小
        let entry_size = Self::level_size(level);
        base_vaddr + index * entry_size
    }

    /// 计算指定级别对应的映射大小（通用版本）
    fn level_size(level: usize) -> usize {
        // 每个级别的条目覆盖的地址空间大小
        // Level N: PAGE_SIZE << (INDEX_BITS * (N - 1))
        // Level 4: PAGE_SIZE << (INDEX_BITS * 3) = 512GB
        // Level 3: PAGE_SIZE << (INDEX_BITS * 2) = 1GB
        // Level 2: PAGE_SIZE << (INDEX_BITS * 1) = 2MB
        // Level 1: PAGE_SIZE << (INDEX_BITS * 0) = 4KB
        T::PAGE_SIZE << (T::INDEX_BITS * (level - 1))
    }
}

impl<'a, T: TableGeneric, A: FramAllocator> PageTableWalker<'a, T, A> {
    /// 创建新的页表遍历器
    pub fn new(_page_table: &'a PageTable<T, A>, config: WalkConfig) -> Self {
        let mut walker = Self {
            _phantom: core::marker::PhantomData,
            config,
            stack: Vec::new(),
            finished: false,
        };

        // 初始化栈，从根页表开始
        if !walker.config.start_vaddr.ge(&walker.config.end_vaddr) {
            let root_state = WalkState {
                frame: Frame {
                    paddr: _page_table.root.paddr,
                    allocator: _page_table.root.allocator,
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

    /// 查找下一个页表项（遍历所有项）
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
            let current_vaddr =
                Frame::<T, A>::reconstruct_vaddr(state.index - 1, state.level, state.base_vaddr);

            // 跳过不在范围内的地址
            if current_vaddr < self.config.start_vaddr {
                continue;
            }

            if current_vaddr >= self.config.end_vaddr {
                self.finished = true;
                return None;
            }

            // 判断是否为最终映射
            // - 无效项：不是最终映射
            // - 有效且是大页：是最终映射
            // - 有效且在叶子级别（level == 1）：是最终映射
            // - 有效但在中间级别且不是大页：不是最终映射（页表指针）
            let is_final_mapping = pte.valid() && (pte.is_huge() || state.level == 1);

            // 如果是有效的子页表项（中间级别的页表指针），需要深入下一级
            if pte.valid() && !pte.is_huge() && state.level > 1 {
                let child_frame = Frame {
                    paddr: pte.paddr(),
                    allocator: state.frame.allocator,
                    _marker: core::marker::PhantomData,
                };

                // 计算子页表的基地址：当前条目的虚拟地址就是子页表覆盖的地址范围起点
                let child_base_vaddr = current_vaddr;

                // 创建子页表状态并压入栈中
                let child_state = WalkState {
                    frame: child_frame,
                    level: state.level - 1,
                    index: 0,
                    base_vaddr: child_base_vaddr,
                };

                // 先返回当前中间级别的页表项，然后压入子页表状态
                let level = state.level;
                let vaddr = current_vaddr;

                self.stack.push(child_state).ok();

                return Some(PteInfo {
                    level,
                    vaddr,
                    pte,
                    is_final_mapping,
                });
            }

            // 返回页表项信息（无效项、叶子级别或大页）
            return Some(PteInfo {
                level: state.level,
                vaddr: current_vaddr,
                pte,
                is_final_mapping,
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
