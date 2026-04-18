# Run Commands for Ace Combat 2

To run the emulator with the BIOS and Ace Combat 2, use the following commands.

### 1. Basic Run (Console only, 20,000 steps)
Runs the emulator for a fixed number of CPU steps and then exits.
```powershell
cargo run -- SCPH1001.BIN ace_combat_2.cue 20000
```

### 2. Windowed Mode (Playable)
Opens the emulator in a window and runs until closed.
```powershell
cargo run -- SCPH1001.BIN ace_combat_2.cue --window
```

### 3. Debugging / Tracing
If you need to debug specific parts of the boot process, you can set environment variables before running:

**Trace Syscalls:**
```powershell
$env:PS1_TRACE_SYSCALLS=1
cargo run -- SCPH1001.BIN ace_combat_2.cue --window
$env:PS1_TRACE_SYSCALLS=$null
```

**Trace BIOS Calls:**
```powershell
$env:PS1_TRACE_BIOS_CALLS=1
cargo run -- SCPH1001.BIN ace_combat_2.cue --window
$env:PS1_TRACE_BIOS_CALLS=$null
```

**Dump Framebuffer to File on Exit:**
```powershell
$env:PS1_DUMP_FRAME="frame.ppm"
cargo run -- SCPH1001.BIN ace_combat_2.cue --window
$env:PS1_DUMP_FRAME=$null
```

---
*Note: Ensure `SCPH1001.BIN` and `ace_combat_2.cue` are in the root directory.*
