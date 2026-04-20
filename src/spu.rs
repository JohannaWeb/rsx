use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub const SPU_RAM_SIZE: usize = 512 * 1024;
const VOICE_COUNT: usize = 24;
const TICKS_PER_SAMPLE: u32 = 768; // 33_868_800 Hz / 44100 Hz ≈ 768
const AUDIO_BUFFER_SAMPLES: usize = 4096;

// ADPCM prediction filter coefficients × 64
const FILTER_POS: [i32; 5] = [0, 60, 115, 98, 122];
const FILTER_NEG: [i32; 5] = [0, 0, -52, -55, -60];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdsrPhase {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone)]
struct Voice {
    // MMIO registers
    vol_left: i16,
    vol_right: i16,
    pitch: u16,
    adpcm_start: u32,  // SPU RAM byte address (reg value << 3)
    adpcm_repeat: u32,
    adsr1: u16,
    adsr2: u16,
    adsr_vol: i16,

    // ADSR runtime
    phase: AdsrPhase,
    adsr_counter: i32,

    // ADPCM decoder state
    old: i32,
    older: i32,
    current_addr: u32,
    sample_index: usize,
    decoded: [i16; 28],
    pitch_counter: u32,
    loop_start: u32,
    flags: u8,
}

impl Voice {
    fn new() -> Self {
        Self {
            vol_left: 0,
            vol_right: 0,
            pitch: 0,
            adpcm_start: 0,
            adpcm_repeat: 0,
            adsr1: 0,
            adsr2: 0,
            adsr_vol: 0,
            phase: AdsrPhase::Off,
            adsr_counter: 1,
            old: 0,
            older: 0,
            current_addr: 0,
            sample_index: 28,
            decoded: [0; 28],
            pitch_counter: 0,
            loop_start: 0,
            flags: 0,
        }
    }

    fn key_on(&mut self) {
        self.current_addr = self.adpcm_start;
        self.sample_index = 28;
        self.old = 0;
        self.older = 0;
        self.phase = AdsrPhase::Attack;
        self.adsr_vol = 0;
        self.adsr_counter = 1;
        self.pitch_counter = 0;
    }

    fn key_off(&mut self) {
        if self.phase != AdsrPhase::Off {
            self.phase = AdsrPhase::Release;
            self.adsr_counter = 1;
        }
    }

    fn decode_block(&mut self, ram: &[u8; SPU_RAM_SIZE]) {
        let addr = self.current_addr as usize & (SPU_RAM_SIZE - 1);
        let shift_filter = ram[addr];
        self.flags = ram[addr + 1];

        let shift = (shift_filter & 0x0f) as i32;
        let filter = ((shift_filter >> 4) & 0x07) as usize;
        let filter = filter.min(4);
        let f0 = FILTER_POS[filter];
        let f1 = FILTER_NEG[filter];

        for i in 0..14usize {
            let byte = ram[(addr + 2 + i) & (SPU_RAM_SIZE - 1)] as i32;
            let nibbles = [byte & 0x0f, byte >> 4];
            for (j, raw_nibble) in nibbles.iter().enumerate() {
                let nibble = if *raw_nibble >= 8 { raw_nibble - 16 } else { *raw_nibble };
                let sample = ((nibble << 12) >> shift) + (self.old * f0 + self.older * f1 + 32) / 64;
                let sample = sample.clamp(-32768, 32767);
                self.decoded[i * 2 + j] = sample as i16;
                self.older = self.old;
                self.old = sample;
            }
        }

        if self.flags & 0x04 != 0 {
            self.loop_start = self.current_addr;
        }

        self.current_addr = (self.current_addr + 16) & (SPU_RAM_SIZE as u32 - 1);

        if self.flags & 0x01 != 0 {
            if self.flags & 0x02 != 0 {
                self.current_addr = self.adpcm_repeat;
            } else {
                self.phase = AdsrPhase::Off;
                self.adsr_vol = 0;
                self.current_addr = self.adpcm_repeat;
            }
        }
    }

    fn next_sample(&mut self, ram: &[u8; SPU_RAM_SIZE]) -> i16 {
        if self.phase == AdsrPhase::Off {
            return 0;
        }

        self.pitch_counter += self.pitch as u32;
        let steps = (self.pitch_counter >> 12) as usize;
        self.pitch_counter &= 0xfff;

        for _ in 0..steps {
            self.sample_index += 1;
            if self.sample_index >= 28 {
                self.decode_block(ram);
                self.sample_index = 0;
            }
        }

        if self.sample_index >= 28 {
            return 0;
        }

        let raw = self.decoded[self.sample_index];
        let vol = self.adsr_vol as i32;
        ((raw as i32 * vol) >> 15) as i16
    }

    fn tick_adsr(&mut self) {
        self.adsr_counter -= 1;
        if self.adsr_counter > 0 {
            return;
        }

        match self.phase {
            AdsrPhase::Off => {
                self.adsr_counter = 1;
            }
            AdsrPhase::Attack => {
                let rate = ((self.adsr1 >> 8) & 0x7f) as u32;
                let exp = (self.adsr1 & 0x8000) != 0;
                let (step, wait) = adsr_rate(rate, false);
                let step = if exp && self.adsr_vol > 0x6000 { step >> 2 } else { step };
                self.adsr_vol = (self.adsr_vol as i32 + step).clamp(0, 0x7fff) as i16;
                self.adsr_counter = wait;
                if self.adsr_vol >= 0x7fff {
                    self.adsr_vol = 0x7fff;
                    self.phase = AdsrPhase::Decay;
                    self.adsr_counter = 1;
                }
            }
            AdsrPhase::Decay => {
                let dr = (self.adsr1 >> 4) & 0x0f;
                let rate = (dr << 2) | 0x18;
                let (step, wait) = adsr_rate(rate as u32, true);
                let step = (step * self.adsr_vol as i32) >> 15;
                self.adsr_vol = (self.adsr_vol as i32 + step).clamp(0, 0x7fff) as i16;
                self.adsr_counter = wait;
                let sustain_level = (((self.adsr1 & 0x0f) as i32 + 1) << 11).min(0x7fff);
                if (self.adsr_vol as i32) <= sustain_level {
                    self.adsr_vol = sustain_level as i16;
                    self.phase = AdsrPhase::Sustain;
                    self.adsr_counter = 1;
                }
            }
            AdsrPhase::Sustain => {
                let rate = ((self.adsr2 >> 6) & 0x7f) as u32;
                let exp = (self.adsr2 & 0x8000) != 0;
                let dec = (self.adsr2 & 0x4000) != 0;
                let (step, wait) = adsr_rate(rate, dec);
                let step = if exp && dec {
                    (step * self.adsr_vol as i32) >> 15
                } else if exp && !dec && self.adsr_vol > 0x6000 {
                    step >> 2
                } else {
                    step
                };
                self.adsr_vol = (self.adsr_vol as i32 + step).clamp(0, 0x7fff) as i16;
                self.adsr_counter = wait;
            }
            AdsrPhase::Release => {
                let rate = (self.adsr2 & 0x1f) as u32;
                let exp = (self.adsr2 & 0x0020) != 0;
                let (step, wait) = adsr_rate(rate << 2, true);
                let step = if exp { (step * self.adsr_vol as i32) >> 15 } else { step };
                self.adsr_vol = (self.adsr_vol as i32 + step).clamp(0, 0x7fff) as i16;
                self.adsr_counter = wait;
                if self.adsr_vol == 0 {
                    self.phase = AdsrPhase::Off;
                }
            }
        }
    }
}

// Returns (signed_step, wait_samples) for a given ADSR rate value (0-127).
fn adsr_rate(rate: u32, decreasing: bool) -> (i32, i32) {
    let rate = rate.min(127);
    let shift = 11i32.saturating_sub(rate as i32 / 4).max(0);
    let base = 1i32 << shift;
    let step = if decreasing { -base } else { base };
    let wait = 1i32.max(1 << ((rate as i32 - 44).max(0) / 4));
    (step, wait)
}

// Wrapper so cpal::Stream (which may not be Send on all platforms) can be
// stored in Bus, which lives entirely on the main thread.
struct HoldStream(cpal::Stream);
// Safety: the stream is only accessed from the main thread (start/stop);
// the audio callback runs on a cpal-internal thread but does not touch this wrapper.
unsafe impl Send for HoldStream {}

pub struct Spu {
    pub ram: Box<[u8; SPU_RAM_SIZE]>,
    voices: [Voice; VOICE_COUNT],
    main_vol_left: i16,
    main_vol_right: i16,
    reverb_vol_left: i16,
    reverb_vol_right: i16,
    noise_mode: u32,
    reverb_mode: u32,
    endx: u32,
    control: u16,
    status: u16,
    transfer_addr: u32,
    transfer_ctrl: u16,
    irq_addr: u32,
    cd_vol_left: i16,
    cd_vol_right: i16,
    reverb_regs: [u16; 32],
    tick_counter: u32,
    audio_buf: Arc<Mutex<VecDeque<i16>>>,
    _stream: Option<HoldStream>,
}

impl Spu {
    pub fn new() -> Self {
        let audio_buf: Arc<Mutex<VecDeque<i16>>> = Arc::new(Mutex::new(VecDeque::with_capacity(AUDIO_BUFFER_SAMPLES * 2)));
        let stream = Self::start_audio(Arc::clone(&audio_buf));

        let voices = std::array::from_fn(|_| Voice::new());

        Self {
            ram: vec![0u8; SPU_RAM_SIZE]
                .into_boxed_slice()
                .try_into()
                .expect("SPU RAM allocation failed"),
            voices,
            main_vol_left: 0x3fff,
            main_vol_right: 0x3fff,
            reverb_vol_left: 0,
            reverb_vol_right: 0,
            noise_mode: 0,
            reverb_mode: 0,
            endx: 0,
            control: 0,
            status: 0,
            transfer_addr: 0,
            transfer_ctrl: 0,
            irq_addr: 0,
            cd_vol_left: 0,
            cd_vol_right: 0,
            reverb_regs: [0; 32],
            tick_counter: 0,
            audio_buf,
            _stream: stream.map(HoldStream),
        }
    }

    fn start_audio(buf: Arc<Mutex<VecDeque<i16>>>) -> Option<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let config = cpal::StreamConfig {
            channels: 2,
            sample_rate: cpal::SampleRate(44100),
            buffer_size: cpal::BufferSize::Default,
        };

        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [i16], _| {
                    let mut guard = buf.lock().unwrap();
                    for sample in output.iter_mut() {
                        *sample = guard.pop_front().unwrap_or(0);
                    }
                },
                |err| log::error!("SPU audio stream error: {err}"),
                None,
            )
            .ok()?;

        stream.play().ok()?;
        Some(stream)
    }

    pub fn tick(&mut self) {
        self.tick_counter += 1;
        if self.tick_counter < TICKS_PER_SAMPLE {
            return;
        }
        self.tick_counter = 0;
        self.generate_sample();
    }

    fn generate_sample(&mut self) {
        let mut left = 0i32;
        let mut right = 0i32;

        for (i, voice) in self.voices.iter_mut().enumerate() {
            voice.tick_adsr();
            let sample = voice.next_sample(&self.ram) as i32;
            left += sample * voice.vol_left as i32 / 0x4000;
            right += sample * voice.vol_right as i32 / 0x4000;
            if voice.phase == AdsrPhase::Off {
                self.endx |= 1 << i;
            }
        }

        left = (left * self.main_vol_left as i32 / 0x4000).clamp(-32768, 32767);
        right = (right * self.main_vol_right as i32 / 0x4000).clamp(-32768, 32767);

        let mut guard = self.audio_buf.lock().unwrap();
        if guard.len() < AUDIO_BUFFER_SAMPLES * 2 {
            guard.push_back(left as i16);
            guard.push_back(right as i16);
        }
    }

    pub fn read16(&self, addr: u32) -> u16 {
        let offset = (addr - 0x1f80_1c00) as usize;

        // Voice registers: 0x000 - 0x17f (24 voices × 16 bytes)
        if offset < 0x180 {
            let v = offset / 16;
            let reg = offset % 16;
            return self.read_voice_reg(v, reg);
        }

        // Global registers: 0x180 - 0x1ff
        match offset {
            0x180 => self.main_vol_left as u16,
            0x182 => self.main_vol_right as u16,
            0x184 => self.reverb_vol_left as u16,
            0x186 => self.reverb_vol_right as u16,
            0x188 => 0, // key on lo (write-only)
            0x18a => 0, // key on hi
            0x18c => 0, // key off lo
            0x18e => 0, // key off hi
            0x190 => 0, // pitch mod lo
            0x192 => 0, // pitch mod hi
            0x194 => (self.noise_mode & 0xffff) as u16,
            0x196 => ((self.noise_mode >> 16) & 0xffff) as u16,
            0x198 => (self.reverb_mode & 0xffff) as u16,
            0x19a => ((self.reverb_mode >> 16) & 0xffff) as u16,
            0x19c => {
                let v = self.endx;
                (v & 0xffff) as u16
            }
            0x19e => ((self.endx >> 16) & 0xffff) as u16,
            0x1a2 => (self.irq_addr >> 3) as u16,
            0x1a4 => (self.transfer_addr >> 3) as u16,
            0x1a8 => 0, // data fifo
            0x1aa => self.control,
            0x1ac => self.transfer_ctrl,
            0x1ae => self.status,
            0x1b0 => self.cd_vol_left as u16,
            0x1b2 => self.cd_vol_right as u16,
            0x1c0..=0x1fe => {
                let idx = (offset - 0x1c0) / 2;
                self.reverb_regs[idx]
            }
            _ => 0,
        }
    }

    fn read_voice_reg(&self, v: usize, reg: usize) -> u16 {
        let voice = &self.voices[v];
        match reg {
            0x0 => voice.vol_left as u16,
            0x2 => voice.vol_right as u16,
            0x4 => voice.pitch,
            0x6 => (voice.adpcm_start >> 3) as u16,
            0x8 => voice.adsr1,
            0xa => voice.adsr2,
            0xc => voice.adsr_vol as u16,
            0xe => (voice.adpcm_repeat >> 3) as u16,
            _ => 0,
        }
    }

    pub fn write16(&mut self, addr: u32, value: u16) {
        let offset = (addr - 0x1f80_1c00) as usize;

        if offset < 0x180 {
            let v = offset / 16;
            let reg = offset % 16;
            self.write_voice_reg(v, reg, value);
            return;
        }

        match offset {
            0x180 => self.main_vol_left = value as i16,
            0x182 => self.main_vol_right = value as i16,
            0x184 => self.reverb_vol_left = value as i16,
            0x186 => self.reverb_vol_right = value as i16,
            0x188 => self.apply_key_on(value as u32),
            0x18a => self.apply_key_on((value as u32) << 16),
            0x18c => self.apply_key_off(value as u32),
            0x18e => self.apply_key_off((value as u32) << 16),
            0x194 => self.noise_mode = (self.noise_mode & 0xffff_0000) | value as u32,
            0x196 => self.noise_mode = (self.noise_mode & 0x0000_ffff) | ((value as u32) << 16),
            0x198 => self.reverb_mode = (self.reverb_mode & 0xffff_0000) | value as u32,
            0x19a => self.reverb_mode = (self.reverb_mode & 0x0000_ffff) | ((value as u32) << 16),
            0x19c | 0x19e => {} // endx is read-only
            0x1a2 => self.irq_addr = (value as u32) << 3,
            0x1a4 => self.transfer_addr = (value as u32) << 3,
            0x1a8 => self.fifo_write(value),
            0x1aa => {
                self.control = value;
                self.status = value & 0x3f;
            }
            0x1ac => self.transfer_ctrl = value,
            0x1b0 => self.cd_vol_left = value as i16,
            0x1b2 => self.cd_vol_right = value as i16,
            0x1c0..=0x1fe => {
                let idx = (offset - 0x1c0) / 2;
                self.reverb_regs[idx] = value;
            }
            _ => {}
        }
    }

    fn write_voice_reg(&mut self, v: usize, reg: usize, value: u16) {
        let voice = &mut self.voices[v];
        match reg {
            0x0 => voice.vol_left = value as i16,
            0x2 => voice.vol_right = value as i16,
            0x4 => voice.pitch = value,
            0x6 => voice.adpcm_start = (value as u32) << 3,
            0x8 => voice.adsr1 = value,
            0xa => voice.adsr2 = value,
            0xc => voice.adsr_vol = value as i16,
            0xe => voice.adpcm_repeat = (value as u32) << 3,
            _ => {}
        }
    }

    fn apply_key_on(&mut self, bits: u32) {
        for i in 0..VOICE_COUNT {
            if bits & (1 << i) != 0 {
                self.voices[i].key_on();
                self.endx &= !(1 << i);
            }
        }
    }

    fn apply_key_off(&mut self, bits: u32) {
        for i in 0..VOICE_COUNT {
            if bits & (1 << i) != 0 {
                self.voices[i].key_off();
            }
        }
    }

    fn fifo_write(&mut self, value: u16) {
        let addr = self.transfer_addr as usize & (SPU_RAM_SIZE - 1);
        let [lo, hi] = value.to_le_bytes();
        self.ram[addr] = lo;
        self.ram[(addr + 1) & (SPU_RAM_SIZE - 1)] = hi;
        self.transfer_addr = (self.transfer_addr + 2) & (SPU_RAM_SIZE as u32 - 1);
    }

    pub fn dma_write(&mut self, words: impl Iterator<Item = u32>) {
        for word in words {
            for byte in word.to_le_bytes() {
                let addr = self.transfer_addr as usize & (SPU_RAM_SIZE - 1);
                self.ram[addr] = byte;
                self.transfer_addr = (self.transfer_addr + 1) & (SPU_RAM_SIZE as u32 - 1);
            }
        }
    }

    pub fn dma_read(&mut self, count: usize) -> Vec<u32> {
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let addr = self.transfer_addr as usize & (SPU_RAM_SIZE - 1);
            let word = u32::from_le_bytes([
                self.ram[addr],
                self.ram[(addr + 1) & (SPU_RAM_SIZE - 1)],
                self.ram[(addr + 2) & (SPU_RAM_SIZE - 1)],
                self.ram[(addr + 3) & (SPU_RAM_SIZE - 1)],
            ]);
            out.push(word);
            self.transfer_addr = (self.transfer_addr + 4) & (SPU_RAM_SIZE as u32 - 1);
        }
        out
    }
}
