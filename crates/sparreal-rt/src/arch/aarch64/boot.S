  .section .text.boot.start,"ax"
  .align 3

// ------------------------------------------------------------

  .global _start_boot
  .type _start_boot, @function
_start_boot:

  // Clear registers
  // ---------------
  // This is primarily for RTL simulators, to avoid
  // possibility of X propagation
  
  ADR      x11, .
  LDR      x10, =_skernel
  SUB      x10, x10, x11 // x10 = va_offset

  MOV      x9, x0 

  LDR      x1, =_stack_top
  SUB      x1, x1, x10 // X1 == STACK_TOP

  MRS      x2, MPIDR_EL1
  AND      x2, x2, #0xFF // X2 == CPU number.
  LDR      x3, =_cpu_stack_size
  MUL      x3, x2, x3 // Create separated stack spaces 
  SUB      x1, x1, x3 // for each processor
  MOV      sp, x1

  SUB      sp, sp, #16
  STR      x9, [sp]

  // Install vector table
  // --------------------- 
  .global  vector_table_el1
  LDR      x0, =vector_table_el1
  MSR      VBAR_EL1, x0

  MOV      X1, #(0x3 << 20) // FPEN disables trapping to EL1.
  MSR      CPACR_EL1, X1
  ISB

  # MOV      x0, x10
  # BL       enable_mmu

  LDR      x0, [sp]
  ADD      sp, sp, #16
  MOV      x1, x10
  BL       __rust_main
  B       .


  # ISB

  // Branch to scatter loading and C library init code
  // -------------------------------------------------
  # LDR      x0, [sp]
  # ADD      sp, sp, #16
  # LDR      x1, x10
  # LDR      x8, =__rust_main
  # BLR      x8
  # B       .
       

// ------------------------------------------------------------
// End of file
// ------------------------------------------------------------