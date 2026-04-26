use super::COP0_STATUS;
use crate::bus::Bus as CpuBusAccess;
use crate::cpu::{
    Cop0FunctionOpcode, Cop0RsOpcode, Cop2RsOpcode, Cpu, PrimaryOpcode, RegImmOpcode, SpecialOpcode,
};
use crate::cpu::{branch_target, imm, jump_target, rd, rs, rt, shamt, unsigned_imm};
use crate::cpu::{load_word_left, load_word_right, store_word_left, store_word_right};
use crate::error::Error;
use crate::error::Result;

pub type InstructionHandler = fn(&mut Cpu, u32, u32, &mut CpuBusAccess) -> Result<()>;

pub const PRIMARY_OPCODE_TABLE: [InstructionHandler; 64] = {
    let mut table = [Cpu::op_unsupported as InstructionHandler; 64];
    table[PrimaryOpcode::Special as usize] = Cpu::op_special;
    table[PrimaryOpcode::RegImm as usize] = Cpu::op_regimm;
    table[PrimaryOpcode::J as usize] = Cpu::op_j;
    table[PrimaryOpcode::Jal as usize] = Cpu::op_jal;
    table[PrimaryOpcode::Beq as usize] = Cpu::op_beq;
    table[PrimaryOpcode::Bne as usize] = Cpu::op_bne;
    table[PrimaryOpcode::Blez as usize] = Cpu::op_blez;
    table[PrimaryOpcode::Bgtz as usize] = Cpu::op_bgtz;
    table[PrimaryOpcode::Beql as usize] = Cpu::op_beql;
    table[PrimaryOpcode::Bnel as usize] = Cpu::op_bnel;
    table[PrimaryOpcode::Blezl as usize] = Cpu::op_blezl;
    table[PrimaryOpcode::Bgtzl as usize] = Cpu::op_bgtzl;
    table[PrimaryOpcode::Addi as usize] = Cpu::op_addi;
    table[PrimaryOpcode::Addiu as usize] = Cpu::op_addiu;
    table[PrimaryOpcode::Slti as usize] = Cpu::op_slti;
    table[PrimaryOpcode::Sltiu as usize] = Cpu::op_sltiu;
    table[PrimaryOpcode::Andi as usize] = Cpu::op_andi;
    table[PrimaryOpcode::Ori as usize] = Cpu::op_ori;
    table[PrimaryOpcode::Xori as usize] = Cpu::op_xori;
    table[PrimaryOpcode::Lui as usize] = Cpu::op_lui;
    table[PrimaryOpcode::Cop0 as usize] = Cpu::op_cop0;
    table[0x11] = Cpu::op_cop1;
    table[PrimaryOpcode::Cop2 as usize] = Cpu::op_cop2;
    table[0x13] = Cpu::op_cop3;
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
    table[PrimaryOpcode::Lwc2 as usize] = Cpu::op_lwc2;
    table[PrimaryOpcode::Swc2 as usize] = Cpu::op_swc2;
    table
};

pub const SPECIAL_FUNCTION_TABLE: [InstructionHandler; 64] = {
    let mut table = [Cpu::op_unsupported as InstructionHandler; 64];
    table[SpecialOpcode::Sll as usize] = Cpu::special_sll;
    table[SpecialOpcode::Srl as usize] = Cpu::special_srl;
    table[SpecialOpcode::Sra as usize] = Cpu::special_sra;
    table[SpecialOpcode::Sllv as usize] = Cpu::special_sllv;
    table[SpecialOpcode::Srlv as usize] = Cpu::special_srlv;
    table[SpecialOpcode::Srav as usize] = Cpu::special_srav;
    table[SpecialOpcode::Jr as usize] = Cpu::special_jr;
    table[SpecialOpcode::Jalr as usize] = Cpu::special_jalr;
    table[SpecialOpcode::Break as usize] = Cpu::special_break;
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
    table[SpecialOpcode::Addu as usize] = Cpu::special_addu;
    table[SpecialOpcode::Sub as usize] = Cpu::special_sub;
    table[SpecialOpcode::Subu as usize] = Cpu::special_subu;
    table[SpecialOpcode::And as usize] = Cpu::special_and;
    table[SpecialOpcode::Or as usize] = Cpu::special_or;
    table[SpecialOpcode::Xor as usize] = Cpu::special_xor;
    table[SpecialOpcode::Nor as usize] = Cpu::special_nor;
    table[SpecialOpcode::Slt as usize] = Cpu::special_slt;
    table[SpecialOpcode::Sltu as usize] = Cpu::special_sltu;
    table
};

pub const REGIMM_FUNCTION_TABLE: [InstructionHandler; 32] = {
    let mut table = [Cpu::op_unsupported as InstructionHandler; 32];
    table[RegImmOpcode::Bltz as usize] = Cpu::regimm_bltz;
    table[RegImmOpcode::Bgez as usize] = Cpu::regimm_bgez;
    table[RegImmOpcode::Bltzal as usize] = Cpu::regimm_bltzal;
    table[RegImmOpcode::Bgezal as usize] = Cpu::regimm_bgezal;
    table
};

pub const COP0_FUNCTION_TABLE: [InstructionHandler; 32] = {
    let mut table = [Cpu::op_unsupported as InstructionHandler; 32];
    table[Cop0RsOpcode::Mfc0 as usize] = Cpu::cop0_mfc0;
    table[Cop0RsOpcode::Mtc0 as usize] = Cpu::cop0_mtc0;
    table[Cop0RsOpcode::Co as usize] = Cpu::cop0_co;
    table
};

impl Cpu {
    pub(super) fn execute(
        &mut self,
        pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let opcode = ((instruction >> 26) & 0x3f) as usize;
        PRIMARY_OPCODE_TABLE[opcode](self, pc, instruction, bus)
    }

    pub(super) fn execute_special(
        &mut self,
        pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let function = (instruction & 0x3f) as usize;
        SPECIAL_FUNCTION_TABLE[function](self, pc, instruction, bus)
    }

    pub(super) fn execute_regimm(
        &mut self,
        pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        REGIMM_FUNCTION_TABLE[rt(instruction)](self, pc, instruction, bus)
    }

    pub(super) fn execute_cop0(
        &mut self,
        pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        COP0_FUNCTION_TABLE[rs(instruction)](self, pc, instruction, bus)
    }

    pub(super) fn op_unsupported(
        &mut self,
        pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        log::error!(
            "unsupported instruction {instruction:#010x} at pc={pc:#010x} next_pc={:#010x} ra={:#010x} sp={:#010x}: {}",
            self.next_pc,
            self.regs[31],
            self.regs[29],
            self.disassemble(instruction)
        );
        Err(crate::error::Error::UnsupportedInstruction { pc, instruction })
    }

    pub(super) fn op_special(
        &mut self,
        pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.execute_special(pc, instruction, bus)
    }

    pub(super) fn op_regimm(
        &mut self,
        pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.execute_regimm(pc, instruction, bus)
    }

    pub(super) fn op_cop0(
        &mut self,
        pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.execute_cop0(pc, instruction, bus)
    }

    pub(super) fn op_cop2(
        &mut self,
        pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let rs_val = ((instruction >> 21) & 0x1f) as u8;
        let rt = ((instruction >> 16) & 0x1f) as usize;
        let rd = ((instruction >> 11) & 0x1f) as usize;
        if instruction & (1 << 25) != 0 {
            self.gte.execute(instruction);
        } else if let Some(opcode) = Cop2RsOpcode::from_u8(rs_val) {
            match opcode {
                Cop2RsOpcode::Mfc2 => self.stage_load(rt, self.gte.read_data(rd)),
                Cop2RsOpcode::Cfc2 => self.stage_load(rt, self.gte.read_ctrl(rd)),
                Cop2RsOpcode::Mtc2 => self.gte.write_data(rd, self.reg(rt)),
                Cop2RsOpcode::Ctc2 => self.gte.write_ctrl(rd, self.reg(rt)),
            }
        } else {
            log::warn!(
                "unrecognized COP2 rs={rs_val:#04x} instr={instruction:#010x} at pc={pc:#010x}"
            );
        }
        Ok(())
    }

    pub(super) fn op_cop1(
        &mut self,
        pc: u32,
        _instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.enter_exception(pc, 0x0b);
        Ok(())
    }

    pub(super) fn op_cop3(
        &mut self,
        pc: u32,
        _instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.enter_exception(pc, 0x0b);
        Ok(())
    }

    pub(super) fn op_lwc2(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let base = ((instruction >> 21) & 0x1f) as usize;
        let rt = ((instruction >> 16) & 0x1f) as usize;
        let address = self.reg(base).wrapping_add(instruction as i16 as u32);
        match bus.read32(address) {
            Ok(value) => self.gte.write_data(rt, value),
            Err(Error::UnalignedAccess { .. }) => self.enter_exception(self.pc, 0x04),
            Err(err) => return Err(err),
        }
        Ok(())
    }

    pub(super) fn op_swc2(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let base = ((instruction >> 21) & 0x1f) as usize;
        let rt = ((instruction >> 16) & 0x1f) as usize;
        let address = self.reg(base).wrapping_add(instruction as i16 as u32);
        match bus.write32(address, self.gte.read_data(rt)) {
            Ok(()) => Ok(()),
            Err(Error::UnalignedAccess { .. }) => {
                self.enter_exception(self.pc, 0x05);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    pub(super) fn op_j(
        &mut self,
        pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.next_pc = jump_target(pc, instruction);
        Ok(())
    }

    pub(super) fn op_jal(
        &mut self,
        pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.set_reg(31, self.next_pc);
        self.next_pc = jump_target(pc, instruction);
        Ok(())
    }

    pub(super) fn op_beq(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if self.reg(rs(instruction)) == self.reg(rt(instruction)) {
            self.next_pc = branch_target(self.pc, imm(instruction));
        }
        Ok(())
    }

    pub(super) fn op_bne(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if self.reg(rs(instruction)) != self.reg(rt(instruction)) {
            self.next_pc = branch_target(self.pc, imm(instruction));
        }
        Ok(())
    }

    pub(super) fn op_blez(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if (self.reg(rs(instruction)) as i32) <= 0 {
            self.next_pc = branch_target(self.pc, imm(instruction));
        }
        Ok(())
    }

    pub(super) fn op_bgtz(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if (self.reg(rs(instruction)) as i32) > 0 {
            self.next_pc = branch_target(self.pc, imm(instruction));
        }
        Ok(())
    }

    fn skip_branch_delay_slot(&mut self) {
        self.pc = self.next_pc;
        self.next_pc = self.next_pc.wrapping_add(4);
    }

    pub(super) fn op_beql(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if self.reg(rs(instruction)) == self.reg(rt(instruction)) {
            self.next_pc = branch_target(self.pc, imm(instruction));
        } else {
            self.skip_branch_delay_slot();
        }
        Ok(())
    }

    pub(super) fn op_bnel(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if self.reg(rs(instruction)) != self.reg(rt(instruction)) {
            self.next_pc = branch_target(self.pc, imm(instruction));
        } else {
            self.skip_branch_delay_slot();
        }
        Ok(())
    }

    pub(super) fn op_blezl(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if (self.reg(rs(instruction)) as i32) <= 0 {
            self.next_pc = branch_target(self.pc, imm(instruction));
        } else {
            self.skip_branch_delay_slot();
        }
        Ok(())
    }

    pub(super) fn op_bgtzl(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if (self.reg(rs(instruction)) as i32) > 0 {
            self.next_pc = branch_target(self.pc, imm(instruction));
        } else {
            self.skip_branch_delay_slot();
        }
        Ok(())
    }

    pub(super) fn op_addi(
        &mut self,
        pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let lhs = self.reg(rs(instruction)) as i32;
        let rhs = imm(instruction) as i32;
        if let Some(value) = lhs.checked_add(rhs) {
            self.set_reg(rt(instruction), value as u32);
        } else {
            self.enter_exception(pc, 0x0c);
        }
        Ok(())
    }

    pub(super) fn op_addiu(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        self.set_reg(rt(instruction), value);
        Ok(())
    }

    pub(super) fn op_slti(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = (self.reg(rs(instruction)) as i32) < (imm(instruction) as i32);
        self.set_reg(rt(instruction), value as u32);
        Ok(())
    }

    pub(super) fn op_sltiu(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rs(instruction)) < (imm(instruction) as u32);
        self.set_reg(rt(instruction), value as u32);
        Ok(())
    }

    pub(super) fn op_andi(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rs(instruction)) & unsigned_imm(instruction);
        self.set_reg(rt(instruction), value);
        Ok(())
    }

    pub(super) fn op_ori(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rs(instruction)) | unsigned_imm(instruction);
        self.set_reg(rt(instruction), value);
        Ok(())
    }

    pub(super) fn op_xori(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rs(instruction)) ^ unsigned_imm(instruction);
        self.set_reg(rt(instruction), value);
        Ok(())
    }

    pub(super) fn op_lui(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.set_reg(rt(instruction), unsigned_imm(instruction) << 16);
        Ok(())
    }

    pub(super) fn op_lb(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        self.stage_load(rt(instruction), bus.read8(address)? as i8 as i32 as u32);
        Ok(())
    }

    pub(super) fn op_lh(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        self.stage_load(rt(instruction), bus.read16(address)? as i16 as i32 as u32);
        Ok(())
    }

    pub(super) fn op_lwl(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        let value = load_word_left(bus, address, self.reg(rt(instruction)))?;
        self.stage_load(rt(instruction), value);
        Ok(())
    }

    pub(super) fn op_lw(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        match bus.read32(address) {
            Ok(value) => self.stage_load(rt(instruction), value),
            Err(Error::UnalignedAccess { .. }) => self.enter_exception(self.pc, 0x04),
            Err(err) => return Err(err),
        }
        Ok(())
    }

    pub(super) fn op_lbu(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        self.stage_load(rt(instruction), bus.read8(address)? as u32);
        Ok(())
    }

    pub(super) fn op_lhu(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        self.stage_load(rt(instruction), bus.read16(address)? as u32);
        Ok(())
    }

    pub(super) fn op_lwr(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        let value = load_word_right(bus, address, self.reg(rt(instruction)))?;
        self.stage_load(rt(instruction), value);
        Ok(())
    }

    pub(super) fn op_sb(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        bus.write8(address, self.reg(rt(instruction)) as u8)
    }

    pub(super) fn op_sh(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        bus.write16(address, self.reg(rt(instruction)) as u16)
    }

    pub(super) fn op_swl(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        store_word_left(bus, address, self.reg(rt(instruction)))
    }

    pub(super) fn op_sw(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        match bus.write32(address, self.reg(rt(instruction))) {
            Ok(()) => Ok(()),
            Err(Error::UnalignedAccess { .. }) => {
                self.enter_exception(self.pc, 0x05);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    pub(super) fn op_swr(
        &mut self,
        _pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let address = self
            .reg(rs(instruction))
            .wrapping_add(imm(instruction) as u32);
        store_word_right(bus, address, self.reg(rt(instruction)))
    }

    pub(super) fn special_sll(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rt(instruction)) << shamt(instruction);
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_srl(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rt(instruction)) >> shamt(instruction);
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_sra(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = ((self.reg(rt(instruction)) as i32) >> shamt(instruction)) as u32;
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_sllv(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rt(instruction)) << (self.reg(rs(instruction)) & 0x1f);
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_srlv(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rt(instruction)) >> (self.reg(rs(instruction)) & 0x1f);
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_srav(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value =
            ((self.reg(rt(instruction)) as i32) >> (self.reg(rs(instruction)) & 0x1f)) as u32;
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_jr(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.next_pc = self.reg(rs(instruction));
        Ok(())
    }

    pub(super) fn special_jalr(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let link = self.next_pc;
        self.next_pc = self.reg(rs(instruction));
        self.set_reg(rd(instruction), link);
        Ok(())
    }

    pub(super) fn special_break(
        &mut self,
        pc: u32,
        _instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        log::warn!("BREAK at pc={pc:#010x} ra={:#010x}", self.regs[31]);
        self.enter_exception(pc, 0x09);
        Ok(())
    }

    pub(super) fn special_syscall(
        &mut self,
        pc: u32,
        _instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.enter_exception(pc, 0x08);
        Ok(())
    }

    pub(super) fn special_mfhi(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.set_reg(rd(instruction), self.hi);
        Ok(())
    }

    pub(super) fn special_mthi(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.hi = self.reg(rs(instruction));
        Ok(())
    }

    pub(super) fn special_mflo(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.set_reg(rd(instruction), self.lo);
        Ok(())
    }

    pub(super) fn special_mtlo(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.lo = self.reg(rs(instruction));
        Ok(())
    }

    pub(super) fn special_mult(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = (self.reg(rs(instruction)) as i32 as i64)
            .wrapping_mul(self.reg(rt(instruction)) as i32 as i64);
        self.lo = value as u32;
        self.hi = (value >> 32) as u32;
        Ok(())
    }

    pub(super) fn special_multu(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value =
            (self.reg(rs(instruction)) as u64).wrapping_mul(self.reg(rt(instruction)) as u64);
        self.lo = value as u32;
        self.hi = (value >> 32) as u32;
        Ok(())
    }

    pub(super) fn special_div(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let dividend = self.reg(rs(instruction)) as i32;
        let divisor = self.reg(rt(instruction)) as i32;
        if divisor == 0 {
            self.lo = if dividend < 0 { 1 } else { u32::MAX };
            self.hi = dividend as u32;
        } else {
            self.lo = dividend.wrapping_div(divisor) as u32;
            self.hi = dividend.wrapping_rem(divisor) as u32;
        }
        Ok(())
    }

    pub(super) fn special_divu(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let dividend = self.reg(rs(instruction));
        let divisor = self.reg(rt(instruction));
        if divisor == 0 {
            self.lo = u32::MAX;
            self.hi = dividend;
        } else {
            self.lo = dividend / divisor;
            self.hi = dividend % divisor;
        }
        Ok(())
    }

    pub(super) fn special_add(
        &mut self,
        pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let lhs = self.reg(rs(instruction)) as i32;
        let rhs = self.reg(rt(instruction)) as i32;
        if let Some(value) = lhs.checked_add(rhs) {
            self.set_reg(rd(instruction), value as u32);
        } else {
            self.enter_exception(pc, 0x0c);
        }
        Ok(())
    }

    pub(super) fn special_addu(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self
            .reg(rs(instruction))
            .wrapping_add(self.reg(rt(instruction)));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_sub(
        &mut self,
        pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let lhs = self.reg(rs(instruction)) as i32;
        let rhs = self.reg(rt(instruction)) as i32;
        if let Some(value) = lhs.checked_sub(rhs) {
            self.set_reg(rd(instruction), value as u32);
        } else {
            self.enter_exception(pc, 0x0c);
        }
        Ok(())
    }

    pub(super) fn special_subu(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self
            .reg(rs(instruction))
            .wrapping_sub(self.reg(rt(instruction)));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_and(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rs(instruction)) & self.reg(rt(instruction));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_or(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rs(instruction)) | self.reg(rt(instruction));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_xor(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rs(instruction)) ^ self.reg(rt(instruction));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_nor(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = !(self.reg(rs(instruction)) | self.reg(rt(instruction)));
        self.set_reg(rd(instruction), value);
        Ok(())
    }

    pub(super) fn special_slt(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = (self.reg(rs(instruction)) as i32) < (self.reg(rt(instruction)) as i32);
        self.set_reg(rd(instruction), value as u32);
        Ok(())
    }

    pub(super) fn special_sltu(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        let value = self.reg(rs(instruction)) < self.reg(rt(instruction));
        self.set_reg(rd(instruction), value as u32);
        Ok(())
    }

    pub(super) fn regimm_bltz(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if (self.reg(rs(instruction)) as i32) < 0 {
            self.next_pc = branch_target(self.pc, imm(instruction));
        }
        Ok(())
    }

    pub(super) fn regimm_bgez(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if (self.reg(rs(instruction)) as i32) >= 0 {
            self.next_pc = branch_target(self.pc, imm(instruction));
        }
        Ok(())
    }

    pub(super) fn regimm_bltzal(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.set_reg(31, self.next_pc);
        if (self.reg(rs(instruction)) as i32) < 0 {
            self.next_pc = branch_target(self.pc, imm(instruction));
        }
        Ok(())
    }

    pub(super) fn regimm_bgezal(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.set_reg(31, self.next_pc);
        if (self.reg(rs(instruction)) as i32) >= 0 {
            self.next_pc = branch_target(self.pc, imm(instruction));
        }
        Ok(())
    }

    pub(super) fn cop0_mfc0(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.stage_load(rt(instruction), self.cop0[rd(instruction)]);
        Ok(())
    }

    pub(super) fn cop0_mtc0(
        &mut self,
        _pc: u32,
        instruction: u32,
        _bus: &mut CpuBusAccess,
    ) -> Result<()> {
        self.cop0[rd(instruction)] = self.reg(rt(instruction));
        Ok(())
    }

    pub(super) fn cop0_co(
        &mut self,
        pc: u32,
        instruction: u32,
        bus: &mut CpuBusAccess,
    ) -> Result<()> {
        if (instruction & 0x3f) == Cop0FunctionOpcode::Rfe as u32 {
            const COP0_STATUS_EXCEPTION_STACK_MASK: u32 = 0x3f;
            let status = self.cop0[COP0_STATUS];
            self.cop0[COP0_STATUS] = (status & !COP0_STATUS_EXCEPTION_STACK_MASK)
                | ((status >> 2) & (COP0_STATUS_EXCEPTION_STACK_MASK >> 2));
            Ok(())
        } else {
            self.op_unsupported(pc, instruction, bus)
        }
    }
}
