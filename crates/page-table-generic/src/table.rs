use crate::{
    FramAllocator, PageTableEntry, PagingError, PagingResult, TableGeneric, VirtAddr,
    frame::Frame,
    map::{MapConfig, MapRecursiveConfig},
    walk::{PageTableWalker, WalkConfig},
};

/// 页表结构
pub struct PageTable<T: TableGeneric, A: FramAllocator> {
    pub root: Frame<T, A>,
}

impl<T: TableGeneric, A: FramAllocator> core::fmt::Debug for PageTable<T, A>
where
    T::P: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageTable")
            .field("root_paddr", &format_args!("{:#x}", self.root.paddr.raw()))
            .field("table_levels", &T::LEVEL_BITS.len())
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
            level: Frame::<T, A>::PT_LEVEL,
            allow_huge: config.allow_huge,
            flush: config.flush,
            pte_template: config.pte,
        })?;

        Ok(())
    }

    /// 创建页表遍历迭代器
    pub fn walk_all(&self, config: WalkConfig) -> PageTableWalker<T, A> {
        PageTableWalker::new(self, config)
    }

    pub fn walk(
        &self,
        config: WalkConfig,
    ) -> impl Iterator<Item = crate::walk::PteInfo<T::P>> + '_ {
        PageTableWalker::new(self, config).filter(|p| p.pte.valid())
    }

    /// 遍历所有有效的最终映射页表项（过滤掉无效项和中间级别的页表指针）
    pub fn walk_valid(&self) -> impl Iterator<Item = crate::walk::PteInfo<T::P>> + '_ {
        let config = WalkConfig {
            start_vaddr: VirtAddr::new(0),
            end_vaddr: VirtAddr::new(usize::MAX),
        };
        self.walk(config)
            .filter(|p| p.pte.valid() && p.is_final_mapping)
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

    pub const fn page_size() -> usize {
        T::PAGE_SIZE
    }

    pub const fn table_levels() -> usize {
        T::LEVEL_BITS.len()
    }

    pub const fn valid_bits() -> usize {
        Frame::<T, A>::PT_VALID_BITS
    }

    /// 销毁整个页表结构
    ///
    /// 此方法会：
    /// 1. 递归释放根帧及所有子帧
    /// 2. 释放页表占用的所有内存
    /// 3. 在释放前将所有PTE设为invalid
    ///
    /// # Safety
    /// 调用者必须确保：
    /// - 没有其他代码在访问这个页表
    /// - 没有CPU正在使用这个页表进行地址翻译
    /// - 调用后不再使用这个PageTable实例
    pub fn destroy(self) {
        // 递归释放根帧及其所有子帧
        self.root.deallocate_recursive();
    }

    /// 释放页表占用的所有帧
    ///
    /// 与destroy()不同，这个方法保留PageTable结构，
    /// 但释放所有关联的帧。调用后PageTable不再可用。
    pub fn deallocate(self) {
        // 递归释放根帧及其所有子帧
        self.root.deallocate_recursive();

        // 注意：self会在函数结束时被drop，但由于所有帧都已释放，
        // 这不会导致双重释放
    }

    /// 释放页表中的指定映射区域
    ///
    /// 释放指定虚拟地址范围内的所有页表项和子页表帧
    /// 在释放前将相关PTE设为invalid
    pub fn deallocate_range(&mut self, start_vaddr: VirtAddr, end_vaddr: VirtAddr) -> PagingResult {
        if start_vaddr >= end_vaddr {
            return Err(PagingError::invalid_range(
                "Start address must be less than end address",
            ));
        }

        // TODO: 实现范围释放逻辑
        // 这里需要实现：
        // 1. 遍历指定虚拟地址范围
        // 2. 释放对应的页表项和子页表
        // 3. 处理部分页表项的情况

        Ok(())
    }

    /// 获取页表的根帧物理地址
    pub fn root_paddr(&self) -> crate::PhysAddr {
        self.root.paddr
    }
}
