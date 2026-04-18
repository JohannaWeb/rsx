use std::fmt;

use crate::bus::Bus;
use crate::error::{Error, Result};

const BIOS_RESET_VECTOR: u32 = 0xbfc0_0000;
const EXCEPTION_VECTOR: u32 = 0x8000_0080;
const COP0_STATUS: usize = 12;
const COP0_CAUSE: usize = 13;
const COP0_EPC: usize = 14;
const COP0_STATUS_INTERRUPT_ENABLE: u32 = 1;
const COP0_STATUS_INTERRUPT_MASK_SHIFT: u32 = 8;
const COP0_CAUSE_INTERRUPT_PENDING_SHIFT: u32 = 8;
const BIOS_CALL_A0: u32 = 0x0000_00a0;
const BIOS_CALL_B0: u32 = 0x0000_00b0;
const BIOS_CALL_C0: u32 = 0x0000_00c0;
const BIOS_HEAP_START: u32 = 0x8001_0000;
const BIOS_A_MALLOC: u32 = 0x33;
const BIOS_B_ALLOC_KERNEL_MEMORY: u32 = 0x00;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimaryOpcode {
    Special = 0x00,
    RegImm = 0x01,
    J = 0x02,
    Jal = 0x03,
    Beq = 0x04,
    Bne = 0x05,
    Blez = 0x06,
    Bgtz = 0x07,
    Addi = 0x08,
    Addiu = 0x09,
    Slti = 0x0a,
    Sltiu = 0x0b,
    Andi = 0x0c,
    Ori = 0x0d,
    Xori = 0x0e,
    Lui = 0x0f,
    Cop0 = 0x10,
    Lb = 0x20,
    Lh = 0x21,
    Lwl = 0x22,
    Lw = 0x23,
    Lbu = 0x24,
    Lhu = 0x25,
    Lwr = 0x26,
    Sb = 0x28,
    Sh = 0x29,
    Swl = 0x2a,
    Sw = 0x2b,
    Swr = 0x2e,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpecialOpcode {
    Sll = 0x00,
    Srl = 0x02,
    Sra = 0x03,
    Sllv = 0x04,
    Srlv = 0x06,
    Srav = 0x07,
    Jr = 0x08,
    Jalr = 0x09,
    Syscall = 0x0c,
    Mfhi = 0x10,
    Mthi = 0x11,
    Mflo = 0x12,
    Mtlo = 0x13,
    Mult = 0x18,
    Multu = 0x19,
    Div = 0x1a,
    Divu = 0x1b,
    Add = 0x20,
    Addu = 0x21,
    Sub = 0x22,
    Subu = 0x23,
    And = 0x24,
    Or = 0x25,
    Xor = 0x26,
    Nor = 0x27,
    Slt = 0x2a,
    Sltu = 0x2b,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegImmOpcode {
    Bltz = 0x00,
    Bgez = 0x01,
    Bltzal = 0x10,
    Bgezal = 0x11,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cop0RsOpcode {
    Mfc0 = 0x00,
    Mtc0 = 0x04,
    Co = 0x10,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cop0FunctionOpcode {
    Rfe = 0x10,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BiosCallVector {
    A0,
    B0,
    C0,
}

type InstructionHandler = fn(&mut Cpu, u32, u32, &mut Bus) -> Result<()>;

const PRIMARY_OPCODE_TABLE: [InstructionHandler; 64] = {
    let mut table = [Cpu::op_unsupported as InstructionHandler; 64];
    table[PrimaryOpcode::Special as usize] = Cpu::op_special;
    table[PrimaryOpcode::RegImm as usize] = Cpu::op_regimm;
    table[PrimaryOpcode::J as usize] = Cpu::op_j;
    table[PrimaryOpcode::Jal as usize] = Cpu::op_jal;
    table[PrimaryOpcode::Beq as usize] = Cpu::op_beq;
    table[PrimaryOpcode::Bne as usize] = Cpu::op_bne;
    table[PrimaryOpcode::Blez as usize] = Cpu::op_blez;
    table[PrimaryOpcode::Bgtz as usize] = Cpu::op_bgtz;
    table[PrimaryOpcode::Addi as usize] = Cpu::op_add_immediate;
    table[PrimaryOpcode::Addiu as usize] = Cpu::op_add_immediate;
    table[PrimaryOpcode::Slti as usize] = Cpu::op_slti;
    table[PrimaryOpcode::Sltiu as usize] = Cpu::op_sltiu;
    table[PrimaryOpcode::Andi as usize] = Cpu::op_andi;
    table[PrimaryOpcode::Ori as usize] = Cpu::op_ori;
    table[PrimaryOpcode::Xori as usize] = Cpu::op_xori;
    table[PrimaryOpcode::Lui as usize] = Cpu::op_lui;
    table[PrimaryOpcode::Cop0 as usize] = Cpu::op_cop0;
    table[PrimaryOpcode::Lb as usize] = Cpu::op_lb;
    table[PrimaryOpcode::Lh as usize] = Cpu::op_lh;
    table[PrimaryOpcode::Lwl as usize] = Cpu::op_lwl;
    table[PrimaryOpcode::Lw as usize] = Cpu::op_lw;
    table[PrimaryOpcode::Lbu as usize] = Cpu::op_lbu;
    table[PrimaryOpcode::Lhu as usize] = Cpu::op_lhu;
    table[PrimaryOpcode::Lwr as usize] = Cpu::op_lwr;
    table[PrimaryOpcode::Sb as usize] = Cpu::op_sb;
    table[PrimaryOpcode::Sh as usize] = Cpu::op_sh;
    table[PrimaryOpcode::Swl as usize] = Cpu::op_swl;
    table[PrimaryOpcode::Sw as usize] = Cpu::op_sw;
    table[PrimaryOpcode::Swr as usize] = Cpu::op_swr;
    table
};

const SPECIAL_FUNCTION_TABLE: [InstructionHandler; 64] = {
    let mut table = [Cpu::op_unsupported as InstructionHandler; 64];
    table[SpecialOpcode::Sll as usize] = Cpu::special_sll;
    table[SpecialOpcode::Srl as usize] = Cpu::special_srl;
    table[SpecialOpcode::Sra as usize] = Cpu::special_sra;
    table[SpecialOpcode::Sllv as usize] = Cpu::special_sllv;
    table[SpecialOpcode::Srlv as usize] = Cpu::special_srlv;
    table[SpecialOpcode::Srav as usize] = Cpu::special_srav;
    table[SpecialOpcode::Jr as usize] = Cpu::special_jr;
    table[SpecialOpcode::Jalr as usize] = Cpu::special_jalr;
    table[SpecialOpcode::Syscall as usize] = Cpu::special_syscall;
    table[SpecialOpcode::Mfhi as usize] = Cpu::special_mfhi;
    table[SpecialOpcode::Mthi as usize] = Cpu::special_mthi;
    table[SpecialOpcode::Mflo as usize] = Cpu::special_mflo;
    table[SpecialOpcode::Mtlo as usize] = Cpu::special_mtlo;
    table[SpecialOpcode::Mult as usize] = Cpu::special_mult;
    table[SpecialOpcode::Multu as usize] = Cpu::special_multu;
    table[SpecialOpcode::Div as usize] = Cpu::special_div;
    table[SpecialOpcode::Divu as usize] = Cpu::special_divu;
    table[SpecialOpcode::Add as usize] = Cpu::special_add;
    table[SpecialOpcode::Addu as usize] = Cpu::special_add;
    table[SpecialOpcode::Sub as usize] = Cpu::special_sub;
    table[SpecialOpcode::Subu as usize] = Cpu::special_sub;
    table[SpecialOpcode::And as usize] = Cpu::special_and;
    table[SpecialOpcode::Or as usize] = Cpu::special_or;
    table[SpecialOpcode::Xor as usize] = Cpu::special_xor;
    table[SpecialOpcode::Nor as usize] = Cpu::special_nor;
    table[SpecialOpcode::Slt as usize] = Cpu::special_slt;
    table[SpecialOpcode::Sltu as usize] = Cpu::special_sltu;
    table
};

const REGIMM_FUNCTION_TABLE: [InstructionHandler; 32] = {
    let mut table = [Cpu::op_unsupported as InstructionHandler; 32];
    table[RegImmOpcode::Bltz as usize] = Cpu::regimm_bltz;
    table[RegImmOpcode::Bgez as usize] = Cpu::regimm_bgez;
    table[RegImmOpcode::Bltzal as usize] = Cpu::regimm_bltzal;
    table[RegImmOpcode::Bgezal as usize] = Cpu::regimm_bgezal;
    table
};

const COP0_FUNCTION_TABLE: [InstructionHandler; 32] = {
    let mut table = [Cpu::op_unsupported as InstructionHandler; 32];
    table[Cop0RsOpcode::Mfc0 as usize] = Cpu::cop0_mfc0;
    table[Cop0RsOpcode::Mtc0 as usize] = Cpu::cop0_mtc0;
    table[Cop0RsOpcode::Co as usize] = Cpu::cop0_co;
    table
};

impl BiosCallVector {
    fn decode(pc: u32) -> Option<Self> {
        match pc {
            BIOS_CALL_A0 => Some(Self::A0),
            BIOS_CALL_B0 => Some(Self::B0),
            BIOS_CALL_C0 => Some(Self::C0),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::A0 => "A0",
            Self::B0 => "B0",
            Self::C0 => "C0",
        }
    }
}

pub struct Cpu {
    regs: [u32; 32],
    cop0: [u32; 32],
    hi: u32,
    lo: u32,
    pc: u32,
    next_pc: u32,
    bios_heap: u32,
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
            cop0: [0; 32],
            hi: 0,
            lo: 0,
            pc: BIOS_RESET_VECTOR,
            next_pc: BIOS_RESET_VECTOR + 4,
            bios_heap: BIOS_HEAP_START,
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

    pub fn step(&mut self, bus: &mut Bus) -> Result<()> {
        if let Some(vector) = BiosCallVector::decode(self.pc) {
            self.execute_bios_call(vector);
            return Ok(());
        }

        if let Some(cause) = self.pending_interrupt_cause(bus) {
            self.enter_exception(self.pc, cause);
            return Ok(());
        }

        let pc = self.pc;
        let instruction = bus.read32(pc)?;
        self.pc = self.next_pc;
        self.next_pc = self.next_pc.wrapping_add(4);
        self.execute(pc, instruction, bus)?;
        self.regs[0] = 0;
        Ok(())
    }

    fn execute(&mut self, pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let opcode = ((instruction >> 26) & 0x3f) as usize;
        PRIMARY_OPCODE_TABLE[opcode](self, pc, instruction, bus)
    }

    fn execute_special(&mut self, pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let function = (instruction & 0x3f) as usize;
        SPECIAL_FUNCTION_TABLE[function](self, pc, instruction, bus)
    }

    fn execute_regimm(&mut self, pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        REGIMM_FUNCTION_TABLE[rt(instruction)](self, pc, instruction, bus)
    }

    fn execute_cop0(&mut self, pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        COP0_FUNCTION_TABLE[rs(instruction)](self, pc, instruction, bus)
    }

    fn op_unsupported(&mut self, pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        Err(Error::UnsupportedInstruction { pc, instruction })
    }

    fn op_special(&mut self, pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        self.execute_special(pc, instruction, bus)
    }

    fn op_regimm(&mut self, pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        self.execute_regimm(pc, instruction, bus)
    }

    fn op_cop0(&mut self, pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        self.execute_cop0(pc, instruction, bus)
    }

    fn op_j(&mut self, pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.next_pc = jump_target(pc, instruction);
        Ok(())
    }

    fn op_jal(&mut self, pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.set_reg(31, self.next_pc);
        self.next_pc = jump_target(pc, instruction);
        Ok(())
    }

    fn op_beq(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        if self.reg(rs(instruction)) == self.reg(rt(instruction)) {
            self.next_pc = branch_target(self.pc, immediate(instruction));
        }
        Ok(())
    }

    fn op_bne(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        if self.reg(rs(instruction)) != self.reg(rt(instruction)) {
            self.next_pc = branch_target(self.pc, immediate(instruction));
        }
        Ok(())
    }

    fn op_blez(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        if (self.reg(rs(instruction)) as i32) <= 0 {
            self.next_pc = branch_target(self.pc, immediate(instruction));
        }
        Ok(())
    }

    fn op_bgtz(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        if (self.reg(rs(instruction)) as i32) > 0 {
            self.next_pc = branch_target(self.pc, immediate(instruction));
        }
        Ok(())
    }

    fn op_add_immediate(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        self.set_reg(rt(instruction), value);
        Ok(())
    }

    fn op_slti(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = (self.reg(rs(instruction)) as i32) < (immediate(instruction) as i32);
        self.set_reg(rt(instruction), value as u32);
        Ok(())
    }

    fn op_sltiu(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rs(instruction)) < (immediate(instruction) as u32);
        self.set_reg(rt(instruction), value as u32);
        Ok(())
    }

    fn op_andi(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rs(instruction)) & unsigned_immediate(instruction);
        self.set_reg(rt(instruction), value);
        Ok(())
    }

    fn op_ori(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rs(instruction)) | unsigned_immediate(instruction);
        self.set_reg(rt(instruction), value);
        Ok(())
    }

    fn op_xori(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rs(instruction)) ^ unsigned_immediate(instruction);
        self.set_reg(rt(instruction), value);
        Ok(())
    }

    fn op_lui(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.set_reg(rt(instruction), unsigned_immediate(instruction) << 16);
        Ok(())
    }

    fn op_lb(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        self.set_reg(rt(instruction), bus.read8(address)? as i8 as i32 as u32);
        Ok(())
    }

    fn op_lh(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        self.set_reg(rt(instruction), bus.read16(address)? as i16 as i32 as u32);
        Ok(())
    }

    fn op_lwl(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        let value = load_word_left(bus, address, self.reg(rt(instruction)))?;
        self.set_reg(rt(instruction), value);
        Ok(())
    }

    fn op_lw(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        self.set_reg(rt(instruction), bus.read32(address)?);
        Ok(())
    }

    fn op_lbu(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        self.set_reg(rt(instruction), bus.read8(address)? as u32);
        Ok(())
    }

    fn op_lhu(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        self.set_reg(rt(instruction), bus.read16(address)? as u32);
        Ok(())
    }

    fn op_lwr(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        let value = load_word_right(bus, address, self.reg(rt(instruction)))?;
        self.set_reg(rt(instruction), value);
        Ok(())
    }

    fn op_sb(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        bus.write8(address, self.reg(rt(instruction)) as u8)
    }

    fn op_sh(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        bus.write16(address, self.reg(rt(instruction)) as u16)
    }

    fn op_swl(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        store_word_left(bus, address, self.reg(rt(instruction)))
    }

    fn op_sw(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        bus.write32(address, self.reg(rt(instruction)))
    }

    fn op_swr(&mut self, _pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(immediate(instruction) as u32);
        store_word_right(bus, address, self.reg(rt(instruction)))
    }

    fn special_sll(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rt(instruction)) << shamt(instruction);
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_srl(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rt(instruction)) >> shamt(instruction);
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_sra(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = ((self.reg(rt(instruction)) as i32) >> shamt(instruction)) as u32;
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_sllv(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rt(instruction)) << (self.reg(rs(instruction)) & 0x1f);
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_srlv(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rt(instruction)) >> (self.reg(rs(instruction)) & 0x1f);
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_srav(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value =
            ((self.reg(rt(instruction)) as i32) >> (self.reg(rs(instruction)) & 0x1f)) as u32;
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_jr(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.next_pc = self.reg(rs(instruction));
        Ok(())
    }

    fn special_jalr(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let link = self.next_pc;
        self.next_pc = self.reg(rs(instruction));
        self.set_reg(rd(instruction), link);
        Ok(())
    }

    fn special_syscall(&mut self, _pc: u32, _instruction: u32, _bus: &mut Bus) -> Result<()> {
        Ok(())
    }

    fn special_mfhi(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.set_reg(rd(instruction), self.hi);
        Ok(())
    }

    fn special_mthi(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.hi = self.reg(rs(instruction));
        Ok(())
    }

    fn special_mflo(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.set_reg(rd(instruction), self.lo);
        Ok(())
    }

    fn special_mtlo(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.lo = self.reg(rs(instruction));
        Ok(())
    }

    fn special_mult(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = (self.reg(rs(instruction)) as i32 as i64)
            .wrapping_mul(self.reg(rt(instruction)) as i32 as i64);
        self.lo = value as u32;
        self.hi = (value >> 32) as u32;
        Ok(())
    }

    fn special_multu(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value =
            (self.reg(rs(instruction)) as u64).wrapping_mul(self.reg(rt(instruction)) as u64);
        self.lo = value as u32;
        self.hi = (value >> 32) as u32;
        Ok(())
    }

    fn special_div(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let dividend = self.reg(rs(instruction)) as i32;
        let divisor = self.reg(rt(instruction)) as i32;
        if divisor != 0 {
            self.lo = dividend.wrapping_div(divisor) as u32;
            self.hi = dividend.wrapping_rem(divisor) as u32;
        }
        Ok(())
    }

    fn special_divu(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let dividend = self.reg(rs(instruction));
        let divisor = self.reg(rt(instruction));
        if divisor != 0 {
            self.lo = dividend / divisor;
            self.hi = dividend % divisor;
        }
        Ok(())
    }

    fn special_add(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self
            .reg(rs(instruction))
            .wrapping_add(self.reg(rt(instruction)));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_sub(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self
            .reg(rs(instruction))
            .wrapping_sub(self.reg(rt(instruction)));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_and(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rs(instruction)) & self.reg(rt(instruction));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_or(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rs(instruction)) | self.reg(rt(instruction));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_xor(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rs(instruction)) ^ self.reg(rt(instruction));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_nor(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = !(self.reg(rs(instruction)) | self.reg(rt(instruction)));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    fn special_slt(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = (self.reg(rs(instruction)) as i32) < (self.reg(rt(instruction)) as i32);
        self.set_reg(rd(instruction), value as u32);
        Ok(())
    }

    fn special_sltu(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        let value = self.reg(rs(instruction)) < self.reg(rt(instruction));
        self.set_reg(rd(instruction), value as u32);
        Ok(())
    }

    fn regimm_bltz(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        if (self.reg(rs(instruction)) as i32) < 0 {
            self.next_pc = branch_target(self.pc, immediate(instruction));
        }
        Ok(())
    }

    fn regimm_bgez(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        if (self.reg(rs(instruction)) as i32) >= 0 {
            self.next_pc = branch_target(self.pc, immediate(instruction));
        }
        Ok(())
    }

    fn regimm_bltzal(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.set_reg(31, self.next_pc);
        if (self.reg(rs(instruction)) as i32) < 0 {
            self.next_pc = branch_target(self.pc, immediate(instruction));
        }
        Ok(())
    }

    fn regimm_bgezal(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.set_reg(31, self.next_pc);
        if (self.reg(rs(instruction)) as i32) >= 0 {
            self.next_pc = branch_target(self.pc, immediate(instruction));
        }
        Ok(())
    }

    fn cop0_mfc0(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.set_reg(rt(instruction), self.cop0[rd(instruction)]);
        Ok(())
    }

    fn cop0_mtc0(&mut self, _pc: u32, instruction: u32, _bus: &mut Bus) -> Result<()> {
        self.cop0[rd(instruction)] = self.reg(rt(instruction));
        Ok(())
    }

    fn cop0_co(&mut self, pc: u32, instruction: u32, bus: &mut Bus) -> Result<()> {
        if (instruction & 0x3f) == Cop0FunctionOpcode::Rfe as u32 {
            let status = self.cop0[COP0_STATUS];
            self.cop0[COP0_STATUS] = (status & !0x3f) | ((status >> 2) & 0x0f);
            Ok(())
        } else {
            self.op_unsupported(pc, instruction, bus)
        }
    }

    fn reg(&self, index: usize) -> u32 {
        self.regs[index]
    }

    fn pending_interrupt_cause(&self, bus: &Bus) -> Option<u32> {
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
        self.cop0[COP0_CAUSE] = cause;
        let status = self.cop0[COP0_STATUS];
        self.cop0[COP0_STATUS] = (status & !0x3f) | ((status << 2) & 0x3f);
        self.pc = EXCEPTION_VECTOR;
        self.next_pc = EXCEPTION_VECTOR + 4;
    }

    fn execute_bios_call(&mut self, vector: BiosCallVector) {
        match (vector, self.regs[9]) {
            (BiosCallVector::A0, BIOS_A_MALLOC) => {
                self.allocate_bios_heap(self.regs[4]);
            }
            (BiosCallVector::B0, BIOS_B_ALLOC_KERNEL_MEMORY) => {
                self.allocate_bios_heap(self.regs[4]);
            }
            _ => {}
        }

        let return_address = self.regs[31];
        self.pc = return_address;
        self.next_pc = return_address.wrapping_add(4);
    }

    fn allocate_bios_heap(&mut self, size: u32) {
        let size = align_up(size, 4);
        self.regs[2] = self.bios_heap;
        self.bios_heap = self.bios_heap.wrapping_add(size);
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

fn rs(instruction: u32) -> usize {
    ((instruction >> 21) & 0x1f) as usize
}

fn rt(instruction: u32) -> usize {
    ((instruction >> 16) & 0x1f) as usize
}

fn rd(instruction: u32) -> usize {
    ((instruction >> 11) & 0x1f) as usize
}

fn shamt(instruction: u32) -> u32 {
    (instruction >> 6) & 0x1f
}

fn immediate(instruction: u32) -> i16 {
    instruction as i16
}

fn unsigned_immediate(instruction: u32) -> u32 {
    instruction & 0xffff
}

fn branch_target(pc_after_delay_slot: u32, offset: i16) -> u32 {
    pc_after_delay_slot.wrapping_add(((offset as i32) << 2) as u32)
}

fn jump_target(pc: u32, instruction: u32) -> u32 {
    (pc & 0xf000_0000) | ((instruction & 0x03ff_ffff) << 2)
}

fn load_word_left(bus: &mut Bus, address: u32, current: u32) -> Result<u32> {
    let aligned = address & !3;
    let word = bus.read32(aligned)?;
    Ok(match address & 3 {
        0 => (current & 0x00ff_ffff) | (word << 24),
        1 => (current & 0x0000_ffff) | (word << 16),
        2 => (current & 0x0000_00ff) | (word << 8),
        _ => word,
    })
}

fn load_word_right(bus: &mut Bus, address: u32, current: u32) -> Result<u32> {
    let aligned = address & !3;
    let word = bus.read32(aligned)?;
    Ok(match address & 3 {
        0 => word,
        1 => (current & 0xff00_0000) | (word >> 8),
        2 => (current & 0xffff_0000) | (word >> 16),
        _ => (current & 0xffff_ff00) | (word >> 24),
    })
}

fn store_word_left(bus: &mut Bus, address: u32, value: u32) -> Result<()> {
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

fn store_word_right(bus: &mut Bus, address: u32, value: u32) -> Result<()> {
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

fn align_up(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
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

    // Instruction encoders
    fn nop() -> u32 {
        0
    }
    fn lui(rt: u32, imm: u32) -> u32 {
        ((PrimaryOpcode::Lui as u32) << 26) | (rt << 16) | (imm & 0xffff)
    }
    fn ori(rt: u32, rs: u32, imm: u32) -> u32 {
        ((PrimaryOpcode::Ori as u32) << 26) | (rs << 21) | (rt << 16) | (imm & 0xffff)
    }
    fn addiu(rt: u32, rs: u32, imm: i16) -> u32 {
        ((PrimaryOpcode::Addiu as u32) << 26) | (rs << 21) | (rt << 16) | (imm as u16 as u32)
    }
    fn beq(rs: u32, rt: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Beq as u32) << 26) | (rs << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn sb(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Sb as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn lb(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Lb as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
    }
    fn lbu(rt: u32, base: u32, offset: i16) -> u32 {
        ((PrimaryOpcode::Lbu as u32) << 26) | (base << 21) | (rt << 16) | (offset as u16 as u32)
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

    fn bus_with_program(words: &[u32]) -> Bus {
        let mut bus = Bus::new(Bios::from_bytes(vec![0; crate::bios::BIOS_SIZE]).unwrap());
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
        ]);

        for _ in 0..6 {
            cpu.step(&mut bus).unwrap();
        }

        assert_eq!(cpu.state().regs[9], 0xffff_ffff);
        assert_eq!(cpu.state().regs[10], 0x0000_00ff);
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
    fn syscall_is_currently_treated_as_noop() {
        let mut cpu = Cpu::new();
        cpu.set_pc(0);
        let mut bus = bus_with_program(&[SpecialOpcode::Syscall as u32]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.state().pc, 4);
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

        for _ in 0..6 {
            cpu.step(&mut bus).unwrap();
        }

        assert_eq!(cpu.state().regs[9], 0x1122_3344);
    }

    #[test]
    fn traps_empty_bios_call_vectors() {
        let mut cpu = Cpu::new();
        cpu.set_pc(BIOS_CALL_B0);
        cpu.set_reg(9, BIOS_B_ALLOC_KERNEL_MEMORY);
        cpu.set_reg(4, 6);
        cpu.set_reg(31, 0x1234);
        let mut bus = bus_with_program(&[]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.state().regs[2], BIOS_HEAP_START);
        assert_eq!(cpu.bios_heap, BIOS_HEAP_START + 8);
        assert_eq!(cpu.state().pc, 0x1234);
        assert_eq!(cpu.state().next_pc, 0x1238);
    }

    #[test]
    fn a0_malloc_uses_bios_heap() {
        let mut cpu = Cpu::new();
        cpu.set_pc(BIOS_CALL_A0);
        cpu.set_reg(9, BIOS_A_MALLOC);
        cpu.set_reg(4, 5);
        cpu.set_reg(31, 0x2000);
        let mut bus = bus_with_program(&[]);

        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.state().regs[2], BIOS_HEAP_START);
        assert_eq!(cpu.bios_heap, BIOS_HEAP_START + 8);
        assert_eq!(cpu.state().pc, 0x2000);
    }
}
