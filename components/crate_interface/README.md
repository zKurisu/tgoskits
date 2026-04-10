<h1 align="center">ax-crate-interface</h1>

<p align="center">Provides a way to define an interface (trait) in a crate, but can implement or use it in any crate</p>

<div align="center">

[![Crates.io](https://img.shields.io/crates/v/ax-crate-interface.svg)](https://crates.io/crates/ax-crate-interface)
[![Docs.rs](https://docs.rs/ax-crate-interface/badge.svg)](https://docs.rs/ax-crate-interface)
[![Rust](https://img.shields.io/badge/edition-2021-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)

</div>

English | [中文](README_CN.md)

# Introduction

`ax-crate-interface` provides Provides a way to define an interface (trait) in a crate, but can implement or use it in any crate. It is maintained as part of the TGOSKits component set and is intended for Rust projects that integrate with ArceOS, AxVisor, or related low-level systems software.

## Quick Start

### Installation

Add this crate to your `Cargo.toml`:

```toml
[dependencies]
ax-crate-interface = "0.5.0"
```

### Run Check and Test

```bash
# Enter the crate directory
cd components/crate_interface

# Format code
cargo fmt --all

# Run clippy
cargo clippy --all-targets --all-features

# Run tests
cargo test --all-features

# Build documentation
cargo doc --no-deps
```

## Integration

### Example

```rust
use ax_crate_interface as _;

fn main() {
    // Integrate `ax-crate-interface` into your project here.
}
```

### Documentation

Generate and view API documentation:

```bash
cargo doc --no-deps --open
```

Online documentation: [docs.rs/ax-crate-interface](https://docs.rs/ax-crate-interface)

# Contributing

1. Fork the repository and create a branch
2. Run local format and checks
3. Run local tests relevant to this crate
4. Submit a PR and ensure CI passes

# License

Licensed under the Apache License, Version 2.0. See [LICENSE](./LICENSE) for details.
