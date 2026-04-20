// PS1 Geometry Transform Engine (COP2)

pub struct Gte {
    pub data: [u32; 32],
    pub ctrl: [u32; 32],
}

// Data register indices
const VXY0: usize = 0;
const VZ0: usize = 1;
const VXY1: usize = 2;
const VZ1: usize = 3;
const VXY2: usize = 4;
const VZ2: usize = 5;
const RGBC: usize = 6;
const OTZ: usize = 7;
const IR0: usize = 8;
const IR1: usize = 9;
const IR2: usize = 10;
const IR3: usize = 11;
const SXY0: usize = 12;
const SXY1: usize = 13;
const SXY2: usize = 14;
const SXYP: usize = 15;
const SZ0: usize = 16;
const SZ1: usize = 17;
const SZ2: usize = 18;
const SZ3: usize = 19;
const RGB0: usize = 20;
const RGB1: usize = 21;
const RGB2: usize = 22;
const MAC0: usize = 24;
const MAC1: usize = 25;
const MAC2: usize = 26;
const MAC3: usize = 27;
const IRGB: usize = 28;
const ORGB: usize = 29;
const LZCS: usize = 30;
// LZCR = 31

// Control register indices
const RT11RT12: usize = 0;
const RT13RT21: usize = 1;
const RT22RT23: usize = 2;
const RT31RT32: usize = 3;
const RT33: usize = 4;
const TRX: usize = 5;
const TRY: usize = 6;
const TRZ: usize = 7;
const L11L12: usize = 8;
const L13L21: usize = 9;
const L22L23: usize = 10;
const L31L32: usize = 11;
const L33: usize = 12;
const RBK: usize = 13;
const GBK: usize = 14;
const BBK: usize = 15;
const LC11LC12: usize = 16;
const LC13LC21: usize = 17;
const LC22LC23: usize = 18;
const LC31LC32: usize = 19;
const LC33: usize = 20;
const RFC: usize = 21;
const GFC: usize = 22;
const BFC: usize = 23;
const OFX: usize = 24;
const OFY: usize = 25;
const H: usize = 26;
const DQA: usize = 27;
const DQB: usize = 28;
const ZSF3: usize = 29;
const ZSF4: usize = 30;
const FLAG: usize = 31;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GteCommand {
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
mod flags {
    pub const MAC1_POS: u32 = 30;
    pub const MAC2_POS: u32 = 29;
    pub const MAC3_POS: u32 = 28;
    pub const MAC1_NEG: u32 = 27;
    pub const MAC2_NEG: u32 = 26;
    pub const MAC3_NEG: u32 = 25;
    pub const IR1_POS: u32 = 24;
    pub const IR1_NEG: u32 = 23;
    pub const IR2_POS: u32 = 22;
    pub const IR2_NEG: u32 = 21;
    pub const IR3_POS: u32 = 20;
    pub const IR3_NEG: u32 = 19;
    pub const SZ3_OTZ_OVERFLOW: u32 = 18;
    pub const DIVIDE_OVERFLOW: u32 = 17;
    pub const MAC0_POS: u32 = 16;
    pub const MAC0_NEG: u32 = 15;
    pub const SX2_OVERFLOW: u32 = 14;
    pub const SY2_OVERFLOW: u32 = 13;
    pub const IR0_OVERFLOW: u32 = 12;
    pub const ERROR: u32 = 31;

    // Quirk: Color overflow bits share with IR bits
    pub const COLOR_R: u32 = IR2_NEG;
    pub const COLOR_G: u32 = IR3_POS;
    pub const COLOR_B: u32 = IR3_NEG;
}

const FLAG_ERROR_MASK: u32 = 0x7f87_e000;

// Reciprocal table for UNR division (256 entries + 1 sentinel)
// The GTE uses this table to find an initial 8-bit guess for 1/SZ.
// The table is derived from the formula: f(i) = floor(0x1FF00 / (0x100 + i)) - 0x100
const UNR_TABLE: [u8; 257] = {
    let mut table = [0u8; 257];
    let mut i = 0;
    while i < 256 {
        let val = (0x1ff00 / (0x100 + i)) - 0x100;
        table[i] = val as u8;
        i += 1;
    }
    table[256] = 0; // Sentinel value for interpolation
    table
};


impl Gte {
    pub fn new() -> Self {
        Self { data: [0; 32], ctrl: [0; 32] }
    }

    pub fn read_data(&self, r: usize) -> u32 {
        match r {
            SXYP => self.data[SXY2],
            IRGB | ORGB => {
                let ir1 = ((self.data[IR1] as i16).clamp(0, 0x7f80) as u32) >> 7;
                let ir2 = ((self.data[IR2] as i16).clamp(0, 0x7f80) as u32) >> 7;
                let ir3 = ((self.data[IR3] as i16).clamp(0, 0x7f80) as u32) >> 7;
                ir1 | (ir2 << 5) | (ir3 << 10)
            }
            30 => {
                // LZCS read returns the raw value
                self.data[LZCS]
            }
            31 => {
                // LZCR: leading zero count of LZCS
                let v = self.data[LZCS] as i32;
                (if v >= 0 { v.leading_zeros() } else { (!v).leading_zeros() }) as u32
            }
            _ => self.data[r],
        }
    }

    pub fn write_data(&mut self, r: usize, v: u32) {
        match r {
            SXYP => {
                self.data[SXY0] = self.data[SXY1];
                self.data[SXY1] = self.data[SXY2];
                self.data[SXY2] = v;
            }
            IRGB => {
                self.data[IRGB] = v & 0x7fff;
                self.data[IR1] = (v & 0x1f) << 7;
                self.data[IR2] = ((v >> 5) & 0x1f) << 7;
                self.data[IR3] = ((v >> 10) & 0x1f) << 7;
            }
            _ => self.data[r] = v,
        }
    }

    pub fn read_ctrl(&self, r: usize) -> u32 {
        if r == FLAG { self.flag_reg() } else { self.ctrl[r] }
    }

    pub fn write_ctrl(&mut self, r: usize, v: u32) {
        self.ctrl[r] = if r == FLAG { v & 0x7fff_f000 } else { v };
    }

    fn flag_reg(&self) -> u32 {
        let f = self.ctrl[FLAG] & 0x7fff_f000;
        let error = (f & FLAG_ERROR_MASK) != 0;
        f | if error { 1 << flags::ERROR } else { 0 }
    }

    pub fn execute(&mut self, cmd: u32) {
        self.ctrl[FLAG] = 0;
        let sf: u32 = if (cmd >> 19) & 1 != 0 { 12 } else { 0 };
        let lm = (cmd >> 10) & 1 != 0;
        let command = cmd & 0x3f;

        match command {
            v if v == GteCommand::Rtps as u32 => self.rtps(sf, lm, 0),
            v if v == GteCommand::Nclip as u32 => self.nclip(),
            v if v == GteCommand::Op as u32 => self.op(sf, lm),
            v if v == GteCommand::Dpcs as u32 => self.dpcs_inner(sf, lm, self.data[RGBC]),
            v if v == GteCommand::Intpl as u32 => self.intpl(sf, lm),
            v if v == GteCommand::Mvmva as u32 => self.mvmva(sf, lm, cmd),
            v if v == GteCommand::Ncds as u32 => self.ncds(sf, lm, 0),
            v if v == GteCommand::Cdp as u32 => self.cdp(sf, lm),
            v if v == GteCommand::Ncdt as u32 => {
                self.ncds(sf, lm, 0);
                self.ncds(sf, lm, 1);
                self.ncds(sf, lm, 2);
            }
            v if v == GteCommand::Nccs as u32 => self.nccs(sf, lm, 0),
            v if v == GteCommand::Cc as u32 => self.cc(sf, lm),
            v if v == GteCommand::Ncs as u32 => self.ncs(sf, lm, 0),
            v if v == GteCommand::Ncst as u32 => {
                self.ncs(sf, lm, 0);
                self.ncs(sf, lm, 1);
                self.ncs(sf, lm, 2);
            }
            v if v == GteCommand::Sqr as u32 => self.sqr(sf, lm),
            v if v == GteCommand::Dcpl as u32 => self.dcpl(sf, lm),
            v if v == GteCommand::Dpcq as u32 => {
                for _ in 0..3 {
                    let rgb = self.data[RGB0];
                    self.dpcs_inner(sf, lm, rgb);
                }
            }
            v if v == GteCommand::Avsz3 as u32 => self.avsz3(),
            v if v == GteCommand::Avsz4 as u32 => self.avsz4(),
            v if v == GteCommand::Rtpt as u32 => {
                self.rtps(sf, lm, 0);
                self.rtps(sf, lm, 1);
                self.rtps(sf, lm, 2);
            }
            v if v == GteCommand::Gpf as u32 => self.gpf(sf, lm),
            v if v == GteCommand::Gpl as u32 => self.gpl(sf, lm),
            v if v == GteCommand::Ncct as u32 => {
                self.nccs(sf, lm, 0);
                self.nccs(sf, lm, 1);
                self.nccs(sf, lm, 2);
            }
            _ => {}
        }
    }

    // ===== Helpers =====

    fn vxy(&self, idx: usize) -> (i32, i32) {
        let v = self.data[idx * 2];
        (v as i16 as i32, (v >> 16) as i16 as i32)
    }

    fn vz(&self, idx: usize) -> i32 {
        self.data[idx * 2 + 1] as i16 as i32
    }

    fn mx16(&self, base_reg: usize, which: usize) -> i32 {
        let v = self.ctrl[base_reg + which / 2];
        if which & 1 == 0 { v as i16 as i32 } else { (v >> 16) as i16 as i32 }
    }

    fn tr32(&self, reg: usize) -> i64 {
        self.ctrl[reg] as i32 as i64
    }

    fn irv(&self, r: usize) -> i32 { self.data[r] as i16 as i32 }
    fn macv(&self, r: usize) -> i32 { self.data[r] as i32 }

    fn set_mac1(&mut self, v: i64, sf: u32) {
        if v > 0x7fff_ffff_ffff { self.ctrl[FLAG] |= 1 << flags::MAC1_POS; }
        if v < -0x8000_0000_0000 { self.ctrl[FLAG] |= 1 << flags::MAC1_NEG; }
        self.data[MAC1] = ((v >> sf) as i32) as u32;
    }
    fn set_mac2(&mut self, v: i64, sf: u32) {
        if v > 0x7fff_ffff_ffff { self.ctrl[FLAG] |= 1 << flags::MAC2_POS; }
        if v < -0x8000_0000_0000 { self.ctrl[FLAG] |= 1 << flags::MAC2_NEG; }
        self.data[MAC2] = ((v >> sf) as i32) as u32;
    }
    fn set_mac3(&mut self, v: i64, sf: u32) {
        if v > 0x7fff_ffff_ffff { self.ctrl[FLAG] |= 1 << flags::MAC3_POS; }
        if v < -0x8000_0000_0000 { self.ctrl[FLAG] |= 1 << flags::MAC3_NEG; }
        self.data[MAC3] = ((v >> sf) as i32) as u32;
    }
    fn set_mac0(&mut self, v: i64) {
        if v > 0x7fff_ffff { self.ctrl[FLAG] |= 1 << flags::MAC0_POS; }
        if v < -0x8000_0000i64 { self.ctrl[FLAG] |= 1 << flags::MAC0_NEG; }
        self.data[MAC0] = (v as i32) as u32;
    }

    fn set_ir1(&mut self, v: i32, lm: bool) { self.data[IR1] = self.ir_clamp(v, lm, flags::IR1_POS, flags::IR1_NEG) as u32; }
    fn set_ir2(&mut self, v: i32, lm: bool) { self.data[IR2] = self.ir_clamp(v, lm, flags::IR2_POS, flags::IR2_NEG) as u32; }
    fn set_ir3(&mut self, v: i32, lm: bool) { self.data[IR3] = self.ir_clamp(v, lm, flags::IR3_POS, flags::IR3_NEG) as u32; }

    fn ir_clamp(&mut self, v: i32, lm: bool, pos_bit: u32, neg_bit: u32) -> i32 {
        let min = if lm { 0 } else { -0x8000 };
        if v > 0x7fff { self.ctrl[FLAG] |= 1 << pos_bit; 0x7fff }
        else if v < min { self.ctrl[FLAG] |= 1 << neg_bit; min }
        else { v }
    }

    fn set_ir0(&mut self, v: i32) {
        if v > 0x1000 { self.ctrl[FLAG] |= 1 << flags::IR0_OVERFLOW; self.data[IR0] = 0x1000; }
        else if v < 0 { self.ctrl[FLAG] |= 1 << flags::IR0_OVERFLOW; self.data[IR0] = 0; }
        else { self.data[IR0] = v as u32; }
    }

    fn push_sz(&mut self, v: i32) {
        self.data[SZ0] = self.data[SZ1];
        self.data[SZ1] = self.data[SZ2];
        self.data[SZ2] = self.data[SZ3];
        if v < 0 { self.ctrl[FLAG] |= 1 << flags::SZ3_OTZ_OVERFLOW; self.data[SZ3] = 0; }
        else if v > 0xffff { self.ctrl[FLAG] |= 1 << flags::SZ3_OTZ_OVERFLOW; self.data[SZ3] = 0xffff; }
        else { self.data[SZ3] = v as u32; }
    }

    fn push_sxy(&mut self, x: i32, y: i32) {
        self.data[SXY0] = self.data[SXY1];
        self.data[SXY1] = self.data[SXY2];
        let cx = x.clamp(-0x400, 0x3ff);
        let cy = y.clamp(-0x400, 0x3ff);
        if cx != x { self.ctrl[FLAG] |= 1 << flags::SX2_OVERFLOW; }
        if cy != y { self.ctrl[FLAG] |= 1 << flags::SY2_OVERFLOW; }
        self.data[SXY2] = (cx as i16 as u16 as u32) | ((cy as i16 as u16 as u32) << 16);
    }

    fn push_rgb(&mut self, r: i32, g: i32, b: i32, code: u8) {
        self.data[RGB0] = self.data[RGB1];
        self.data[RGB1] = self.data[RGB2];
        let rc = if r < 0 { self.ctrl[FLAG] |= 1 << flags::COLOR_R; 0u32 } else if r > 255 { self.ctrl[FLAG] |= 1 << flags::COLOR_R; 255 } else { r as u32 };
        let gc = if g < 0 { self.ctrl[FLAG] |= 1 << flags::COLOR_G; 0u32 } else if g > 255 { self.ctrl[FLAG] |= 1 << flags::COLOR_G; 255 } else { g as u32 };
        let bc = if b < 0 { self.ctrl[FLAG] |= 1 << flags::COLOR_B; 0u32 } else if b > 255 { self.ctrl[FLAG] |= 1 << flags::COLOR_B; 255 } else { b as u32 };
        self.data[RGB2] = rc | (gc << 8) | (bc << 16) | ((code as u32) << 24);
    }

    /// UNR (Universal Reciprocal) division using hardware-accurate linear interpolation.
    /// The GTE uses this for perspective projection (calculating 1/SZ).
    /// It first normalizes the divisor and looks up an initial guess in a table,
    /// then performs linear interpolation to reach the final reciprocal.
    fn unr_divide(&mut self, lhs: u32, rhs: u32) -> u32 {
        if rhs == 0 || (lhs as u64 >= (rhs as u64) * 2) {
            self.ctrl[FLAG] |= 1 << flags::DIVIDE_OVERFLOW;
            return 0x1ffff;
        }

        // Normalize divisor to 0x8000-0xFFFF range
        let lz = rhs.leading_zeros();
        let shift = lz.saturating_sub(16);
        let n_h = (rhs << shift) as u32;

        // Index into the UNR table using bits 7-14 of normalized value
        // The index formula is (n_h - 0x7FC0) >> 7, but since bit 15 is always 1,
        // it simplifies to ((n_h >> 7) & 0xFF) for indices 0-255.
        // We look up two consecutive values for interpolation.
        let index = ((n_h >> 7) & 0xff) as usize;
        let a = UNR_TABLE[index] as u32;
        let b = UNR_TABLE[index + 1] as u32;

        // Linear interpolation: a - (a - b) * (fractional_part / 128)
        let fraction = n_h & 0x7f;
        let mut reciprocal = ((a << 7) - (a.wrapping_sub(b)).wrapping_mul(fraction) + 0x40) >> 7;

        // Rescale based on the initial normalization
        reciprocal >>= shift;

        // Apply dividend (H) and return result
        let result = (lhs as u64 * (reciprocal as u64 + 0x100)) >> 8;
        result.min(0x1ffff) as u32
    }

    /// Performs perspective transformation on calculated IR registers.
    /// Formulas:
    ///   SX = (IR1 * (H / SZ) + OFX)
    ///   SY = (IR2 * (H / SZ) + OFY)
    ///   MAC0 = (DQA * (H / SZ) + DQB)
    /// Where H is the projection plane distance, OFX/OFY are screen offsets,
    /// and DQA/DQB are depth-cueing parameters.
    fn perspective_div(&mut self, sf: u32, lm: bool) {
        let sz3 = self.data[SZ3];
        let h_val = self.ctrl[H] as u16 as u32;
        let div = self.unr_divide(h_val, sz3);

        let ofx = self.ctrl[OFX] as i32 as i64;
        let ofy = self.ctrl[OFY] as i32 as i64;
        let dqa = self.ctrl[DQA] as i16 as i64;
        let dqb = self.ctrl[DQB] as i32 as i64;

        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;

        let sx = (ir1 * div as i64 + ofx) >> 16;
        let sy = (ir2 * div as i64 + ofy) >> 16;
        self.push_sxy(sx as i32, sy as i32);

        let mac0_v = dqa * div as i64 + dqb;
        self.set_mac0(mac0_v >> 12);
        self.set_ir0((mac0_v >> 12) as i32);
        let _ = (sf, lm);
    }

    fn rt_mul(&self, vx: i32, vy: i32, vz: i32) -> (i64, i64, i64) {
        let r = |b, i| self.mx16(b, i) as i64;
        let m1 = self.tr32(TRX) * 0x1000
            + r(RT11RT12,0)*vx as i64 + r(RT11RT12,1)*vy as i64 + r(RT13RT21,0)*vz as i64;
        let m2 = self.tr32(TRY) * 0x1000
            + r(RT13RT21,1)*vx as i64 + r(RT22RT23,0)*vy as i64 + r(RT22RT23,1)*vz as i64;
        let m3 = self.tr32(TRZ) * 0x1000
            + r(RT31RT32,0)*vx as i64 + r(RT31RT32,1)*vy as i64 + (self.ctrl[RT33] as i16 as i64)*vz as i64;
        (m1, m2, m3)
    }

    fn ll_mul(&self, vx: i32, vy: i32, vz: i32) -> (i64, i64, i64) {
        let r = |b, i| self.mx16(b, i) as i64;
        let m1 = r(L11L12,0)*vx as i64 + r(L11L12,1)*vy as i64 + r(L13L21,0)*vz as i64;
        let m2 = r(L13L21,1)*vx as i64 + r(L22L23,0)*vy as i64 + r(L22L23,1)*vz as i64;
        let m3 = r(L31L32,0)*vx as i64 + r(L31L32,1)*vy as i64 + (self.ctrl[L33] as i16 as i64)*vz as i64;
        (m1, m2, m3)
    }

    fn bk_lc_ir(&self) -> (i64, i64, i64) {
        let r = |b, i| self.mx16(b, i) as i64;
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        let n1 = self.tr32(RBK)*0x1000 + r(LC11LC12,0)*ir1 + r(LC11LC12,1)*ir2 + r(LC13LC21,0)*ir3;
        let n2 = self.tr32(GBK)*0x1000 + r(LC13LC21,1)*ir1 + r(LC22LC23,0)*ir2 + r(LC22LC23,1)*ir3;
        let n3 = self.tr32(BBK)*0x1000 + r(LC31LC32,0)*ir1 + r(LC31LC32,1)*ir2 + (self.ctrl[LC33] as i16 as i64)*ir3;
        (n1, n2, n3)
    }

    fn rgb_ir(&self) -> (i64, i64, i64, u8) {
        let r = (self.data[RGBC] & 0xff) as i64;
        let g = ((self.data[RGBC] >> 8) & 0xff) as i64;
        let b = ((self.data[RGBC] >> 16) & 0xff) as i64;
        let code = ((self.data[RGBC] >> 24) & 0xff) as u8;
        let p1 = (r * self.irv(IR1) as i64) << 4;
        let p2 = (g * self.irv(IR2) as i64) << 4;
        let p3 = (b * self.irv(IR3) as i64) << 4;
        (p1, p2, p3, code)
    }

    // ===== GTE Commands =====

    /// RTPS (Rotation, Translation, and Perspective Transformation Single)
    /// Multiplies a vector by the rotation matrix, adds the translation vector,
    /// and performs perspective projection.
    fn rtps(&mut self, sf: u32, lm: bool, v_idx: usize) {
        let (vx, vy) = self.vxy(v_idx);
        let vz = self.vz(v_idx);
        let (m1, m2, m3) = self.rt_mul(vx, vy, vz);
        self.set_mac1(m1, sf); self.set_mac2(m2, sf); self.set_mac3(m3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        self.push_sz(self.macv(MAC3) >> 12);
        self.perspective_div(sf, lm);
    }

    /// NCLIP (Normal Clipping)
    /// Calculates the 2D cross product of screen coordinates SXY0, SXY1, SXY2.
    /// Used for back-face culling: a negative result indicates a back-facing triangle.
    fn nclip(&mut self) {
        let sxy = |r: usize| -> (i32, i32) {
            let v = self.data[r];
            (v as i16 as i32, (v >> 16) as i16 as i32)
        };
        let (x0, y0) = sxy(SXY0);
        let (x1, y1) = sxy(SXY1);
        let (x2, y2) = sxy(SXY2);
        let mac0 = x0 as i64 * (y1 - y2) as i64
                 + x1 as i64 * (y2 - y0) as i64
                 + x2 as i64 * (y0 - y1) as i64;
        self.set_mac0(mac0);
    }

    /// OP (Outer Product)
    /// Calculates the cross product of two vectors (using matrix components and IR registers).
    fn op(&mut self, sf: u32, lm: bool) {
        let d1 = self.ctrl[RT11RT12] as i16 as i64;
        let d2 = (self.ctrl[RT22RT23] & 0xffff) as i16 as i64;
        let d3 = self.ctrl[RT33] as i16 as i64;
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        self.set_mac1(d2*ir3 - d3*ir2, sf);
        self.set_mac2(d3*ir1 - d1*ir3, sf);
        self.set_mac3(d1*ir2 - d2*ir1, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
    }

    /// SQR (Square)
    /// Calculates the element-wise square of the IR1, IR2, IR3 registers.
    fn sqr(&mut self, sf: u32, lm: bool) {
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        self.set_mac1(ir1*ir1, sf); self.set_mac2(ir2*ir2, sf); self.set_mac3(ir3*ir3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
    }

    /// MVMVA (Matrix-Vector Multiplication and Vector Addition)
    /// Performs a flexible matrix-vector multiplication with an optional addition of a constant vector.
    /// The matrix, vector, and addition vector are selected based on command bits.
    fn mvmva(&mut self, sf: u32, lm: bool, cmd: u32) {
        let mx = (cmd >> 17) & 3;
        let vv = (cmd >> 15) & 3;
        let cv = (cmd >> 13) & 3;

        let (vx, vy, vz) = match vv {
            0 => { let (x,y)=self.vxy(0); (x,y,self.vz(0)) }
            1 => { let (x,y)=self.vxy(1); (x,y,self.vz(1)) }
            2 => { let (x,y)=self.vxy(2); (x,y,self.vz(2)) }
            _ => (self.irv(IR1), self.irv(IR2), self.irv(IR3)),
        };

        let (row0, row1, row2): ([i64;3],[i64;3],[i64;3]) = match mx {
            0 => ([self.mx16(RT11RT12,0) as i64, self.mx16(RT11RT12,1) as i64, self.mx16(RT13RT21,0) as i64],
                  [self.mx16(RT13RT21,1) as i64, self.mx16(RT22RT23,0) as i64, self.mx16(RT22RT23,1) as i64],
                  [self.mx16(RT31RT32,0) as i64, self.mx16(RT31RT32,1) as i64, self.ctrl[RT33] as i16 as i64]),
            1 => ([self.mx16(L11L12,0) as i64, self.mx16(L11L12,1) as i64, self.mx16(L13L21,0) as i64],
                  [self.mx16(L13L21,1) as i64, self.mx16(L22L23,0) as i64, self.mx16(L22L23,1) as i64],
                  [self.mx16(L31L32,0) as i64, self.mx16(L31L32,1) as i64, self.ctrl[L33] as i16 as i64]),
            2 => ([self.mx16(LC11LC12,0) as i64, self.mx16(LC11LC12,1) as i64, self.mx16(LC13LC21,0) as i64],
                  [self.mx16(LC13LC21,1) as i64, self.mx16(LC22LC23,0) as i64, self.mx16(LC22LC23,1) as i64],
                  [self.mx16(LC31LC32,0) as i64, self.mx16(LC31LC32,1) as i64, self.ctrl[LC33] as i16 as i64]),
            _ => ([-self.mx16(RT11RT12,0) as i64, -self.mx16(RT11RT12,1) as i64, -self.mx16(RT13RT21,0) as i64],
                  [-self.mx16(RT13RT21,1) as i64, -self.mx16(RT22RT23,0) as i64, -self.mx16(RT22RT23,1) as i64],
                  [-self.mx16(RT31RT32,0) as i64, -self.mx16(RT31RT32,1) as i64, -(self.ctrl[RT33] as i16 as i64)]),
        };

        let (tx, ty, tz): (i64, i64, i64) = match cv {
            0 => (self.tr32(TRX)*0x1000, self.tr32(TRY)*0x1000, self.tr32(TRZ)*0x1000),
            1 => (self.tr32(RBK)*0x1000, self.tr32(GBK)*0x1000, self.tr32(BBK)*0x1000),
            2 => (self.tr32(RFC)*0x1000, self.tr32(GFC)*0x1000, self.tr32(BFC)*0x1000),
            _ => (0, 0, 0),
        };

        let vx64 = vx as i64; let vy64 = vy as i64; let vz64 = vz as i64;
        self.set_mac1(tx + row0[0]*vx64 + row0[1]*vy64 + row0[2]*vz64, sf);
        self.set_mac2(ty + row1[0]*vx64 + row1[1]*vy64 + row1[2]*vz64, sf);
        self.set_mac3(tz + row2[0]*vx64 + row2[1]*vy64 + row2[2]*vz64, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
    }

    /// NCS (Normal Color Single)
    /// Transforms a normal vector and applies light source calculations to determine vertex color.
    fn ncs(&mut self, sf: u32, lm: bool, v_idx: usize) {
        let (vx, vy) = self.vxy(v_idx);
        let vz = self.vz(v_idx);
        let (m1, m2, m3) = self.ll_mul(vx, vy, vz);
        self.set_mac1(m1, sf); self.set_mac2(m2, sf); self.set_mac3(m3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        let (n1, n2, n3) = self.bk_lc_ir();
        self.set_mac1(n1, sf); self.set_mac2(n2, sf); self.set_mac3(n3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        let code = ((self.data[RGBC] >> 24) & 0xff) as u8;
        self.push_rgb(self.macv(MAC1) >> 4, self.macv(MAC2) >> 4, self.macv(MAC3) >> 4, code);
    }

    /// NCCS (Normal Color Color Single)
    /// Transforms a normal vector, applies light source calculations, and adds ambient color.
    fn nccs(&mut self, sf: u32, lm: bool, v_idx: usize) {
        let (vx, vy) = self.vxy(v_idx);
        let vz = self.vz(v_idx);
        let (m1, m2, m3) = self.ll_mul(vx, vy, vz);
        self.set_mac1(m1, sf); self.set_mac2(m2, sf); self.set_mac3(m3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        let (n1, n2, n3) = self.bk_lc_ir();
        self.set_mac1(n1, sf); self.set_mac2(n2, sf); self.set_mac3(n3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        let (p1, p2, p3, code) = self.rgb_ir();
        self.set_mac1(p1, sf); self.set_mac2(p2, sf); self.set_mac3(p3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(self.macv(MAC1) >> 4, self.macv(MAC2) >> 4, self.macv(MAC3) >> 4, code);
    }

    /// NCDS (Normal Color Depth Single)
    /// Transforms a normal vector, applies light source calculations, and performs depth-cueing.
    fn ncds(&mut self, sf: u32, lm: bool, v_idx: usize) {
        let (vx, vy) = self.vxy(v_idx);
        let vz = self.vz(v_idx);
        let (m1, m2, m3) = self.ll_mul(vx, vy, vz);
        self.set_mac1(m1, sf); self.set_mac2(m2, sf); self.set_mac3(m3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        let (n1, n2, n3) = self.bk_lc_ir();
        self.set_mac1(n1, sf); self.set_mac2(n2, sf); self.set_mac3(n3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        // Far color interpolation with IR0
        let ir0 = self.irv(IR0) as i64;
        let fc_r = self.ctrl[RFC] as i32 as i64;
        let fc_g = self.ctrl[GFC] as i32 as i64;
        let fc_b = self.ctrl[BFC] as i32 as i64;
        let r = (self.data[RGBC] & 0xff) as i64;
        let g = ((self.data[RGBC] >> 8) & 0xff) as i64;
        let b = ((self.data[RGBC] >> 16) & 0xff) as i64;
        let code = ((self.data[RGBC] >> 24) & 0xff) as u8;
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        let p1 = fc_r*0x1000 - (r*ir1 << 4);
        let p2 = fc_g*0x1000 - (g*ir2 << 4);
        let p3 = fc_b*0x1000 - (b*ir3 << 4);
        self.set_mac1(p1, sf); self.set_mac2(p2, sf); self.set_mac3(p3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        let q1 = (r*ir1 << 4) + self.irv(IR1) as i64 * ir0;
        let q2 = (g*ir2 << 4) + self.irv(IR2) as i64 * ir0;
        let q3 = (b*ir3 << 4) + self.irv(IR3) as i64 * ir0;
        self.set_mac1(q1, sf); self.set_mac2(q2, sf); self.set_mac3(q3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(self.macv(MAC1) >> 4, self.macv(MAC2) >> 4, self.macv(MAC3) >> 4, code);
    }

    /// CC (Color-Color Transformation)
    /// Transforms the color values in the RGBC register using the Color Transformation Matrix (LC)
    /// and adds the background color (BK).
    fn cc(&mut self, sf: u32, lm: bool) {
        let (n1, n2, n3) = self.bk_lc_ir();
        self.set_mac1(n1, sf); self.set_mac2(n2, sf); self.set_mac3(n3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        let (p1, p2, p3, code) = self.rgb_ir();
        self.set_mac1(p1, sf); self.set_mac2(p2, sf); self.set_mac3(p3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(self.macv(MAC1) >> 4, self.macv(MAC2) >> 4, self.macv(MAC3) >> 4, code);
    }

    /// CDP (Color Depth and Perspective)
    /// Performs color transformation, adds background color, and then applies far-color
    /// interpolation (depth-cueing) using the depth factor IR0.
    fn cdp(&mut self, sf: u32, lm: bool) {
        let (n1, n2, n3) = self.bk_lc_ir();
        self.set_mac1(n1, sf); self.set_mac2(n2, sf); self.set_mac3(n3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        let ir0 = self.irv(IR0) as i64;
        let fc_r = self.ctrl[RFC] as i32 as i64;
        let fc_g = self.ctrl[GFC] as i32 as i64;
        let fc_b = self.ctrl[BFC] as i32 as i64;
        let r = (self.data[RGBC] & 0xff) as i64;
        let g = ((self.data[RGBC] >> 8) & 0xff) as i64;
        let b = ((self.data[RGBC] >> 16) & 0xff) as i64;
        let code = ((self.data[RGBC] >> 24) & 0xff) as u8;
        let ir1 = self.irv(IR1) as i64; let ir2 = self.irv(IR2) as i64; let ir3 = self.irv(IR3) as i64;
        let p1 = fc_r*0x1000 - (r*ir1 << 4);
        let p2 = fc_g*0x1000 - (g*ir2 << 4);
        let p3 = fc_b*0x1000 - (b*ir3 << 4);
        self.set_mac1(p1, sf); self.set_mac2(p2, sf); self.set_mac3(p3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        let q1 = (r*ir1 << 4) + self.irv(IR1) as i64 * ir0;
        let q2 = (g*ir2 << 4) + self.irv(IR2) as i64 * ir0;
        let q3 = (b*ir3 << 4) + self.irv(IR3) as i64 * ir0;
        self.set_mac1(q1, sf); self.set_mac2(q2, sf); self.set_mac3(q3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(self.macv(MAC1) >> 4, self.macv(MAC2) >> 4, self.macv(MAC3) >> 4, code);
    }

    /// Internal implementation for Depth-cueing Perspective Color Single (DPCS)
    /// Interpolates between the input color and the far-color (RFC, GFC, BFC)
    /// using IR0 as the interpolation factor.
    fn dpcs_inner(&mut self, sf: u32, lm: bool, rgbc: u32) {
        let r = (rgbc & 0xff) as i64;
        let g = ((rgbc >> 8) & 0xff) as i64;
        let b = ((rgbc >> 16) & 0xff) as i64;
        let code = ((self.data[RGBC] >> 24) & 0xff) as u8;
        let ir0 = self.irv(IR0) as i64;
        let fc_r = self.ctrl[RFC] as i32 as i64;
        let fc_g = self.ctrl[GFC] as i32 as i64;
        let fc_b = self.ctrl[BFC] as i32 as i64;
        let m1 = (r << 16) + (fc_r - (r << 4)) * ir0;
        let m2 = (g << 16) + (fc_g - (g << 4)) * ir0;
        let m3 = (b << 16) + (fc_b - (b << 4)) * ir0;
        self.set_mac1(m1 >> 4, sf); self.set_mac2(m2 >> 4, sf); self.set_mac3(m3 >> 4, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(self.macv(MAC1) >> 4, self.macv(MAC2) >> 4, self.macv(MAC3) >> 4, code);
    }

    /// INTPL (Interpolation)
    /// Interpolates between the current vertex color (IR registers) and the
    /// far-color (FC) using IR0 as the factor.
    fn intpl(&mut self, sf: u32, lm: bool) {
        let ir0 = self.irv(IR0) as i64;
        let fc_r = self.ctrl[RFC] as i32 as i64;
        let fc_g = self.ctrl[GFC] as i32 as i64;
        let fc_b = self.ctrl[BFC] as i32 as i64;
        let ir1 = self.irv(IR1) as i64; let ir2 = self.irv(IR2) as i64; let ir3 = self.irv(IR3) as i64;
        let code = ((self.data[RGBC] >> 24) & 0xff) as u8;
        let m1 = (ir1 << 12) + (fc_r - ir1) * ir0;
        let m2 = (ir2 << 12) + (fc_g - ir2) * ir0;
        let m3 = (ir3 << 12) + (fc_b - ir3) * ir0;
        self.set_mac1(m1, sf); self.set_mac2(m2, sf); self.set_mac3(m3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(self.macv(MAC1) >> 4, self.macv(MAC2) >> 4, self.macv(MAC3) >> 4, code);
    }

    /// DCPL (Depth-cueing Color)
    /// Alias for DPCS using the global RGBC register.
    fn dcpl(&mut self, sf: u32, lm: bool) {
        let rgbc = self.data[RGBC];
        self.dpcs_inner(sf, lm, rgbc);
    }

    /// GPF (General-purpose Filter)
    /// Multiplies the IR registers by IR0 and pushes the result as a color.
    fn gpf(&mut self, sf: u32, lm: bool) {
        let ir0 = self.irv(IR0) as i64;
        let ir1 = self.irv(IR1) as i64; let ir2 = self.irv(IR2) as i64; let ir3 = self.irv(IR3) as i64;
        let code = ((self.data[RGBC] >> 24) & 0xff) as u8;
        self.set_mac0(0);
        self.set_mac1(ir0*ir1, sf); self.set_mac2(ir0*ir2, sf); self.set_mac3(ir0*ir3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(self.macv(MAC1) >> 4, self.macv(MAC2) >> 4, self.macv(MAC3) >> 4, code);
    }

    /// GPL (General-purpose Filter with Accumulation)
    /// Multiplies the IR registers by IR0 and adds the previous MAC register values.
    fn gpl(&mut self, sf: u32, lm: bool) {
        let ir0 = self.irv(IR0) as i64;
        let ir1 = self.irv(IR1) as i64; let ir2 = self.irv(IR2) as i64; let ir3 = self.irv(IR3) as i64;
        let mac1_old = self.macv(MAC1) as i64; let mac2_old = self.macv(MAC2) as i64; let mac3_old = self.macv(MAC3) as i64;
        let code = ((self.data[RGBC] >> 24) & 0xff) as u8;
        self.set_mac1((mac1_old << sf) + ir0*ir1, sf);
        self.set_mac2((mac2_old << sf) + ir0*ir2, sf);
        self.set_mac3((mac3_old << sf) + ir0*ir3, sf);
        self.set_ir1(self.macv(MAC1), lm); self.set_ir2(self.macv(MAC2), lm); self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(self.macv(MAC1) >> 4, self.macv(MAC2) >> 4, self.macv(MAC3) >> 4, code);
    }

    /// AVSZ3 (Average Z for 3 Vertices)
    /// Calculates the average Z value of 3 vertices and stores it in the OTZ register.
    fn avsz3(&mut self) {
        let zsf3 = self.ctrl[ZSF3] as i16 as i64;
        let sum = self.data[SZ1] as i64 + self.data[SZ2] as i64 + self.data[SZ3] as i64;
        let mac0 = zsf3 * sum;
        self.set_mac0(mac0 >> 12);
        self.data[OTZ] = (mac0 >> 12).clamp(0, 0xffff) as u32;
    }

    /// AVSZ4 (Average Z for 4 Vertices)
    /// Calculates the average Z value of 4 vertices and stores it in the OTZ register.
    fn avsz4(&mut self) {
        let zsf4 = self.ctrl[ZSF4] as i16 as i64;
        let sum = self.data[SZ0] as i64 + self.data[SZ1] as i64 + self.data[SZ2] as i64 + self.data[SZ3] as i64;
        let mac0 = zsf4 * sum;
        self.set_mac0(mac0 >> 12);
        self.data[OTZ] = (mac0 >> 12).clamp(0, 0xffff) as u32;
    }

}
