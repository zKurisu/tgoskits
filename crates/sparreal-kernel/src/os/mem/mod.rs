use buddy_system_allocator::LockedHeap;
use spin::Mutex;

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::empty();
