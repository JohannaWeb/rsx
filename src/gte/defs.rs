pub(crate) const GTE_REGISTER_COUNT: usize = 32;
pub(crate) const GTE_COMMAND_MASK: u32 = 0x3f;
pub(crate) const GTE_SF_BIT_SHIFT: u32 = 19;
pub(crate) const GTE_LM_BIT_SHIFT: u32 = 10;
pub(crate) const GTE_MX_SHIFT: u32 = 17;
pub(crate) const GTE_V_SHIFT: u32 = 15;
pub(crate) const GTE_CV_SHIFT: u32 = 13;
pub(crate) const GTE_MATRIX_SELECT_MASK: u32 = 0x3;
pub(crate) const GTE_VECTOR_SELECT_MASK: u32 = 0x3;
pub(crate) const GTE_CONTROL_VECTOR_SELECT_MASK: u32 = 0x3;
pub(crate) const GTE_SHIFT_FRACTIONAL: u32 = 12;
pub(crate) const GTE_SHIFT_NONE: u32 = 0;
pub(crate) const GTE_COLOR_FRACTION_SHIFT: u32 = 4;
pub(crate) const GTE_COLOR_PACK_SHIFT: u32 = 7;
pub(crate) const GTE_COLOR_GREEN_SHIFT: u32 = 8;
pub(crate) const GTE_COLOR_BLUE_SHIFT: u32 = 16;
pub(crate) const GTE_CODE_SHIFT: u32 = 24;
pub(crate) const GTE_IR2_PACK_SHIFT: u32 = 5;
pub(crate) const GTE_IR3_PACK_SHIFT: u32 = 10;
pub(crate) const GTE_HALFWORD_SHIFT: u32 = 16;
pub(crate) const GTE_UNR_INDEX_SHIFT: u32 = 7;
pub(crate) const GTE_UNR_INDEX_MASK: u32 = 0xff;
pub(crate) const GTE_UNR_FRACTION_MASK: u32 = 0x7f;
pub(crate) const GTE_UNR_INTERPOLATION_SHIFT: u32 = 7;
pub(crate) const GTE_UNR_INTERPOLATION_ROUNDING: u32 = 0x40;
pub(crate) const GTE_UNR_RECIPROCAL_BIAS: u64 = 0x100;
pub(crate) const GTE_UNR_RESULT_SHIFT: u32 = 8;
pub(crate) const GTE_UNR_OVERFLOW_RESULT: u32 = 0x1ffff;
pub(crate) const GTE_UNR_NORMALIZE_SHIFT_BASE: u32 = 16;
pub(crate) const GTE_UNR_TABLE_SIZE: usize = 257;
pub(crate) const GTE_UNR_TABLE_SENTINEL_INDEX: usize = 256;
pub(crate) const GTE_UNR_TABLE_FORMULA_NUMERATOR: usize = 0x1ff00;
pub(crate) const GTE_UNR_TABLE_FORMULA_DENOMINATOR_BASE: usize = 0x100;
pub(crate) const GTE_MAC123_MAX: i64 = 0x7fff_ffff_ffff;
pub(crate) const GTE_MAC123_MIN: i64 = -0x8000_0000_0000;
pub(crate) const GTE_MAC0_MAX: i64 = 0x7fff_ffff;
pub(crate) const GTE_MAC0_MIN: i64 = -0x8000_0000i64;
pub(crate) const GTE_IR_MIN: i32 = -0x8000;
pub(crate) const GTE_IR_MAX: i32 = 0x7fff;
pub(crate) const GTE_IR0_MIN: i32 = 0;
pub(crate) const GTE_IR0_MAX: i32 = 0x1000;
pub(crate) const GTE_SZ_MIN: i32 = 0;
pub(crate) const GTE_SZ_MAX_I32: i32 = 0xffff;
pub(crate) const GTE_SZ_MAX_U32: u32 = 0xffff;
pub(crate) const GTE_SXY_MIN: i32 = -0x400;
pub(crate) const GTE_SXY_MAX: i32 = 0x3ff;
pub(crate) const GTE_COLOR_MIN: i32 = 0;
pub(crate) const GTE_COLOR_MAX: i32 = 0xff;
pub(crate) const GTE_IRGB_MASK: u32 = 0x7fff;
pub(crate) const GTE_IRGB_CHANNEL_MASK: u32 = 0x1f;
pub(crate) const GTE_IRGB_CHANNEL_CLAMP_MAX: i16 = 0x7f80;
pub(crate) const GTE_FLAG_WRITE_MASK: u32 = 0x7fff_f000;
pub(crate) const GTE_FIXED_POINT_ONE: i64 = 0x1000;

// Data register indices
pub(crate) const RGBC: usize = 6;
pub(crate) const OTZ: usize = 7;
pub(crate) const IR0: usize = 8;
pub(crate) const IR1: usize = 9;
pub(crate) const IR2: usize = 10;
pub(crate) const IR3: usize = 11;
pub(crate) const SXY0: usize = 12;
pub(crate) const SXY1: usize = 13;
pub(crate) const SXY2: usize = 14;
pub(crate) const SXYP: usize = 15;
pub(crate) const SZ0: usize = 16;
pub(crate) const SZ1: usize = 17;
pub(crate) const SZ2: usize = 18;
pub(crate) const SZ3: usize = 19;
pub(crate) const RGB0: usize = 20;
pub(crate) const RGB1: usize = 21;
pub(crate) const RGB2: usize = 22;
pub(crate) const MAC0: usize = 24;
pub(crate) const MAC1: usize = 25;
pub(crate) const MAC2: usize = 26;
pub(crate) const MAC3: usize = 27;
pub(crate) const IRGB: usize = 28;
pub(crate) const ORGB: usize = 29;
pub(crate) const GTE_LZCS_REGISTER: usize = 30;
pub(crate) const GTE_LZCR_REGISTER: usize = 31;

// Control register indices
pub(crate) const RT11RT12: usize = 0;
pub(crate) const RT13RT21: usize = 1;
pub(crate) const RT22RT23: usize = 2;
pub(crate) const RT31RT32: usize = 3;
pub(crate) const RT33: usize = 4;
pub(crate) const TRX: usize = 5;
pub(crate) const TRY: usize = 6;
pub(crate) const TRZ: usize = 7;
pub(crate) const L11L12: usize = 8;
pub(crate) const L13L21: usize = 9;
pub(crate) const L22L23: usize = 10;
pub(crate) const L31L32: usize = 11;
pub(crate) const L33: usize = 12;
pub(crate) const RBK: usize = 13;
pub(crate) const GBK: usize = 14;
pub(crate) const BBK: usize = 15;
pub(crate) const LC11LC12: usize = 16;
pub(crate) const LC13LC21: usize = 17;
pub(crate) const LC22LC23: usize = 18;
pub(crate) const LC31LC32: usize = 19;
pub(crate) const LC33: usize = 20;
pub(crate) const RFC: usize = 21;
pub(crate) const GFC: usize = 22;
pub(crate) const BFC: usize = 23;
pub(crate) const OFX: usize = 24;
pub(crate) const OFY: usize = 25;
pub(crate) const H: usize = 26;
pub(crate) const DQA: usize = 27;
pub(crate) const DQB: usize = 28;
pub(crate) const ZSF3: usize = 29;
pub(crate) const ZSF4: usize = 30;
pub(crate) const FLAG: usize = 31;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GteCommand {
    Rtps = 0x01,
    Nclip = 0x06,
    Op = 0x0c,
    Dpcs = 0x10,
    Intpl = 0x11,
    Mvmva = 0x12,
    Ncds = 0x13,
    Cdp = 0x14,
    Ncdt = 0x16,
    Nccs = 0x1b,
    Cc = 0x1c,
    Ncs = 0x1e,
    Ncst = 0x20,
    Sqr = 0x28,
    Dcpl = 0x29,
    Dpcq = 0x2a,
    Avsz3 = 0x2d,
    Avsz4 = 0x2e,
    Rtpt = 0x30,
    Gpf = 0x3d,
    Gpl = 0x3e,
    Ncct = 0x3f,
}

#[allow(dead_code)]
pub(crate) mod flags {
    pub(crate) const MAC1_POS: u32 = 30;
    pub(crate) const MAC2_POS: u32 = 29;
    pub(crate) const MAC3_POS: u32 = 28;
    pub(crate) const MAC1_NEG: u32 = 27;
    pub(crate) const MAC2_NEG: u32 = 26;
    pub(crate) const MAC3_NEG: u32 = 25;
    pub(crate) const IR1_POS: u32 = 24;
    pub(crate) const IR1_NEG: u32 = 23;
    pub(crate) const IR2_POS: u32 = 22;
    pub(crate) const IR2_NEG: u32 = 21;
    pub(crate) const IR3_POS: u32 = 20;
    pub(crate) const IR3_NEG: u32 = 19;
    pub(crate) const SZ3_OTZ_OVERFLOW: u32 = 18;
    pub(crate) const DIVIDE_OVERFLOW: u32 = 17;
    pub(crate) const MAC0_POS: u32 = 16;
    pub(crate) const MAC0_NEG: u32 = 15;
    pub(crate) const SX2_OVERFLOW: u32 = 14;
    pub(crate) const SY2_OVERFLOW: u32 = 13;
    pub(crate) const IR0_OVERFLOW: u32 = 12;
    pub(crate) const ERROR: u32 = 31;

    // Quirk: Color overflow bits share with IR bits
    pub(crate) const COLOR_R: u32 = IR2_NEG;
    pub(crate) const COLOR_G: u32 = IR3_POS;
    pub(crate) const COLOR_B: u32 = IR3_NEG;
}

pub(crate) const FLAG_ERROR_MASK: u32 = 0x7f87_e000;

// Reciprocal table for UNR division (256 entries + 1 sentinel)
// The GTE uses this table to find an initial 8-bit guess for 1/SZ.
// The table is derived from the formula: f(i) = floor(0x1FF00 / (0x100 + i)) - 0x100
pub(crate) const UNR_TABLE: [u8; GTE_UNR_TABLE_SIZE] = {
    let mut table = [0u8; GTE_UNR_TABLE_SIZE];
    let mut i = 0;
    while i < GTE_UNR_TABLE_SENTINEL_INDEX {
        let val = (GTE_UNR_TABLE_FORMULA_NUMERATOR / (GTE_UNR_TABLE_FORMULA_DENOMINATOR_BASE + i))
            - GTE_UNR_TABLE_FORMULA_DENOMINATOR_BASE;
        table[i] = val as u8;
        i += 1;
    }
    table[GTE_UNR_TABLE_SENTINEL_INDEX] = 0;
    table
};
