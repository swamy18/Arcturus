# Arcturus OS Architecture

## Overview

The **Arcturus OS** is a bare-metal quantum-relational computing firmware designed for the Arcturus chip (Spancon v3 GDS). It implements a complete firmware/software stack that unlocks hidden quantum-relational capabilities without any hardware modifications.

## Hardware Platform

| Parameter | Value |
|-----------|-------|
| Processor | RISC-V RV64GC @ 100 MHz |
| SRAM | 512 KB on-chip |
| External Memory | 64 MB SPI PSRAM |
| Node Grid | 100×100 = 10,000 nodes |
| Connectivity | 4-nearest-neighbor + 100 long-range edges |
| Communication | USB-to-SPI bridge (FT2232H) |

## Memory Hierarchy

### 1. Time-Slicing Memory (1000 Banks)
- **Purpose**: Store W(t) relational states for reversible evolution
- **Capacity**: 1000 time steps (t=0 to t=999)
- **Storage**: Sparse matrix format in external PSRAM
- **Operations**: Forward (U), Backward (U†), Jump, Save/Load

### 2. Eigenbasis Memory (1024 Modes)
- **Purpose**: Map data to Laplacian eigenvalues
- **Range**: λ₁ to λ₁₀₂₄ (skip λ₀ = 0)
- **Data encoding**: Eigenvalue perturbation δ = ±Δ
- **Storage**: 1024 modes × 2 bits each

### 3. Edge Cache (10,000 Nodes × 2 bits)
- **Purpose**: L1 cache using bismuthene edge states
- **Physics**: Conductance quantization G = n·e²/h
- **Encoding**: 4 conductance levels → 2 bits/node
- **Capacity**: 20,000 bits = 2,500 bytes logical

## Compute Engine

### Graph Laplacian (L = D - A)
```
L[i,j] = {
  degree(i)              if i == j
  -1                     if i and j are neighbors
  0                      otherwise
}
```
- Grid: 100×100 = 10,000 nodes
- Edges: ~20,000 grid edges + 100 long-range edges
- Storage: Sparse format (neighbor lists)

### Unitary Evolution (U = exp(iαL))
```
U = exp(iαL) ≈ I + iαL - (αL)²/2! - i(αL)³/3! + ...
```
- α: Evolution parameter (avoid α ≈ 0.8 dead zone)
- Computation: Krylov subspace method or Padé approximation
- Properties: U·U† = I (unitary)

### Relational State Evolution
```
W(t+1) = U · W(t) · U†
```
- W: Relational state matrix (10,000×10,000)
- Storage: Sparse (non-zero elements only)
- Invariant: ||W(t+1)||_F = ||W(t)||_F (Frobenius norm)

## Block Universe Synchronization

### GHZ Entanglement
```
|Ψ⟩ = (|000⟩ + |111⟩) / √2
     = (|PPP⟩ + |FFF⟩) / √2
     (Past-Present-Future superposition)
```

### Correlation Structure
- C(Present, Past) = λ (correlation strength)
- C(Present, Future) = λ
- C(Past, Future) = λ

### Synchronization
1. Initialize: Create GHZ entanglement (λ = 1)
2. Evolve: Apply U to each slice (λ preserved)
3. Measure: C(P,Past) = C(P,Future) = λ(t)
4. Sync: Re-establish GHZ if λ < threshold

## Communication Protocol

### SPI Command Format
```
[CMD:1][PARAM1:1][PARAM2:1][LEN:1][DATA:0-252]
Total: 4-256 bytes
```

### Response Format
```
[STATUS:1][DATA_LEN:1][DATA:0-7]
Total: 2-9 bytes
```

### Command Set
| ID | Command | Description |
|----|---------|-------------|
| 0x00 | NOP | Ping/heartbeat |
| 0x01 | READ | Read memory |
| 0x02 | WRITE | Write memory |
| 0x03 | APPLY_PHASE | Apply phase to node |
| 0x04 | MEASURE | Measure conductance |
| 0x05 | EVOLVE | Evolve system |
| 0x06 | SYNC | Synchronize time |
| 0x07 | GET_STATUS | Get system status |
| 0x08 | RESET | Reset chip |
| 0x09 | SET_ALPHA | Set evolution parameter |
| 0x0A-0x11 | ... | Extended commands |

## File Structure

```
arcturus/
├── Cargo.toml                  # Workspace root
├── firmware/                   # Bare-metal firmware
│   ├── Cargo.toml
│   ├── .cargo/config.toml      # RISC-V build config
│   └── src/
│       ├── main.rs             # Entry point
│       ├── hal/                # Hardware abstraction
│       │   ├── mod.rs
│       │   ├── gpio.rs         # Node addressing
│       │   ├── spi.rs          # SPI/PSRAM
│       │   └── analog.rs       # DAC/ADC
│       ├── memory/             # Memory management
│       │   ├── mod.rs
│       │   ├── time_slicer.rs  # 1000 time banks
│       │   ├── eigen_manager.rs # Eigenbasis storage
│       │   └── edge_cache.rs   # L1 edge cache
│       ├── compute/            # QRE engine
│       │   ├── mod.rs
│       │   ├── laplacian.rs    # Graph Laplacian
│       │   └── evolution.rs    # Unitary evolution
│       ├── sync/               # Block universe
│       │   ├── mod.rs
│       │   └── block_universe.rs
│       └── api/                # PC communication
│           ├── mod.rs
│           └── commands.rs
└── pc/                         # Host software
    └── arcturus_cli/           # Command-line tool
        ├── Cargo.toml
        └── src/
            └── main.rs
```

## Build Instructions

### Prerequisites
- Rust toolchain with RISC-V target: `rustup target add riscv64gc-unknown-none-elf`
- `cargo-binutils`: `cargo install cargo-binutils`
- OpenOCD for debugging (optional)

### Build Firmware
```bash
cd arcturus/firmware
cargo build --release
```

### Build CLI Tool
```bash
cd arcturus/pc/arcturus_cli
cargo build --release
```

### Flash Firmware
```bash
# Using OpenOCD
openocd -f interface/ftdi/ft2232h.cfg -f target/riscv.cfg

# In GDB
riscv64-unknown-elf-gdb target/riscv64gc-unknown-none-elf/release/arcturus-firmware
(gdb) target remote :3333
(gdb) load
(gdb) continue
```

## Usage Examples

### Check chip status
```bash
arcturus-cli --port COM3 status
```

### Apply phase to node
```bash
arcturus-cli --port COM3 apply-phase --node 5050 --angle 1.5708
```

### Evolve system
```bash
arcturus-cli --port COM3 evolve --alpha 1.2 --steps 100
```

### Synchronize time states
```bash
arcturus-cli --port COM3 sync
```

## License

MIT OR Apache-2.0
