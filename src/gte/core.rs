// PS1 Geometry Transform Engine (COP2)

pub struct Gte {
    pub data: [u32; 32],
    pub ctrl: [u32; 32],
}

#[path = "commands.rs"]
mod commands;
#[path = "defs.rs"]
mod defs;
#[path = "math.rs"]
mod math;

pub(crate) use self::defs::*;

impl Gte {
    pub fn new() -> Self {
        Self {
            data: [0; GTE_REGISTER_COUNT],
            ctrl: [0; GTE_REGISTER_COUNT],
        }
    }

    pub fn read_data(&self, r: usize) -> u32 {
        match r {
            SXYP => self.data[SXY2],
            IRGB | ORGB => {
                let ir1 = ((self.data[IR1] as i16)
                    .clamp(GTE_IR0_MIN as i16, GTE_IRGB_CHANNEL_CLAMP_MAX)
                    as u32)
                    >> GTE_COLOR_PACK_SHIFT;
                let ir2 = ((self.data[IR2] as i16)
                    .clamp(GTE_IR0_MIN as i16, GTE_IRGB_CHANNEL_CLAMP_MAX)
                    as u32)
                    >> GTE_COLOR_PACK_SHIFT;
                let ir3 = ((self.data[IR3] as i16)
                    .clamp(GTE_IR0_MIN as i16, GTE_IRGB_CHANNEL_CLAMP_MAX)
                    as u32)
                    >> GTE_COLOR_PACK_SHIFT;
                ir1 | (ir2 << GTE_IR2_PACK_SHIFT) | (ir3 << GTE_IR3_PACK_SHIFT)
            }
            GTE_LZCS_REGISTER => self.data[GTE_LZCS_REGISTER],
            GTE_LZCR_REGISTER => {
                let v = self.data[GTE_LZCS_REGISTER] as i32;
                (if v >= 0 {
                    v.leading_zeros()
                } else {
                    (!v).leading_zeros()
                }) as u32
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
                self.data[IRGB] = v & GTE_IRGB_MASK;
                self.data[IR1] = (v & GTE_IRGB_CHANNEL_MASK) << GTE_COLOR_PACK_SHIFT;
                self.data[IR2] =
                    ((v >> GTE_IR2_PACK_SHIFT) & GTE_IRGB_CHANNEL_MASK) << GTE_COLOR_PACK_SHIFT;
                self.data[IR3] =
                    ((v >> GTE_IR3_PACK_SHIFT) & GTE_IRGB_CHANNEL_MASK) << GTE_COLOR_PACK_SHIFT;
            }
            _ => self.data[r] = v,
        }
    }

    pub fn read_ctrl(&self, r: usize) -> u32 {
        if r == FLAG {
            self.flag_reg()
        } else {
            self.ctrl[r]
        }
    }

    pub fn write_ctrl(&mut self, r: usize, v: u32) {
        self.ctrl[r] = if r == FLAG {
            v & GTE_FLAG_WRITE_MASK
        } else {
            v
        };
    }

    fn flag_reg(&self) -> u32 {
        let f = self.ctrl[FLAG] & GTE_FLAG_WRITE_MASK;
        let error = (f & FLAG_ERROR_MASK) != 0;
        f | if error { 1 << flags::ERROR } else { 0 }
    }

    pub fn execute(&mut self, cmd: u32) {
        self.ctrl[FLAG] = 0;
        let sf: u32 = if (cmd >> GTE_SF_BIT_SHIFT) & 1 != 0 {
            GTE_SHIFT_FRACTIONAL
        } else {
            GTE_SHIFT_NONE
        };
        let lm = (cmd >> GTE_LM_BIT_SHIFT) & 1 != 0;
        let command = cmd & GTE_COMMAND_MASK;

        match command {
            v if v == GteCommand::Rtps as u32 => self.rtps(sf, lm, 0),
            v if v == GteCommand::Nclip as u32 => self.nclip(),
            v if v == GteCommand::Op as u32 => self.op(sf, lm),
            v if v == GteCommand::Dpcs as u32 => self.dpcs_inner(sf, lm, self.data[RGBC]),
            v if v == GteCommand::Intpl as u32 => self.intpl(sf, lm),
            v if v == GteCommand::Mvmva as u32 => self.mvmva(sf, lm, cmd),
            v if v == GteCommand::Ncds as u32 => self.ncs(sf, lm, 0),
            v if v == GteCommand::Cdp as u32 => self.cdp(sf, lm),
            v if v == GteCommand::Ncdt as u32 => {
                self.ncs(sf, lm, 0);
                self.ncs(sf, lm, 1);
                self.ncs(sf, lm, 2);
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
}
