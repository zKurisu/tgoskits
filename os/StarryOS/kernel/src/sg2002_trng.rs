//! SG2002 true random number generator: a trusted boot-entropy source.
//!
//! Why this exists: this board boots from a U-Boot FIT whose DTB has no
//! `/chosen/rng-seed`, and U-Boot 2021.10 cannot generate that property. The
//! platform therefore starts without a trusted seed ("No trusted boot entropy
//! source is available"), while *secure* Wi-Fi association (aic8800 driver +
//! ax-net) requires real entropy and refuses to substitute replayable
//! timing/address state (see `platforms/someboot/src/entropy.rs`).
//!
//! The chip's own TRNG is described in the TRM chapter `security/trng`:
//! `GEN_NOISE` (seed the DRBG from noise) → `CREATE_STATE` → `GEN_RANDOM`, then
//! read `RAND0..RAND3` for 128 bits at a time. Two rounds yield 32 bytes.

use ax_memory_addr::PhysAddr;

/// TRNG register block (TRM: base `0x0207_0000`).
const TRNG_BASE: usize = 0x0207_0000;
const TRNG_SIZE: usize = 0x1000;

const REG_CTRL: usize = 0x000;
const REG_STAT: usize = 0x00c;
const REG_ISTAT: usize = 0x014;
const REG_RAND0: usize = 0x024;

/// `CTRL.CMD` (bits 3:0).
const CMD_GEN_NOISE: u32 = 0x1;
const CMD_CREATE_STATE: u32 = 0x3;
const CMD_GEN_RANDOM: u32 = 0x6;

/// `STAT.BUSY` (bit 31): the command engine is running.
const STAT_BUSY: u32 = 1 << 31;
/// `ISTAT.DONE` (bit 4): an unacknowledged command completion; write 1 to clear.
const ISTAT_DONE: u32 = 1 << 4;

/// Poll bound. A full-entropy seed takes milliseconds per the TRM; this leaves
/// plenty of headroom without letting a wedged engine hang the boot forever.
const POLL_LIMIT: u32 = 1_000_000;

struct Trng {
    base: usize,
}

impl Trng {
    fn read(&self, offset: usize) -> u32 {
        // SAFETY: `base` comes from `ax_mm::iomap` and covers the whole 4 KiB
        // register block; every offset below is inside it and 4-byte aligned.
        unsafe { core::ptr::read_volatile((self.base + offset) as *const u32) }
    }

    fn write(&self, offset: usize, value: u32) {
        // SAFETY: as above.
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

    /// Issues one command and waits for completion (`ISTAT.DONE`, acknowledged
    /// by writing 1 back).
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

/// Reads 32 bytes (two rounds of 128 bits) of boot entropy.
///
/// Any failing step returns `None` on purpose: a caller that needs entropy must
/// see an explicit error rather than a "random" value of unknown provenance.
pub fn read_seed() -> Option<[u8; 32]> {
    let base = ax_mm::iomap(PhysAddr::from_usize(TRNG_BASE), TRNG_SIZE)
        .ok()?
        .as_usize();
    let trng = Trng { base };

    if !trng.command(CMD_GEN_NOISE) {
        warn!("[trng] GEN_NOISE failed: the SG2002 TRNG did not answer");
        return None;
    }
    if !trng.command(CMD_CREATE_STATE) {
        warn!("[trng] CREATE_STATE failed: the SG2002 TRNG did not answer");
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

/// Reads one seed and registers it with `axhal`; already-registered or a failed
/// read is not fatal.
pub fn provide_boot_entropy() {
    match read_seed() {
        Some(seed) => {
            if ax_hal::boot::provide_boot_entropy(seed) {
                info!("[trng] registered 32 bytes of SG2002 TRNG boot entropy");
            }
        }
        None => warn!("[trng] SG2002 TRNG unavailable: secure Wi-Fi will report EntropyUnavailable"),
    }
}
