/*
 * void __asm_flush_dcache_range(start, end)
 *
 * clean & invalidate data cache in the range
 *
 * x0: start address
 * x1: end address
 */
.pushsection .text.__asm_flush_dcache_range, "ax"
ENTRY(__asm_flush_dcache_range)
	mrs	x3, ctr_el0
	ubfx	x3, x3, #16, #4
	mov	x2, #4
	lsl	x2, x2, x3		/* cache line size */

	/* x2 <- minimal cache line size in cache system */
	sub	x3, x2, #1
	bic	x0, x0, x3
1:	dc	civac, x0	/* clean & invalidate data or unified cache */
	add	x0, x0, x2
	cmp	x0, x1
	b.lo	1b
	dsb	sy
	ret
ENDPROC(__asm_flush_dcache_range)
.popsection

/*
 * void __asm_invalidate_dcache_range(start, end)
 *
 * invalidate data cache in the range
 *
 * x0: start address
 * x1: end address
 */
.pushsection .text.__asm_invalidate_dcache_range, "ax"
ENTRY(__asm_invalidate_dcache_range)
	mrs	x3, ctr_el0
	ubfx	x3, x3, #16, #4
	mov	x2, #4
	lsl	x2, x2, x3		/* cache line size */

	/* x2 <- minimal cache line size in cache system */
	sub	x3, x2, #1
	bic	x0, x0, x3
1:	dc	ivac, x0	/* invalidate data or unified cache */
	add	x0, x0, x2
	cmp	x0, x1
	b.lo	1b
	dsb	sy
	ret
ENDPROC(__asm_invalidate_dcache_range)
.popsection

