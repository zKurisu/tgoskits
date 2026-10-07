//! 复位与启动控制寄存器：对端核（C906L）与整芯片。
//!
//! 两部分内容：
//! - [`CoreCtl`]：对端核的复位与启动。这里操作的三个寄存器组就是 FSBL `reset_c906l()`
//!   用的那几个（`LicheeRV-Nano-Build/fsbl/plat/cv181x/platform.c`），顺序不能改：
//!   **先按住复位 → 使能该核并写入口地址 → 再放开复位**。反过来会让小核从一个
//!   内容还没写好的地址取指。
//! - [`ChipReset`]：整芯片软复位（RTC 域）。序列与 U-Boot 自己的 `cv_system_reset()`
//!   （`u-boot-2021.10/board/cvitek/*/board.c`，即 `reset` 命令走的路径）逐字一致。
//!
//! 为什么需要这一层：小核由 FSBL 在开机时释放，而冷启动会偶发"根本没收过释放"
//! （`diag` 看到小核 PC 恒为 0），过去只能断电重来。大核在 S 模式下可以直接写
//! 这几个寄存器，于是有了"现场把它重新放开"这条恢复路径；判据与实板现象见
//! `docs/11` 第 8.1、8.4 节。
//! 整芯片复位是同一个动机的另一半：迭代内核要在 U-Boot 里刷 SD 卡，而这片板上
//! 没有别的办法从运行中的系统回到 U-Boot（`reboot` 只是用户态软重启）。

use core::ptr::{read_volatile, write_volatile};

/// RSTC 基址：`SOFT_CPU_RSTN` 在这里。
pub const RSTC_BASE: usize = 0x0300_3000;
/// `SOFT_CPU_RSTN`：bit6 是小核（CPUSYS2）的复位释放位。**置 1 表示放开复位**，
/// 所以"按住复位"是把这一位清掉。
pub const SOFT_CPU_RSTN_OFFSET: usize = 0x24;
/// SEC_SYS 基址：安全子系统里的协处理器控制。
pub const SEC_SYS_BASE: usize = 0x020B_0000;
/// `SEC_SYS_CTRL`：bit13 是协处理器使能位。
pub const SEC_SYS_CTRL_OFFSET: usize = 0x04;
/// 小核入口地址低 32 位。
pub const SEC_SYS_BOOT_ADDR_L_OFFSET: usize = 0x20;
/// 小核入口地址高 32 位。
pub const SEC_SYS_BOOT_ADDR_H_OFFSET: usize = 0x24;
/// AXI SRAM 基址：FSBL 与小核之间的握手字在这里。
pub const AXI_SRAM_BASE: usize = 0x0E00_0000;
/// 握手字偏移（FSBL 里的 `AXI_SRAM_RTOS_BASE`）。
pub const AXI_SRAM_RTOS_OFFSET: usize = 0x7C;

/// `SOFT_CPU_RSTN` 的 bit6：小核复位释放位。
pub const BIT_C906L_BOOT_FROM_RTCSYS_EN: u32 = 1 << 6;
/// `SEC_SYS_CTRL` 的 bit13：协处理器使能。
pub const BIT_SEC_CPU_EN: u32 = 1 << 13;
/// 小核固件的运行地址：FIP 的 param2 里写的就是它，FSBL 开机日志 `C2S` 的第三段也是它。
pub const C906L_RUN_ADDR: u32 = 0x8FE0_0000;
/// 小核跑起来以后写进握手字的值（FSBL 里的 `CVI_RTOS_MAGIC_CODE`）。
///
/// FSBL 见到它就认定"小核还活着"，于是**只把新的运行地址写进握手字、不动复位**；
/// 否则才走完整的复位释放。读它的用途是区分两种"小核没起来"：握手字等于运行地址
/// 说明 FSBL 走了暖启动分支——而小核已经被断电，根本不会来取那个地址。
pub const RTOS_MAGIC_CODE: u32 = 0x0ABC_0DEF;

/// RTC 控制寄存器组基址（`REG_RTC_CTRL_BASE`）。
pub const RTC_CTRL_BASE: usize = 0x0502_5000;
/// `RTC_CTRL0` 的解锁字：写寄存器前要先把这把钥匙填进去。
pub const RTC_CTRL_UNLOCKKEY_OFFSET: usize = 0x04;
/// `RTC_CTRL0`：整芯片复位/断电的请求位在这里。
pub const RTC_CTRL0_OFFSET: usize = 0x08;
/// RTC 请求寄存器组基址（`REG_RTC_BASE`）。
pub const RTC_REQ_BASE: usize = 0x0502_6000;
/// `RTC_EN_WARM_RST_REQ`：允许"热复位请求"。
pub const RTC_EN_WARM_RESET_OFFSET: usize = 0xCC;
/// 解锁 `RTC_CTRL0` 用的固定密钥（FSBL 与 U-Boot 都用它）。
pub const RTC_CTRL_UNLOCK_KEY: u32 = 0xAB18;
/// `RTC_CTRL0` 的 bit4：整芯片软复位请求（U-Boot `cv_system_reset()` 用的那一位）。
pub const BIT_RTC_CTRL0_WARM_RESET_REQ: u32 = 1 << 4;
/// 写 `RTC_CTRL0` 时必须一起带上的掩码——U-Boot 是 `RTC_CTRL0 |= 0xFFFF0800 | bit`，
/// 高 16 位的 1 与 bit11 是这块寄存器自己的写使能约定，不能省。
pub const RTC_CTRL0_WRITE_MASK: u32 = 0xFFFF_0800;

/// 控制寄存器的一次快照。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoreCtlSnapshot {
    /// `SOFT_CPU_RSTN` 原值。
    pub rstn: u32,
    /// `SEC_SYS_CTRL` 原值。
    pub sec_ctrl: u32,
    /// 入口地址寄存器低位。
    pub boot_addr_lo: u32,
    /// 入口地址寄存器高位（本 SoC 上 DRAM 在 4 GiB 以内，正常恒为 0）。
    pub boot_addr_hi: u32,
    /// AXI SRAM 里的握手字。
    pub handshake: u32,
}

impl CoreCtlSnapshot {
    /// 小核使能位是否已置起。
    pub const fn core_enabled(self) -> bool {
        self.sec_ctrl & BIT_SEC_CPU_EN != 0
    }

    /// 小核是否已放开复位。
    pub const fn reset_released(self) -> bool {
        self.rstn & BIT_C906L_BOOT_FROM_RTCSYS_EN != 0
    }

    /// FSBL 上次是不是走了暖启动分支：握手字已被写成运行地址。
    pub const fn fsbl_took_warm_path(self) -> bool {
        self.handshake == C906L_RUN_ADDR
    }

    /// 小核有没有在握手字里报过自己的 magic。
    pub const fn rtos_reported_magic(self) -> bool {
        self.handshake == RTOS_MAGIC_CODE
    }
}

/// 对端核控制寄存器句柄。
///
/// 三段基址按"已映射的虚拟地址"存放，而不是直接存裸指针——与 [`crate::mailbox::Mailbox`]
/// 同一套做法：句柄本身因此是 `Send`，内核把它放进 `Arc<设备>` 没有额外负担，
/// 访问时再按偏移换算成指针。
#[derive(Debug)]
pub struct CoreCtl {
    /// RSTC 窗口基址。
    rstc: usize,
    /// SEC_SYS 窗口基址。
    sec_sys: usize,
    /// AXI SRAM 窗口基址。
    sram: usize,
}

impl CoreCtl {
    /// 用三段已映射的寄存器窗口基址创建句柄。
    ///
    /// # Safety
    ///
    /// 调用方必须保证三个基址各自指向一段可读写的非缓存 MMIO 映射，`rstc` 至少
    /// 覆盖 [`SOFT_CPU_RSTN_OFFSET`]、`sec_sys` 至少覆盖 [`SEC_SYS_BOOT_ADDR_H_OFFSET`]、
    /// `sram` 至少覆盖 [`AXI_SRAM_RTOS_OFFSET`]，且在句柄存续期间保持有效。
    pub const unsafe fn new(rstc: usize, sec_sys: usize, sram: usize) -> Self {
        Self {
            rstc,
            sec_sys,
            sram,
        }
    }

    fn rstn_addr(&self) -> usize {
        self.rstc + SOFT_CPU_RSTN_OFFSET
    }

    fn sec_ctrl_addr(&self) -> usize {
        self.sec_sys + SEC_SYS_CTRL_OFFSET
    }

    fn boot_lo_addr(&self) -> usize {
        self.sec_sys + SEC_SYS_BOOT_ADDR_L_OFFSET
    }

    fn boot_hi_addr(&self) -> usize {
        self.sec_sys + SEC_SYS_BOOT_ADDR_H_OFFSET
    }

    fn handshake_addr(&self) -> usize {
        self.sram + AXI_SRAM_RTOS_OFFSET
    }

    fn rd(addr: usize) -> u32 {
        unsafe { read_volatile(addr as *const u32) }
    }

    fn wr(addr: usize, value: u32) {
        unsafe { write_volatile(addr as *mut u32, value) };
    }

    /// 读一遍全部控制寄存器。只读，不改任何状态。
    pub fn snapshot(&self) -> CoreCtlSnapshot {
        CoreCtlSnapshot {
            rstn: Self::rd(self.rstn_addr()),
            sec_ctrl: Self::rd(self.sec_ctrl_addr()),
            boot_addr_lo: Self::rd(self.boot_lo_addr()),
            boot_addr_hi: Self::rd(self.boot_hi_addr()),
            handshake: Self::rd(self.handshake_addr()),
        }
    }

    /// 按住小核复位。返回操作前的 `SOFT_CPU_RSTN`。
    pub fn hold(&self) -> u32 {
        let addr = self.rstn_addr();
        let before = Self::rd(addr);
        Self::wr(addr, before & !BIT_C906L_BOOT_FROM_RTCSYS_EN);
        before
    }

    /// 放开小核复位：使能协处理器、写入口地址、再放开复位。返回操作后的
    /// `SOFT_CPU_RSTN`。
    ///
    /// 这三步的顺序与 FSBL 完全一致，中间不做任何等待——FSBL 也是背靠背写的。
    pub fn release(&self, entry: u32) -> u32 {
        let ctrl_addr = self.sec_ctrl_addr();
        let ctrl = Self::rd(ctrl_addr);
        Self::wr(ctrl_addr, ctrl | BIT_SEC_CPU_EN);
        Self::wr(self.boot_lo_addr(), entry);
        // 入口地址在这颗 SoC 上永远落在 4 GiB 以内，高位显式写 0 而不是留旧值。
        Self::wr(self.boot_hi_addr(), 0);
        let rstn_addr = self.rstn_addr();
        let rstn = Self::rd(rstn_addr);
        Self::wr(rstn_addr, rstn | BIT_C906L_BOOT_FROM_RTCSYS_EN);
        Self::rd(rstn_addr)
    }
}

/// 整芯片软复位（RTC 域）。
///
/// 与 U-Boot 的 `cv_system_reset()` 逐字一致：先允许"热复位请求"，再解锁 `RTC_CTRL0`
/// 并置请求位。复位一旦生效，**调用方不会返回**——启动链从 ROM 重跑，串口会重新
/// 打印 FSBL/U-Boot 的启动信息（本板的 `bootdelay=-1`，所以最终停在 U-Boot 提示符）。
///
/// 为什么用"热复位"而不是"断电再上电"：本板的电源由外部决定（小车电池/USB），断电
/// 之后能不能自己回来不由我们控制；热复位会重跑完整启动链，效果与按一下 reset 相同。
/// 同族的 `RTC_EN_SHDN_REQ` + `RTC_CTRL0` bit0 是"关机"，本模块刻意不提供——那会把
/// 板子留在断电状态，只能靠人重新上电才能恢复。
pub struct ChipReset {
    /// `REG_RTC_CTRL_BASE`（0x05025000）。
    rtc_ctrl: usize,
    /// `REG_RTC_BASE`（0x05026000）。
    rtc: usize,
}

impl ChipReset {
    /// 用两段已映射的寄存器窗口基址创建句柄。
    ///
    /// # Safety
    ///
    /// 调用方必须保证两个基址各自指向一段可读写的非缓存 MMIO 映射，`rtc_ctrl` 至少
    /// 覆盖 [`RTC_CTRL0_OFFSET`]、`rtc` 至少覆盖 [`RTC_EN_WARM_RESET_OFFSET`]，
    /// 且在句柄存续期间保持有效。
    pub const unsafe fn new(rtc_ctrl: usize, rtc: usize) -> Self {
        Self { rtc_ctrl, rtc }
    }

    /// 请求整芯片软复位。正常情况这一调用不会返回。
    pub fn request_warm_reset(&self) {
        let en = self.rtc + RTC_EN_WARM_RESET_OFFSET;
        CoreCtl::wr(en, 0x01);
        // U-Boot 也是回读等到 1 —— RTC 在 32 kHz 域里，写下去要一拍才落下。
        for _ in 0..RTC_LATCH_POLLS {
            if CoreCtl::rd(en) == 0x01 {
                break;
            }
            core::hint::spin_loop();
        }
        CoreCtl::wr(
            self.rtc_ctrl + RTC_CTRL_UNLOCKKEY_OFFSET,
            RTC_CTRL_UNLOCK_KEY,
        );
        let ctrl = CoreCtl::rd(self.rtc_ctrl + RTC_CTRL0_OFFSET);
        CoreCtl::wr(
            self.rtc_ctrl + RTC_CTRL0_OFFSET,
            ctrl | RTC_CTRL0_WRITE_MASK | BIT_RTC_CTRL0_WARM_RESET_REQ,
        );
    }
}

/// `RTC_EN_WARM_RST_REQ` 的回读等待次数上限（U-Boot 是无上限死等）。
const RTC_LATCH_POLLS: usize = 100_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_interpretation_matches_fsbl_semantics() {
        // 小核在跑的稳态：使能位置起、复位已放开、握手字是小核自己写的 magic。
        let running = CoreCtlSnapshot {
            rstn: BIT_C906L_BOOT_FROM_RTCSYS_EN | 0x20,
            sec_ctrl: BIT_SEC_CPU_EN,
            boot_addr_lo: C906L_RUN_ADDR,
            boot_addr_hi: 0,
            handshake: RTOS_MAGIC_CODE,
        };
        assert!(running.core_enabled());
        assert!(running.reset_released());
        assert!(running.rtos_reported_magic());
        assert!(!running.fsbl_took_warm_path());

        // 冷启动"小核没起来"：复位已放开但握手字只是 FSBL 写下的运行地址，
        // 说明 FSBL 走的是暖启动分支，小核从来没被复位释放过。
        let never_released = CoreCtlSnapshot {
            rstn: BIT_C906L_BOOT_FROM_RTCSYS_EN,
            sec_ctrl: BIT_SEC_CPU_EN,
            boot_addr_lo: C906L_RUN_ADDR,
            boot_addr_hi: 0,
            handshake: C906L_RUN_ADDR,
        };
        assert!(never_released.fsbl_took_warm_path());
        assert!(!never_released.rtos_reported_magic());

        // 按住复位后 PC 恒为 0，这一位必须看得出来。
        let held = CoreCtlSnapshot {
            rstn: 0x20,
            ..running
        };
        assert!(!held.reset_released());
    }
}
