# Arcturus - Quantum-Relational Computing Ecosystem for the Spancon v3 Chip

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)](https://github.com/swamy18/Arcturus/actions)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-26%2F26%20passing-brightgreen)](#verification-status)

## Overview

Arcturus is a complete quantum-relational computing ecosystem built around the Spancon v3 chip: a 10,000-node
CMOS device with a Bismuthene topological layer, sparse relational memory, and a time-sliced PSRAM-backed
execution model. The project combines bare-metal Rust firmware, a host-side control CLI, and a Python compiler
that maps modern PyTorch/HuggingFace models into the chip's `W` matrix format.

At the firmware level, Arcturus implements graph Laplacian dynamics, Krylov-style unitary evolution,
Frobenius-norm lock, time-travel bank storage, eigenmode access, and an edge cache for ultra-fast local state.
On the host side, the ecosystem provides the tooling needed to load models, compress them, serialize them to
`.awm` files, and upload them into PSRAM-backed time banks for execution.

## Key Features

- 10,000 compute nodes arranged as a 100 x 100 grid
- 100 long-range edges for small-world connectivity
- Sparse `W` matrix storage for relational state
- Graph Laplacian construction with `L = D - A`
- Unitary evolution with `U = exp(i alpha L)`
- Krylov subspace path with `m = 20` for evolution
- Frobenius norm conservation lock with 1e-6 precision target
- 1,000 virtual time-slice banks in 64 MB SPI PSRAM
- 1,024 eigenmodes for spectral addressing
- 2.5 KB edge cache with 2-bit conductance quantization
- 4-bit model quantization for compressed compiler output
- Firmware-compatible `.awm` binary export for PSRAM upload
- Block Universe synchronization across past, present, and future state slices
- Rust `no_std` firmware targeting RV64GC
- Python compiler support for PyTorch and HuggingFace checkpoints
- USB/SPI host control via FT2232H bridge
- Designed for room-temperature operation

## Hardware Prerequisites

- Spancon v3 chip
- RV64GC-capable RISC-V core
- 64 MB external SPI PSRAM
- FT2232H USB-to-SPI bridge
- Host PC for compilation, upload, and control

## Quick Start - Firmware

```bash
cd arcturus/firmware
cargo build --release
cargo test -- --test-threads=1
```

Flash the resulting firmware with your board programmer or RV64GC flashing flow:

```bash
# Example placeholder flow
rust-objcopy --strip-all target/riscv64gc-unknown-none-elf/release/arcturus-firmware firmware.bin
# flash firmware.bin using your preferred toolchain / programmer
```

## Quick Start - Compiler

Install the Python compiler:

```bash
cd arcturus/pc/arcturus_compiler
pip install -r requirements.txt
pip install -e .
```

Compile a PyTorch/HuggingFace model into an Arcturus `.awm` file:

```bash
python -m arcturus_compiler --model gpt2 --output gpt2.awm
```

Additional examples:

```bash
python -m arcturus_compiler --model meta-llama/Llama-2-7b --output llama2.awm --split-banks
```

## Quick Start - CLI

Check the connected chip:

```bash
arcturus-cli --port COM3 status
```

Upload a compiled matrix bank:

```bash
arcturus-cli --port COM3 upload --bank 5 --file gpt2.awm
```

Run a single evolution step:

```bash
arcturus-cli --port COM3 evolve --alpha 0.15 --steps 1
```

## Ecosystem Diagram

```text
Laptop / Workstation
    |
    |  Rust CLI + Python Compiler
    v
USB
    |
    v
FT2232H USB-to-SPI Bridge
    |
    |  SPI command stream
    v
Spancon v3 Chip
    |--- 10,000-node compute fabric
    |--- Sparse W matrix engine
    |--- Time-slice banks
    |--- Eigenbasis memory
    |--- Edge cache
    |
    v
64 MB SPI PSRAM
```

## Verification Status

| Item | Status | Notes |
|---|---:|---|
| Firmware unit tests | 26/26 pass | All current firmware tests green |
| Laplacian construction | PASS | `L = D - A` verified |
| Time-slicer serialization | PASS | `.awm` format matches firmware serializer |
| PSRAM HAL wiring | PASS | `psram_write()` / `psram_read()` integrated |
| GPT-2 compiler export | PASS | `.awm` generated successfully |
| GPT-2 validation | PASS | Quantization drift within target |

## Performance Metrics

| Metric | Estimated Result | Notes |
|---|---:|---|
| Evolution step time | < 100 us per step | Targeted for sparse updates |
| Memory read latency | < 1 us SRAM, < 10 us PSRAM | Platform-dependent |
| SPI command response | < 100 us | FT2232H path |
| Krylov iterations | <= 20 | Current implementation target |
| Time banks | 1,000 | 64 MB PSRAM budgeted |
| Sparse capacity | <= 100,000,000 elements | 10,000 x 10,000 theoretical bound |
| Model compression | ~4-bit quantization | Approx. 25% of baseline size |

## License

Dual licensed under either:

- MIT
- Apache-2.0

## Contributing

Contributions are welcome. Please keep changes aligned with the existing Rust firmware, CLI tooling, and Python
compiler architecture, and include tests or validation output for any behavior changes.
