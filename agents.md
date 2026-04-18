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

## 2. The Graphics Agent (GPU)
**Symbol:** `src/gpu.rs`  
**Role:** Simulates the specialized graphics hardware and its 1MB Video RAM (VRAM).

### Responsibilities:
- **GP0 (Rendering):** Executes drawing commands (polygons, lines, sprites) and handles CPU-to-VRAM/VRAM-to-CPU transfers.
- **GP1 (Status & Control):** Manages display modes, VRAM horizontal/vertical start/stop, and GPU resets.
- **VRAM Management:** Maintains a 1024x512 16-bit pixel buffer.

### Common Pitfalls & Debugging:
- **Semi-Transparency:** Implementing the 4 different semi-transparency modes (B+F, B-F, B+F/4, etc.) correctly is critical for effects.
- **Coordinate Overflow:** PS1 uses 11-bit signed coordinates; drawing outside the drawing area must be clipped properly.
- **VRAM Transfers:** Large image uploads via GP0 often exceed single-packet limits and must be handled across multiple bus cycles.

---

## 3. The Interconnect Agent (Bus)
**Symbol:** `src/bus.rs`  
**Role:** The "Nervous System" of the project, routing all memory-mapped I/O (MMIO) requests.

### Responsibilities:
- **Address Decoding:** Maps the 4GB address space into physical components.
- **Memory Mirroring:** Implements KSEG0, KSEG1, and KSEG2.

### Common Pitfalls & Debugging:
- **Side Effects on Read:** Some I/O registers (like CDROM or DMA) change state or clear flags just by being read.
- **Wait States:** Real hardware has different access speeds for RAM vs. BIOS; while often ignored in basic emulators, it can cause timing bugs in picky games.

---

## 4. The Data Mover Agent (DMA)
**Symbol:** `src/dma.rs`  
**Role:** High-speed data transport between RAM and peripherals.

### Common Pitfalls & Debugging:
- **Linked List Loops:** If a game provides a circular linked list to the GPU DMA, the emulator must detect and break it to avoid infinite loops.
- **Ordering:** Some games expect DMA transfers to finish within a certain number of CPU cycles.

---

## 5. The Media Agent (CDROM)
**Symbol:** `src/cdrom.rs`, `src/ecm.rs`  
**Role:** Simulates the asynchronous CD-ROM drive and its filesystem.

### Common Pitfalls & Debugging:
- **Timing:** CDROM is slow. Sending a command and expecting an immediate INT2 result will break games. The state machine must simulate the physical delay of a spinning disc.
- **Sector Header:** Ensure the 12-byte sync and 4-byte header are handled correctly when reading Raw sectors vs. Data sectors.

---

## 6. The Firmware Agent (BIOS)
**Symbol:** `src/bios.rs`  
**Role:** Loads and serves the original SCPH1001.BIN binary.

### Responsibilities:
- **Initialization:** Sets up the initial CPU state and jump vectors.
- **Syscall Handling:** Provides high-level functions (A0, B0, C0 tables) for memory management, string formatting, and I/O.
- **HLE (High-Level Emulation):** *Optional future capability* to intercept BIOS calls for faster performance.

---

## 7. The Orchestrator Agent (Console)
**Symbol:** `src/console.rs`, `src/main.rs`  
**Role:** The top-level system integrator.

### Responsibilities:
- **Main Loop:** Drives the `Cpu::step()` and `Bus::tick()` functions.
- **Executable Loading:** Supports loading `.exe` and `.psx` files directly into RAM for development and testing.
- **State Management:** Manages the lifecycle of all components and provides the interface for the frontend (SDL2/PPM).

---

## Development Standards & Patterns
- **Safety:** Use Rust's ownership model to prevent data races between components.
- **Timing:** All components should be "clock-cycle aware" where possible to ensure synchronization.
- **Modularity:** Peripherals must communicate exclusively through the `Bus` or `DMA` to maintain clean separation of concerns.
