OUTPUT_ARCH(%ARCH%)

STACK_SIZE = 0x40000;

SECTIONS
{
   .driver : ALIGN(4K) {
        _sdriver = .;
        KEEP(*(.driver.register))
        _edriver = .;
    }
} INSERT AFTER .data;