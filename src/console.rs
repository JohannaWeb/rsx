use std::path::Path;

use crate::bios::Bios;
use crate::bus::Bus;
use crate::cdrom::{CdImage, CdRomDebugState};
use crate::cpu::{BiosCall, Cpu, CpuState};
use crate::error::Result;
use crate::exe::PsxExe;
use crate::gpu::GpuDebugState;

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
        self.cpu.set_reg(28, exe.initial_gp);

        if exe.stack_pointer != 0 {
            self.cpu.set_reg(29, exe.stack_pointer);
            self.cpu.set_reg(30, exe.stack_pointer);
        }

        Ok(())
    }

    pub fn load_cd_image(&mut self, cd: CdImage) {
        self.bus.load_cd_image(cd);
    }

    pub fn cd_image(&self) -> Option<&CdImage> {
        self.bus.cd_image()
    }

    pub fn cdrom_command_count(&self) -> u64 {
        self.bus.cdrom_command_count()
    }

    pub fn cdrom_dma_read_bytes(&self) -> u64 {
        self.bus.cdrom_dma_read_bytes()
    }

    pub fn cdrom_debug_state(&self) -> CdRomDebugState {
        self.bus.cdrom_debug_state()
    }

    pub fn framebuffer_rgb(&self) -> Vec<u8> {
        self.bus.framebuffer_rgb()
    }

    pub fn display_width(&self) -> usize {
        self.bus.display_width()
    }

    pub fn display_height(&self) -> usize {
        self.bus.display_height()
    }

    pub fn gpu_debug_state(&self) -> GpuDebugState {
        self.bus.gpu_debug_state()
    }

    pub fn step(&mut self) -> Result<()> {
        self.cpu.step(&mut self.bus)?;
        self.bus.tick();
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
