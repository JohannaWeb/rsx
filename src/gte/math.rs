use super::*;

impl Gte {
    // ===== Helpers =====

    pub(super) fn vxy(&self, idx: usize) -> (i32, i32) {
        let v = self.data[idx * 2];
        (v as i16 as i32, (v >> GTE_HALFWORD_SHIFT) as i16 as i32)
    }

    pub(super) fn vz(&self, idx: usize) -> i32 {
        self.data[idx * 2 + 1] as i16 as i32
    }

    pub(super) fn mx16(&self, base_reg: usize, which: usize) -> i32 {
        let v = self.ctrl[base_reg + which / 2];
        if which & 1 == 0 {
            v as i16 as i32
        } else {
            (v >> GTE_HALFWORD_SHIFT) as i16 as i32
        }
    }

    pub(super) fn tr32(&self, reg: usize) -> i64 {
        self.ctrl[reg] as i32 as i64
    }

    pub(super) fn irv(&self, r: usize) -> i32 {
        self.data[r] as i16 as i32
    }
    pub(super) fn macv(&self, r: usize) -> i32 {
        self.data[r] as i32
    }

    pub(super) fn set_mac1(&mut self, v: i64, sf: u32) {
        if v > GTE_MAC123_MAX {
            self.ctrl[FLAG] |= 1 << flags::MAC1_POS;
        }
        if v < GTE_MAC123_MIN {
            self.ctrl[FLAG] |= 1 << flags::MAC1_NEG;
        }
        self.data[MAC1] = ((v >> sf) as i32) as u32;
    }
    pub(super) fn set_mac2(&mut self, v: i64, sf: u32) {
        if v > GTE_MAC123_MAX {
            self.ctrl[FLAG] |= 1 << flags::MAC2_POS;
        }
        if v < GTE_MAC123_MIN {
            self.ctrl[FLAG] |= 1 << flags::MAC2_NEG;
        }
        self.data[MAC2] = ((v >> sf) as i32) as u32;
    }
    pub(super) fn set_mac3(&mut self, v: i64, sf: u32) {
        if v > GTE_MAC123_MAX {
            self.ctrl[FLAG] |= 1 << flags::MAC3_POS;
        }
        if v < GTE_MAC123_MIN {
            self.ctrl[FLAG] |= 1 << flags::MAC3_NEG;
        }
        self.data[MAC3] = ((v >> sf) as i32) as u32;
    }
    pub(super) fn set_mac0(&mut self, v: i64) {
        if v > GTE_MAC0_MAX {
            self.ctrl[FLAG] |= 1 << flags::MAC0_POS;
        }
        if v < GTE_MAC0_MIN {
            self.ctrl[FLAG] |= 1 << flags::MAC0_NEG;
        }
        self.data[MAC0] = (v as i32) as u32;
    }

    pub(super) fn set_ir1(&mut self, v: i32, lm: bool) {
        self.data[IR1] = self.ir_clamp(v, lm, flags::IR1_POS, flags::IR1_NEG) as u32;
    }
    pub(super) fn set_ir2(&mut self, v: i32, lm: bool) {
        self.data[IR2] = self.ir_clamp(v, lm, flags::IR2_POS, flags::IR2_NEG) as u32;
    }
    pub(super) fn set_ir3(&mut self, v: i32, lm: bool) {
        self.data[IR3] = self.ir_clamp(v, lm, flags::IR3_POS, flags::IR3_NEG) as u32;
    }

    pub(super) fn ir_clamp(&mut self, v: i32, lm: bool, pos_bit: u32, neg_bit: u32) -> i32 {
        let min = if lm { GTE_IR0_MIN } else { GTE_IR_MIN };
        if v > GTE_IR_MAX {
            self.ctrl[FLAG] |= 1 << pos_bit;
            GTE_IR_MAX
        } else if v < min {
            self.ctrl[FLAG] |= 1 << neg_bit;
            min
        } else {
            v
        }
    }

    pub(super) fn set_ir0(&mut self, v: i32) {
        if v > GTE_IR0_MAX {
            self.ctrl[FLAG] |= 1 << flags::IR0_OVERFLOW;
            self.data[IR0] = GTE_IR0_MAX as u32;
        } else if v < 0 {
            self.ctrl[FLAG] |= 1 << flags::IR0_OVERFLOW;
            self.data[IR0] = GTE_IR0_MIN as u32;
        } else {
            self.data[IR0] = v as u32;
        }
    }

    pub(super) fn push_sz(&mut self, v: i32) {
        self.data[SZ0] = self.data[SZ1];
        self.data[SZ1] = self.data[SZ2];
        self.data[SZ2] = self.data[SZ3];
        if v < GTE_SZ_MIN {
            self.ctrl[FLAG] |= 1 << flags::SZ3_OTZ_OVERFLOW;
            self.data[SZ3] = GTE_SZ_MIN as u32;
        } else if v > GTE_SZ_MAX_I32 {
            self.ctrl[FLAG] |= 1 << flags::SZ3_OTZ_OVERFLOW;
            self.data[SZ3] = GTE_SZ_MAX_U32;
        } else {
            self.data[SZ3] = v as u32;
        }
    }

    pub(super) fn push_sxy(&mut self, x: i32, y: i32) {
        self.data[SXY0] = self.data[SXY1];
        self.data[SXY1] = self.data[SXY2];
        let cx = x.clamp(GTE_SXY_MIN, GTE_SXY_MAX);
        let cy = y.clamp(GTE_SXY_MIN, GTE_SXY_MAX);
        if cx != x {
            self.ctrl[FLAG] |= 1 << flags::SX2_OVERFLOW;
        }
        if cy != y {
            self.ctrl[FLAG] |= 1 << flags::SY2_OVERFLOW;
        }
        self.data[SXY2] =
            (cx as i16 as u16 as u32) | ((cy as i16 as u16 as u32) << GTE_HALFWORD_SHIFT);
    }

    pub(super) fn push_rgb(&mut self, r: i32, g: i32, b: i32, code: u8) {
        self.data[RGB0] = self.data[RGB1];
        self.data[RGB1] = self.data[RGB2];
        let rc = if r < GTE_COLOR_MIN {
            self.ctrl[FLAG] |= 1 << flags::COLOR_R;
            GTE_COLOR_MIN as u32
        } else if r > GTE_COLOR_MAX {
            self.ctrl[FLAG] |= 1 << flags::COLOR_R;
            GTE_COLOR_MAX as u32
        } else {
            r as u32
        };
        let gc = if g < GTE_COLOR_MIN {
            self.ctrl[FLAG] |= 1 << flags::COLOR_G;
            GTE_COLOR_MIN as u32
        } else if g > GTE_COLOR_MAX {
            self.ctrl[FLAG] |= 1 << flags::COLOR_G;
            GTE_COLOR_MAX as u32
        } else {
            g as u32
        };
        let bc = if b < GTE_COLOR_MIN {
            self.ctrl[FLAG] |= 1 << flags::COLOR_B;
            GTE_COLOR_MIN as u32
        } else if b > GTE_COLOR_MAX {
            self.ctrl[FLAG] |= 1 << flags::COLOR_B;
            GTE_COLOR_MAX as u32
        } else {
            b as u32
        };
        self.data[RGB2] = rc
            | (gc << GTE_COLOR_GREEN_SHIFT)
            | (bc << GTE_COLOR_BLUE_SHIFT)
            | ((code as u32) << GTE_CODE_SHIFT);
    }

    /// UNR (Universal Reciprocal) division using hardware-accurate linear interpolation.
    /// The GTE uses this for perspective projection (calculating 1/SZ).
    /// It first normalizes the divisor and looks up an initial guess in a table,
    /// then performs linear interpolation to reach the final reciprocal.
    pub(super) fn unr_divide(&mut self, lhs: u32, rhs: u32) -> u32 {
        if rhs == 0 || (lhs as u64 >= (rhs as u64) * 2) {
            self.ctrl[FLAG] |= 1 << flags::DIVIDE_OVERFLOW;
            return GTE_UNR_OVERFLOW_RESULT;
        }

        // Normalize divisor to 0x8000-0xFFFF range
        let lz = rhs.leading_zeros();
        let shift = lz.saturating_sub(GTE_UNR_NORMALIZE_SHIFT_BASE);
        let n_h = (rhs << shift) as u32;

        // Index into the UNR table using bits 7-14 of normalized value
        // The index formula is (n_h - 0x7FC0) >> 7, but since bit 15 is always 1,
        // it simplifies to ((n_h >> 7) & 0xFF) for indices 0-255.
        // We look up two consecutive values for interpolation.
        let index = ((n_h >> GTE_UNR_INDEX_SHIFT) & GTE_UNR_INDEX_MASK) as usize;
        let a = UNR_TABLE[index] as u32;
        let b = UNR_TABLE[index + 1] as u32;

        // Linear interpolation: a - (a - b) * (fractional_part / 128)
        let fraction = n_h & GTE_UNR_FRACTION_MASK;
        let mut reciprocal = ((a << GTE_UNR_INTERPOLATION_SHIFT)
            - (a.wrapping_sub(b)).wrapping_mul(fraction)
            + GTE_UNR_INTERPOLATION_ROUNDING)
            >> GTE_UNR_INTERPOLATION_SHIFT;

        // Rescale based on the initial normalization
        reciprocal >>= shift;

        // Apply dividend (H) and return result
        let result =
            (lhs as u64 * (reciprocal as u64 + GTE_UNR_RECIPROCAL_BIAS)) >> GTE_UNR_RESULT_SHIFT;
        result.min(GTE_UNR_OVERFLOW_RESULT as u64) as u32
    }

    /// Performs perspective transformation on calculated IR registers.
    /// Formulas:
    ///   SX = (IR1 * (H / SZ) + OFX)
    ///   SY = (IR2 * (H / SZ) + OFY)
    ///   MAC0 = (DQA * (H / SZ) + DQB)
    /// Where H is the projection plane distance, OFX/OFY are screen offsets,
    /// and DQA/DQB are depth-cueing parameters.
    pub(super) fn perspective_div(&mut self, sf: u32, lm: bool) {
        let sz3 = self.data[SZ3];
        let h_val = self.ctrl[H] as u16 as u32;
        let div = self.unr_divide(h_val, sz3);

        let ofx = self.ctrl[OFX] as i32 as i64;
        let ofy = self.ctrl[OFY] as i32 as i64;
        let dqa = self.ctrl[DQA] as i16 as i64;
        let dqb = self.ctrl[DQB] as i32 as i64;

        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;

        let sx = (ir1 * div as i64 + ofx) >> GTE_HALFWORD_SHIFT;
        let sy = (ir2 * div as i64 + ofy) >> GTE_HALFWORD_SHIFT;
        self.push_sxy(sx as i32, sy as i32);

        let mac0_v = dqa * div as i64 + dqb;
        self.set_mac0(mac0_v >> GTE_SHIFT_FRACTIONAL);
        self.set_ir0((mac0_v >> GTE_SHIFT_FRACTIONAL) as i32);
        let _ = (sf, lm);
    }

    pub(super) fn rt_mul(&self, vx: i32, vy: i32, vz: i32) -> (i64, i64, i64) {
        let r = |b, i| self.mx16(b, i) as i64;
        let m1 = self.tr32(TRX) * GTE_FIXED_POINT_ONE
            + r(RT11RT12, 0) * vx as i64
            + r(RT11RT12, 1) * vy as i64
            + r(RT13RT21, 0) * vz as i64;
        let m2 = self.tr32(TRY) * GTE_FIXED_POINT_ONE
            + r(RT13RT21, 1) * vx as i64
            + r(RT22RT23, 0) * vy as i64
            + r(RT22RT23, 1) * vz as i64;
        let m3 = self.tr32(TRZ) * GTE_FIXED_POINT_ONE
            + r(RT31RT32, 0) * vx as i64
            + r(RT31RT32, 1) * vy as i64
            + (self.ctrl[RT33] as i16 as i64) * vz as i64;
        (m1, m2, m3)
    }

    pub(super) fn ll_mul(&self, vx: i32, vy: i32, vz: i32) -> (i64, i64, i64) {
        let r = |b, i| self.mx16(b, i) as i64;
        let m1 = r(L11L12, 0) * vx as i64 + r(L11L12, 1) * vy as i64 + r(L13L21, 0) * vz as i64;
        let m2 = r(L13L21, 1) * vx as i64 + r(L22L23, 0) * vy as i64 + r(L22L23, 1) * vz as i64;
        let m3 = r(L31L32, 0) * vx as i64
            + r(L31L32, 1) * vy as i64
            + (self.ctrl[L33] as i16 as i64) * vz as i64;
        (m1, m2, m3)
    }

    pub(super) fn bk_lc_ir(&self) -> (i64, i64, i64) {
        let r = |b, i| self.mx16(b, i) as i64;
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        let n1 = self.tr32(RBK) * GTE_FIXED_POINT_ONE
            + r(LC11LC12, 0) * ir1
            + r(LC11LC12, 1) * ir2
            + r(LC13LC21, 0) * ir3;
        let n2 = self.tr32(GBK) * GTE_FIXED_POINT_ONE
            + r(LC13LC21, 1) * ir1
            + r(LC22LC23, 0) * ir2
            + r(LC22LC23, 1) * ir3;
        let n3 = self.tr32(BBK) * GTE_FIXED_POINT_ONE
            + r(LC31LC32, 0) * ir1
            + r(LC31LC32, 1) * ir2
            + (self.ctrl[LC33] as i16 as i64) * ir3;
        (n1, n2, n3)
    }

    pub(super) fn rgb_ir(&self) -> (i64, i64, i64, u8) {
        let r = (self.data[RGBC] & GTE_COLOR_MAX as u32) as i64;
        let g = ((self.data[RGBC] >> GTE_COLOR_GREEN_SHIFT) & GTE_COLOR_MAX as u32) as i64;
        let b = ((self.data[RGBC] >> GTE_COLOR_BLUE_SHIFT) & GTE_COLOR_MAX as u32) as i64;
        let code = ((self.data[RGBC] >> GTE_CODE_SHIFT) & GTE_COLOR_MAX as u32) as u8;
        let p1 = (r * self.irv(IR1) as i64) << GTE_COLOR_FRACTION_SHIFT;
        let p2 = (g * self.irv(IR2) as i64) << GTE_COLOR_FRACTION_SHIFT;
        let p3 = (b * self.irv(IR3) as i64) << GTE_COLOR_FRACTION_SHIFT;
        (p1, p2, p3, code)
    }
}
