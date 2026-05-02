# Run Commands for Ace Combat 2

To run the emulator with the BIOS and Ace Combat 2, use the following commands.

### 1. Basic Run (Console only, 20,000 steps)
Runs the emulator for a fixed number of CPU steps and then exits.
```powershell
cargo run -- "bios/SCPH1001.BIN" "games/Ace Combat 2 (USA).cue" 20000
```

### 2. Windowed Mode (Playable)
Opens the emulator in a window and runs until closed.
```powershell
cargo run -- "bios/SCPH1001.BIN" "games/Ace Combat 2 (USA).cue" --window
```

### 3. Debugging / Tracing
If you need to debug specific parts of the boot process, you can set environment variables before running:

**Trace Syscalls:**
```powershell
$env:PS1_TRACE_SYSCALLS=1
cargo run -- "bios/SCPH1001.BIN" "games/Ace Combat 2 (USA).cue" --window
$env:PS1_TRACE_SYSCALLS=$null
```

**Trace BIOS Calls:**
```powershell
$env:PS1_TRACE_BIOS_CALLS=1
cargo run -- "bios/SCPH1001.BIN" "games/Ace Combat 2 (USA).cue" --window
$env:PS1_TRACE_BIOS_CALLS=$null
```

**Dump Framebuffer to File on Exit:**
```powershell
$env:PS1_DUMP_FRAME="artifacts/frames/frame.ppm"
cargo run -- "bios/SCPH1001.BIN" "games/Ace Combat 2 (USA).cue" --window
$env:PS1_DUMP_FRAME=$null
```

---
*Note: Keep BIOS files in `bios/`, game images in `games/`, and generated dumps in `artifacts/`.*
