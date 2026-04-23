#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BiosCallVector {
    A0,
    B0,
    C0,
}

impl BiosCallVector {
    pub fn decode(pc: u32) -> Option<Self> {
        match pc {
            0x0000_00a0 => Some(Self::A0),
            0x0000_00b0 => Some(Self::B0),
            0x0000_00c0 => Some(Self::C0),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::A0 => "A0",
            Self::B0 => "B0",
            Self::C0 => "C0",
        }
    }
}
