use core::ptr::NonNull;

use alloc::vec::Vec;

use crate::{Direction, DmaHandle, get_osal};

pub struct ScatterEntry {
    pub handle: DmaHandle,
    pub offset: usize,
    pub size: usize,
}

pub struct DmaStream<'a, 'b> {
    scatter_list: &'b mut Vec<ScatterEntry>,
    _phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a, 'b> DmaStream<'a, 'b> {
    pub fn new(data: &'a [u8], tmp: &'b mut Vec<ScatterEntry>, direction: Direction) -> Self {
        tmp.clear();
        let mut remain = data.len();
        let addr = data.as_ptr() as usize;
        let align_addr = align_down(addr, get_osal().page_size());
        let mut offset = addr - align_addr;
        let mut addr = unsafe { NonNull::new_unchecked(align_addr as *mut u8) };
        while remain > 0 {
            let handle = crate::map(addr, remain, direction);
            let mut handle = ScatterEntry {
                offset,
                size: handle.layout.size(),
                handle,
            };
            if offset > 0 {
                handle.offset = offset;
                handle.size -= offset;
                offset = 0;
            }
            let mapped_size = handle.size;
            tmp.push(handle);
            remain -= mapped_size;
            addr = unsafe { NonNull::new_unchecked(addr.as_ptr().add(mapped_size)) };
        }

        Self {
            scatter_list: tmp,
            _phantom: core::marker::PhantomData,
        }
    }

    pub fn scatter_list(&self) -> &[ScatterEntry] {
        self.scatter_list.as_slice()
    }
}

fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}
