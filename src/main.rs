use std::collections::VecDeque;
use std::env;
use std::fs;
use std::path::PathBuf;

use minifb::{Key, ScaleMode, Window, WindowOptions};
use ps1_emulator::{
    CdImage, Console, CrashContext, InstructionTraceEntry, PsxExe, VRAM_HEIGHT, VRAM_WIDTH,
};

const WINDOW_CPU_STEPS_PER_FRAME: usize = 20_000;
const DEFAULT_CPU_STEPS: usize = 16;
const CRASH_HISTORY_SIZE: usize = 256;
const SYSCALL_OPCODE: u32 = 0x0000_000c;
const RA_REGISTER: usize = 31;
const V0_REGISTER: usize = 2;
const A0_REGISTER: usize = 4;
const A1_REGISTER: usize = 5;
const A2_REGISTER: usize = 6;
const LOW_PC_THRESHOLD: u32 = 0x0001_0000;
const UNKNOWN_INSTRUCTION: u32 = 0;
const INVALID_SYNC_FLAG_WORD: u32 = 0xffff_ffff;
const PSYQ_CD_SYNC_FLAG_ADDRESS: u32 = 0x8008_9d9c;
const RGB24_STRIDE: usize = 3;
const ZERO_RUN_BREAK_THRESHOLD: usize = 128;

fn main() {
    init_logger();
    if let Err(err) = run() {
        log::error!("{err}");
        std::process::exit(1);
    }
}

fn run() -> ps1_emulator::Result<()> {
    let mut window_mode = false;
    let mut bios_boot = false;
    let args = env::args_os()
        .skip(1)
        .filter(|arg| {
            if arg == "--window" {
                window_mode = true;
                false
            } else if arg == "--bios-boot" {
                bios_boot = true;
                false
            } else {
                true
            }
        })
        .collect::<Vec<_>>();
    let mut args = args.into_iter();
    let bios_path = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            print_usage();
            return Ok(());
        }
    };

    let second = args.next();
    let third = args.next();
    let (game_path, steps_arg) = match (second, third) {
        (Some(value), None) if value.to_string_lossy().parse::<usize>().is_ok() => {
            (None, Some(value))
        }
        (game, steps) => (game.map(PathBuf::from), steps),
    };
    let steps = steps_arg
        .map(|raw| raw.to_string_lossy().parse::<usize>())
        .transpose()
        .map_err(|_| ps1_emulator::Error::InvalidArgument("steps must be a number".into()))?
        .unwrap_or(DEFAULT_CPU_STEPS);

    let mut console = Console::from_bios_file(bios_path)?;

    if let Some(path) = game_path {
        if is_psx_exe(&path) {
            let exe = PsxExe::from_file(path)?;
            console.load_exe(&exe)?;
            println!(
                "loaded PS-X EXE: pc={:#010x} gp={:#010x} sp={:#010x}",
                exe.initial_pc, exe.initial_gp, exe.stack_pointer
            );
        } else {
            let cd = CdImage::from_path(path)?;
            let sectors = cd.sector_count();
            let mode = cd.mode();
            let path = cd.path().to_owned();
            if bios_boot {
                console.load_cd_image(cd);
            } else {
                let boot_exe = cd.boot_exe()?;
                console.load_exe(&boot_exe)?;
                console.load_cd_image(cd);
                println!(
                    "loaded CD boot EXE: pc={:#010x} gp={:#010x} sp={:#010x}",
                    boot_exe.initial_pc, boot_exe.initial_gp, boot_exe.stack_pointer
                );
            }
            println!(
                "loaded CD image: {} sectors, mode {:?}, file {}",
                sectors,
                mode,
                path.display()
            );
            if bios_boot {
                println!("booting through BIOS reset vector");
            }
        }
    }

    let trace_low_pc = env::var_os("PS1_TRACE_LOW_PC").is_some();
    let trace_zero_run = env::var_os("PS1_TRACE_ZERO_RUN").is_some();
    let trace_syscalls = env::var_os("PS1_TRACE_SYSCALLS").is_some();
    let trace_bios_calls = env::var_os("PS1_TRACE_BIOS_CALLS").is_some();
    let dump_pc = env::var_os("PS1_DUMP_PC").is_some();
    let mut zero_run = 0usize;
    let mut last_nonzero_pc = 0;
    let mut last_nonzero_instruction = 0;
    let mut trace = TraceState {
        trace_low_pc,
        trace_zero_run,
        trace_syscalls,
        trace_bios_calls,
        dump_pc,
        zero_run: &mut zero_run,
        last_nonzero_pc: &mut last_nonzero_pc,
        last_nonzero_instruction: &mut last_nonzero_instruction,
    };
    let mut instruction_history = VecDeque::with_capacity(CRASH_HISTORY_SIZE);

    if window_mode {
        run_window(&mut console, &mut trace, &mut instruction_history)?;
    } else {
        for _ in 0..steps {
            if step_with_trace(&mut console, &mut trace, &mut instruction_history)? {
                break;
            }
        }
    }

    let cpu = console.cpu_state();
    println!("{}", cpu);
    if console.cd_image_loaded() {
        let cdrom = console.cdrom_debug_state();
        let cd_sync_flag = console
            .peek32(PSYQ_CD_SYNC_FLAG_ADDRESS)
            .unwrap_or(INVALID_SYNC_FLAG_WORD);
        println!(
            "cdrom: commands={} dma_read_bytes={} last_command={:?} response_len={} data_len={} irq_enable={:#04x} irq_flag={:#04x} status={:#04x} mode={:#04x} sync_flag={:#010x}",
            console.cdrom_command_count(),
            console.cdrom_dma_read_bytes(),
            cdrom.last_command,
            cdrom.response_len,
            cdrom.data_len,
            cdrom.interrupt_enable,
            cdrom.interrupt_flag,
            cdrom.status,
            cdrom.mode,
            cd_sync_flag
        );
    }
    if dump_pc {
        dump_near_pc(&console)?;
    }
    if let Some(raw) = env::var_os("PS1_DUMP_WORDS") {
        dump_words(&console, raw.to_string_lossy().as_ref())?;
    }
    if let Some(path) = env::var_os("PS1_DUMP_FRAME") {
        dump_framebuffer(&console, PathBuf::from(path))?;
    }
    Ok(())
}

fn init_logger() {
    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(log::LevelFilter::Warn);

    let file = fern::log_file("ps1.log").expect("failed to open ps1.log");

    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {}] {}",
                chrono::Local::now().format("%H:%M:%S%.3f"),
                record.level(),
                message
            ))
        })
        .level(level)
        .chain(std::io::stderr())
        .chain(file)
        .apply()
        .expect("failed to init logger");
}

fn print_usage() {
    eprintln!(
        "usage: ps1_emulator <bios.bin> [game.exe|game.cue|game.bin] [steps] [--window] [--bios-boot]"
    );
    eprintln!(
        "loads a PlayStation BIOS, optionally loads a PS-X EXE or CD image, then executes CPU steps"
    );
}

struct TraceState<'a> {
    trace_low_pc: bool,
    trace_zero_run: bool,
    trace_syscalls: bool,
    trace_bios_calls: bool,
    dump_pc: bool,
    zero_run: &'a mut usize,
    last_nonzero_pc: &'a mut u32,
    last_nonzero_instruction: &'a mut u32,
}

fn step_with_trace(
    console: &mut Console,
    trace: &mut TraceState<'_>,
    instruction_history: &mut VecDeque<InstructionTraceEntry>,
) -> ps1_emulator::Result<bool> {
    let before = console.cpu_state();
    let before_instruction = if before.pc & 3 == 0 {
        Some(console.peek32(before.pc)?)
    } else {
        None
    };
    if let Some(opcode) = before_instruction {
        push_instruction_history(
            instruction_history,
            InstructionTraceEntry {
                address: before.pc,
                opcode,
            },
        );
    }
    if trace.trace_syscalls && before_instruction == Some(SYSCALL_OPCODE) {
        println!(
            "syscall at pc={:#010x} ra={:#010x} v0={:#010x} a0={:#010x} a1={:#010x} a2={:#010x}",
            before.pc,
            before.regs[RA_REGISTER],
            before.regs[V0_REGISTER],
            before.regs[A0_REGISTER],
            before.regs[A1_REGISTER],
            before.regs[A2_REGISTER]
        );
    }
    if trace.trace_bios_calls
        && let Some(call) = console.pending_bios_call()
    {
        println!(
            "bios call {}({:#04x}) ra={:#010x} v0={:#010x} a0={:#010x} a1={:#010x} a2={:#010x}",
            call.vector,
            call.function,
            before.regs[RA_REGISTER],
            before.regs[V0_REGISTER],
            before.regs[A0_REGISTER],
            before.regs[A1_REGISTER],
            before.regs[A2_REGISTER]
        );
    }
    if let Err(err) = console.step() {
        let instruction = before_instruction.unwrap_or(UNKNOWN_INSTRUCTION);
        let crash = console.crash_context(
            instruction_history.iter().copied().collect(),
            Some(err.to_string()),
        );
        eprintln!(
            "step failed at pc={:#010x} instr={:#010x}",
            before.pc, instruction
        );
        print_crash_context(&crash);
        if trace.dump_pc {
            dump_near_pc(console)?;
        }
        return Err(err);
    }
    let after = console.cpu_state();
    if before_instruction == Some(0) {
        *trace.zero_run += 1;
    } else if let Some(instruction) = before_instruction {
        *trace.zero_run = 0;
        *trace.last_nonzero_pc = before.pc;
        *trace.last_nonzero_instruction = instruction;
    }
    if trace.trace_low_pc && before.pc >= LOW_PC_THRESHOLD && after.pc < LOW_PC_THRESHOLD {
        println!(
            "low-pc transition: before_pc={:#010x} before_next={:#010x} instr={:#010x} after_pc={:#010x} after_next={:#010x}",
            before.pc,
            before.next_pc,
            before_instruction.unwrap_or(console.peek32(before.pc)?),
            after.pc,
            after.next_pc
        );
        return Ok(true);
    }
    if trace.trace_zero_run && *trace.zero_run >= ZERO_RUN_BREAK_THRESHOLD {
        println!(
            "zero-run transition: last_nonzero_pc={:#010x} instr={:#010x} current_pc={:#010x}",
            *trace.last_nonzero_pc, *trace.last_nonzero_instruction, after.pc
        );
        return Ok(true);
    }

    Ok(false)
}

fn push_instruction_history(
    history: &mut VecDeque<InstructionTraceEntry>,
    entry: InstructionTraceEntry,
) {
    if history.len() == CRASH_HISTORY_SIZE {
        history.pop_front();
    }
    history.push_back(entry);
}

fn print_crash_context(crash: &CrashContext) {
    eprintln!("crash context:");
    if let Some(error) = &crash.last_error {
        eprintln!("  error: {error}");
    }
    eprintln!(
        "  cpu: pc={:#010x} next_pc={:#010x} hi={:#010x} lo={:#010x}",
        crash.cpu.pc, crash.cpu.next_pc, crash.cpu.hi, crash.cpu.lo
    );
    for (index, value) in crash.cpu.regs.iter().enumerate() {
        eprintln!("  r{index:02}={value:#010x}");
    }
    eprintln!(
        "  dma: control={:#010x} interrupt={:#010x}",
        crash.dma.control, crash.dma.interrupt
    );
    for (index, channel) in crash.dma.channels.iter().enumerate() {
        eprintln!(
            "  dma[{index}]: base={:#010x} block={:#010x} control={:#010x}",
            channel.base_address, channel.block_control, channel.control
        );
    }
    eprintln!(
        "  gpu: status={:#010x} gp0_fifo_depth={} image_load_active={} last_command={:?} draws={} uploads={} unknown={}",
        crash.gpu.status,
        crash.gpu.gp0_fifo_depth,
        crash.gpu.image_load_active,
        crash.gpu.last_command,
        crash.gpu.draw_count,
        crash.gpu.image_upload_count,
        crash.gpu.unknown_command_count
    );
    eprintln!(
        "  cdrom: last_command={:?} response_len={} data_len={} irq_enable={:#04x} irq_flag={:#04x} status={:#04x} mode={:#04x}",
        crash.cdrom.last_command,
        crash.cdrom.response_len,
        crash.cdrom.data_len,
        crash.cdrom.interrupt_enable,
        crash.cdrom.interrupt_flag,
        crash.cdrom.status,
        crash.cdrom.mode
    );
    eprintln!("  recent instructions:");
    for entry in &crash.recent_instructions {
        eprintln!("    {:#010x}: {:#010x}", entry.address, entry.opcode);
    }
}

fn run_window(
    console: &mut Console,
    trace: &mut TraceState<'_>,
    instruction_history: &mut VecDeque<InstructionTraceEntry>,
) -> ps1_emulator::Result<()> {
    let mut window = Window::new(
        "ps1 emulator",
        VRAM_WIDTH,
        VRAM_HEIGHT,
        WindowOptions {
            resize: true,
            scale_mode: ScaleMode::AspectRatioStretch,
            ..WindowOptions::default()
        },
    )
    .map_err(|err| ps1_emulator::Error::Window(err.to_string()))?;
    window.set_target_fps(60);

    let mut window_buffer = vec![0_u32; VRAM_WIDTH * VRAM_HEIGHT];
    let mut display_rgb = vec![0_u8; VRAM_WIDTH * VRAM_HEIGHT * RGB24_STRIDE];
    while window.is_open() && !window.is_key_down(Key::Escape) {
        for _ in 0..WINDOW_CPU_STEPS_PER_FRAME {
            if step_with_trace(console, trace, instruction_history)? {
                break;
            }
        }

        let dw = console.display_width();
        let dh = console.display_height();
        let needed = dw * dh;
        if window_buffer.len() != needed {
            window_buffer.resize(needed, 0);
        }
        let needed_rgb = needed * RGB24_STRIDE;
        if display_rgb.len() != needed_rgb {
            display_rgb.resize(needed_rgb, 0);
        }
        console.copy_display_rgb_into(&mut display_rgb);
        rgb_to_window_buffer(&display_rgb[..needed_rgb], &mut window_buffer);
        let cpu = console.cpu_state();
        let gpu = console.gpu_debug_state();
        let cdrom_command_count = console.cdrom_command_count();
        window.set_title(&format!(
            "ps1 emulator pc={:#010x} cd={} last_cd={:?} gp0={} draws={} uploads={} unk={}",
            cpu.pc,
            cdrom_command_count,
            gpu.last_command,
            gpu.command_count,
            gpu.draw_count,
            gpu.image_upload_count,
            gpu.unknown_command_count
        ));
        window
            .update_with_buffer(&window_buffer, dw, dh)
            .map_err(|err| ps1_emulator::Error::Window(err.to_string()))?;
    }

    Ok(())
}

fn rgb_to_window_buffer(rgb: &[u8], window_buffer: &mut [u32]) {
    for (source, dest) in rgb.chunks_exact(RGB24_STRIDE).zip(window_buffer.iter_mut()) {
        *dest = ((source[0] as u32) << 16) | ((source[1] as u32) << 8) | source[2] as u32;
    }
}

fn is_psx_exe(path: &PathBuf) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

fn dump_near_pc(console: &Console) -> ps1_emulator::Result<()> {
    let state = console.cpu_state();
    let start = state.pc.saturating_sub(16) & !3;
    println!("registers:");
    for (index, value) in state.regs.iter().enumerate() {
        println!("  r{index:02}={value:#010x}");
    }
    println!("instructions:");
    for offset in 0..24 {
        let address = start + offset * 4;
        let marker = if address == state.pc { "=>" } else { "  " };
        println!(
            "{marker} {address:#010x}: {:#010x}",
            console.peek32(address)?
        );
    }
    Ok(())
}

fn dump_words(console: &Console, spec: &str) -> ps1_emulator::Result<()> {
    let (address, words) = match spec.split_once(':') {
        Some((address, words)) => (parse_u32(address), words.parse::<u32>().unwrap_or(16)),
        None => (parse_u32(spec), 16),
    };
    let start = address & !3;
    println!("words at {start:#010x}:");
    for offset in 0..words {
        let address = start.wrapping_add(offset * 4);
        println!("  {address:#010x}: {:#010x}", console.peek32(address)?);
    }
    Ok(())
}

fn parse_u32(value: &str) -> u32 {
    let value = value.trim();
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    u32::from_str_radix(value, 16).unwrap_or(0)
}

fn dump_framebuffer(console: &Console, path: PathBuf) -> ps1_emulator::Result<()> {
    let rgb = console.framebuffer_rgb();
    let mut ppm = format!("P6\n{VRAM_WIDTH} {VRAM_HEIGHT}\n255\n").into_bytes();
    ppm.extend_from_slice(&rgb);
    fs::write(&path, ppm)?;
    println!("wrote framebuffer: {}", path.display());
    Ok(())
}
