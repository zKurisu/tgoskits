use crate::{
    FramAllocator, PageTableEntry, PagingError, PagingResult, PhysAddr, TableGeneric, VirtAddr,
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

/// 页表映射配置
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MapConfig<P: PageTableEntry> {
    pub vaddr: VirtAddr,
    pub paddr: PhysAddr,
    pub size: usize,
    /// Page Table Entry
    ///
    /// All pte will be set as this value, except the address bits
    pub pte: P,
    pub allow_huge: bool,
    pub flush: bool,
}

/// 内部映射递归配置
#[derive(Clone, Copy)]
struct MapRecursiveConfig<P: PageTableEntry> {
    start_vaddr: VirtAddr,
    start_paddr: PhysAddr,
    end_vaddr: VirtAddr,
    level: usize,
    allow_huge: bool,
    flush: bool,
    pte_template: P,
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

impl<P: PageTableEntry> core::fmt::Debug for MapConfig<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MapConfig")
            .field("vaddr", &format_args!("{:#x}", self.vaddr.raw()))
            .field("paddr", &format_args!("{:#x}", self.paddr.raw()))
            .field("size", &format_args!("{:#x}", self.size))
            .field("allow_huge", &self.allow_huge)
            .field("flush", &self.flush)
            .finish()
    }
}

/// 页表结构
pub struct PageTable<T: TableGeneric, A: FramAllocator> {
    root: Frame<T, A>,
}

impl<T: TableGeneric, A: FramAllocator> core::fmt::Debug for PageTable<T, A>
where
    T::P: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageTable")
            .field("root_paddr", &format_args!("{:#x}", self.root.paddr.raw()))
            .field("table_levels", &T::LEVEL)
            .field("max_block_level", &T::MAX_BLOCK_LEVEL)
            .field("page_size", &format_args!("{:#x}", T::PAGE_SIZE))
            .finish()
    }
}

impl<T: TableGeneric, A: FramAllocator> PageTable<T, A> {
    /// 创建一个新的页表
    pub fn new(allocator: A) -> PagingResult<Self> {
        let root = Frame::new(allocator)?;
        Ok(Self { root })
    }

    /// 映射虚拟地址范围到物理地址范围
    pub fn map(&mut self, config: &MapConfig<T::P>) -> PagingResult {
        // 验证输入参数
        self.validate_map_config(config)?;

        // 检查大小溢出
        if config.vaddr.raw().checked_add(config.size).is_none()
            || config.paddr.raw().checked_add(config.size).is_none()
        {
            return Err(PagingError::address_overflow(
                "Virtual or physical address overflow",
            ));
        }

        self.root.map_range_recursive(MapRecursiveConfig {
            start_vaddr: config.vaddr,
            start_paddr: config.paddr,
            end_vaddr: config.vaddr + config.size,
            level: T::LEVEL,
            allow_huge: config.allow_huge,
            flush: config.flush,
            pte_template: config.pte,
        })?;

        Ok(())
    }

    /// 创建页表遍历迭代器
    pub fn walk(&self, config: WalkConfig) -> PageTableWalker<T, A> {
        PageTableWalker::new(self, config)
    }

    /// 遍历所有有效页表项
    pub fn walk_valid(&self) -> PageTableWalker<T, A> {
        let config = WalkConfig {
            start_vaddr: VirtAddr::new(0),
            end_vaddr: VirtAddr::new(core::usize::MAX),
            visit_invalid: false,
        };
        self.walk(config)
    }

    /// 验证映射配置的有效性
    fn validate_map_config(&self, config: &MapConfig<T::P>) -> PagingResult {
        if config.size == 0 {
            return Err(PagingError::invalid_size("Size cannot be zero"));
        }

        // 检查虚拟地址和物理地址是否页对齐
        if config.vaddr.raw() % T::PAGE_SIZE != 0 {
            return Err(PagingError::alignment_error(
                "Virtual address not page aligned",
            ));
        }

        if config.paddr.raw() % T::PAGE_SIZE != 0 {
            return Err(PagingError::alignment_error(
                "Physical address not page aligned",
            ));
        }

        Ok(())
    }


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

    fn new(allocator: A) -> PagingResult<Self> {
        let paddr = allocator.alloc_frame().ok_or(PagingError::NoMemory)?;
        unsafe {
            let vaddr = allocator.phys_to_virt(paddr);
            core::ptr::write_bytes(vaddr, 0, T::PAGE_SIZE);
        }

        Ok(Self {
            paddr,
            allocator,
            _marker: core::marker::PhantomData,
        })
    }

    fn as_slice_mut(&mut self) -> &mut [T::P] {
        let vaddr = self.allocator.phys_to_virt(self.paddr);
        unsafe { core::slice::from_raw_parts_mut(vaddr as *mut T::P, T::TABLE_LEN) }
    }

    #[allow(dead_code)]
    fn as_slice(&self) -> &[T::P] {
        let vaddr = self.allocator.phys_to_virt(self.paddr);
        unsafe { core::slice::from_raw_parts(vaddr as *const T::P, T::TABLE_LEN) }
    }

    /// 获取指定索引对应的虚拟地址
    fn index_to_vaddr(&self, index: usize, level: usize, base_vaddr: VirtAddr) -> VirtAddr {
        let entry_size = Self::level_size(level);
        base_vaddr + index * entry_size
    }

    /// 根据页表项索引和级别计算准确的虚拟地址
    fn calculate_vaddr_for_entry(index: usize, level: usize, base_vaddr: VirtAddr) -> VirtAddr {
        if level == 1 {
            // 叶子页表级别，直接计算
            base_vaddr + index * T::PAGE_SIZE
        } else {
            // 更高级别，需要考虑整个子表覆盖的地址范围
            let entry_size = Self::level_size(level);
            base_vaddr + index * entry_size
        }
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

  
    /// 递归映射的核心实现
    fn map_range_recursive(&mut self, config: MapRecursiveConfig<T::P>) -> PagingResult<()> {
        let mut vaddr = config.start_vaddr;
        let mut paddr = config.start_paddr;

        while vaddr < config.end_vaddr {
            let index = Self::virt_to_index(vaddr, config.level);
            let level_size = Self::level_size(config.level);
            let remaining_size = config.end_vaddr - vaddr;

            // 检查是否可以使用大页映射
            if config.allow_huge
                && config.level <= T::MAX_BLOCK_LEVEL
                && level_size <= remaining_size
                && vaddr.raw() % level_size == 0
                && paddr.raw() % level_size == 0
            {
                // 创建大页映射
                let entries = self.as_slice_mut();
                let pte_ref = &mut entries[index];
                if pte_ref.valid() {
                    return Err(PagingError::mapping_conflict(vaddr, pte_ref.paddr()));
                }

                let mut new_pte = config.pte_template;
                new_pte.set_paddr(paddr);
                new_pte.set_valid(true);
                new_pte.set_is_huge(true);
                *pte_ref = new_pte;

                vaddr += level_size;
                paddr += level_size;
                continue;
            }

            // 如果到达页表级别，进行普通页映射
            if config.level == 1 {
                let entries = self.as_slice_mut();
                let pte_ref = &mut entries[index];
                if pte_ref.valid() {
                    return Err(PagingError::mapping_conflict(vaddr, pte_ref.paddr()));
                }

                let mut new_pte = config.pte_template;
                new_pte.set_paddr(paddr);
                new_pte.set_valid(true);
                new_pte.set_is_huge(false);
                *pte_ref = new_pte;

                vaddr += T::PAGE_SIZE;
                paddr += T::PAGE_SIZE;
                continue;
            }

            // 检查当前页表项状态并决定如何处理
            let allocator = self.allocator;
            let current_pte = self.as_slice()[index];

            let child_frame = if current_pte.valid() {
                if current_pte.is_huge() {
                    return Err(PagingError::hierarchy_error(
                        "Cannot create page table under huge page",
                    ));
                }

                // 子页表已存在，获取它
                Frame {
                    paddr: current_pte.paddr(),
                    allocator,
                    _marker: core::marker::PhantomData::<T>,
                }
            } else {
                // 需要创建新的子页表
                let new_frame = Frame::new(allocator)?;
                let new_frame_paddr = new_frame.paddr;

                // 链接子页表
                let entries = self.as_slice_mut();
                let pte_ref = &mut entries[index];
                let mut new_pte = config.pte_template;
                new_pte.set_paddr(new_frame_paddr);
                new_pte.set_valid(true);
                new_pte.set_is_huge(false);
                *pte_ref = new_pte;

                new_frame
            };

            let next_level_vaddr = vaddr + level_size.min(config.end_vaddr - vaddr);
            let mut child_frame = child_frame;
            let child_config = MapRecursiveConfig {
                start_vaddr: vaddr,
                start_paddr: paddr,
                end_vaddr: next_level_vaddr,
                level: config.level - 1,
                allow_huge: config.allow_huge,
                flush: config.flush,
                pte_template: config.pte_template,
            };
            child_frame.map_range_recursive(child_config)?;

            vaddr = next_level_vaddr;
            paddr += next_level_vaddr - vaddr;
        }

        Ok(())
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
    fn new(page_table: &'a PageTable<T, A>, config: WalkConfig) -> Self {
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

