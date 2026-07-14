const ROOT_COUNTER_VALUE_BYTES: usize = 2;
const ROOT_COUNTER_MODE_OFFSET: usize = 4;
const ROOT_COUNTER_TARGET_OFFSET: usize = 8;
const ROOT_COUNTER_BASE_OFFSET: usize = 0x100;
const ROOT_COUNTER_STRIDE: usize = 0x10;
pub const ROOT_COUNTER_COUNT: usize = 3;
pub const VBLANK_INTERVAL_CYCLES: u32 = 564_480;

// Counter Mode register bit layout (0x1f801104/14/24), per the PS1 hardware docs.
const COUNTER_MODE_RESET_ON_TARGET_BIT: u16 = 1 << 3;
const COUNTER_MODE_IRQ_ON_TARGET_BIT: u16 = 1 << 4;
const COUNTER_MODE_IRQ_ON_OVERFLOW_BIT: u16 = 1 << 5;
const COUNTER_MODE_IRQ_REPEAT_BIT: u16 = 1 << 6;
const COUNTER_MODE_REACHED_TARGET_BIT: u16 = 1 << 11;
const COUNTER_MODE_REACHED_OVERFLOW_BIT: u16 = 1 << 12;
const COUNTER_MODE_LATCH_BITS: u16 =
    COUNTER_MODE_REACHED_TARGET_BIT | COUNTER_MODE_REACHED_OVERFLOW_BIT;
const COUNTER_VALUE_PERIOD: u32 = 1 << 16;

pub struct SystemTimers {
    root_counters: [u16; ROOT_COUNTER_COUNT],
    mode: [u16; ROOT_COUNTER_COUNT],
    target: [u16; ROOT_COUNTER_COUNT],
    // Tracks whether a one-shot (non-repeat) IRQ has already fired since the
    // mode register was last written, so it isn't re-raised every tick.
    one_shot_fired: [bool; ROOT_COUNTER_COUNT],
    pending_interrupt: [bool; ROOT_COUNTER_COUNT],
    vblank_ticks: u32,
    vblank_interrupt: bool,
}

impl SystemTimers {
    pub fn new() -> Self {
        Self {
            root_counters: [0; ROOT_COUNTER_COUNT],
            mode: [0; ROOT_COUNTER_COUNT],
            target: [0; ROOT_COUNTER_COUNT],
            one_shot_fired: [false; ROOT_COUNTER_COUNT],
            pending_interrupt: [false; ROOT_COUNTER_COUNT],
            vblank_ticks: 0,
            vblank_interrupt: false,
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        for counter in 0..ROOT_COUNTER_COUNT {
            self.tick_counter(counter, cycles);
        }

        self.vblank_ticks = self.vblank_ticks.saturating_add(cycles);
        if self.vblank_ticks >= VBLANK_INTERVAL_CYCLES {
            self.vblank_ticks %= VBLANK_INTERVAL_CYCLES;
            self.vblank_interrupt = true;
        }
    }

    // Simplification: hardware sync modes (pause/reset gated on H/V-blank) are
    // not modeled, so every counter always free-runs at the system clock rate.
    fn tick_counter(&mut self, index: usize, cycles: u32) {
        let mode = self.mode[index];
        let target = self.target[index] as u32;
        let reset_on_target = mode & COUNTER_MODE_RESET_ON_TARGET_BIT != 0;
        let irq_on_target = mode & COUNTER_MODE_IRQ_ON_TARGET_BIT != 0;
        let irq_on_overflow = mode & COUNTER_MODE_IRQ_ON_OVERFLOW_BIT != 0;

        let start = self.root_counters[index] as u32;
        let total = start + cycles;

        // The counter always sweeps through its target value once per 0..=0xffff
        // pass, whether or not reset-on-target is enabled.
        if target != 0 && start < target && total >= target {
            self.mode[index] |= COUNTER_MODE_REACHED_TARGET_BIT;
            if irq_on_target {
                self.raise_interrupt(index);
            }
        }

        let period = if reset_on_target && target != 0 {
            target
        } else {
            COUNTER_VALUE_PERIOD
        };

        if total >= period {
            if period == COUNTER_VALUE_PERIOD {
                self.mode[index] |= COUNTER_MODE_REACHED_OVERFLOW_BIT;
                if irq_on_overflow {
                    self.raise_interrupt(index);
                }
            }
            self.root_counters[index] = (total % period) as u16;
        } else {
            self.root_counters[index] = total as u16;
        }
    }

    fn raise_interrupt(&mut self, index: usize) {
        let repeat = self.mode[index] & COUNTER_MODE_IRQ_REPEAT_BIT != 0;
        if repeat || !self.one_shot_fired[index] {
            self.pending_interrupt[index] = true;
            self.one_shot_fired[index] = true;
        }
    }

    pub fn take_counter_interrupt(&mut self, index: usize) -> bool {
        let pending = self.pending_interrupt[index];
        self.pending_interrupt[index] = false;
        pending
    }

    pub fn write_root_counter_byte(&mut self, index: usize, byte_index: usize, value: u8) {
        let mut bytes = self.root_counters[index].to_le_bytes();
        bytes[byte_index] = value;
        self.root_counters[index] = u16::from_le_bytes(bytes);
    }

    pub fn write_counter_mode_byte(&mut self, index: usize, byte_index: usize, value: u8) {
        let mut bytes = self.mode[index].to_le_bytes();
        bytes[byte_index] = value;
        // Real hardware resets the counter value and latched status bits on any
        // write to the mode register (and re-arms one-shot IRQs). We also clear
        // the "reached" latch bits here rather than on next read, which is a
        // simplification versus real hardware's reset-on-read behavior.
        self.mode[index] = u16::from_le_bytes(bytes) & !COUNTER_MODE_LATCH_BITS;
        self.root_counters[index] = 0;
        self.one_shot_fired[index] = false;
    }

    pub fn write_counter_target_byte(&mut self, index: usize, byte_index: usize, value: u8) {
        let mut bytes = self.target[index].to_le_bytes();
        bytes[byte_index] = value;
        self.target[index] = u16::from_le_bytes(bytes);
    }

    pub fn take_vblank_interrupt(&mut self) -> bool {
        let pending = self.vblank_interrupt;
        self.vblank_interrupt = false;
        pending
    }

    pub fn sync_io_buffer(&self, io: &mut [u8]) {
        for counter in 0..ROOT_COUNTER_COUNT {
            let base = ROOT_COUNTER_BASE_OFFSET + counter * ROOT_COUNTER_STRIDE;
            io[base..base + ROOT_COUNTER_VALUE_BYTES]
                .copy_from_slice(&self.root_counters[counter].to_le_bytes());
            let mode_offset = base + ROOT_COUNTER_MODE_OFFSET;
            io[mode_offset..mode_offset + ROOT_COUNTER_VALUE_BYTES]
                .copy_from_slice(&self.mode[counter].to_le_bytes());
            let target_offset = base + ROOT_COUNTER_TARGET_OFFSET;
            io[target_offset..target_offset + ROOT_COUNTER_VALUE_BYTES]
                .copy_from_slice(&self.target[counter].to_le_bytes());
        }
    }
}
