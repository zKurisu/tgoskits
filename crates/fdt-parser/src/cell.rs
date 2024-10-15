#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MetaData {
    pub address_cells: u8,
    pub size_cells: u8,
    pub clock_cells: u8,
    pub interrupt_cells: u8,
    pub gpio_cells: u8,
    pub dma_cells: u8,
    pub cooling_cells: u8,
}
