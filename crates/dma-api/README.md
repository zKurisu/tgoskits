# DMA API

[![Rust](https://github.com/drivercraft/dma-api/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/drivercraft/dma-api/actions/workflows/rust.yml)

A Rust-style DMA streaming API inspired by Linux kernel's scatterlist and DMA mapping APIs.

## Features

- **Unified `DmaStream` API**: Single type that adapts based on feature flags
- **Automatic Feature Selection**: Uses Scatter-Gather with `alloc` feature, single buffer without
- **Automatic Cache Synchronization**: Proper cache flush/invalidate based on transfer direction
- **Linux-compatible Semantics**: API design follows Linux DMA mapping conventions
- **Safe Abstractions**: Type-safe wrappers that prevent common DMA programming errors

## Core Types

### `Direction` - DMA Transfer Direction

```rust
pub enum Direction {
    ToDevice,       // DMA_TO_DEVICE: CPU writes, device reads
    FromDevice,     // DMA_FROM_DEVICE: Device writes, CPU reads  
    Bidirectional,  // DMA_BIDIRECTIONAL: Both directions
}
```

### `DmaStream` - Unified Streaming DMA

RAII wrapper for DMA mapping with automatic unmap on drop.

- With `alloc` feature: Uses Scatter-Gather internally (supports multiple buffers)
- Without `alloc` feature: Uses single buffer mode

### `ScatterEntry` - Single Scatter-Gather Entry (requires `alloc`)

Represents a single DMA memory segment, similar to Linux's `struct scatterlist`.

### `SgTable` - Scatter-Gather Table (requires `alloc`)

A collection of scatter-gather entries, similar to Linux's `struct sg_table`.

## Example

```rust
use dma_api::*;

// ----- OS Side -----

init(&Impled);

struct Impled;

impl Osal for Impled {
    fn map(&self, addr: std::ptr::NonNull<u8>, size: usize, direction: Direction) -> u64 {
        // Virtual to physical/DMA address translation
        addr.as_ptr() as usize as _
    }

    fn unmap(&self, addr: std::ptr::NonNull<u8>, size: usize) {
        // Release DMA mapping
    }

    fn flush(&self, addr: std::ptr::NonNull<u8>, size: usize) {
        // Clean cache - write back to memory
    }

    fn invalidate(&self, addr: std::ptr::NonNull<u8>, size: usize) {
        // Invalidate cache
    }
}

// ----- Driver Side: Simple usage (works with or without alloc) -----

let mut buffer = [0u8; 4096];

// Map buffer for DMA (CPU -> Device)
let mapping = unsafe {
    DmaStream::from_buffer(&mut buffer, 0x1000_0000, Direction::ToDevice).unwrap()
};

// Program DMA controller with mapping.dma_addr()
let dma_addr = mapping.dma_addr();

// ... DMA transfer happens ...

// Mapping is automatically unmapped when dropped

// ----- Driver Side: Scatter-Gather DMA (requires alloc feature) -----

#[cfg(feature = "alloc")]
{
    // Build scatter-gather table
    let mut sgt = SgTable::with_capacity(2);
    
    // Add entries (virt_addr, phys_addr, offset, length)
    sgt.push(ScatterEntry::new(ptr1, phys1, 0, 4096));
    sgt.push(ScatterEntry::new(ptr2, phys2, 0, 8192));
    
    // Map for DMA
    let sg_mapping = unsafe {
        DmaStream::new(sgt, Direction::FromDevice).unwrap()
    };
    
    // Program scatter-gather DMA descriptors
    sg_mapping.for_each_dma(|dma_addr, len| {
        // Fill hardware descriptor with dma_addr and len
    });
    
    // ... DMA transfer happens ...
    
    // Sync for CPU access before reading data
    sg_mapping.sync_for_cpu();
    
    // All mappings automatically unmapped on drop
}
```

## Cache Synchronization

The API follows Linux DMA cache coherency semantics:

### Before DMA Transfer

- **ToDevice**: `flush` - write CPU data to memory
- **FromDevice**: `invalidate` - discard stale CPU cache
- **Bidirectional**: `flush` - ensure memory is up-to-date

### After DMA Transfer (sync_for_cpu)

- **FromDevice/Bidirectional**: `invalidate` - read fresh device data
- **ToDevice**: No action needed

### Between Multiple Transfers (sync_for_device)

- **ToDevice/Bidirectional**: `flush` - write new CPU data
- **FromDevice**: `invalidate` - prepare for device write

## API Reference (Linux Equivalent)

| Rust API | Linux Equivalent |
|----------|------------------|
| `DmaStream::new()` | `dma_map_sg()` / `dma_map_single()` |
| `DmaStream::drop()` | `dma_unmap_sg()` / `dma_unmap_single()` |
| `DmaStream::sync_for_cpu()` | `dma_sync_sg_for_cpu()` / `dma_sync_single_for_cpu()` |
| `DmaStream::sync_for_device()` | `dma_sync_sg_for_device()` / `dma_sync_single_for_device()` |
| `ScatterEntry` | `struct scatterlist` |
| `SgTable` | `struct sg_table` |

