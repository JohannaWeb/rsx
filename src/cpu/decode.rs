#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryOpcode {
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
    Cop2 = 0x12,
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
    Lwc2 = 0x32,
    Swc2 = 0x3a,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialOpcode {
    Sll = 0x00,
    Srl = 0x02,
    Sra = 0x03,
    Sllv = 0x04,
    Srlv = 0x06,
    Srav = 0x07,
    Jr = 0x08,
    Jalr = 0x09,
    Break = 0x0d,
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
pub enum RegImmOpcode {
    Bltz = 0x00,
    Bgez = 0x01,
    Bltzal = 0x10,
    Bgezal = 0x11,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cop0RsOpcode {
    Mfc0 = 0x00,
    Mtc0 = 0x04,
    Co = 0x10,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cop0FunctionOpcode {
    Rfe = 0x10,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cop2RsOpcode {
    Mfc2 = 0x00,
    Cfc2 = 0x02,
    Mtc2 = 0x04,
    Ctc2 = 0x06,
}

impl Cop2RsOpcode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::Mfc2),
            0x02 => Some(Self::Cfc2),
            0x04 => Some(Self::Mtc2),
            0x06 => Some(Self::Ctc2),
            _ => None,
        }
    }
}
