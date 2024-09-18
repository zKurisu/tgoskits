use sparreal_kernel::mem::Phys;

pub struct MemoryRange {
    pub name: &'static str,
    pub start: Phys<u8>,
    pub size: usize,
}

#[derive(Default)]
pub struct MemoryMap {
    pub memory_ranges: [Option<MemoryRange>; 10],
}
