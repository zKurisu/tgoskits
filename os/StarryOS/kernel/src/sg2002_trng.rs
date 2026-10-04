//! SG2002 的真随机数发生器（TRNG）：给内核补一份可信的启动熵。
//!
//! 为什么需要它：这台板子由 U-Boot 从 FIT 起飞，设备树里没有 `/chosen/rng-seed`，
//! 而 U-Boot 2021.10 也没有生成该属性的功能。于是 `someboot` 在启动时会报
//! "No trusted boot entropy source is available"，而**安全** Wi-Fi 连接
//! （aic8800 驱动 + ax_net）明确要求一份真实熵，并拒绝用可重放的时间/地址状态凑数
//! （见 `platforms/someboot/src/entropy.rs` 的注释与它自己的测试）。
//!
//! 芯片自带 TRNG，流程在 TRM 的 `security/trng` 一章里写得很清楚：
//! `GEN_NOISE`（从噪声生成全熵种子）→ `CREATE_STATE`（DRBG 进入创建态）
//! → `GEN_RANDOM` → 读 `RAND0..RAND3`（一次 128 位）。要 32 字节就做两轮。

use ax_memory_addr::PhysAddr;

/// TRNG 寄存器块（TRM：基址 `0x0207_0000`）。
const TRNG_BASE: usize = 0x0207_0000;
const TRNG_SIZE: usize = 0x1000;

const REG_CTRL: usize = 0x000;
const REG_STAT: usize = 0x00c;
const REG_ISTAT: usize = 0x014;
const REG_RAND0: usize = 0x024;

/// `CTRL.CMD`（bits 3:0）。
const CMD_GEN_NOISE: u32 = 0x1;
const CMD_CREATE_STATE: u32 = 0x3;
const CMD_GEN_RANDOM: u32 = 0x6;

/// `STAT.BUSY`（bit 31）：1 表示正在执行命令。
const STAT_BUSY: u32 = 1 << 31;
/// `ISTAT.DONE`（bit 4）：读 1 表示有未确认的命令完成；写 1 清除。
const ISTAT_DONE: u32 = 1 << 4;

/// 轮询上限。TRNG 生成一次全熵种子在 TRM 里是毫秒级，这里给足余量，
/// 免得硬件异常时把内核卡死在一个死循环里。
const POLL_LIMIT: u32 = 1_000_000;

struct Trng {
    base: usize,
}

impl Trng {
    fn read(&self, offset: usize) -> u32 {
        // SAFETY: base 来自 `ax_mm::iomap` 的映射，覆盖整个 4 KiB 寄存器块，
        // 下面的偏移都在块内且按 4 字节对齐。
        unsafe { core::ptr::read_volatile((self.base + offset) as *const u32) }
    }

    fn write(&self, offset: usize, value: u32) {
        // SAFETY: 同上。
        unsafe { core::ptr::write_volatile((self.base + offset) as *mut u32, value) }
    }

    fn wait_idle(&self) -> bool {
        for _ in 0..POLL_LIMIT {
            if self.read(REG_STAT) & STAT_BUSY == 0 {
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }

    /// 发一条命令并等它完成（`ISTAT.DONE` 置位后写 1 清掉）。
    fn command(&self, cmd: u32) -> bool {
        if !self.wait_idle() {
            return false;
        }
        self.write(REG_CTRL, cmd);
        for _ in 0..POLL_LIMIT {
            if self.read(REG_ISTAT) & ISTAT_DONE != 0 {
                self.write(REG_ISTAT, ISTAT_DONE);
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }

    fn random_128(&self) -> Option<[u32; 4]> {
        if !self.command(CMD_GEN_RANDOM) {
            return None;
        }
        Some([
            self.read(REG_RAND0),
            self.read(REG_RAND0 + 0x4),
            self.read(REG_RAND0 + 0x8),
            self.read(REG_RAND0 + 0xc),
        ])
    }
}

/// 读 32 字节（两轮 128 位）作为启动熵。任何一步失败都返回 `None`——
/// 宁可让需要熵的调用方明确报错，也不要给出一份来路不明的"随机数"。
pub fn read_seed() -> Option<[u8; 32]> {
    let base = ax_mm::iomap(PhysAddr::from_usize(TRNG_BASE), TRNG_SIZE)
        .ok()?
        .as_usize();
    let trng = Trng { base };

    if !trng.command(CMD_GEN_NOISE) {
        warn!("[trng] GEN_NOISE 失败：SG2002 TRNG 没回应");
        return None;
    }
    if !trng.command(CMD_CREATE_STATE) {
        warn!("[trng] CREATE_STATE 失败：SG2002 TRNG 没回应");
        return None;
    }

    let mut seed = [0u8; 32];
    for half in 0..2 {
        let words = trng.random_128()?;
        for (index, word) in words.iter().enumerate() {
            let at = half * 16 + index * 4;
            seed[at..at + 4].copy_from_slice(&word.to_ne_bytes());
        }
    }
    Some(seed)
}

/// 读一份熵并登记给 `axhal`；已经登记过或读取失败都不算致命。
pub fn provide_boot_entropy() {
    match read_seed() {
        Some(seed) => {
            if ax_hal::boot::provide_boot_entropy(seed) {
                info!("[trng] 已用 SG2002 TRNG 补上 32 字节启动熵（安全 Wi-Fi 依赖它）");
            }
        }
        None => warn!("[trng] 读不到 SG2002 TRNG：安全 Wi-Fi 连接会报 EntropyUnavailable"),
    }
}
