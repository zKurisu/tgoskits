//! Boot-time metadata exposed through boot-protocol-agnostic accessors.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// 板级/驱动登记的熵（例如 SG2002 的 TRNG）。
///
/// 只有当平台本身（UEFI RNG 或 FDT `/chosen/rng-seed`）没有给出可信种子时才会被采用。
/// 一旦有人登记过就不再覆盖，避免把已经在用的种子中途换掉。
static PROVIDED_ENTROPY: [AtomicU32; 8] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];
static PROVIDED_ENTROPY_VALID: AtomicBool = AtomicBool::new(false);

/// 由板级代码或驱动在早期登记一份 32 字节熵。
///
/// 用途是补齐"启动协议没给种子"的板子：本仓库的 SG2002 板由 U-Boot 从 FIT 起飞，
/// DTB 里没有 `/chosen/rng-seed`（U-Boot 2021.10 也不会生成它），而安全 Wi-Fi 连接
/// 需要一份**真实**熵——它拒绝用可重放的时间/地址状态凑数。
///
/// 返回 `true` 表示这一份被采纳（此前没有人登记过）。这是早期单线程初始化路径上的
/// 一次性动作，不在中断或多核竞争下调用。
pub fn provide_boot_entropy(seed: [u8; 32]) -> bool {
    if PROVIDED_ENTROPY_VALID.load(Ordering::Acquire) {
        return false;
    }
    for (word, chunk) in PROVIDED_ENTROPY.iter().zip(seed.chunks_exact(4)) {
        word.store(
            u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            Ordering::Relaxed,
        );
    }
    PROVIDED_ENTROPY_VALID.store(true, Ordering::Release);
    true
}

/// Returns kernel boot arguments when the active boot path provides them.
///
/// The facade keeps runtime users independent from whether the arguments came
/// from FDT, UEFI load options, ACPI-related firmware data, or another future
/// boot protocol. The current implementation falls back to FDT
/// `/chosen/bootargs`.
pub fn bootargs() -> Option<&'static str> {
    #[cfg(not(any(test, feature = "host-test")))]
    if let Some(bootargs) = axplat_dyn::bootargs() {
        return Some(bootargs);
    }

    crate::dtb::get_chosen_bootargs()
}

/// Returns the trusted firmware seed captured during early boot.
pub fn boot_entropy() -> Option<[u8; 32]> {
    #[cfg(not(any(test, feature = "host-test")))]
    if let Some(seed) = axplat_dyn::boot_entropy() {
        return Some(seed);
    }

    if PROVIDED_ENTROPY_VALID.load(Ordering::Acquire) {
        let mut seed = [0u8; 32];
        for (chunk, word) in seed.chunks_exact_mut(4).zip(PROVIDED_ENTROPY.iter()) {
            chunk.copy_from_slice(&word.load(Ordering::Relaxed).to_ne_bytes());
        }
        return Some(seed);
    }

    None
}
