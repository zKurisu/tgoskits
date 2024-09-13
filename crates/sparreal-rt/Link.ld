OUTPUT_ARCH(%ARCH%)

ENTRY(_start)

/* Define the program headers we want so the bootloader gives us the right */
/* MMU permissions; this also allows us to exert more control over the linking */
/* process. */
PHDRS
{
    text    PT_LOAD;
    rodata  PT_LOAD;
    data    PT_LOAD;
}

SECTIONS
{
    . = %KERNEL_VADDR%;
    _skernel = .;

    .text : ALIGN(4K) {
        _stext = .;
        KEEP(*(.head.text))
        KEEP(*(.text.boot.start))
        KEEP(*(.text.boot .text.boot.*))
        . = ALIGN(4);
        *(.text .text.*);
        . = ALIGN(4K);
        _etext = .;
    } :text

    .rodata : ALIGN(4K) {
        _srodata = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        *(.sdata2 .sdata2.*)
        . = ALIGN(4K);
        _erodata = .;
    } :rodata

   
    .data : ALIGN(4K) {
        _sdata = .;
        *(.data.boot .data.boot.*)
        . = ALIGN(4K);
        *(.data .data.*)
        *(.sdata .sdata.*)
        *(.got .got.*)
    } :data

    .tdata : ALIGN(0x10) {
        _stdata = .;
        *(.tdata .tdata.*)
        _etdata = .;
    } :data

    .tbss : ALIGN(0x10) {
        _stbss = .;
        *(.tbss .tbss.*)
        *(.tcommon)
        _etbss = .;
    } :data

    . = ALIGN(4K);
    _edata = .;

    .bss : ALIGN(4K) {
        _sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        . = ALIGN(4K);
        _ebss = .;
    } :data

    _ekernel = .;

    . = ALIGN(4K);
    _stack_top = . + %STACK_SIZE%;
    _cpu_stack_size = %CPU_STACK_SIZE%;
    _kernel_size = _stack_top - _skernel;
	/DISCARD/ : {
        *(.comment) *(.gnu*) *(.note*) *(.eh_frame*)
    }
}
