#[path = "bios.rs"]
mod bios;
#[path = "decode.rs"]
mod decode;
#[path = "instructions.rs"]
mod instructions;

use std::env;
use std::fmt;

use crate::bus::Bus as CpuBusAccess;
use crate::error::Result;
use crate::gte::Gte;

#[path = "hle.rs"]
mod hle;
pub use bios::BiosCallVector;
pub use decode::{
    Cop0FunctionOpcode, Cop0RsOpcode, Cop2RsOpcode, PrimaryOpcode, RegImmOpcode, SpecialOpcode,
};
use hle::BiosHle;

const BIOS_RESET_VECTOR: u32 = 0xbfc0_0000;
const EXCEPTION_VECTOR: u32 = 0x8000_0080;
const ROM_EXCEPTION_VECTOR: u32 = 0xbfc0_0180;
const KSEG0_BASE: u32 = 0x8000_0000;
const JUMP_TARGET_MASK: u32 = 0x03ff_ffff;
const COP0_STATUS: usize = 12;
const COP0_CAUSE: usize = 13;
const COP0_EPC: usize = 14;
const COP0_STATUS_INTERRUPT_ENABLE: u32 = 1;
const COP0_STATUS_BEV: u32 = 1 << 22;
const COP0_STATUS_INTERRUPT_MASK_SHIFT: u32 = 8;
const COP0_STATUS_EXCEPTION_STACK_MASK: u32 = 0x3f;
const COP0_CAUSE_INTERRUPT_PENDING_SHIFT: u32 = 8;
const COP0_CAUSE_EXCEPTION_CODE_SHIFT: u32 = 2;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterruptHook {
    pub pc: u32,
    pub sp: u32,
    pub fp: u32,
    pub saved: [u32; 8],
    pub gp: u32,
}

pub struct Cpu {
    regs: [u32; 32],
    cop0: [u32; 32],
    gte: Gte,
    hi: u32,
    lo: u32,
    pc: u32,
    next_pc: u32,
    trace_pc: bool,
    trace_interrupts: bool,
    trace_tty: bool,
    load_delay: Option<(usize, u32)>,
    load_delay_pending: Option<(usize, u32)>,
    bios_hle: BiosHle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuState {
    pub pc: u32,
    pub next_pc: u32,
    pub regs: [u32; 32],
    pub hi: u32,
    pub lo: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BiosCall {
    pub vector: &'static str,
    pub function: u32,
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            regs: [0; 32],
            cop0: {
                let mut cop0 = [0; 32];
                cop0[COP0_STATUS] = COP0_STATUS_BEV;
                cop0
            },
            gte: Gte::new(),
            hi: 0,
            lo: 0,
            pc: BIOS_RESET_VECTOR,
            next_pc: BIOS_RESET_VECTOR + 4,
            trace_pc: trace_flag("PS1_TRACE_PC"),
            trace_interrupts: trace_flag("PS1_TRACE_INTERRUPTS"),
            trace_tty: trace_flag("PS1_TRACE_TTY"),
            load_delay: None,
            load_delay_pending: None,
            bios_hle: BiosHle::new(),
        }
    }

    pub fn set_pc(&mut self, pc: u32) {
        self.pc = pc;
        self.next_pc = pc.wrapping_add(4);
    }

    pub fn set_reg(&mut self, index: usize, value: u32) {
        if index != 0 {
            self.regs[index] = value;
        }
    }

    pub fn reg(&self, index: usize) -> u32 {
        self.regs[index]
    }

    pub fn state(&self) -> CpuState {
        CpuState {
            pc: self.pc,
            next_pc: self.next_pc,
            regs: self.regs,
            hi: self.hi,
            lo: self.lo,
        }
    }

    pub fn pending_bios_call(&self) -> Option<BiosCall> {
        let vector = BiosCallVector::decode(self.pc)?;
        Some(BiosCall {
            vector: vector.name(),
            function: self.regs[9],
        })
    }

    pub fn step(&mut self, bus: &mut CpuBusAccess) -> Result<u32> {
        let pending_load = self.load_delay.take();
        self.load_delay_pending = None;

        if let Some(vector) = BiosCallVector::decode(self.pc) {
            log::info!(
                "BIOS call at PC={:#010x} vector={:?} function={:#04x}",
                self.pc,
                vector,
                self.regs[9]
            );
            self.commit_load_delay(pending_load);
            let mut bios_hle = std::mem::take(&mut self.bios_hle);
            bios_hle.execute_bios_call(self, vector, bus);
            self.bios_hle = bios_hle;
            self.regs[0] = 0;
            return Ok(8);
        }

        if let Some(cause) = self.pending_interrupt_cause(bus) {
            log::info!(
                "Interrupt at PC={:#010x} cause={:#010x} status={:#010x}",
                self.pc,
                cause,
                self.cop0[COP0_STATUS]
            );
            self.commit_load_delay(pending_load);
            let pc = self.pc;
            let mut bios_hle = std::mem::take(&mut self.bios_hle);
            bios_hle.enter_interrupt(self, pc, cause, bus);
            self.bios_hle = bios_hle;
            self.regs[0] = 0;
            return Ok(8);
        }

        let mut bios_hle = std::mem::take(&mut self.bios_hle);
        if let Some(cycles) = bios_hle.fast_forward_startup_copy(self, bus) {
            self.bios_hle = bios_hle;
            self.commit_load_delay(pending_load);
            self.load_delay = self.load_delay_pending.take();
            self.regs[0] = 0;
            return Ok(cycles);
        }
        self.bios_hle = bios_hle;

        let pc = self.pc;
        let instruction = bus.read32(pc)?;

        if self.trace_pc && pc >= KSEG0_BASE {
            log::info!(
                "PC={:#010x} instr={:#010x} {}",
                pc,
                instruction,
                self.disassemble(instruction)
            );
        }

        self.pc = self.next_pc;
        self.next_pc = self.next_pc.wrapping_add(4);
        self.execute(pc, instruction, bus)?;
        self.commit_load_delay(pending_load);
        self.load_delay = self.load_delay_pending.take();
        self.regs[0] = 0;
        Ok(instruction_cycles(instruction))
    }

    fn disassemble(&self, instruction: u32) -> String {
        let opcode = (instruction >> 26) & 0x3f;
        match opcode {
            0x00 => self.disassemble_special(instruction),
            0x01 => self.disassemble_regimm(instruction),
            0x02 => self.disassemble_jump("j", instruction),
            0x03 => self.disassemble_jump("jal", instruction),
            0x04 => self.disassemble_branch("beq", instruction),
            0x05 => self.disassemble_branch("bne", instruction),
            0x06 => self.disassemble_branch("blez", instruction),
            0x07 => self.disassemble_branch("bgtz", instruction),
            0x14 => self.disassemble_branch("beql", instruction),
            0x15 => self.disassemble_branch("bnel", instruction),
            0x16 => self.disassemble_branch("blezl", instruction),
            0x17 => self.disassemble_branch("bgtzl", instruction),
            0x08 => self.disassemble_imm("addi", instruction),
            0x09 => self.disassemble_imm("addiu", instruction),
            0x0a => self.disassemble_imm("slti", instruction),
            0x0b => self.disassemble_imm("sltiu", instruction),
            0x0c => self.disassemble_imm_unsigned("andi", instruction),
            0x0d => self.disassemble_imm_unsigned("ori", instruction),
            0x0e => self.disassemble_imm_unsigned("xori", instruction),
            0x0f => format!(
                "lui {}, {}",
                reg_name(rt(instruction)),
                instruction & 0xffff
            ),
            0x10 => "cop0".to_string(),
            0x11 => "cop1".to_string(),
            0x12 => "cop2".to_string(),
            0x13 => "cop3".to_string(),
            0x20 | 0x21 | 0x22 | 0x23 | 0x24 | 0x25 | 0x26 | 0x28 | 0x29 | 0x2a | 0x2b | 0x2e
            | 0x32 | 0x3a => self.disassemble_memory(opcode, instruction),
            _ => format!("opcode {:#02x}", opcode),
        }
    }

    fn disassemble_special(&self, instruction: u32) -> String {
        let rs_idx = rs(instruction);
        let rt_idx = rt(instruction);
        let rd_idx = rd(instruction);
        match instruction & 0x3f {
            0x00 => format!(
                "sll {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rt_idx),
                shamt(instruction)
            ),
            0x02 => format!(
                "srl {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rt_idx),
                shamt(instruction)
            ),
            0x03 => format!(
                "sra {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rt_idx),
                shamt(instruction)
            ),
            0x08 => format!("jr {}", reg_name(rs_idx)),
            0x09 => format!("jalr {}, {}", reg_name(rd_idx), reg_name(rs_idx)),
            0x0c => "syscall".to_string(),
            0x10 => format!("mfhi {}", reg_name(rd_idx)),
            0x11 => format!("mthi {}", reg_name(rs_idx)),
            0x12 => format!("mflo {}", reg_name(rd_idx)),
            0x13 => format!("mtlo {}", reg_name(rs_idx)),
            0x18 => format!("mult {}, {}", reg_name(rs_idx), reg_name(rt_idx)),
            0x19 => format!("multu {}, {}", reg_name(rs_idx), reg_name(rt_idx)),
            0x1a => format!("div {}, {}", reg_name(rs_idx), reg_name(rt_idx)),
            0x1b => format!("divu {}, {}", reg_name(rs_idx), reg_name(rt_idx)),
            0x20 => format!(
                "add {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rs_idx),
                reg_name(rt_idx)
            ),
            0x21 => format!(
                "addu {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rs_idx),
                reg_name(rt_idx)
            ),
            0x22 => format!(
                "sub {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rs_idx),
                reg_name(rt_idx)
            ),
            0x23 => format!(
                "subu {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rs_idx),
                reg_name(rt_idx)
            ),
            0x24 => format!(
                "and {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rs_idx),
                reg_name(rt_idx)
            ),
            0x25 => format!(
                "or {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rs_idx),
                reg_name(rt_idx)
            ),
            0x26 => format!(
                "xor {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rs_idx),
                reg_name(rt_idx)
            ),
            0x27 => format!(
                "nor {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rs_idx),
                reg_name(rt_idx)
            ),
            0x2a => format!(
                "slt {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rs_idx),
                reg_name(rt_idx)
            ),
            0x2b => format!(
                "sltu {}, {}, {}",
                reg_name(rd_idx),
                reg_name(rs_idx),
                reg_name(rt_idx)
            ),
            funct => format!("special {:#02x}", funct),
        }
    }

    fn disassemble_regimm(&self, instruction: u32) -> String {
        let rs_idx = rs(instruction);
        match rt(instruction) {
            0x00 => format!("bltz {}, {}", reg_name(rs_idx), imm(instruction)),
            0x01 => format!("bgez {}, {}", reg_name(rs_idx), imm(instruction)),
            0x10 => format!("bltzal {}, {}", reg_name(rs_idx), imm(instruction)),
            0x11 => format!("bgezal {}, {}", reg_name(rs_idx), imm(instruction)),
            rt_idx => format!("regimm {:#02x}", rt_idx),
        }
    }

    fn disassemble_jump(&self, mnemonic: &str, instruction: u32) -> String {
        format!(
            "{mnemonic} {:#010x}",
            (self.pc & 0xf000_0000) | ((instruction & JUMP_TARGET_MASK) << 2)
        )
    }

    fn disassemble_branch(&self, mnemonic: &str, instruction: u32) -> String {
        format!(
            "{mnemonic} {}, {}, {}",
            reg_name(rs(instruction)),
            reg_name(rt(instruction)),
            imm(instruction)
        )
    }

    fn disassemble_imm(&self, mnemonic: &str, instruction: u32) -> String {
        format!(
            "{mnemonic} {}, {}, {}",
            reg_name(rt(instruction)),
            reg_name(rs(instruction)),
            imm(instruction)
        )
    }

    fn disassemble_imm_unsigned(&self, mnemonic: &str, instruction: u32) -> String {
        format!(
            "{mnemonic} {}, {}, {}",
            reg_name(rt(instruction)),
            reg_name(rs(instruction)),
            instruction & 0xffff
        )
    }

    fn disassemble_memory(&self, opcode: u32, instruction: u32) -> String {
        let rt_idx = rt(instruction);
        let rs_idx = rs(instruction);
        let imm_val = imm(instruction);
        let mnemonic = match opcode {
            0x20 => "lb",
            0x21 => "lh",
            0x22 => "lwl",
            0x23 => "lw",
            0x24 => "lbu",
            0x25 => "lhu",
            0x26 => "lwr",
            0x28 => "sb",
            0x29 => "sh",
            0x2a => "swl",
            0x2b => "sw",
            0x2e => "swr",
            0x32 => "lwc2",
            0x3a => "swc2",
            _ => unreachable!(),
        };
        format!(
            "{mnemonic} {}, {}({})",
            reg_name(rt_idx),
            imm_val,
            reg_name(rs_idx)
        )
    }

    fn pending_interrupt_cause(&self, bus: &CpuBusAccess) -> Option<u32> {
        let status = self.cop0[COP0_STATUS];
        if status & COP0_STATUS_INTERRUPT_ENABLE == 0 {
            return None;
        }

        let pending = bus.interrupt_pending_bits() & 0xff;
        let mask = (status >> COP0_STATUS_INTERRUPT_MASK_SHIFT) & 0xff;
        let active = pending & mask;
        (active != 0).then_some(active << COP0_CAUSE_INTERRUPT_PENDING_SHIFT)
    }

    fn enter_exception(&mut self, pc: u32, cause: u32) {
        self.cop0[COP0_EPC] = pc;
        self.cop0[COP0_CAUSE] = cause << COP0_CAUSE_EXCEPTION_CODE_SHIFT;
        let status = self.cop0[COP0_STATUS];
        self.cop0[COP0_STATUS] = (status & !COP0_STATUS_EXCEPTION_STACK_MASK)
            | ((status << 2) & COP0_STATUS_EXCEPTION_STACK_MASK);
        let vector = exception_vector(status);
        self.pc = vector;
        self.next_pc = vector + 4;
    }

    pub(super) fn stage_load(&mut self, register: usize, value: u32) {
        if register != 0 {
            self.load_delay_pending = Some((register, value));
        }
    }

    fn commit_load_delay(&mut self, pending: Option<(usize, u32)>) {
        if let Some((register, value)) = pending {
            self.set_reg(register, value);
        }
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CpuState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "pc={:#010x} next_pc={:#010x} ra={:#010x} sp={:#010x} gp={:#010x}",
            self.pc, self.next_pc, self.regs[31], self.regs[29], self.regs[28]
        )
    }
}

pub(super) fn rs(instruction: u32) -> usize {
    ((instruction >> 21) & 0x1f) as usize
}
pub(super) fn rt(instruction: u32) -> usize {
    ((instruction >> 16) & 0x1f) as usize
}
pub(super) fn rd(instruction: u32) -> usize {
    ((instruction >> 11) & 0x1f) as usize
}
pub(super) fn shamt(instruction: u32) -> u32 {
    (instruction >> 6) & 0x1f
}
pub(super) fn imm(instruction: u32) -> i16 {
    instruction as i16
}
pub(super) fn unsigned_imm(instruction: u32) -> u32 {
    instruction & 0xffff
}

pub(super) fn branch_target(pc_after_delay_slot: u32, offset: i16) -> u32 {
    pc_after_delay_slot.wrapping_add(((offset as i32) << 2) as u32)
}

pub(super) fn jump_target(pc: u32, instruction: u32) -> u32 {
    (pc & 0xf000_0000) | ((instruction & 0x03ff_ffff) << 2)
}

pub(super) fn exception_vector(status: u32) -> u32 {
    if status & COP0_STATUS_BEV != 0 {
        ROM_EXCEPTION_VECTOR
    } else {
        EXCEPTION_VECTOR
    }
}

fn trace_flag(name: &str) -> bool {
    env::var_os(name).is_some()
}

fn instruction_cycles(instruction: u32) -> u32 {
    let opcode = instruction >> 26;
    match opcode {
        0x00 => match instruction & 0x3f {
            0x18 | 0x19 | 0x1a | 0x1b => 8,
            _ => 1,
        },
        0x02 | 0x03 => 3,
        0x20 | 0x21 | 0x22 | 0x23 | 0x24 | 0x25 | 0x26 | 0x28 | 0x29 | 0x2a | 0x2b | 0x2e
        | 0x32 | 0x3a => 4,
        0x10 | 0x12 => 2,
        _ => 1,
    }
}

pub(super) fn load_word_left(bus: &mut CpuBusAccess, address: u32, current: u32) -> Result<u32> {
    let aligned = address & !3;
    let word = bus.read32(aligned)?;
    Ok(match address & 3 {
        0 => (current & 0x00ff_ffff) | (word << 24),
        1 => (current & 0x0000_ffff) | (word << 16),
        2 => (current & 0x0000_00ff) | (word << 8),
        _ => word,
    })
}

pub(super) fn load_word_right(bus: &mut CpuBusAccess, address: u32, current: u32) -> Result<u32> {
    let aligned = address & !3;
    let word = bus.read32(aligned)?;
    Ok(match address & 3 {
        0 => word,
        1 => (current & 0xff00_0000) | (word >> 8),
        2 => (current & 0xffff_0000) | (word >> 16),
        _ => (current & 0xffff_ff00) | (word >> 24),
    })
}

pub(super) fn store_word_left(bus: &mut CpuBusAccess, address: u32, value: u32) -> Result<()> {
    let aligned = address & !3;
    let current = bus.read32(aligned)?;
    let word = match address & 3 {
        0 => (current & 0xffff_ff00) | (value >> 24),
        1 => (current & 0xffff_0000) | (value >> 16),
        2 => (current & 0xff00_0000) | (value >> 8),
        _ => value,
    };
    bus.write32(aligned, word)
}

pub(super) fn store_word_right(bus: &mut CpuBusAccess, address: u32, value: u32) -> Result<()> {
    let aligned = address & !3;
    let current = bus.read32(aligned)?;
    let word = match address & 3 {
        0 => value,
        1 => (current & 0x0000_00ff) | (value << 8),
        2 => (current & 0x0000_ffff) | (value << 16),
        _ => (current & 0x00ff_ffff) | (value << 24),
    };
    bus.write32(aligned, word)
}

fn reg_name(index: usize) -> &'static str {
    const NAMES: [&str; 32] = [
        "zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6",
        "t7", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1", "gp", "sp",
        "fp", "ra",
    ];
    NAMES.get(index).copied().unwrap_or("??")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bios, CdRomCommand};

    // Register aliases
    const ZERO: u32 = 0;
    const T0: u32 = 8;
    const T1: u32 = 9;
    const T2: u32 = 10;
    const T3: u32 = 11;

    // Instruction encoders
    fn nop() -> u32 {
        0
    }
    fn lui(rt: u32, imm_val: u32) -> u32 {
        ((PrimaryOpcode::Lui as u32) << 26) | (rt << 16) | (imm_val & 0xffff)
    }
    fn ori(rt: u32, rs_val: u32, imm_val: u32) -> u32 {
        ((PrimaryOpcode::Ori as u32) << 26) | (rs_val << 21) | (rt << 16) | (imm_val & 0xffff)
    }
    fn addiu(rt: u32, rs_val: u32, imm_val: i16) -> u32 {
        ((PrimaryOpcode::Addiu as u32) << 26)
            | (rs_val << 21)
            | (rt << 16)
            | (imm_val as u16 as u32)
    }
    fn addi(rt: u32, rs_val: u32, imm_val: i16) -> u32 {
        ((PrimaryOpcode::Addi as u32) << 26) | (rs_val << 21) | (rt << 16) | (imm_val as u16 as u32)
    }
    fn div(rs_val: u32, rt_val: u32) -> u32 {
        ((SpecialOpcode::Div as u32) << 26) | (rs_val << 21) | (rt_val << 16)
    }
    fn divu(rs_val: u32, rt_val: u32) -> u32 {
        ((SpecialOpcode::Divu as u32) << 26) | (rs_val << 21) | (rt_val << 16)
    }
    fn beq(rs_val: u32, rt: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Beq as u32) << 26) | (rs_val << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn sb(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Sb as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn sw(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Sw as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn lb(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Lb as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn lbu(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Lbu as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn lw(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Lw as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn lwl(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Lwl as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn lwr(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Lwr as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn swl(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Swl as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn swr(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Swr as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }

    fn bus_with_program(words: &[u32]) -> crate::bus::Bus {
        let mut bus =
            crate::bus::Bus::new(Bios::from_bytes(vec![0; crate::bios::BIOS_SIZE]).unwrap());
        for (index, word) in words.iter().copied().enumerate() {
            bus.write32((index * 4) as u32, word).unwrap();
        }
        bus
    }

    #[test]
    fn executes_basic_integer_program() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        let mut bus = bus_with_program(&[lui(T0, 0x1234), ori(T0, T0, 0x5678), addiu(T1, T0, 1)]);

        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.state().regs[8], 0x1234_5678);
        assert_eq!(cpu.state().regs[9], 0x1234_5679);
    }

    #[test]
    fn keeps_register_zero_hardwired() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        let mut bus = bus_with_program(&[addiu(ZERO, ZERO, 1)]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.state().regs[0], 0);
    }

    #[test]
    fn executes_branch_delay_slot() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        let mut bus = bus_with_program(&[
            beq(ZERO, ZERO, 1),
            addiu(T0, ZERO, 1),
            addiu(T0, ZERO, 2),
            addiu(T1, ZERO, 3),
        ]);

        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.state().regs[8], 2);
        assert_eq!(cpu.state().regs[9], 0);
    }

    #[test]
    fn branch_likely_skips_delay_slot_when_not_taken() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        let mut bus = bus_with_program(&[
            ((PrimaryOpcode::Blezl as u32) << 26) | (T0 << 21) | 1,
            addiu(T1, ZERO, 1),
            addiu(T2, ZERO, 2),
        ]);

        cpu.set_reg(T0 as usize, 1);

        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.state().regs[9], 0);
        assert_eq!(cpu.state().regs[10], 2);
    }

    #[test]
    fn sign_and_zero_extends_memory_loads() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        let mut bus = bus_with_program(&[
            addiu(T0, ZERO, 0x7f),
            sb(T0, ZERO, 0x100),
            addiu(T0, ZERO, -1),
            sb(T0, ZERO, 0x101),
            lb(T1, ZERO, 0x101),
            lbu(T2, ZERO, 0x101),
            nop(),
        ]);

        for _ in 0..7 {
            cpu.step(&mut bus).unwrap();
        }

        assert_eq!(cpu.state().regs[9], 0xffff_ffff);
        assert_eq!(cpu.state().regs[10], 0x0000_00ff);
    }

    #[test]
    fn load_delay_defers_loaded_value_by_one_instruction() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        let mut bus = bus_with_program(&[
            lui(T0, 0x1234),
            ori(T0, T0, 0x5678),
            sw(T0, ZERO, 0x100),
            lw(T1, ZERO, 0x100),
            addiu(T2, T1, 1),
            addiu(T3, T1, 2),
            nop(),
        ]);

        for _ in 0..7 {
            cpu.step(&mut bus).unwrap();
        }

        assert_eq!(cpu.state().regs[9], 0x1234_5678);
        assert_eq!(cpu.state().regs[10], 1);
        assert_eq!(cpu.state().regs[11], 0x1234_567a);
    }

    #[test]
    fn addi_overflow_enters_exception_vector() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        cpu.cop0[COP0_STATUS] = 0;
        let mut bus = bus_with_program(&[lui(T0, 0x7fff), ori(T0, T0, 0xffff), addi(T1, T0, 1)]);

        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.state().pc, EXCEPTION_VECTOR);
        assert_eq!(cpu.cop0[COP0_EPC], 8);
        assert_eq!(cpu.cop0[COP0_CAUSE], 0x0c << 2);
    }

    #[test]
    fn div_by_zero_uses_r3000a_semantics() {
        let mut cpu = Cpu::new();
        let mut bus = bus_with_program(&[nop()]);
        cpu.set_reg(T0 as usize, 0xffff_fffb);
        cpu.set_reg(T1 as usize, 0);

        cpu.special_div(0, div(T0, T1), &mut bus).unwrap();

        assert_eq!(cpu.lo, 1);
        assert_eq!(cpu.hi, 0xffff_fffb);
    }

    #[test]
    fn divu_by_zero_uses_r3000a_semantics() {
        let mut cpu = Cpu::new();
        let mut bus = bus_with_program(&[nop()]);
        cpu.set_reg(T0 as usize, 0x1234_5678);
        cpu.set_reg(T1 as usize, 0);

        cpu.special_divu(0, divu(T0, T1), &mut bus).unwrap();

        assert_eq!(cpu.lo, 0xffff_ffff);
        assert_eq!(cpu.hi, 0x1234_5678);
    }

    #[test]
    fn enters_exception_when_interrupt_is_pending_and_enabled() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        cpu.cop0[COP0_STATUS] = 0x0000_0401;
        let mut bus = bus_with_program(&[nop()]);

        bus.write32(0x1f80_1074, 0x0000_0004).unwrap();
        bus.write8(0x1f80_1800, 1).unwrap();
        bus.write8(0x1f80_1802, 0x1f).unwrap();
        bus.write8(0x1f80_1800, 0).unwrap();
        bus.write8(0x1f80_1801, CdRomCommand::GetStat.code())
            .unwrap();

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.state().pc, EXCEPTION_VECTOR);
        assert_eq!(cpu.cop0[COP0_EPC], 0);
        assert_eq!(cpu.cop0[COP0_CAUSE], 1 << 10);
    }

    #[test]
    fn enters_exception_for_vblank_interrupt() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        cpu.cop0[COP0_STATUS] = 0x0000_0101;
        let mut bus = bus_with_program(&[nop()]);

        bus.write32(0x1f80_1074, 0x0000_0001).unwrap();
        for _ in 0..crate::bus::VBLANK_INTERVAL_TICKS {
            bus.tick();
        }

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.state().pc, EXCEPTION_VECTOR);
        assert_eq!(cpu.cop0[COP0_CAUSE], 1 << 8);
    }

    #[test]
    fn unaligned_word_load_pair_reconstructs_word() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        let mut bus = bus_with_program(&[
            lui(T0, 0x1122),
            ori(T0, T0, 0x3344),
            swr(T0, ZERO, 0x100),
            swl(T0, ZERO, 0x103),
            lwr(T1, ZERO, 0x100),
            lwl(T1, ZERO, 0x103),
        ]);

        for _ in 0..7 {
            cpu.step(&mut bus).unwrap();
        }

        assert_eq!(cpu.state().regs[9], 0x1122_3344);
    }
}
