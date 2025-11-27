#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate sparreal_rt;

#[sparreal_rt::entry]
fn main() {
    println!("Hello, world!");

    // 测试 Page Fault: 访问一个未映射的地址
    println!("Testing page fault by accessing unmapped address 0x6000_0000_0000...");
    unsafe {
        let ptr = 0x6000_0000_0000usize as *const u64;
        let _value = core::ptr::read_volatile(ptr);
    }

    println!("All tests passed!");
}
