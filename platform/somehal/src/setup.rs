use someboot::PagingResult;

pub trait KernelOp {
    fn ioremap(&self, paddr: usize, size: usize) -> PagingResult<*mut u8>;
}

struct EmptyKernelOp;

impl KernelOp for EmptyKernelOp {
    fn ioremap(&self, _paddr: usize, _size: usize) -> PagingResult<*mut u8> {
        unimplemented!()
    }
}

static mut KERNEL_OP: &'static dyn KernelOp = &EmptyKernelOp;

pub(crate) fn set_kernel_op(op: &'static dyn KernelOp) {
    unsafe {
        KERNEL_OP = op;
    }
}

pub(crate) fn kernel() -> &'static dyn KernelOp {
    unsafe { KERNEL_OP }
}
