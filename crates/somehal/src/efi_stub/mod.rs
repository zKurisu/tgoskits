use core::fmt::Write;

use rgb::RGB8;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi_raw::table::system::SystemTable;

use crate::arch::relocate;

pub mod pe;

// 定义红色常量
const RED: RGB8 = RGB8::new(255, 0, 0);

// 长方体结构体
struct Rectangle {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

// 像素绘制函数
unsafe fn set_pixel(
    framebuffer: *mut u8,
    x: usize,
    y: usize,
    width: usize,
    pixel_format: uefi::proto::console::gop::PixelFormat,
    color: RGB8,
) {
    let pixel_size = 4; // 假设32位色深
    let offset = (y * width + x) * pixel_size;
    unsafe {
        let pixel_ptr = framebuffer.add(offset);

        match pixel_format {
            uefi::proto::console::gop::PixelFormat::Rgb => {
                *pixel_ptr = color.r;
                *pixel_ptr.add(1) = color.g;
                *pixel_ptr.add(2) = color.b;
            }
            uefi::proto::console::gop::PixelFormat::Bgr => {
                *pixel_ptr = color.b;
                *pixel_ptr.add(1) = color.g;
                *pixel_ptr.add(2) = color.r;
            }
            _ => {} // 其他格式暂不支持
        }
    }
}

// 长方体绘制函数
unsafe fn draw_rectangle(
    framebuffer: *mut u8,
    rect: &Rectangle,
    screen_width: usize,
    pixel_format: uefi::proto::console::gop::PixelFormat,
    color: RGB8,
) {
    unsafe {
        for y in rect.y..(rect.y + rect.height) {
            for x in rect.x..(rect.x + rect.width) {
                set_pixel(framebuffer, x, y, screen_width, pixel_format, color);
            }
        }
    }
}

/// EFI PE 入口点 - 符合 EFI ABI 的汇编包装
/// 参数: a0 = image_handle, a1 = system_table
#[unsafe(export_name = "efi_pe_entry")]
#[unsafe(link_section = ".text")]
pub unsafe extern "C" fn efi_pe_entry(
    image_handle: Handle,
    system_table: *const SystemTable,
) -> Status {
    unsafe {
        relocate();
        ::uefi::boot::set_image_handle(image_handle);
        ::uefi::table::set_system_table(system_table);

        crate::console::set_printer(&UefiPrinter);

        println!("Hello {}", 123);

        // 步骤1：获取UEFI图形协议服务
        let handle = ::uefi::boot::image_handle();
        let gop_result = ::uefi::boot::open_protocol_exclusive::<GraphicsOutput>(handle);

        match gop_result {
            Ok(mut gop) => {
                println!("gop success!");

                // 步骤2：查询并设置图形模式
                let mode_info = gop.current_mode_info();
                let mut framebuffer = gop.frame_buffer();
                let pixel_format = mode_info.pixel_format();

                println!(
                    "resv: {}x{}",
                    mode_info.resolution().0,
                    mode_info.resolution().1
                );
                println!("fmt: {:?}", pixel_format);

                // 步骤3：定义长方体参数
                let rect = Rectangle {
                    x: 100,
                    y: 100,
                    width: 200,
                    height: 100,
                };

                // 步骤4：绘制红色长方体
                draw_rectangle(
                    framebuffer.as_mut_ptr(),
                    &rect,
                    mode_info.resolution().0,
                    pixel_format,
                    RED,
                );

                println!("rect drawn!");

                // 步骤5：实现持续动画效果
                let mut offset = 0i32;
                let mut direction = 1i32;
                loop {
                    // 清除之前的长方体（可选）
                    // 这里简单地移动长方体位置

                    let animated_rect = Rectangle {
                        x: (100 + offset) as usize,
                        y: 100,
                        width: 200,
                        height: 100,
                    };

                    // 绘制新位置的长方体
                    draw_rectangle(
                        framebuffer.as_mut_ptr(),
                        &animated_rect,
                        mode_info.resolution().0,
                        pixel_format,
                        RED,
                    );

                    // 更新偏移量
                    offset += direction * 2;
                    if offset > 200 || offset == 0 {
                        direction = -direction;
                    }

                    // 简单延时
                    for _ in 0..1000000 {
                        ::core::hint::spin_loop();
                    }
                }
            }
            Err(e) => {
                println!("got fail: {:?}", e);
            }
        }

        crate::arch::entry::prepare_kernel_entry();
    }

    // 返回成功状态
    Status::SUCCESS
}

struct UefiPrinter;
impl crate::console::Printer for UefiPrinter {
    fn read_byte(&self) -> Option<u8> {
        // system::with_stdin(|stdin| {
        //     let mut buffer = [0u16; 1];
        //     match stdin.read_key(&mut buffer) {
        //         Ok(()) => Some(buffer[0] as u8),
        //         Err(_) => None,
        //     }
        // })
        None
    }

    fn write_str(&self, s: &str) {
        system::with_stdout(|stdout| {
            let _ = stdout.write_str(s);
        });
    }
}
