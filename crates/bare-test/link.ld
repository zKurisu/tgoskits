INCLUDE link.x

SECTIONS
{
    .test_case : ALIGN(4)
    {
        _stest_case = .;
        *(.test_case)
        *(.test_case.*)
        _etest_case = .;
    }
    /* 其他段定义 */
}
INSERT AFTER .data;