// Typical exception vector table code.
.balign 0x800
vector_table_el1:
    curr_el_sp0_sync: // The exception handler for the synchronous
    B .
    // exception from the current EL using SP0.
    .balign 0x80
    curr_el_sp0_irq: // The exception handler for the IRQ exception
    B .
    // from the current EL using SP0.
    .balign 0x80
    curr_el_sp0_fiq: // The exception handler for the FIQ exception
    B .
    // from the current EL using SP0.
    .balign 0x80
    curr_el_sp0_serror: // The exception handler for the system error 
    B .
    .balign 0x80
    curr_el_spx_sync:
    B CommonHandler
    .balign 0x80
    curr_el_spx_irq: 
    B IRQHandler
    .balign 0x80
    curr_el_spx_frq: 
    B .
    .balign 0x80
    curr_el_spx_serror: // The exception handler for the system error 
    B .
    // exception from the current EL using the
    // current SP.
    .balign 0x80
    lower_el_aarch64_sync: // The exception handler for the synchronous
    B .
    // exception from a lower EL (AArch64).
 
    .balign 0x80
    lower_el_aarch64_irq: // The exception handler for the IRQ exception 
    // from a lower EL (AArch64).
    .balign 0x80
    lower_el_aarch64_fiq: // The exception handler for the FIQ exception 
    // from a lower EL (AArch64).
    .balign 0x80
    lower_el_aarch64_serror: // The exception handler for the system error 
    // exception from a lower EL(AArch64).
    .balign 0x80
    lower_el_aarch32_sync: // The exception handler for the synchronous
    // exception from a lower EL(AArch32).
    .balign 0x80
    lower_el_aarch32_irq: // The exception handler for the IRQ exception 
    // from a lower EL (AArch32).
    .balign 0x80
    lower_el_aarch32_fiq: // The exception handler for the FIQ exception 
    // from a lower EL (AArch32).
    .balign 0x80
    lower_el_aarch32_serror: // The exception handler for the system error
    // exception from a lower EL(AArch32).

// ------------------------------------------------------------

CommonHandler:
  STP      x29, x30, [sp, #-16]!
  STP      x18, x19, [sp, #-16]!
  STP      x16, x17, [sp, #-16]!
  STP      x14, x15, [sp, #-16]!
  STP      x12, x13, [sp, #-16]!
  STP      x10, x11, [sp, #-16]!
  STP      x8, x9, [sp, #-16]!
  STP      x6, x7, [sp, #-16]!
  STP      x4, x5, [sp, #-16]!
  STP      x2, x3, [sp, #-16]!
  STP      x0, x1, [sp, #-16]!
  MOV      x0, sp

  bl       handle_sync

  LDP      x0, x1, [sp], #16
  LDP      x2, x3, [sp], #16
  LDP      x4, x5, [sp], #16
  LDP      x6, x7, [sp], #16
  LDP      x8, x9, [sp], #16
  LDP      x10, x11, [sp], #16
  LDP      x12, x13, [sp], #16
  LDP      x14, x15, [sp], #16
  LDP      x16, x17, [sp], #16
  LDP      x18, x19, [sp], #16
  LDP      x29, x30, [sp], #16
  ERET

IRQHandler:
  STP      x29, x30, [sp, #-16]!
  STP      x18, x19, [sp, #-16]!
  STP      x16, x17, [sp, #-16]!
  STP      x14, x15, [sp, #-16]!
  STP      x12, x13, [sp, #-16]!
  STP      x10, x11, [sp, #-16]!
  STP      x8, x9, [sp, #-16]!
  STP      x6, x7, [sp, #-16]!
  STP      x4, x5, [sp, #-16]!
  STP      x2, x3, [sp, #-16]!
  STP      x0, x1, [sp, #-16]!
  MOV      x0, sp

  bl       handle_irq

  LDP      x0, x1, [sp], #16
  LDP      x2, x3, [sp], #16
  LDP      x4, x5, [sp], #16
  LDP      x6, x7, [sp], #16
  LDP      x8, x9, [sp], #16
  LDP      x10, x11, [sp], #16
  LDP      x12, x13, [sp], #16
  LDP      x14, x15, [sp], #16
  LDP      x16, x17, [sp], #16
  LDP      x18, x19, [sp], #16
  LDP      x29, x30, [sp], #16
  ERET




