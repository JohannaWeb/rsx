# A Clean Code Review

*By yours truly. Let's have a talk.*

> "Clean code is not written by following a set of rules. You don't become a software craftsman by learning a list of heuristics. Professionalism and craftsmanship come from values that drive disciplines."

Good morning. I've read your PlayStation emulator. I want you to know, up front, that I'm proud of you for building it. Building a MIPS CPU core in Rust is *real* work. You shipped tests. You split things into modules. You used an enum for opcodes instead of bare integers. These are the habits of a craftsman.

Now let's talk about the rest.

Because I'm going to be honest with you, friend: this code is not *dirty*, but it is *careless*. And careless code, left alone, rots. Rot is how we got here — a 500-line file that nobody wants to touch, a bug that nobody can reproduce, a feature that nobody dares add. You are not there yet. But the road you are walking leads there. Let us, together, walk a different road.

---

## Part I — The Five Principles

### S — The Single Responsibility Principle

> "A class should have one, and only one, reason to change."

Look at `src/cpu/mod.rs:38`:

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

Count the reasons this struct changes:

1. The MIPS R3000A specification changes (it won't, but *conceptually*).
2. The GTE coprocessor specification changes.
3. The PlayStation **BIOS** allocation scheme changes.
4. The BIOS interrupt-hooking convention changes.
5. Your interrupt-save policy changes.

Five reasons. Five responsibilities. One struct.

The `Cpu` should know about registers and the program counter. It should not know that `$a0 == 1` means "enter critical section" (`src/cpu/instructions.rs:488`). It should not own a *heap pointer*. It should not snapshot and restore every register when an interrupt fires. These are the responsibilities of a *kernel emulator*, not a CPU.

**The fix** is not complicated. Create a `Kernel` struct. Move `bios_heap`, `interrupt_hook`, `interrupt_return_pc`, and `interrupt_saved_registers` into it. Let it receive notifications of BIOS-vector calls. Let the `Cpu` go back to doing what a CPU does: decode and execute instructions.

And the same sickness lives in `Bus` (`src/bus/mod.rs:71`). It routes memory. It also executes DMA transfers. It also aggregates interrupts. It also ticks timers. It also synchronizes CDROM state. It is five classes wearing one trench coat.

Separate them. Your future self — the one debugging DMA at 2 AM — will send a thank-you card.

### O — The Open/Closed Principle

> "Open for extension, closed for modification."

Adding a new MIPS instruction to this emulator requires:
1. Adding a variant to `PrimaryOpcode` (`src/cpu/decode.rs`).
2. Adding a handler `op_foo` in `src/cpu/instructions.rs`.
3. Editing `PRIMARY_OPCODE_TABLE` at `src/cpu/instructions.rs:10` to register it.

Three files modified. Three places to forget. That's not extension, that's surgery.

Similarly, adding an MMIO region means walking into `Bus::read8` and `Bus::write8` (two near-identical `match` blocks) and `Bus::peek8` (a third, nearly identical) and adding an arm to each. If you forget one, reads work but peeks don't, or writes work but reads return garbage — and the compiler can't help you, because the `match` has a catch-all `_ =>`.

**The fix**: a registry. `struct MmioRegion { range, read, write, peek }`. A `Vec<MmioRegion>` or a sorted table. Each device registers its own region. Adding CDROM is one new `.register()` call, not three edits to three functions. The day you add MDEC — and you *will* add MDEC — you will be glad of this.

### L — The Liskov Substitution Principle

Your `MemoryBusAccess` trait (`src/bus/mod.rs:60`) has exactly **one** implementor: `Bus`. It is never substituted. It exists only to abstract what it never abstracts.

Worse, your CPU instruction handlers all take `&mut dyn CpuBusAccess`. Every `bus.read32` is a virtual call through a vtable. You paid the cost of Liskov substitutability and received none of the benefits.

Either:
- **Delete the trait.** Pass `&mut Bus`. The compiler will inline, the vtable vanishes, and your code gets faster. Or:
- **Honor the trait.** Write a `MockBus` that tests can use. Write an `InstrumentedBus` that counts MMIO accesses. *Then* the trait pays its rent.

Paying for an abstraction you do not use is like buying a gym membership for the mirrors.

### I — The Interface Segregation Principle

`MemoryBusAccess` (`src/bus/mod.rs:60`) bundles together:

- byte/halfword/word reads
- byte/halfword/word writes
- `interrupt_pending_bits()`
- `peek32()` (debugger-style read)

A CPU-instruction handler that wants to do `lb` needs `read8`. It does not need `interrupt_pending_bits`. It does not need `peek32`. Yet because you've glued them into one trait, any mock or alternate implementation must provide all seven methods, or nothing.

Segregate. `trait BusRead`, `trait BusWrite`, `trait InterruptSource`, `trait Peekable`. Let types opt in.

And while you're at it: your `RuntimeState` (`src/console/mod.rs:41`) is a god-object. Fields for CPU, video, CDROM, GPU, DMA, counters, booleans, all in one struct, cloned and returned every frame. Nobody needs all of that. The window title needs a `u32 pc` and a few counters. The PPM dumper needs a framebuffer. The crash reporter needs everything.

Three callers, three needs, one bloated struct. That is textbook interface pollution.

### D — The Dependency Inversion Principle

> "High-level modules should not depend on low-level modules. Both should depend on abstractions."

Look at `Bus::new` (`src/bus/mod.rs:85`):

```rust
Self {
    ram, scratchpad, io: Box::new([0; ...]),
    timers: SystemTimers::new(),
    cdrom: CdRomController::new(),
    dma: DmaController::new(),
    gpu: Gpu::new(),
    spu: Spu::new(),
    ...
}
```

The `Bus` — a high-level coordinator — directly constructs every low-level peripheral. You cannot inject a test GPU. You cannot swap in a faster SPU. You cannot disable the CDROM for a quick boot. The dependencies point the wrong way.

The remedy is constructor injection. `Bus::new(bios, gpu, spu, cdrom, dma, timers)`. Ugly? A little. So wrap it: `Bus::default_for_bios(bios)` composes the real peripherals, and the constructor stays open for tests to pass fakes.

---

## Part II — Clean Code Violations

### Functions should be small. Smaller than that.

`Cpu::disassemble` at `src/cpu/mod.rs:161` is 80 lines of one enormous `match`. Eighty lines. In one function. Doing one thing? Only if you squint and tilt your head.

```rust
0x20 => format!("add {}, {}, {}", reg_name(rd_idx), ...),
0x21 => format!("addu {}, {}, {}", ...),
0x22 => format!("sub {}, {}, {}", ...),
// ...seventy more lines...
```

Extract. `disassemble_special`, `disassemble_regimm`, `disassemble_load`, `disassemble_store`, `disassemble_branch`. Five functions of ten lines each, instead of one function of eighty. I promise — I *promise* — you will read this code again in six months and weep if you do not.

Same indictment: `Cpu::step` at `src/cpu/mod.rs:117` does six things:

1. Reads an environment variable (in the hot path, no less!).
2. Checks for a BIOS vector dispatch.
3. Checks for a pending interrupt.
4. Fetches an instruction.
5. Logs it.
6. Advances the PC *and* executes *and* zeros `$zero`.

A function should do one thing. It should do it well. It should do it only. `step` should fetch. `dispatch` should dispatch. `maybe_enter_interrupt` should enter interrupts. One thought per function. One abstraction level per function.

### Naming

> "The name of a variable, function, or class should answer all the big questions."

- `op_add_immediate` (`src/cpu/instructions.rs:235`) handles **both** `addi` and `addiu`. The name says "immediate". It does not say "unchecked". So when a future reader sees `op_add_immediate` registered for `PrimaryOpcode::Addi`, they will reasonably assume you handle signed-overflow trapping. You do not. Misleading name, missing behavior.

- `bios_heap` is a field. It is not a heap. It is a bump pointer. Call it `bump_pointer` or — better — delete it (see below).

- `InterruptHook` has fields `saved: [u32; 8]`. Eight what? Callee-saved registers $s0–$s7? The name `saved` says nothing. Call it `saved_s_registers` if that's what it is.

- The constant `BIOS_HEAP_START: u32 = 0x8001_0000` (`src/cpu/mod.rs:27`). This is not in the PlayStation specification. It is a magic number you picked. Document it or, again, delete the feature.

- `RAM_BASE` (`src/bus/mod.rs:27`) is `0x0000_0000` — a value that would be spelled more clearly as `const RAM_BASE: u32 = 0;`. You made it a `u32` for type consistency, which is good. But now it's a "constant" that does nothing the literal `0` wouldn't. It's noise.

### Comments should disappear

`src/cpu/mod.rs:244`:

```rust
if status & 1 == 0 { // Interrupt Enable bit
    return None;
}
```

The comment exists because the expression `status & 1 == 0` does not say what it means. **Make the code say what it means.**

```rust
const COP0_STATUS_IE: u32 = 1 << 0;
if status & COP0_STATUS_IE == 0 {
    return None;
}
```

Now the comment is redundant. Delete it. A well-named constant is worth a thousand `// what this is` comments.

And `src/cpu/mod.rs:23-26` has exactly that constant defined already — as `COP0_STATUS_INTERRUPT_ENABLE`. You declared the constant, then forgot to use it, and wrote a comment instead. The discipline slipped. The Boy Scout Rule says: when you walk past that comment, delete it and use the constant.

### Dead parameters and boilerplate

Every instruction handler has this signature:

```rust
fn op_xxx(&mut self, _pc: u32, instruction: u32, _bus: &mut dyn CpuBusAccess) -> Result<()>
```

Thirty handlers. Most of them ignore `_pc` and `_bus`. The underscores betray it. The signature exists to satisfy a function pointer table, which exists to dispatch opcodes. We've contorted the handlers to satisfy the machinery.

A craftsman asks: *does the machinery serve the code, or does the code serve the machinery?* Right now, the code serves. Consider a `match` on the opcode directly in `step`. The compiler builds a jump table for you. The signatures collapse. The unused parameters vanish. The code becomes shorter.

### The `Result` that never fails

Half your opcode handlers return `Result<()>` and then always produce `Ok(())`. `op_lui`. `op_add_immediate`. `op_ori`. Arithmetic does not fail.

Yet every call site must write `?`. Every fallible `bus.read32(...)?` looks exactly like an infallible `Ok(())`-producer. The `Result` lies about which operations can fault.

Sort them. Instructions that can raise exceptions (loads, stores, arithmetic-on-overflow) — return `Result`. Pure computation — returns `()`. The type system should make the difference visible at a glance.

### Tests that read like recipes, not stories

`src/cpu/mod.rs:467`:

```rust
cpu.cop0[COP0_STATUS] = 0x0000_0401;
// ...
bus.write32(0x1f80_1074, 0x0000_0004).unwrap();
bus.write8(0x1f80_1800, 1).unwrap();
bus.write8(0x1f80_1802, 0x1f).unwrap();
```

What is `0x0000_0401`? *You* know. I do not. The test says nothing about the intent.

```rust
const IE_AND_IM2: u32 = COP0_STATUS_IE | (1 << (COP0_STATUS_IM_SHIFT + 2));
cpu.cop0[COP0_STATUS] = IE_AND_IM2;
```

Now the test tells a story: *we enable interrupts globally and unmask the CDROM interrupt.* The magic number was hiding the intent. Surface it.

A test is a form of documentation. Write it for the person who will read it while the build is broken.

### The `// Helpers moved to file scope but visible to submodules` comment

`src/cpu/mod.rs:297`:

```rust
// Helpers moved to file scope but visible to submodules
pub(super) fn rs(instruction: u32) -> usize { ... }
```

That comment is **commit-message leakage**. It tells me what *changed*, not what the code *is*. Comments rot the moment they describe history rather than behavior. Delete it. Let `git log` tell the story of the move. Let the code tell the story of the present.

---

## Part III — The Boy Scout Rule

> "Leave the campground cleaner than you found it."

Here is a week of small, cheap cleanings. None of them require a rewrite. Each makes the next easier.

1. **Day 1**: Delete every `std::env::var_os("PS1_TRACE_*")` call from hot paths. Replace with a `TraceConfig` struct built once at startup. *One commit.*
2. **Day 2**: Rename `op_add_immediate` → `op_addiu`. Create a *separate* `op_addi` that checks for overflow. Now the names tell the truth. *One commit.*
3. **Day 3**: Delete the `// Interrupt Enable bit` comment and use the already-defined `COP0_STATUS_INTERRUPT_ENABLE` constant. Hunt for five more comments like it. Kill them. *One commit.*
4. **Day 4**: Extract `disassemble_special`, `disassemble_load`, `disassemble_store` from `disassemble`. Each under 15 lines. *One commit.*
5. **Day 5**: Move `bios_heap`, `interrupt_hook`, `interrupt_return_pc`, `interrupt_saved_registers` out of `Cpu` into a new `kernel` module. *One commit.* This one is bigger. Do it on a Friday. Break nothing.
6. **Day 6**: Segregate `MemoryBusAccess` into `BusRead`, `BusWrite`, `InterruptSource`. Delete the `dyn` — or test-drive a `MockBus`. Pick a lane. *One commit.*
7. **Day 7**: Write three tests for load-delay-slot behavior. They will fail. *That's the point.* Now you have a failing test driving a correctness fix. *One commit.*

Seven commits. One week. Your emulator does not become clean overnight. It becomes clean *gradually*, while it continues to work.

That is craftsmanship. That is the discipline.

---

## Closing

I want to say something gentle, because I know these reviews can sting.

You wrote `Box<[u8; RAM_SIZE]>` instead of `Vec<u8>`. That tells me you care about performance and type-level guarantees. You wrote `const fn`-style initialization of the dispatch tables. That tells me you care about compile-time correctness. You wrote a `CrashContext` struct with a rolling instruction history. That tells me you've been burned by a mystery crash and you swore "never again" — a lesson only real engineers learn.

These instincts are good. They are the instincts of someone who will, in time, write clean code. The missing step is *discipline*: the willingness to go back, again and again, and ask — *does this function do one thing?* — *does this name lie?* — *does this comment hide a bad variable name?* — *would I be proud to show this to a colleague?*

Make one thing cleaner today. Then another tomorrow. The code is the teacher. Let it teach you.

Now go.

— *Bob*
