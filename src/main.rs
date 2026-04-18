use std::env;
use std::fs;
use std::path::PathBuf;

use minifb::{Key, ScaleMode, Window, WindowOptions};
use ps1_emulator::{CdImage, Console, PsxExe, VRAM_HEIGHT, VRAM_WIDTH};

const WINDOW_CPU_STEPS_PER_FRAME: usize = 20_000;

fn main() {
    env_logger::init();
    if let Err(err) = run() {
        log::error!("{err}");
        std::process::exit(1);
    }
}

fn run() -> ps1_emulator::Result<()> {
    let mut window_mode = false;
    let args = env::args_os()
        .skip(1)
        .filter(|arg| {
            if arg == "--window" {
                window_mode = true;
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
        .unwrap_or(16);

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
            let boot_exe = cd.boot_exe()?;
            console.load_exe(&boot_exe)?;
            console.load_cd_image(cd);
            println!(
                "loaded CD image: {} sectors, mode {:?}, file {}",
                sectors,
                mode,
                path.display()
            );
            println!(
                "loaded CD boot EXE: pc={:#010x} gp={:#010x} sp={:#010x}",
                boot_exe.initial_pc, boot_exe.initial_gp, boot_exe.stack_pointer
            );
        }
    }

    let trace_low_pc = env::var_os("PS1_TRACE_LOW_PC").is_some();
    let trace_zero_run = env::var_os("PS1_TRACE_ZERO_RUN").is_some();
    let trace_syscalls = env::var_os("PS1_TRACE_SYSCALLS").is_some();
    let trace_bios_calls = env::var_os("PS1_TRACE_BIOS_CALLS").is_some();
    let dump_pc = env::var_os("PS1_DUMP_PC").is_some();
    let needs_instruction_trace = trace_zero_run || trace_syscalls;
    let mut zero_run = 0usize;
    let mut last_nonzero_pc = 0;
    let mut last_nonzero_instruction = 0;
    let mut trace = TraceState {
        trace_low_pc,
        trace_zero_run,
        trace_syscalls,
        trace_bios_calls,
        dump_pc,
        needs_instruction_trace,
        zero_run: &mut zero_run,
        last_nonzero_pc: &mut last_nonzero_pc,
        last_nonzero_instruction: &mut last_nonzero_instruction,
    };

    if window_mode {
        run_window(&mut console, &mut trace)?;
    } else {
        for _ in 0..steps {
            if step_with_trace(&mut console, &mut trace)? {
                break;
            }
        }
    }

    println!("{}", console.cpu_state());
    if console.cd_image().is_some() {
        let cdrom = console.cdrom_debug_state();
        let cd_sync_flag = console.peek32(0x8008_9d9c).unwrap_or(0xffff_ffff);
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
    if let Some(path) = env::var_os("PS1_DUMP_FRAME") {
        dump_framebuffer(&console, PathBuf::from(path))?;
    }
    Ok(())
}

fn print_usage() {
    eprintln!("usage: ps1_emulator <bios.bin> [game.exe|game.cue|game.bin] [steps] [--window]");
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
    needs_instruction_trace: bool,
    zero_run: &'a mut usize,
    last_nonzero_pc: &'a mut u32,
    last_nonzero_instruction: &'a mut u32,
}

fn step_with_trace(
    console: &mut Console,
    trace: &mut TraceState<'_>,
) -> ps1_emulator::Result<bool> {
    let before = console.cpu_state();
    let before_instruction = if trace.needs_instruction_trace {
        Some(console.peek32(before.pc)?)
    } else {
        None
    };
    if trace.trace_syscalls && before_instruction == Some(0x0000_000c) {
        println!(
            "syscall at pc={:#010x} ra={:#010x} v0={:#010x} a0={:#010x} a1={:#010x} a2={:#010x}",
            before.pc,
            before.regs[31],
            before.regs[2],
            before.regs[4],
            before.regs[5],
            before.regs[6]
        );
    }
    if trace.trace_bios_calls
        && let Some(call) = console.pending_bios_call()
    {
        println!(
            "bios call {}({:#04x}) ra={:#010x}",
            call.vector, call.function, before.regs[31]
        );
    }
    if let Err(err) = console.step() {
        let instruction = match before_instruction {
            Some(instruction) => instruction,
            None => console.peek32(before.pc)?,
        };
        eprintln!(
            "step failed at pc={:#010x} instr={:#010x}",
            before.pc, instruction
        );
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
    if trace.trace_low_pc && before.pc >= 0x0001_0000 && after.pc < 0x0001_0000 {
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
    if trace.trace_zero_run && *trace.zero_run >= 128 {
        println!(
            "zero-run transition: last_nonzero_pc={:#010x} instr={:#010x} current_pc={:#010x}",
            *trace.last_nonzero_pc, *trace.last_nonzero_instruction, after.pc
        );
        return Ok(true);
    }

    Ok(false)
}

fn run_window(console: &mut Console, trace: &mut TraceState<'_>) -> ps1_emulator::Result<()> {
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
    while window.is_open() && !window.is_key_down(Key::Escape) {
        for _ in 0..WINDOW_CPU_STEPS_PER_FRAME {
            if step_with_trace(console, trace)? {
                break;
            }
        }

        let dw = console.display_width();
        let dh = console.display_height();
        let needed = dw * dh;
        if window_buffer.len() != needed {
            window_buffer.resize(needed, 0);
        }
        rgb_to_window_buffer(&console.framebuffer_rgb(), &mut window_buffer);
        let cpu = console.cpu_state();
        let cdrom = console.cdrom_debug_state();
        let gpu = console.gpu_debug_state();
        window.set_title(&format!(
            "ps1 emulator pc={:#010x} cd={} last_cd={:?} gp0={} draws={} uploads={} unk={}",
            cpu.pc,
            console.cdrom_command_count(),
            cdrom.last_command,
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
    for (source, dest) in rgb.chunks_exact(3).zip(window_buffer.iter_mut()) {
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

fn dump_framebuffer(console: &Console, path: PathBuf) -> ps1_emulator::Result<()> {
    let rgb = console.framebuffer_rgb();
    let mut ppm = format!("P6\n{VRAM_WIDTH} {VRAM_HEIGHT}\n255\n").into_bytes();
    ppm.extend_from_slice(&rgb);
    fs::write(&path, ppm)?;
    println!("wrote framebuffer: {}", path.display());
    Ok(())
}
