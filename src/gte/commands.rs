use super::*;

impl Gte {
    // ===== GTE Commands =====

    /// RTPS (Rotation, Translation, and Perspective Transformation Single)
    /// Multiplies a vector by the rotation matrix, adds the translation vector,
    /// and performs perspective projection.
    pub(super) fn rtps(&mut self, sf: u32, lm: bool, v_idx: usize) {
        let (vx, vy) = self.vxy(v_idx);
        let vz = self.vz(v_idx);
        let (m1, m2, m3) = self.rt_mul(vx, vy, vz);
        self.set_mac1(m1, sf);
        self.set_mac2(m2, sf);
        self.set_mac3(m3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        self.push_sz(self.macv(MAC3) >> GTE_SHIFT_FRACTIONAL);
        self.perspective_div(sf, lm);
    }

    /// NCLIP (Normal Clipping)
    /// Calculates the 2D cross product of screen coordinates SXY0, SXY1, SXY2.
    /// Used for back-face culling: a negative result indicates a back-facing triangle.
    pub(super) fn nclip(&mut self) {
        let sxy = |r: usize| -> (i32, i32) {
            let v = self.data[r];
            (v as i16 as i32, (v >> GTE_HALFWORD_SHIFT) as i16 as i32)
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
    pub(super) fn op(&mut self, sf: u32, lm: bool) {
        let d1 = self.ctrl[RT11RT12] as i16 as i64;
        let d2 = (self.ctrl[RT22RT23] & GTE_SZ_MAX_U32) as i16 as i64;
        let d3 = self.ctrl[RT33] as i16 as i64;
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        self.set_mac1(d2 * ir3 - d3 * ir2, sf);
        self.set_mac2(d3 * ir1 - d1 * ir3, sf);
        self.set_mac3(d1 * ir2 - d2 * ir1, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
    }

    /// SQR (Square)
    /// Calculates the element-wise square of the IR1, IR2, IR3 registers.
    pub(super) fn sqr(&mut self, sf: u32, lm: bool) {
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        self.set_mac1(ir1 * ir1, sf);
        self.set_mac2(ir2 * ir2, sf);
        self.set_mac3(ir3 * ir3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
    }

    /// MVMVA (Matrix-Vector Multiplication and Vector Addition)
    /// Performs a flexible matrix-vector multiplication with an optional addition of a constant vector.
    /// The matrix, vector, and addition vector are selected based on command bits.
    pub(super) fn mvmva(&mut self, sf: u32, lm: bool, cmd: u32) {
        let mx = (cmd >> GTE_MX_SHIFT) & GTE_MATRIX_SELECT_MASK;
        let vv = (cmd >> GTE_V_SHIFT) & GTE_VECTOR_SELECT_MASK;
        let cv = (cmd >> GTE_CV_SHIFT) & GTE_CONTROL_VECTOR_SELECT_MASK;

        let (vx, vy, vz) = match vv {
            0 => {
                let (x, y) = self.vxy(0);
                (x, y, self.vz(0))
            }
            1 => {
                let (x, y) = self.vxy(1);
                (x, y, self.vz(1))
            }
            2 => {
                let (x, y) = self.vxy(2);
                (x, y, self.vz(2))
            }
            _ => (self.irv(IR1), self.irv(IR2), self.irv(IR3)),
        };

        let (row0, row1, row2): ([i64; 3], [i64; 3], [i64; 3]) = match mx {
            0 => (
                [
                    self.mx16(RT11RT12, 0) as i64,
                    self.mx16(RT11RT12, 1) as i64,
                    self.mx16(RT13RT21, 0) as i64,
                ],
                [
                    self.mx16(RT13RT21, 1) as i64,
                    self.mx16(RT22RT23, 0) as i64,
                    self.mx16(RT22RT23, 1) as i64,
                ],
                [
                    self.mx16(RT31RT32, 0) as i64,
                    self.mx16(RT31RT32, 1) as i64,
                    self.ctrl[RT33] as i16 as i64,
                ],
            ),
            1 => (
                [
                    self.mx16(L11L12, 0) as i64,
                    self.mx16(L11L12, 1) as i64,
                    self.mx16(L13L21, 0) as i64,
                ],
                [
                    self.mx16(L13L21, 1) as i64,
                    self.mx16(L22L23, 0) as i64,
                    self.mx16(L22L23, 1) as i64,
                ],
                [
                    self.mx16(L31L32, 0) as i64,
                    self.mx16(L31L32, 1) as i64,
                    self.ctrl[L33] as i16 as i64,
                ],
            ),
            2 => (
                [
                    self.mx16(LC11LC12, 0) as i64,
                    self.mx16(LC11LC12, 1) as i64,
                    self.mx16(LC13LC21, 0) as i64,
                ],
                [
                    self.mx16(LC13LC21, 1) as i64,
                    self.mx16(LC22LC23, 0) as i64,
                    self.mx16(LC22LC23, 1) as i64,
                ],
                [
                    self.mx16(LC31LC32, 0) as i64,
                    self.mx16(LC31LC32, 1) as i64,
                    self.ctrl[LC33] as i16 as i64,
                ],
            ),
            _ => (
                [
                    -self.mx16(RT11RT12, 0) as i64,
                    -self.mx16(RT11RT12, 1) as i64,
                    -self.mx16(RT13RT21, 0) as i64,
                ],
                [
                    -self.mx16(RT13RT21, 1) as i64,
                    -self.mx16(RT22RT23, 0) as i64,
                    -self.mx16(RT22RT23, 1) as i64,
                ],
                [
                    -self.mx16(RT31RT32, 0) as i64,
                    -self.mx16(RT31RT32, 1) as i64,
                    -(self.ctrl[RT33] as i16 as i64),
                ],
            ),
        };

        let (tx, ty, tz): (i64, i64, i64) = match cv {
            0 => (
                self.tr32(TRX) * GTE_FIXED_POINT_ONE,
                self.tr32(TRY) * GTE_FIXED_POINT_ONE,
                self.tr32(TRZ) * GTE_FIXED_POINT_ONE,
            ),
            1 => (
                self.tr32(RBK) * GTE_FIXED_POINT_ONE,
                self.tr32(GBK) * GTE_FIXED_POINT_ONE,
                self.tr32(BBK) * GTE_FIXED_POINT_ONE,
            ),
            2 => (
                self.tr32(RFC) * GTE_FIXED_POINT_ONE,
                self.tr32(GFC) * GTE_FIXED_POINT_ONE,
                self.tr32(BFC) * GTE_FIXED_POINT_ONE,
            ),
            _ => (0, 0, 0),
        };

        let vx64 = vx as i64;
        let vy64 = vy as i64;
        let vz64 = vz as i64;
        self.set_mac1(tx + row0[0] * vx64 + row0[1] * vy64 + row0[2] * vz64, sf);
        self.set_mac2(ty + row1[0] * vx64 + row1[1] * vy64 + row1[2] * vz64, sf);
        self.set_mac3(tz + row2[0] * vx64 + row2[1] * vy64 + row2[2] * vz64, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
    }

    /// NCS (Normal Color Single)
    /// Transforms a normal vector and applies light source calculations to determine vertex color.
    pub(super) fn ncs(&mut self, sf: u32, lm: bool, v_idx: usize) {
        let (vx, vy) = self.vxy(v_idx);
        let vz = self.vz(v_idx);
        let (m1, m2, m3) = self.ll_mul(vx, vy, vz);
        self.set_mac1(m1, sf);
        self.set_mac2(m2, sf);
        self.set_mac3(m3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        let (n1, n2, n3) = self.bk_lc_ir();
        self.set_mac1(n1, sf);
        self.set_mac2(n2, sf);
        self.set_mac3(n3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        let code = ((self.data[RGBC] >> GTE_CODE_SHIFT) & GTE_COLOR_MAX as u32) as u8;
        self.push_rgb(
            self.macv(MAC1) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC2) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC3) >> GTE_COLOR_FRACTION_SHIFT,
            code,
        );
    }

    /// NCCS (Normal Color Color Single)
    /// Transforms a normal vector, applies light source calculations, and adds ambient color.
    pub(super) fn nccs(&mut self, sf: u32, lm: bool, v_idx: usize) {
        let (vx, vy) = self.vxy(v_idx);
        let vz = self.vz(v_idx);
        let (m1, m2, m3) = self.ll_mul(vx, vy, vz);
        self.set_mac1(m1, sf);
        self.set_mac2(m2, sf);
        self.set_mac3(m3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        let (n1, n2, n3) = self.bk_lc_ir();
        self.set_mac1(n1, sf);
        self.set_mac2(n2, sf);
        self.set_mac3(n3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        let (p1, p2, p3, code) = self.rgb_ir();
        self.set_mac1(p1, sf);
        self.set_mac2(p2, sf);
        self.set_mac3(p3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(
            self.macv(MAC1) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC2) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC3) >> GTE_COLOR_FRACTION_SHIFT,
            code,
        );
    }

    /// CC (Color-Color Transformation)
    /// Transforms the color values in the RGBC register using the Color Transformation Matrix (LC)
    /// and adds the background color (BK).
    pub(super) fn cc(&mut self, sf: u32, lm: bool) {
        let (n1, n2, n3) = self.bk_lc_ir();
        self.set_mac1(n1, sf);
        self.set_mac2(n2, sf);
        self.set_mac3(n3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        let (p1, p2, p3, code) = self.rgb_ir();
        self.set_mac1(p1, sf);
        self.set_mac2(p2, sf);
        self.set_mac3(p3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(
            self.macv(MAC1) >> 4,
            self.macv(MAC2) >> 4,
            self.macv(MAC3) >> 4,
            code,
        );
    }

    /// CDP (Color Depth and Perspective)
    /// Performs color transformation, adds background color, and then applies far-color
    /// interpolation (depth-cueing) using the depth factor IR0.
    pub(super) fn cdp(&mut self, sf: u32, lm: bool) {
        let (n1, n2, n3) = self.bk_lc_ir();
        self.set_mac1(n1, sf);
        self.set_mac2(n2, sf);
        self.set_mac3(n3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        let ir0 = self.irv(IR0) as i64;
        let fc_r = self.ctrl[RFC] as i32 as i64;
        let fc_g = self.ctrl[GFC] as i32 as i64;
        let fc_b = self.ctrl[BFC] as i32 as i64;
        let r = (self.data[RGBC] & GTE_COLOR_MAX as u32) as i64;
        let g = ((self.data[RGBC] >> GTE_COLOR_GREEN_SHIFT) & GTE_COLOR_MAX as u32) as i64;
        let b = ((self.data[RGBC] >> GTE_COLOR_BLUE_SHIFT) & GTE_COLOR_MAX as u32) as i64;
        let code = ((self.data[RGBC] >> GTE_CODE_SHIFT) & GTE_COLOR_MAX as u32) as u8;
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        let p1 = fc_r * GTE_FIXED_POINT_ONE - (r * ir1 << GTE_COLOR_FRACTION_SHIFT);
        let p2 = fc_g * GTE_FIXED_POINT_ONE - (g * ir2 << GTE_COLOR_FRACTION_SHIFT);
        let p3 = fc_b * GTE_FIXED_POINT_ONE - (b * ir3 << GTE_COLOR_FRACTION_SHIFT);
        self.set_mac1(p1, sf);
        self.set_mac2(p2, sf);
        self.set_mac3(p3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        let q1 = (r * ir1 << GTE_COLOR_FRACTION_SHIFT) + self.irv(IR1) as i64 * ir0;
        let q2 = (g * ir2 << GTE_COLOR_FRACTION_SHIFT) + self.irv(IR2) as i64 * ir0;
        let q3 = (b * ir3 << GTE_COLOR_FRACTION_SHIFT) + self.irv(IR3) as i64 * ir0;
        self.set_mac1(q1, sf);
        self.set_mac2(q2, sf);
        self.set_mac3(q3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(
            self.macv(MAC1) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC2) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC3) >> GTE_COLOR_FRACTION_SHIFT,
            code,
        );
    }

    /// Internal implementation for Depth-cueing Perspective Color Single (DPCS)
    /// Interpolates between the input color and the far-color (RFC, GFC, BFC)
    /// using IR0 as the interpolation factor.
    pub(super) fn dpcs_inner(&mut self, sf: u32, lm: bool, rgbc: u32) {
        let r = (rgbc & GTE_COLOR_MAX as u32) as i64;
        let g = ((rgbc >> GTE_COLOR_GREEN_SHIFT) & GTE_COLOR_MAX as u32) as i64;
        let b = ((rgbc >> GTE_COLOR_BLUE_SHIFT) & GTE_COLOR_MAX as u32) as i64;
        let code = ((self.data[RGBC] >> GTE_CODE_SHIFT) & GTE_COLOR_MAX as u32) as u8;
        let ir0 = self.irv(IR0) as i64;
        let fc_r = self.ctrl[RFC] as i32 as i64;
        let fc_g = self.ctrl[GFC] as i32 as i64;
        let fc_b = self.ctrl[BFC] as i32 as i64;
        let m1 = (r << GTE_HALFWORD_SHIFT) + (fc_r - (r << GTE_COLOR_FRACTION_SHIFT)) * ir0;
        let m2 = (g << GTE_HALFWORD_SHIFT) + (fc_g - (g << GTE_COLOR_FRACTION_SHIFT)) * ir0;
        let m3 = (b << GTE_HALFWORD_SHIFT) + (fc_b - (b << GTE_COLOR_FRACTION_SHIFT)) * ir0;
        self.set_mac1(m1 >> GTE_COLOR_FRACTION_SHIFT, sf);
        self.set_mac2(m2 >> GTE_COLOR_FRACTION_SHIFT, sf);
        self.set_mac3(m3 >> GTE_COLOR_FRACTION_SHIFT, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(
            self.macv(MAC1) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC2) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC3) >> GTE_COLOR_FRACTION_SHIFT,
            code,
        );
    }

    /// INTPL (Interpolation)
    /// Interpolates between the current vertex color (IR registers) and the
    /// far-color (FC) using IR0 as the factor.
    pub(super) fn intpl(&mut self, sf: u32, lm: bool) {
        let ir0 = self.irv(IR0) as i64;
        let fc_r = self.ctrl[RFC] as i32 as i64;
        let fc_g = self.ctrl[GFC] as i32 as i64;
        let fc_b = self.ctrl[BFC] as i32 as i64;
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        let code = ((self.data[RGBC] >> GTE_CODE_SHIFT) & GTE_COLOR_MAX as u32) as u8;
        let m1 = (ir1 << GTE_SHIFT_FRACTIONAL) + (fc_r - ir1) * ir0;
        let m2 = (ir2 << GTE_SHIFT_FRACTIONAL) + (fc_g - ir2) * ir0;
        let m3 = (ir3 << GTE_SHIFT_FRACTIONAL) + (fc_b - ir3) * ir0;
        self.set_mac1(m1, sf);
        self.set_mac2(m2, sf);
        self.set_mac3(m3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(
            self.macv(MAC1) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC2) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC3) >> GTE_COLOR_FRACTION_SHIFT,
            code,
        );
    }

    /// DCPL (Depth-cueing Color)
    /// Alias for DPCS using the global RGBC register.
    pub(super) fn dcpl(&mut self, sf: u32, lm: bool) {
        let rgbc = self.data[RGBC];
        self.dpcs_inner(sf, lm, rgbc);
    }

    /// GPF (General-purpose Filter)
    /// Multiplies the IR registers by IR0 and pushes the result as a color.
    pub(super) fn gpf(&mut self, sf: u32, lm: bool) {
        let ir0 = self.irv(IR0) as i64;
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        let code = ((self.data[RGBC] >> GTE_CODE_SHIFT) & GTE_COLOR_MAX as u32) as u8;
        self.set_mac0(0);
        self.set_mac1(ir0 * ir1, sf);
        self.set_mac2(ir0 * ir2, sf);
        self.set_mac3(ir0 * ir3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(
            self.macv(MAC1) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC2) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC3) >> GTE_COLOR_FRACTION_SHIFT,
            code,
        );
    }

    /// GPL (General-purpose Filter with Accumulation)
    /// Multiplies the IR registers by IR0 and adds the previous MAC register values.
    pub(super) fn gpl(&mut self, sf: u32, lm: bool) {
        let ir0 = self.irv(IR0) as i64;
        let ir1 = self.irv(IR1) as i64;
        let ir2 = self.irv(IR2) as i64;
        let ir3 = self.irv(IR3) as i64;
        let mac1_old = self.macv(MAC1) as i64;
        let mac2_old = self.macv(MAC2) as i64;
        let mac3_old = self.macv(MAC3) as i64;
        let code = ((self.data[RGBC] >> GTE_CODE_SHIFT) & GTE_COLOR_MAX as u32) as u8;
        self.set_mac1((mac1_old << sf) + ir0 * ir1, sf);
        self.set_mac2((mac2_old << sf) + ir0 * ir2, sf);
        self.set_mac3((mac3_old << sf) + ir0 * ir3, sf);
        self.set_ir1(self.macv(MAC1), lm);
        self.set_ir2(self.macv(MAC2), lm);
        self.set_ir3(self.macv(MAC3), lm);
        self.push_rgb(
            self.macv(MAC1) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC2) >> GTE_COLOR_FRACTION_SHIFT,
            self.macv(MAC3) >> GTE_COLOR_FRACTION_SHIFT,
            code,
        );
    }

    /// AVSZ3 (Average Z for 3 Vertices)
    /// Calculates the average Z value of 3 vertices and stores it in the OTZ register.
    pub(super) fn avsz3(&mut self) {
        let zsf3 = self.ctrl[ZSF3] as i16 as i64;
        let sum = self.data[SZ1] as i64 + self.data[SZ2] as i64 + self.data[SZ3] as i64;
        let mac0 = zsf3 * sum;
        self.set_mac0(mac0 >> GTE_SHIFT_FRACTIONAL);
        self.data[OTZ] = (mac0 >> GTE_SHIFT_FRACTIONAL).clamp(0, GTE_SZ_MAX_I32 as i64) as u32;
    }

    /// AVSZ4 (Average Z for 4 Vertices)
    /// Calculates the average Z value of 4 vertices and stores it in the OTZ register.
    pub(super) fn avsz4(&mut self) {
        let zsf4 = self.ctrl[ZSF4] as i16 as i64;
        let sum = self.data[SZ0] as i64
            + self.data[SZ1] as i64
            + self.data[SZ2] as i64
            + self.data[SZ3] as i64;
        let mac0 = zsf4 * sum;
        self.set_mac0(mac0 >> GTE_SHIFT_FRACTIONAL);
        self.data[OTZ] = (mac0 >> GTE_SHIFT_FRACTIONAL).clamp(0, GTE_SZ_MAX_I32 as i64) as u32;
    }
}
