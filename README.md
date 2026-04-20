# PS1 Emulator

A PlayStation 1 emulator written in Rust, focusing on accuracy and clean implementation.

## Features

- **CPU**: MIPS R3000A emulation with full instruction set, coprocessor 0 (COP0) for exceptions and MMU
- **GTE**: Geometry Transformation Engine for 3D vertex transformation and lighting
- **GPU**: Polygon rendering, sprites, VRAM management, and display output
- **DMA**: High-speed data transfers between RAM and peripherals
- **SPU**: 24-voice audio with ADSR envelopes and ADPCM decoding
- **CDROM**: BIN/CUE and ECM image support with async command processing
- **BIOS**: HLE for BIOS call tracing and debugging

## Building

Requires Rust 1.85+ (2024 edition):

```bash
cargo build --release
```

## Usage

### Run a game from CD image:
```bash
./ps1_emulator bios.bin game.cue
```

### Run a standalone EXE:
```bash
./ps1_emulator bios.bin game.exe
```

### Boot through BIOS (no fast boot):
```bash
./ps1_emulator bios.bin game.cue --bios-boot
```

### Run with window display:
```bash
./ps1_emulator bios.bin game.cue --window
```

### Run for a specific number of CPU steps:
```bash
./ps1_emulator bios.bin game.cue 1000000
```

## Debug Options

Set environment variables for debugging:

- `PS1_TRACE_SYSCALLS=1` - Print syscall instructions
- `PS1_TRACE_BIOS_CALLS=1` - Print BIOS function calls (A0/B0/C0 vectors)
- `PS1_TRACE_LOW_PC=1` - Trace PC transitions below 0x10000
- `PS1_DUMP_PC=1` - Dump PC-relative memory on errors
- `PS1_DUMP_FRAME=<path>` - Save framebuffer to PPM file on exit
- `PS1_DUMP_WORDS=<addr>:<count>` - Dump memory words at address

## Project Structure

| File | Component | Status |
|------|-------------|--------|
| `src/cpu.rs` | MIPS R3000A CPU | Done |
| `src/gte.rs` | Geometry Transformation Engine | Done |
| `src/gpu.rs` | Graphics Processing Unit | Done |
| `src/bus.rs` | Memory bus and I/O routing |  Done |
| `src/dma.rs` | Direct Memory Access controller | Done |
| `src/spu.rs` | Sound Processing Unit |  Done |
| `src/cdrom.rs` | CD-ROM drive emulation |  Done |
| `src/bios.rs` | BIOS ROM and HLE |  Done |
| `src/exe.rs` | PS-X EXE file loader | Done |
| `src/console.rs` | System integration |  Done |

## Agent Architecture

This project uses an "Agent" architecture where each hardware component is modeled as an independent agent. See [agents.md](agents.md) for detailed documentation of each component's responsibilities and common pitfalls.

## License

MIT License - See [LICENSE](LICENSE) for details.
