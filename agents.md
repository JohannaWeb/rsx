# PS1 Emulator: Agent Architectures

This document defines the specialized "Agent" roles within the PS1 emulator project. Each agent represents a critical hardware component, detailing its responsibilities, key data structures, and the logic required for accurate simulation.

---

## 1. The Processor Agent (CPU)
**Symbol:** `src/cpu.rs`  
**Role:** The core executor of the system, simulating the MIPS-compatible R3000A (Little Endian).

### Responsibilities:
- **Instruction Fetch & Decode:** Implements the full MIPS I instruction set.
- **Pipeline Simulation:** Accurately handles the **Load Delay Slot** and **Branch Delay Slot** behaviors.
- **COP0 (System Control):** Manages exceptions (syscalls, breakpoints, invalid instructions), interrupts, and the Memory Management Unit (MMU) status.
- **Register File:** Manages 32 general-purpose registers (GPRs), `HI`/`LO` for multiplication/division, and the Program Counter (`PC`).

### Common Pitfalls & Debugging:
- **Branch Delay Slots:** Remember that the instruction *immediately following* a branch is always executed, even if the branch is taken. Nesting branches in delay slots is undefined behavior.
- **Load Delay Slots:** A value loaded from memory is not available to the very next instruction.
- **Unaligned Access:** The PS1 CPU does not support unaligned memory access and will trigger an exception.

---

## 2. The Graphics Agent (GPU) ✅ DONE
**Symbol:** `src/gpu.rs`  
**Role:** Simulates the specialized graphics hardware and its 1MB Video RAM (VRAM).

### Responsibilities:
- **GP0 (Rendering):** Executes drawing commands (polygons, lines, sprites) and handles CPU-to-VRAM/VRAM-to-CPU transfers.
- **GP1 (Status & Control):** Manages display modes, VRAM horizontal/vertical start/stop, and GPU resets.
- **VRAM Management:** Maintains a 1024x512 16-bit pixel buffer.
- **Display Output:** Provides RGB framebuffer data for the frontend (minifb window or PPM dump).

### Common Pitfalls & Debugging:
- **Semi-Transparency:** Implementing the 4 different semi-transparency modes (B+F, B-F, B+F/4, etc.) correctly is critical for effects.
- **Coordinate Overflow:** PS1 uses 11-bit signed coordinates; drawing outside the drawing area must be clipped properly.
- **VRAM Transfers:** Large image uploads via GP0 often exceed single-packet limits and must be handled across multiple bus cycles.

---

## 3. The Geometry Agent (GTE) ✅ DONE
**Symbol:** `src/gte.rs`  
**Role:** The Geometry Transformation Engine (COP2 coprocessor) - handles 3D math operations.

### Responsibilities:
- **Matrix Operations:** 3D vertex transformation, rotation, and translation matrices.
- **Lighting Calculations:** Computes light vectors, normal vectors, and color calculations.
- **Projection:** Transforms 3D coordinates into 2D screen space for the GPU.
- **Depth Handling:** Z-coordinate management and depth cueing.
- **Registers:** Exposes 32 data registers (GTE data) and 32 control registers (GTE control) via coprocessor instructions.

### Common Pitfalls & Debugging:
- **Fixed-Point Math:** The GTE uses 32-bit fixed-point arithmetic with specific fractional bits (different precision for screen vs. world coordinates).
- **Flag Register:** The FLAG register accumulates errors during operations; games check this for overflow/underflow detection.
- **Instruction Timing:** GTE operations take multiple cycles; proper pipelining with CPU is essential.

---

## 4. The Interconnect Agent (Bus) ✅ DONE
**Symbol:** `src/bus.rs`  
**Role:** The "Nervous System" of the project, routing all memory-mapped I/O (MMIO) requests.

### Responsibilities:
- **Address Decoding:** Maps the 4GB address space into physical components (RAM, BIOS, IO ports).
- **Memory Regions:** Handles KUSEG (user), KSEG0 (cached kernel), KSEG1 (uncached kernel), and KSEG2.
- **RAM:** 2MB main RAM (0x00000000-0x00200000, mirrored in KSEG0/KSEG1).
- **Scratchpad:** 1KB fast scratchpad RAM at 0x1F800000 (uncached) for COP0 register $28.
- **BIOS ROM:** 512KB BIOS at 0x1FC00000 (read-only, mirrored).
- **Hardware Registers:** Maps access to GPU, DMA, CDROM, SPU, Timers, and Interrupt Controller.

### Common Pitfalls & Debugging:
- **Side Effects on Read:** Some I/O registers (like CDROM or DMA) change state or clear flags just by being read.
- **Wait States:** Real hardware has different access speeds for RAM vs. BIOS; while often ignored in basic emulators, it can cause timing bugs in picky games.
- **Unaligned Access:** The PS1 CPU does not support unaligned memory access and will trigger an exception.

---

## 5. The Data Mover Agent (DMA) ✅ DONE
**Symbol:** `src/dma.rs`  
**Role:** High-speed data transport between RAM and peripherals.

### Responsibilities:
- **Channels:** 7 DMA channels (MDECin, MDECout, GPU, CDROM, SPU, PIO, GPU OT/Ordering Table).
- **Transfer Modes:** Burst, Slice, Linked List (for GPU commands), and Normal modes.
- **Direction:** RAM-to-device and device-to-RAM transfers.
- **Control:** DPCR (channel priority) and DICR (interrupt control) registers.

### Common Pitfalls & Debugging:
- **Linked List Loops:** If a game provides a circular linked list to the GPU DMA, the emulator must detect and break it to avoid infinite loops.
- **Ordering:** Some games expect DMA transfers to finish within a certain number of CPU cycles.

---

## 6. The Media Agent (CDROM) ✅ DONE
**Symbol:** `src/cdrom.rs`, `src/ecm.rs`  
**Role:** Simulates the asynchronous CD-ROM drive and its filesystem.

### Responsibilities:
- **Command Processing:** Handles CDROM commands (GetStat, ReadN, Setloc, SeekL, etc.) with proper timing.
- **Response Queues:** Manages INT2 responses and data FIFO for async operation.
- **Sector Reading:** Reads MODE1 and MODE2 sectors from BIN/CUE or ECM images.
- **XA-ADPCM:** Decodes CD-XA audio sectors (ADPCM encoded).
- **ISO 9660:** Locates files and boot executables from the CD filesystem.

### Common Pitfalls & Debugging:
- **Timing:** CDROM is slow. Sending a command and expecting an immediate INT2 result will break games. The state machine must simulate the physical delay of a spinning disc.
- **Sector Header:** Ensure the 12-byte sync and 4-byte header are handled correctly when reading Raw sectors vs. Data sectors.

---

## 7. The Firmware Agent (BIOS) ✅ DONE
**Symbol:** `src/bios.rs`  
**Role:** Loads and serves the original SCPH1001.BIN binary.

### Responsibilities:
- **Initialization:** Sets up the initial CPU state and jump vectors.
- **Syscall Handling:** Provides high-level functions (A0, B0, C0 tables) for memory management, string formatting, and I/O.
- **HLE (High-Level Emulation):** Intercepts BIOS calls for debugging and trace output (A0, B0, C0 vector tables).

### Common Pitfalls & Debugging:
- **Region:** Games may check BIOS region strings; SCPH1001 (NTSC-U) is the most compatible.
- **Vectors:** The A0/B0/C0 function tables must be accessible at specific RAM addresses for games to call BIOS functions.

---

## 8. The Sound Agent (SPU) ✅ DONE
**Symbol:** `src/spu.rs`  
**Role:** Simulates the PS1's Sound Processing Unit — 24 voices with ADSR envelopes and 512KB of sound RAM.

### Responsibilities:
- **Voice Management:** Handles 24 independent voices, each with pitch, volume, and ADSR envelope control.
- **ADSR Envelopes:** Implements Attack, Decay, Sustain, and Release phases per voice.
- **Sound RAM:** Manages the 512KB SPU RAM used for sample storage (ADPCM encoded).
- **Reverb:** Simulates the hardware reverb unit applied to the mixed output.
- **CD Audio Mixing:** Mixes CD-DA audio input with SPU voice output.
- **DMA Transfer:** Receives audio data via DMA transfers from main RAM.

### Common Pitfalls & Debugging:
- **ADPCM Decoding:** Each SPU sample block is 16 bytes decoding to 28 samples; filter coefficients and prediction must be applied correctly.
- **Pitch Modulation:** Voice pitch modulation (using the previous voice's output) is easy to get wrong in ordering.
- **Key On/Off Timing:** Key-on is not instantaneous; games rely on specific timing behavior when rapidly keying voices.

---

## 9. The Executable Loader (EXE) ✅ DONE
**Symbol:** `src/exe.rs`  
**Role:** Parses and loads PS-X EXE files (playable game executables).

### Responsibilities:
- **Header Parsing:** Reads the PS-X EXE header (initial PC, GP, SP, destination address, file size).
- **Section Loading:** Loads text/data sections into the appropriate RAM addresses.
- **Boot Configuration:** Supports both direct EXE boot and CD boot EXE extraction.

### Common Pitfalls & Debugging:
- **Destination Address:** EXE files specify both file offset and RAM destination; these must be handled correctly.
- **Stack Pointer:** Some EXEs rely on the BIOS to set up the stack; direct loading must initialize SP correctly.

---

## 10. The Orchestrator Agent (Console) ✅ DONE
**Symbol:** `src/console.rs`, `src/main.rs`  
**Role:** The top-level system integrator.

### Responsibilities:
- **Main Loop:** Drives the `Cpu::step()` function and coordinates all hardware components.
- **Executable Loading:** Supports loading `.exe` and CD images directly into RAM for development and testing.
- **State Management:** Manages the lifecycle of all components (CPU, GPU, GTE, DMA, SPU, CDROM, BIOS).
- **Debug Interface:** Provides CPU state inspection, VRAM dumping, and instruction tracing via environment variables.
- **Frontend Integration:** Presents display output via minifb window or PPM file dump.

### Environment Variables for Debugging:
- `PS1_TRACE_SYSCALLS`: Print syscall instructions
- `PS1_TRACE_BIOS_CALLS`: Print BIOS function calls
- `PS1_DUMP_PC`: Dump PC-relative memory on errors
- `PS1_DUMP_FRAME`: Save framebuffer to PPM file
- `PS1_TRACE_LOW_PC`: Trace PC transitions below 0x10000

---

## 11. The Error Handler (Error) ✅ DONE
**Symbol:** `src/error.rs`  
**Role:** Centralized error types and Result alias for the emulator.

### Responsibilities:
- **Error Types:** Defines emulator-specific errors (invalid instructions, memory errors, CD read errors, etc.).
- **Result Alias:** Provides a convenient `Result<T>` type for the crate.
- **Error Conversion:** Implements conversions from external error types (IO, parsing, etc.).

---

## Development Standards & Patterns
- **Safety:** Use Rust's ownership model to prevent data races between components.
- **Timing:** All components should be "clock-cycle aware" where possible to ensure synchronization.
- **Modularity:** Peripherals must communicate exclusively through the `Bus` or `DMA` to maintain clean separation of concerns.
- **Testing:** Support headless execution for automated testing via step count and state inspection.
