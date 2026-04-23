const ROOT_COUNTER_VALUE_BYTES: usize = 2;
const ROOT_COUNTER_BASE_OFFSET: usize = 0x100;
const ROOT_COUNTER_STRIDE: usize = 0x10;
pub const ROOT_COUNTER_COUNT: usize = 3;
pub const VBLANK_INTERVAL_CYCLES: u32 = 564_480;

pub struct SystemTimers {
    root_counters: [u16; ROOT_COUNTER_COUNT],
    vblank_ticks: u32,
    vblank_interrupt: bool,
}

impl SystemTimers {
    pub fn new() -> Self {
        Self {
            root_counters: [0; ROOT_COUNTER_COUNT],
            vblank_ticks: 0,
            vblank_interrupt: false,
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        for counter in 0..ROOT_COUNTER_COUNT {
            self.root_counters[counter] = self.root_counters[counter].wrapping_add(cycles as u16);
        }

        self.vblank_ticks = self.vblank_ticks.saturating_add(cycles);
        if self.vblank_ticks >= VBLANK_INTERVAL_CYCLES {
            self.vblank_ticks %= VBLANK_INTERVAL_CYCLES;
            self.vblank_interrupt = true;
        }
    }

    pub fn write_root_counter_byte(&mut self, index: usize, byte_index: usize, value: u8) {
        let mut bytes = self.root_counters[index].to_le_bytes();
        bytes[byte_index] = value;
        self.root_counters[index] = u16::from_le_bytes(bytes);
    }

    pub fn take_vblank_interrupt(&mut self) -> bool {
        let pending = self.vblank_interrupt;
        self.vblank_interrupt = false;
        pending
    }

    pub fn sync_io_buffer(&self, io: &mut [u8]) {
        for counter in 0..ROOT_COUNTER_COUNT {
            let offset = ROOT_COUNTER_BASE_OFFSET + counter * ROOT_COUNTER_STRIDE;
            io[offset..offset + ROOT_COUNTER_VALUE_BYTES]
                .copy_from_slice(&self.root_counters[counter].to_le_bytes());
        }
    }
}
