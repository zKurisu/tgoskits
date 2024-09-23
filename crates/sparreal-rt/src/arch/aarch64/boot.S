.section ".head.text","ax"
.global _start

// Linux header
_start:
    // code0/code1
    nop
    b entry

    // text_offset
    .quad 0x80000

    // image_size
    .quad _kernel_size

    // flags
    .quad 0

    // Reserved fields
    .quad 0
    .quad 0
    .quad 0

    // magic - yes 0x644d5241 is the same as ASCII string "ARM\x64"
    .ascii "ARM\x64"

    // Another reserved field at the end of the header
    .byte 0, 0, 0, 0

entry:
    bl _start_boot 

loop:
    b loop


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
  LDR      x10, =_start_boot
  SUB      x10, x10, x11 // x10 = va_offset

  MOV      x19, x0        // x9 = dtb_addr

  LDR      x1, =_stack_top
  SUB      x1, x1, x10 // X1 == STACK_TOP

  MRS      x2, MPIDR_EL1
  AND      x2, x2, #0xFF // X2 == CPU number.

  # cbnz     x2, _suspend

  LDR      x3, =_cpu_stack_size
  MUL      x3, x2, x3 // Create separated stack spaces 
  SUB      x1, x1, x3 // for each processor
  MOV      sp, x1

  BL       __switch_to_el1

.global _el1_entry
.type _el1_entry, @function
_el1_entry:
  // Install vector table
  // --------------------- 
  .global  vector_table_el1
  LDR      x0, =vector_table_el1
  MSR      VBAR_EL1, x0
  
  MOV      X1, #(0x3 << 20) // FPEN disables trapping to EL1.
  MSR      CPACR_EL1, X1
  ISB


  MOV      x0, x19
  MOV      x1, x10
  BL       __rust_main
  B        .


_suspend:
  wfi