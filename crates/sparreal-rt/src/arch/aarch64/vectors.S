.macro SaveRegister
	stp	X0,X1, [sp,#-0x10]!
	stp	X2,X3, [sp,#-0x10]!
	stp	X4,X5, [sp,#-0x10]!
	stp	X6,X7, [sp,#-0x10]!
	stp	X8,X9, [sp,#-0x10]!
	stp	X10,X11, [sp,#-0x10]!
	stp	X12,X13, [sp,#-0x10]!
	stp	X14,X15, [sp,#-0x10]!
	stp	X16,X17, [sp,#-0x10]!
	stp X18,X19, [sp,#-0x10]!
	stp X29,X30, [sp,#-0x10]!

	stp	q0,q1, [sp,#-0x20]!
	stp	q2,q3, [sp,#-0x20]!
	stp	q4,q5, [sp,#-0x20]!
	stp	q6,q7, [sp,#-0x20]!
	stp	q8,q9, [sp,#-0x20]!
	stp	q10,q11, [sp,#-0x20]!
	stp	q12,q13, [sp,#-0x20]!
	stp	q14,q15, [sp,#-0x20]!
	stp	q16,q17, [sp,#-0x20]!
	stp	q18,q19, [sp,#-0x20]!
	stp	q20,q21, [sp,#-0x20]!
	stp	q22,q23, [sp,#-0x20]!
	stp	q24,q25, [sp,#-0x20]!
	stp	q26,q27, [sp,#-0x20]!
	stp	q28,q29, [sp,#-0x20]!
	stp	q30,q31, [sp,#-0x20]!
.endm

.macro RestoreRegister
	ldp	q30,q31, [sp], #0x20
	ldp	q28,q29, [sp], #0x20
	ldp	q26,q27, [sp], #0x20
	ldp	q24,q25, [sp], #0x20
	ldp	q22,q23, [sp], #0x20
	ldp	q20,q21, [sp], #0x20
	ldp	q18,q19, [sp], #0x20
	ldp	q16,q17, [sp], #0x20
	ldp	q14,q15, [sp], #0x20
	ldp	q12,q13, [sp], #0x20
	ldp	q10,q11, [sp], #0x20
	ldp	q8,q9, [sp], #0x20
	ldp	q6,q7, [sp], #0x20
	ldp	q4,q5, [sp], #0x20
	ldp	q2,q3, [sp], #0x20
	ldp	q0,q1, [sp], #0x20

	ldp X29,X30, [sp], #0x10
	ldp X18,X19, [sp], #0x10
	ldp	X16,X17, [sp], #0x10
	ldp	X14,X15, [sp], #0x10
	ldp	X12,X13, [sp], #0x10
	ldp	X10,X11, [sp], #0x10
	ldp	X8,X9, [sp], #0x10
	ldp	X6,X7, [sp], #0x10
	ldp	X4,X5, [sp], #0x10
	ldp	X2,X3, [sp], #0x10
	ldp	X0,X1, [sp], #0x10
.endm



// Typical exception vector table code.
.balign 0x800
.global vector_table_el1
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
    B {sync_handler}
    .balign 0x80
    curr_el_spx_irq: 
	B {irq_handler}
    .balign 0x80
    curr_el_spx_frq: 
    B {fiq_handler}
    .balign 0x80
    curr_el_spx_serror: // The exception handler for the system error 
    B {serror_handler}
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

.align 8


.end