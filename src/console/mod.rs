use std::path::Path;

use crate::bios::Bios;
use crate::bus::Bus;
use crate::cdrom::{CdImage, CdRomDebugState};
use crate::cpu::{BiosCall, Cpu, CpuState};
use crate::dma::DmaDebugState;
use crate::error::Result;
use crate::exe::PsxExe;
use crate::gpu::GpuDebugState;

const GP_REGISTER: usize = 28;
const SP_REGISTER: usize = 29;
const FP_REGISTER: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstructionTraceEntry {
    pub address: u32,
    pub opcode: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrashContext {
    pub pc: u32,
    pub cpu: CpuState,
    pub recent_instructions: Vec<InstructionTraceEntry>,
    pub dma: DmaDebugState,
    pub gpu: GpuDebugState,
    pub cdrom: CdRomDebugState,
    pub last_error: Option<String>,
}

pub struct Console {
    cpu: Cpu,
    bus: Bus,
}

impl Console {
    pub fn from_bios_file(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::new(Bios::from_file(path)?))
    }

    pub fn new(bios: Bios) -> Self {
        Self {
            cpu: Cpu::new(),
            bus: Bus::new(bios),
        }
    }

    pub fn load_exe(&mut self, exe: &PsxExe) -> Result<()> {
        self.bus.load_ram(exe.load_address, exe.payload())?;
        self.cpu.set_pc(exe.initial_pc);
        self.cpu.set_reg(GP_REGISTER, exe.initial_gp);

        if exe.stack_pointer != 0 {
            self.cpu.set_reg(SP_REGISTER, exe.stack_pointer);
            self.cpu.set_reg(FP_REGISTER, exe.stack_pointer);
        }

        Ok(())
    }

    pub fn load_cd_image(&mut self, cd: CdImage) {
        self.bus.load_cd_image(cd);
    }

    pub fn display_width(&self) -> usize {
        self.bus.display_width()
    }

    pub fn display_height(&self) -> usize {
        self.bus.display_height()
    }

    pub fn copy_display_rgb_into(&self, out: &mut [u8]) {
        self.bus.copy_display_rgb_into(out);
    }

    pub fn gpu_debug_state(&self) -> GpuDebugState {
        self.bus.gpu_debug_state()
    }

    pub fn framebuffer_rgb(&self) -> Vec<u8> {
        self.bus.framebuffer_rgb()
    }

    pub fn cdrom_debug_state(&self) -> CdRomDebugState {
        self.bus.cdrom_debug_state()
    }

    pub fn dma_debug_state(&self) -> DmaDebugState {
        self.bus.dma_debug_state()
    }

    pub fn cd_image_loaded(&self) -> bool {
        self.bus.cd_image().is_some()
    }

    pub fn cdrom_command_count(&self) -> u64 {
        self.bus.cdrom_command_count()
    }

    pub fn cdrom_dma_read_bytes(&self) -> u64 {
        self.bus.cdrom_dma_read_bytes()
    }

    pub fn crash_context(
        &self,
        recent_instructions: Vec<InstructionTraceEntry>,
        last_error: Option<String>,
    ) -> CrashContext {
        let cpu = self.cpu_state();
        CrashContext {
            pc: cpu.pc,
            cpu,
            recent_instructions,
            dma: self.dma_debug_state(),
            gpu: self.gpu_debug_state(),
            cdrom: self.cdrom_debug_state(),
            last_error,
        }
    }

    pub fn step(&mut self) -> Result<()> {
        let cycles = self.cpu.step(&mut self.bus)?;
        self.bus.tick_cycles(cycles);
        Ok(())
    }

    pub fn cpu_state(&self) -> CpuState {
        self.cpu.state()
    }

    pub fn pending_bios_call(&self) -> Option<BiosCall> {
        self.cpu.pending_bios_call()
    }

    pub fn peek32(&self, address: u32) -> Result<u32> {
        self.bus.peek32(address)
    }
}
