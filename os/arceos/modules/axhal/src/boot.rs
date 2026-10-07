//! Boot-time metadata exposed through boot-protocol-agnostic accessors.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Entropy registered by board code or a driver (SG2002 TRNG, for example).
///
/// Adopted only when the boot protocol itself supplies no trusted seed (UEFI RNG
/// or FDT `/chosen/rng-seed`). Registration happens once: swapping a seed that
/// is already in use would silently change derived state (Wi-Fi PMKs, RNG
/// streams), so later attempts are refused instead.
static PROVIDED_ENTROPY: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];
static PROVIDED_ENTROPY_VALID: AtomicBool = AtomicBool::new(false);

/// Registers 32 bytes of entropy from board code or a driver.
///
/// Boards whose boot protocol cannot provide a seed need this: this tree's
/// SG2002 target boots from a U-Boot FIT whose DTB carries no
/// `/chosen/rng-seed` (U-Boot 2021.10 cannot emit one), while secure Wi-Fi
/// association requires *real* entropy and deliberately refuses replayable
/// timing/address state.
///
/// Returns `true` when this seed was adopted. This is a one-shot early-boot
/// action, not a concurrent or interrupt-context API.
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
