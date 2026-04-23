mod bios;
mod bus;
mod cdrom;
mod console;
mod cpu;
mod dma;
mod ecm;
mod error;
mod exe;
mod gpu;
mod gte;
mod spu;

pub use bios::Bios;
pub use bus::Bus;
pub use cdrom::{CdImage, CdRomCommand, CdRomDebugState, TrackMode};
pub use console::{Console, CrashContext, InstructionTraceEntry};
pub use cpu::{BiosCall, Cpu, CpuState};
pub use dma::{DmaChannel, DmaController, DmaDebugState, DmaDirection, DmaStep, DmaSyncMode};
pub use ecm::decode_ecm_file;
pub use error::{Error, Result};
pub use exe::PsxExe;
pub use gpu::{Gpu, GpuDebugState, VRAM_HEIGHT, VRAM_WIDTH};

#[derive(Clone, Copy, Debug, Default)]
pub struct Config {
    pub trace_pc: bool,
    pub trace_interrupts: bool,
    pub trace_gpu: bool,
    pub trace_cdrom_reads: bool,
    pub trace_cdrom_writes: bool,
    pub trace_tty: bool,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            trace_pc: std::env::var_os("PS1_TRACE_PC").is_some(),
            trace_interrupts: std::env::var_os("PS1_TRACE_INTERRUPTS").is_some(),
            trace_gpu: std::env::var_os("PS1_TRACE_GPU").is_some(),
            trace_cdrom_reads: std::env::var_os("PS1_TRACE_CDROM_READS").is_some(),
            trace_cdrom_writes: std::env::var_os("PS1_TRACE_CDROM_WRITES").is_some(),
            trace_tty: std::env::var_os("PS1_TRACE_TTY").is_some(),
        }
    }
}
