//! CV181x/SG2002 TPU SoC-level clock and reset control.
//!
//! This is a small, hardware-only port of the pieces used by SOPHGO's Linux
//! TPU driver:
//!
//! - `platform_tpu_init()` enables `clk_tpu_axi` and `clk_tpu_fab`, then
//!   pulses `res_tdma`, `res_tpu` and `res_tpusys`;
//! - `platform_tpu_deinit()` disables both clocks;
//! - `clk_get_rate(clk_tpu_axi)` is written back to the command-buffer header.
//!
//! Parent selection and rate calculation follow
//! `drivers/clk/cvitek/clk-cv181x.c`.  In particular, the A0PLL synthesizer is
//! driven by MIPIMPLL, not directly by the 25 MHz oscillator.

use core::sync::atomic::{Ordering, fence};

use ax_kspin::SpinNoIrq;

/// CVITEK's Linux clock/reset helpers protect read-modify-write sequences with
/// irqsave spinlocks. SG2002 is single-core in this configuration, so an
/// IRQ-disabling spin lock gives the same exclusion against task/IRQ users.
static SOC_CTRL_LOCK: SpinNoIrq<()> = SpinNoIrq::new(());

const OSC_RATE_HZ: u64 = 25_000_000;

const REG_CLK_EN_0: usize = 0x000;
const REG_CLK_BYP_0: usize = 0x030;
const REG_DIV_CLK_TPU: usize = 0x054;

const REG_MIPIMPLL_CSR: usize = 0x808;
const REG_APLL0_CSR: usize = 0x80c;
const REG_PLL_G2_SSC_SYN_CTRL: usize = 0x840;
const REG_APLL_SSC_SYN_SET: usize = 0x854;
const REG_TPLL_CSR: usize = 0x90c;
const REG_FPLL_CSR: usize = 0x910;

/// SG2002 official FSBL `TPU_PERF_MODE` setting:
/// source TPLL, live one-based divider 2, update/select bits enabled.
const SG2002_TPU_PERF_DIV: u32 = 0x0002_0009;
const SG2002_TPU_PERF_PARENT_HZ: u32 = 1_400_000_000;
const SG2002_TPU_PERF_RATE_HZ: u32 = 700_000_000;

const CLK_TPU_GATE: u32 = 1 << 4;
const CLK_TPU_FAB_GATE: u32 = 1 << 5;
const CLK_TPU_BYPASS: u32 = 1 << 3;
const CLK_TPU_FAB_BYPASS: u32 = 1 << 4;

const RST_TDMA: u32 = 1 << 7;
const RST_TPU: u32 = 1 << 8;
const RST_TPUSYS: u32 = 1 << 9;

/// A register snapshot matching the clock state used for a TPU submission.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cv181xTpuClockSnapshot {
    pub tpu_rate_hz: u32,
    pub fab_rate_hz: u32,
    pub tpll_rate_hz: u32,
    pub fpll_rate_hz: u32,
    pub mipimpll_rate_hz: u32,
    pub clock_enable_0: u32,
    pub clock_bypass_0: u32,
    pub clock_div_tpu: u32,
    pub reset_0: u32,
}

/// Direct CV181x clock/reset controller access for the TPU block.
pub struct Cv181xTpuSoc {
    clkgen: *mut u8,
    reset: *mut u8,
}

impl Cv181xTpuSoc {
    /// Construct from permanently mapped CLKGEN and reset-controller bases.
    ///
    /// # Safety
    /// Both pointers must cover the full CV181x CLKGEN/RSTC register ranges
    /// and remain valid for the lifetime of this value.
    pub const unsafe fn new(clkgen: *mut u8, reset: *mut u8) -> Self {
        Self { clkgen, reset }
    }

    #[inline]
    unsafe fn clk_read(&self, offset: usize) -> u32 {
        unsafe { self.clkgen.add(offset).cast::<u32>().read_volatile() }
    }

    #[inline]
    unsafe fn clk_write(&self, offset: usize, value: u32) {
        unsafe { self.clkgen.add(offset).cast::<u32>().write_volatile(value) }
    }

    #[inline]
    unsafe fn reset_read(&self) -> u32 {
        unsafe { self.reset.cast::<u32>().read_volatile() }
    }

    #[inline]
    unsafe fn reset_write(&self, value: u32) {
        unsafe { self.reset.cast::<u32>().write_volatile(value) }
    }

    /// Apply the SG2002 FSBL's official 700 MHz TPU divider when the shared
    /// TPLL is already running at its official 1400 MHz performance rate.
    ///
    /// This deliberately never reprograms the shared PLL or voltage rail from
    /// the kernel. If firmware selected a different TPLL rate, the driver only
    /// reports it and leaves the system clock tree untouched.
    pub fn configure_official_performance_clock(&self) -> bool {
        let _guard = SOC_CTRL_LOCK.lock();
        let before = self.snapshot();
        if before.tpu_rate_hz == SG2002_TPU_PERF_RATE_HZ
            || before.tpll_rate_hz != SG2002_TPU_PERF_PARENT_HZ
        {
            return false;
        }

        // SAFETY: constructor requires a valid CLKGEN mapping. Temporarily
        // gate the idle block while switching, then restore the prior gate
        // state. The divider value is the exact SG2002 FSBL setting.
        unsafe {
            let enable = self.clk_read(REG_CLK_EN_0);
            self.clk_write(REG_CLK_EN_0, enable & !(CLK_TPU_GATE | CLK_TPU_FAB_GATE));
            fence(Ordering::SeqCst);
            let bypass = self.clk_read(REG_CLK_BYP_0) & !CLK_TPU_BYPASS;
            self.clk_write(REG_CLK_BYP_0, bypass);
            self.clk_write(REG_DIV_CLK_TPU, SG2002_TPU_PERF_DIV);
            fence(Ordering::SeqCst);
            self.clk_write(REG_CLK_EN_0, enable);
            fence(Ordering::SeqCst);
        }
        self.snapshot().tpu_rate_hz == SG2002_TPU_PERF_RATE_HZ
    }

    /// Equivalent to the clock-enable and reset parts of
    /// SOPHGO `platform_tpu_init()`.
    pub fn prepare(&self) -> Cv181xTpuClockSnapshot {
        let _guard = SOC_CTRL_LOCK.lock();
        // SAFETY: constructor requires valid device mappings.  Preserve every
        // unrelated clock/reset bit, as Linux regmap/reset/clk helpers do.
        unsafe {
            let mut enable = self.clk_read(REG_CLK_EN_0);
            enable |= CLK_TPU_GATE;
            self.clk_write(REG_CLK_EN_0, enable);
            enable |= CLK_TPU_FAB_GATE;
            self.clk_write(REG_CLK_EN_0, enable);
            fence(Ordering::SeqCst);

            // CVITEK reset controller is active-low: clear to assert, set to
            // deassert.  The official TPU driver pulses TDMA, TPU and TPUSYS
            // in that order without an interposed delay.
            let mut reset = self.reset_read();
            reset &= !RST_TDMA;
            self.reset_write(reset);
            reset &= !RST_TPU;
            self.reset_write(reset);
            reset &= !RST_TPUSYS;
            self.reset_write(reset);
            fence(Ordering::SeqCst);
            reset |= RST_TDMA;
            self.reset_write(reset);
            reset |= RST_TPU;
            self.reset_write(reset);
            reset |= RST_TPUSYS;
            self.reset_write(reset);
            fence(Ordering::SeqCst);
        }
        self.snapshot()
    }

    /// Equivalent to SOPHGO `platform_tpu_deinit()`.
    pub fn finish(&self) {
        let _guard = SOC_CTRL_LOCK.lock();
        // SAFETY: constructor requires a valid device mapping.
        unsafe {
            fence(Ordering::SeqCst);
            let mut enable = self.clk_read(REG_CLK_EN_0);
            enable &= !CLK_TPU_GATE;
            self.clk_write(REG_CLK_EN_0, enable);
            enable &= !CLK_TPU_FAB_GATE;
            self.clk_write(REG_CLK_EN_0, enable);
            fence(Ordering::SeqCst);
        }
    }

    /// Read the selected parents/divider and calculate the real running rates
    /// using the same formula as `clk_get_rate()` on CV181x Linux.
    pub fn snapshot(&self) -> Cv181xTpuClockSnapshot {
        // SAFETY: constructor requires valid device mappings.
        unsafe {
            let enable = self.clk_read(REG_CLK_EN_0);
            let bypass = self.clk_read(REG_CLK_BYP_0);
            let div = self.clk_read(REG_DIV_CLK_TPU);
            let mipimpll = pll_rate(self.clk_read(REG_MIPIMPLL_CSR), OSC_RATE_HZ);
            let tpll = pll_rate(self.clk_read(REG_TPLL_CSR), OSC_RATE_HZ);
            let fpll = pll_rate(self.clk_read(REG_FPLL_CSR), OSC_RATE_HZ);

            // A0PLL's Linux parent is clk_mipimpll.  Its synthesizer first
            // derives clk_ref, which then feeds the ordinary PLL calculation.
            let a0_set = self.clk_read(REG_APLL_SSC_SYN_SET);
            let g2_ctrl = self.clk_read(REG_PLL_G2_SSC_SYN_CTRL);
            let a0_ref = synthesizer_rate(mipimpll, a0_set, g2_ctrl);
            let a0pll = pll_rate(self.clk_read(REG_APLL0_CSR), a0_ref);

            let tpu_parent = if bypass & CLK_TPU_BYPASS != 0 {
                OSC_RATE_HZ
            } else {
                match (div >> 8) & 0x3 {
                    0 => tpll,
                    1 => a0pll,
                    2 => mipimpll,
                    _ => fpll,
                }
            };
            let tpu_rate = if bypass & CLK_TPU_BYPASS != 0 {
                tpu_parent
            } else {
                // clk-cv181x.c uses initval=3 until bit 3 selects the live
                // divider field, and registers CLK_DIVIDER_ONE_BASED plus
                // CLK_DIVIDER_ALLOW_ZERO: value N means /N, while zero keeps
                // the parent rate.
                let divider_value = if div & (1 << 3) == 0 {
                    3
                } else {
                    (div >> 16) & 0xf
                };
                one_based_div_rate(tpu_parent, divider_value)
            };
            let fab_rate = if bypass & CLK_TPU_FAB_BYPASS != 0 {
                OSC_RATE_HZ
            } else {
                mipimpll
            };

            Cv181xTpuClockSnapshot {
                tpu_rate_hz: saturating_u32(tpu_rate),
                fab_rate_hz: saturating_u32(fab_rate),
                tpll_rate_hz: saturating_u32(tpll),
                fpll_rate_hz: saturating_u32(fpll),
                mipimpll_rate_hz: saturating_u32(mipimpll),
                clock_enable_0: enable,
                clock_bypass_0: bypass,
                clock_div_tpu: div,
                reset_0: self.reset_read(),
            }
        }
    }
}

#[inline]
fn saturating_u32(rate: u64) -> u32 {
    rate.min(u64::from(u32::MAX)) as u32
}

/// Linux `CLK_DIVIDER_ONE_BASED | CLK_DIVIDER_ALLOW_ZERO` semantics.
fn one_based_div_rate(parent_rate: u64, value: u32) -> u64 {
    if value == 0 {
        parent_rate
    } else {
        parent_rate / u64::from(value)
    }
}

/// CV181x PLL rate: parent * DIV_SEL / (PRE_DIV_SEL * POST_DIV_SEL).
fn pll_rate(csr: u32, parent_rate: u64) -> u64 {
    let prediv = u64::from(csr & 0x7f);
    let postdiv = u64::from((csr >> 8) & 0x7f);
    let divsel = u64::from((csr >> 17) & 0x7f);
    let denominator = prediv.saturating_mul(postdiv);
    if denominator == 0 {
        0
    } else {
        parent_rate.saturating_mul(divsel) / denominator
    }
}

/// G2 synthesizer used by A0PLL.  `g2_ctrl.bit0` selects parent or parent/2.
fn synthesizer_rate(parent_rate: u64, set: u32, g2_ctrl: u32) -> u64 {
    if set == 0 {
        return 0;
    }
    let input = if g2_ctrl & 1 != 0 {
        parent_rate
    } else {
        parent_rate >> 1
    };
    input.saturating_mul(1 << 26) / u64::from(set)
}

// The hardware instance is serialised by the TPU driver's run lock/worker.
unsafe impl Send for Cv181xTpuSoc {}
unsafe impl Sync for Cv181xTpuSoc {}

#[cfg(test)]
mod tests {
    use super::{
        Cv181xTpuSoc, OSC_RATE_HZ, REG_CLK_EN_0, REG_DIV_CLK_TPU, REG_FPLL_CSR, REG_TPLL_CSR,
        SG2002_TPU_PERF_DIV, one_based_div_rate, pll_rate, synthesizer_rate,
    };

    #[test]
    fn pll_formula_matches_simple_1_40_2_case() {
        let csr = 1 | (2 << 8) | (40 << 17);
        assert_eq!(pll_rate(csr, OSC_RATE_HZ), 500_000_000);
    }

    #[test]
    fn invalid_pll_divider_is_reported_as_zero_rate() {
        assert_eq!(pll_rate(0, OSC_RATE_HZ), 0);
    }

    #[test]
    fn synthesizer_uses_selected_parent() {
        assert_eq!(synthesizer_rate(1_000_000_000, 1 << 26, 1), 1_000_000_000);
        assert_eq!(synthesizer_rate(1_000_000_000, 1 << 26, 0), 500_000_000);
    }

    #[test]
    fn tpu_divider_matches_official_700mhz_fsbl_setting() {
        // Official SG2002 FSBL writes REG_DIV_CLK_TPU=0x00020009:
        // live divider value 2 and TPLL parent 1400 MHz => 700 MHz.
        assert_ne!(SG2002_TPU_PERF_DIV & (1 << 3), 0);
        assert_eq!((SG2002_TPU_PERF_DIV >> 8) & 0x3, 0);
        assert_eq!((SG2002_TPU_PERF_DIV >> 16) & 0xf, 2);
        assert_eq!(one_based_div_rate(1_400_000_000, 2), 700_000_000);
        assert_eq!(one_based_div_rate(1_400_000_000, 0), 1_400_000_000);
    }

    #[test]
    fn performance_clock_register_selects_700mhz() {
        let mut clkgen = [0u32; 1024];
        let mut reset = [0u32; 4];
        clkgen[REG_CLK_EN_0 / 4] = 0xa5a5_0000;
        clkgen[REG_TPLL_CSR / 4] = 1 | (1 << 8) | (56 << 17); // 1400 MHz
        clkgen[REG_FPLL_CSR / 4] = 1 | (1 << 8) | (60 << 17); // 1500 MHz
        clkgen[REG_DIV_CLK_TPU / 4] = 0x0003_0309; // FPLL / 3 = 500 MHz
        let original_enable = clkgen[REG_CLK_EN_0 / 4];

        let soc = unsafe {
            Cv181xTpuSoc::new(
                clkgen.as_mut_ptr().cast::<u8>(),
                reset.as_mut_ptr().cast::<u8>(),
            )
        };
        assert_eq!(soc.snapshot().tpu_rate_hz, 500_000_000);
        clkgen[REG_DIV_CLK_TPU / 4] = SG2002_TPU_PERF_DIV;
        assert_eq!(soc.snapshot().tpu_rate_hz, 700_000_000);
        assert_eq!(clkgen[REG_DIV_CLK_TPU / 4], SG2002_TPU_PERF_DIV);
        assert_eq!(clkgen[REG_CLK_EN_0 / 4], original_enable);
    }
}
