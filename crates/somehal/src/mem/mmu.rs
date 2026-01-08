use page_table_generic::PageTable;

use crate::mem::ram::Ram;

pub(crate) type ArchPageTable<A> = PageTable<<crate::arch::Arch as crate::ArchTrait>::P, A>;
pub(crate) type ArchPte =
    <<crate::arch::Arch as crate::ArchTrait>::P as page_table_generic::TableGeneric>::P;

static BOOT_TABLE: spin::Once<ArchPageTable<Ram>> = spin::Once::new();

pub(crate) fn new_boot_table() -> ArchPageTable<Ram> {
    let mut table = ArchPageTable::<Ram>::new(Ram).unwrap();

    table
}
