# PS1 Emulator Review — Unfiltered Edition

**Reviewer:** grumpy principal, too much coffee, has written an R3000A emulator before.
**Scope:** ~8k lines. Everything in `src/`.
**TL;DR:** The structure is fine. The hot path is a crime. The BIOS handling is a philosophical crisis. The timing model is a polite suggestion. Fixable. Let's go.

---

## The roast (with receipts)

### 1. You are calling `std::env::var_os` on every single CPU instruction. Stop.

`src/cpu/mod.rs:118` — `cpu.step()` opens with:

```rust
let trace_pc = std::env::var_os("PS1_TRACE_PC").is_some();
```

That's a locked read of the process environment table. Per instruction. At 33 MHz target, that's ~33 million syscalls per emulated second. You are paying a process-wide mutex to answer the same yes/no question forever.

Same disease:
- `src/cpu/mod.rs:255` — `PS1_TRACE_INTERRUPTS` checked every interrupt entry.
- `src/cpu/bios.rs:101,118` — `PS1_TRACE_INTERRUPTS`, `PS1_TRACE_TTY`.
- `src/gpu/mod.rs:239` — `PS1_TRACE_GPU` checked every GP0 command.

**Fix:** Read env vars *once* at startup into `bool` fields on `Cpu`/`Gpu`, or a `TraceFlags` struct passed in. A `once_cell::Lazy<bool>` is acceptable. Don't make me see this again.

### 2. `Bus::read32` is four calls to `Bus::read8`. That's not an emulator, that's a byte-at-a-time state machine.

`src/bus/mod.rs:224-234`:

```rust
pub fn read32(&mut self, address: u32) -> Result<u32> {
    require_aligned(address, 4)?;
    if mask_region(address) == MDEC_STATUS { return Ok(MDEC_STATUS_FIFO_EMPTY); }
    Ok(u32::from_le_bytes([
        self.read8(address)?,
        self.read8(address.wrapping_add(1))?,
        self.read8(address.wrapping_add(2))?,
        self.read8(address.wrapping_add(3))?,
    ]))
}
```

Every instruction fetch walks that giant `match` **four times**. Every MMIO read decodes the address range four times. Every `lw` pays for four range-comparisons. The aligned case is 95%+ of accesses — inline a fast-path that indexes RAM directly and only falls back to the byte machine for MMIO.

Same for `write32` — even after your "fast paths" for GP0/GP1/DMA/SPU, the fallback loops `write8` four times through the match.

**Fix:** Split the hot paths. RAM/BIOS/scratchpad should be direct `u32` loads/stores using `from_le_bytes` on a 4-byte slice. MMIO dispatch happens once per word, not per byte.

### 3. `Console::step` ticks the bus once per CPU instruction. Timing is fiction.

`src/console/mod.rs:122`:

```rust
pub fn step(&mut self) -> Result<()> {
    self.cpu.step(&mut self.bus)?;
    self.bus.tick();
    Ok(())
}
```

`bus.tick()` increments root counters by 1, decrements CDROM timers by 1, ticks SPU by 1. But a PS1 CPU instruction is not a single bus cycle — loads cost 4+ cycles, BIOS fetches cost 22 cycles, RAM fetches ~5. Your `VBLANK_INTERVAL_TICKS = 33_868` (why?) will fire whenever you've stepped that many instructions, not when 1/60s of emulated time has passed. Any game that relies on VBlank cadence vs. code length gets warped time.

**Fix:** Return a cycle count from `cpu.step()` (even if you hand-wave it as 2 cycles/instr for now). Advance a `cycles: u64` counter. Drive `bus.tick()` off real cycle budgets: `counters_tick(cycles_elapsed)`, `hsync/vsync` at 2172/263 GPU cycles, etc. NTSC vblank is ~564_480 CPU cycles, *not* 33_868 instructions.

### 4. The CPU struct has a fake heap, BIOS hooks, and register snapshots. This is not a CPU anymore.

`src/cpu/mod.rs:38-50`:

```rust
pub struct Cpu {
    regs: [u32; 32],
    cop0: [u32; 32],
    gte: Gte,
    hi: u32, lo: u32,
    pc: u32, next_pc: u32,
    bios_heap: u32,
    interrupt_hook: Option<InterruptHook>,
    interrupt_return_pc: Option<u32>,
    interrupt_saved_registers: Option<([u32; 32], u32, u32)>,
}
```

You've smuggled HLE state — a *heap pointer*, an interrupt handler pointer, and a full register snapshot — into the CPU. `src/cpu/bios.rs:85-89` has:

```rust
fn allocate_bios_heap(&mut self, size: u32) {
    let size = (size + 3) & !3;
    self.regs[2] = self.bios_heap;
    self.bios_heap = self.bios_heap.wrapping_add(size);
}
```

A bump allocator that never frees, living on the CPU, handed out to games as if it were BIOS malloc. It *works* until a game calls `free()` and expects the pointer to actually be reclaimable, or until it allocates past wherever you started (`0x8001_0000`), at which point you're corrupting the kernel area.

`return_from_exception` restoring a full 32-register snapshot on `B(0x17)` is worse: real hardware doesn't do that. If a game's IRQ handler legitimately writes $v0 and expects it visible in the interrupted code, you'll silently clobber it.

**Fix — pick one lane:**
- **Full LLE:** delete all of this. Run the real BIOS. Syscall/break trigger the real exception vector. Let the BIOS allocate memory, hook interrupts, and return itself. Correct and simpler.
- **Full HLE:** move this into a proper `Kernel` struct in its own module. Give it real state (free list, TCB table, event queue). Document it.

Half-HLE is how you spend a year chasing ghosts in Square games.

### 5. No load-delay slot emulation. Your `AGENTS.md` lies about this.

`src/cpu/instructions.rs:308-314`:

```rust
pub(super) fn op_lw(&mut self, _pc: u32, instruction: u32, bus: &mut dyn CpuBusAccess) -> Result<()> {
    let address = self.reg(rs(instruction)).wrapping_add(imm(instruction) as u32);
    self.set_reg(rt(instruction), bus.read32(address)?);
    Ok(())
}
```

This sets `rt` immediately. On real R3000A, the loaded value is **not visible to the next instruction** — that slot sees the old register. Hand-written asm and optimized compilers absolutely rely on this. Your `agents.md` claims "Pipeline Simulation: Accurately handles Load Delay Slot." It does not. Delete the claim or implement it.

**Fix:** Add a `load_delay: Option<(usize, u32)>` pair of pending load slots. On each `step`: (1) execute instruction with current regs, (2) commit the previous pending load, (3) stage the new one. Mednafen and Duckstation both document this pattern. Takes about 30 lines.

### 6. `add`, `addi`, `sub` don't trap on overflow. You implemented `addu`/`subu` for both.

`src/cpu/instructions.rs:235-246,601-611`:

```rust
pub(super) fn op_add_immediate(...) {
    let value = self.reg(rs(instruction)).wrapping_add(imm(instruction) as u32);
    self.set_reg(rt(instruction), value);
    Ok(())
}
```

And the dispatch table points *both* `Addi` and `Addiu` at the same handler. Same with `Add` and `Addu`, `Sub` and `Subu`. On real MIPS, the non-`u` variants raise an arithmetic overflow exception. Most games don't hit this, but GCC-compiled code occasionally does, and when they do you'll silently compute garbage.

**Fix:** `addi` / `add` / `sub` must use `checked_add_signed`/`checked_sub` and raise a proper Overflow exception (Cause code 12).

### 7. `syscall` and `break` don't go through the exception vector.

`special_syscall` (`src/cpu/instructions.rs:478-503`) does some ad-hoc HLE for `EnterCriticalSection` / `ExitCriticalSection` based on `$a0`, then just falls out. `special_break` (line 465-476) just logs and returns `Ok`. Neither jumps to `0x8000_0080` or sets CAUSE. Games that do their own syscall routing (i.e. any commercial game) will wander off into nowhere.

**Fix:** Both should call a proper `enter_exception(cause, epc)` helper. Delete the HLE. Let the BIOS routine at the exception vector handle it.

### 8. Divide-by-zero is silently ignored. Real R3000 has specific semantics.

`src/cpu/instructions.rs:571-599`:

```rust
if divisor != 0 {
    self.lo = dividend.wrapping_div(divisor) as u32;
    self.hi = dividend.wrapping_rem(divisor) as u32;
}
```

On divide by zero, real hardware loads specific values: signed `div` → `lo = (dividend < 0) ? 1 : -1`, `hi = dividend`. Unsigned `divu` → `lo = 0xFFFF_FFFF`, `hi = dividend`. Compilers emit these divisions counting on that behavior for subsequent checks.

**Fix:** encode the documented semantics. Five lines.

### 9. `runtime_state()` is a DoS on yourself. You call it every frame.

`src/console/mod.rs:87-103` — `runtime_state()` allocates:
- `display_rgb: Vec<u8>` (up to ~450 KB)
- `framebuffer_rgb: Vec<u8>` (always 1.5 MB — `vram_rgb()` walks all 512 K VRAM pixels)
- `CdRomDebugState`, `GpuDebugState`, `DmaDebugState` clones

And `src/main.rs:396` calls it **every frame** in `run_window`. That's ~2 MB of heap churn per frame, 60 times per second, just to update a window title and read the display region. Also: `crash_context` calls `runtime_state` too, so crashes also allocate a framebuffer copy. Cute.

**Fix:**
- `display_rgb` should write into a caller-provided buffer: `fn copy_display_into(&self, out: &mut [u8])`.
- The window-title info (pc, command counts) can come from cheap `Copy` getters, not a full runtime snapshot.
- Only call `vram_rgb()` when actually dumping to PPM.

### 10. `gp0_words.clone()` on every completed GPU command.

`src/gpu/mod.rs:235` — you copy the accumulated word buffer into a new `Vec` just so you can clear and then pass to `execute_gp0`. Thousands of allocations per frame.

**Fix:** pass `&self.gp0_words` into `execute_gp0` and clear after. Or swap with a scratch buffer via `std::mem::take`/`std::mem::swap`.

### 11. `dyn CpuBusAccess` for a trait with one impl.

Every instruction handler takes `&mut dyn CpuBusAccess`. You have exactly one implementer: `Bus`. Every `bus.read32`, `bus.write8`, etc. is going through a vtable. For no reason.

**Fix:** Make the trait go away, or make `Cpu::step` generic: `fn step<B: CpuBusAccess>(&mut self, bus: &mut B)`. Monomorphized, inlinable, no vtable. The `InstructionHandler` fn-pointer table will still work (you'll just instantiate it per `B`). Or skip the table entirely and use a `match` — modern rustc turns a dense match on 64 values into a jump table, and you lose the `dyn`.

### 12. `Bus::interrupt_pending_bits` is recomputed from IO memory on every CPU step.

`src/cpu/mod.rs:131,242-252` — every step, read 8 bytes from `io`, mask, check. This is the hottest hot-path in the loop. And every CDROM write8 calls `sync_cdrom_interrupt()` which redoes the read/write dance.

**Fix:** Maintain `irq_status: u32` and `irq_mask: u32` as plain fields on `Bus`. `interrupt_pending_bits()` becomes `self.irq_status & self.irq_mask`. Update on writes to `0x1F801070`/`0x1F801074` and on event raises. One word load, not eight byte loads plus bitmath.

### 13. The `io: Box<[u8; 8192]>` approach is a trap.

You're storing interrupt status, root counters, and everything in-between as raw bytes, then doing byte-level masking in `write_io8` to emulate word-wide register semantics. That's why writing byte-by-byte to `0x1F801070` works but is baroque. Also means reads of the interrupt mask don't go through any logic — just raw memory.

**Fix:** Give each device its own typed register block. `io` as a byte buffer for "unknown I/O" is fine; the *known* registers should be fields.

### 14. Tests: thin where it matters most.

- No GTE tests visible from the top level — you have 1151 lines of fixed-point matrix math with no reference vectors. Grab the `amidog` GTE test ROM or Psy-Q test vectors and lock in the MAC/IR flags. This will bite you later, hard.
- No load-delay-slot test (because it's not implemented, see #5).
- No overflow-exception test (because see #6).
- No "interrupt interrupts a branch delay slot" test — a classic emu-dev trap.

### 15. Small fry, in one breath

- `src/cpu/mod.rs:301` — `pub(super) fn rs/rt/rd/...` — these are fine but every call-site manually reconstructs `((instruction>>21)&0x1f) as u8` in `op_cop2`. Use the helpers everywhere.
- `src/cpu/mod.rs:179` — `format!("syscall")` — clippy will scream; it's `"syscall".to_string()`.
- `src/cpu/instructions.rs:1-7` — 7-line import salad. Re-export from `cpu::mod` with a `pub use` block.
- `src/error/mod.rs` — hand-rolled `Display`. `thiserror` already is a dependency of `fern`; add a direct dep and delete the impl.
- `.gitignore` is 119 bytes and `git status` shows 5 multi-gig `.bin` files, `.7z` archives, `.ppm` frame dumps, and `ps1.log` untracked in your working tree. Get these out of the repo folder or add them to `.gitignore`. You're one `git add .` away from pushing 1.3 GB of ISOs to GitHub and getting a DMCA love letter.
- `src/bus/mod.rs:322-335` — `SPU_BASE..SPU_END` on `write32` splits a word into two `write16` calls, but does not `require_aligned(address, 4)` first. Unaligned SPU word writes silently succeed in ways the hardware would fault on.
- `src/bus/mod.rs:226-228` — the `MDEC_STATUS` hack (always returns FIFO-empty). Write a `// TODO: real MDEC` and pretend you meant it, or implement a stub `Mdec` struct. This will manifest as hanging FMV videos, and you won't know why.
- `src/bus/mod.rs:481-495` — `execute_spu_dma_write` collects into `Vec<u32>` just to pass an iterator downstream. Give `Spu::dma_write` a `&mut dyn FnMut() -> u32` or an iterator-by-ref.
- `main.rs:11` — `WINDOW_CPU_STEPS_PER_FRAME: usize = 20_000`. See #3. This is how fast your emulator runs, not how fast the emulated CPU runs. Rename it and remove the implied promise.
- `src/bios/mod.rs` is 71 lines — fine. `exe/mod.rs` is 105 — fine. But there's nothing in `src/bin/unecm.rs` that couldn't be a `--decode-ecm` flag on the main binary. Two binaries for a home-project emulator is cruft.

---

## Tangible improvements, priority-ordered

| # | Change | Effort | Payoff |
|---|---|---|---|
| 1 | Kill all `env::var_os` from hot path; read once at startup | 30 min | 5–20× CPU speedup (no joke) |
| 2 | Implement load-delay slots | 1–2 hr | Correctness for any non-trivial game |
| 3 | Trap on `add`/`addi`/`sub` overflow; send `syscall`/`break` to exception vector; fix div-by-zero | 2–3 hr | Unblocks a whole class of games |
| 4 | Remove BIOS HLE from `Cpu`; let the real BIOS run | 4–6 hr | Eliminates entire bug category; simplifies code |
| 5 | Bus fast-path: direct RAM/BIOS word loads, MMIO dispatched once per access | 2 hr | 3–10× memory-access speedup |
| 6 | Stop allocating debug state per frame; write display into caller buffer | 1 hr | Smooth frame pacing |
| 7 | Replace `dyn CpuBusAccess` with generic `B: CpuBusAccess` | 30 min | Maybe 1.5–2× instruction throughput |
| 8 | Real cycle budget; drive `bus.tick()` off cycles, not per-step | 3–4 hr | Games-that-care will start working |
| 9 | Typed interrupt status/mask on `Bus`; drop `io_word` lookups | 1 hr | Measurable in profiler, cleans up `sync_cdrom_interrupt` |
| 10 | GTE test harness with amidog/Psy-Q vectors | 1 day | Catches regressions before they eat a weekend |
| 11 | `.gitignore` the ISOs and `.ppm` frame dumps | 1 min | Don't lose your account |

---

## What you did well (credit where due)

- **Module layout is sane.** CPU / Bus / GPU / DMA / CDROM / GTE / SPU as separate concerns. `pub use` re-exports in `lib.rs` are tidy.
- **Dispatch tables** (`PRIMARY_OPCODE_TABLE` etc.) built at compile-time with `const fn`-ish initialization — nice use of Rust 2024 edition.
- **`Box<[u8; 2*1024*1024]>`** for RAM instead of `Vec` — correctly avoids heap indirection on every access.
- **Crash context with recent-instruction ring** is exactly the kind of thing you'll thank yourself for at 2 AM.
- **The README-level concept** of having tests boot a BIOS and check `GetStat` CDROM behavior — genuinely smart scaffolding.
- **The `Gp0Command::word_count` enum matcher** is clean. Resist the urge to "simplify" it into a lookup table.

---

## The meanest thing I can say

You've clearly read Mednafen's source. Now go read Duckstation's CPU core. Yours looks like what happens when you read the spec and then start typing without a profiler open and without a commercial game to fail against. The structure is fine. The correctness gaps and perf sins are textbook, and they're textbook *because they always happen this way*. Fix #1–#6 above and you'll leapfrog 80% of hobby emulators on crates.io.

Now go make Doom boot for real.
