use super::{BiosCallVector, COP0_STATUS, Cpu, InterruptHook, exception_vector};
use crate::bus::Bus as CpuBusAccess;

const BIOS_A_MALLOC: u32 = 0x33;
const BIOS_A_STRLEN: u32 = 0x1b;
const BIOS_A_INIT_HEAP: u32 = 0x39;
const BIOS_A_WRITE: u32 = 0x03;
const BIOS_A_PUTCHAR: u32 = 0x3c;
const BIOS_A_PUTS: u32 = 0x3e;
const BIOS_A_PRINTF: u32 = 0x3f;
const BIOS_A_ISATTY: u32 = 0x07;
const BIOS_B_ALLOC_KERNEL_MEMORY: u32 = 0x00;
const BIOS_B_RETURN_FROM_EXCEPTION: u32 = 0x17;
const BIOS_B_RESET_ENTRY_INT: u32 = 0x18;
const BIOS_B_HOOK_ENTRY_INT: u32 = 0x19;
const BIOS_B_WRITE: u32 = 0x35;
const BIOS_B_PUTCHAR: u32 = 0x3d;
const BIOS_B_PUTS: u32 = 0x3f;
const BIOS_B_ISATTY: u32 = 0x39;
const EXCEPTION_VECTOR: u32 = 0x8000_0080;
const ROM_EXCEPTION_VECTOR: u32 = 0xbfc0_0180;
const KERNEL_VARIABLES_BASE: u32 = 0x0000_7460;
const KERNEL_VARIABLES_END: u32 = 0x0000_8920;
const BIOS_HEAP_START: u32 = 0x8001_0000;
const BIOS_STARTUP_COPY_PC: u32 = 0xbfc0_2b68;
const BIOS_STARTUP_COPY_EXIT_PC: u32 = 0xbfc0_2b80;
const BIOS_STARTUP_COPY_PATTERN: [u32; 6] = [
    0x90ae_0000,
    0x24c6_ffff,
    0x24a5_0001,
    0x2484_0001,
    0x1cc0_fffb,
    0xa08e_ffff,
];

#[derive(Clone, Debug)]
pub(super) struct BiosHle {
    pub(super) bios_heap: u32,
    pub(super) kernel_heap: u32,
    pub(super) kernel_heap_limit: u32,
    pub(super) interrupt_hook: Option<InterruptHook>,
    pub(super) interrupt_return_pc: Option<u32>,
    pub(super) interrupt_saved_registers: Option<([u32; 32], u32, u32)>,
}

impl Default for BiosHle {
    fn default() -> Self {
        Self::new()
    }
}

impl BiosHle {
    pub(super) fn new() -> Self {
        Self {
            bios_heap: BIOS_HEAP_START,
            kernel_heap: 0,
            kernel_heap_limit: 0,
            interrupt_hook: None,
            interrupt_return_pc: None,
            interrupt_saved_registers: None,
        }
    }

    pub(super) fn execute_bios_call(
        &mut self,
        cpu: &mut Cpu,
        vector: BiosCallVector,
        bus: &mut CpuBusAccess,
    ) {
        log::info!(
            "BIOS call: {} function={:#04x} a0={:#010x} a1={:#010x} a2={:#010x} ra={:#010x}",
            vector.name(),
            cpu.regs[9],
            cpu.regs[4],
            cpu.regs[5],
            cpu.regs[6],
            cpu.regs[31]
        );

        match (vector, cpu.regs[9]) {
            (BiosCallVector::A0, BIOS_A_MALLOC) => {
                self.allocate_bios_heap(cpu, cpu.regs[4]);
            }
            (BiosCallVector::A0, BIOS_A_STRLEN) => {
                cpu.regs[2] = read_c_string_len(bus, cpu.regs[4], 4096) as u32;
            }
            (BiosCallVector::A0, BIOS_A_INIT_HEAP) => {
                self.init_heap(cpu, cpu.regs[4], cpu.regs[5]);
            }
            (BiosCallVector::B0, BIOS_B_ALLOC_KERNEL_MEMORY) => {
                self.allocate_kernel_heap(cpu, cpu.regs[4]);
            }
            (BiosCallVector::B0, BIOS_B_RETURN_FROM_EXCEPTION) => {
                self.return_from_exception(cpu);
                return;
            }
            (BiosCallVector::B0, BIOS_B_RESET_ENTRY_INT) => {
                self.interrupt_hook = None;
            }
            (BiosCallVector::B0, BIOS_B_HOOK_ENTRY_INT) => {
                self.interrupt_hook = read_interrupt_hook(bus, cpu.regs[4]);
            }
            (BiosCallVector::C0, 0x07) => {
                install_exception_handlers(bus);
                cpu.regs[2] = 0;
            }
            (BiosCallVector::C0, 0x09) => {
                zero_kernel_variables(bus);
                cpu.regs[2] = 0;
            }
            (BiosCallVector::C0, 0x08) => {
                self.init_kernel_memory(cpu.regs[4], cpu.regs[5]);
                cpu.regs[2] = 0;
            }
            (BiosCallVector::C0, 0x12) => {
                cpu.regs[2] = 0;
            }
            (BiosCallVector::A0, BIOS_A_PUTCHAR) | (BiosCallVector::B0, BIOS_B_PUTCHAR) => {
                trace_tty_char(cpu.regs[4] as u8, cpu.trace_tty);
                cpu.regs[2] = cpu.regs[4] & 0xff;
            }
            (BiosCallVector::A0, BIOS_A_PUTS) | (BiosCallVector::B0, BIOS_B_PUTS) => {
                cpu.regs[2] = trace_tty_string(bus, cpu.regs[4], cpu.trace_tty) as u32;
            }
            (BiosCallVector::A0, BIOS_A_PRINTF) => {
                cpu.regs[2] = trace_tty_string(bus, cpu.regs[4], cpu.trace_tty) as u32;
            }
            (BiosCallVector::A0, BIOS_A_WRITE) | (BiosCallVector::B0, BIOS_B_WRITE) => {
                trace_tty_write(bus, cpu.regs[5], cpu.regs[6], cpu.trace_tty);
                cpu.regs[2] = cpu.regs[6];
            }
            (BiosCallVector::A0, BIOS_A_ISATTY) | (BiosCallVector::B0, BIOS_B_ISATTY) => {
                cpu.regs[2] = 1;
            }
            _ => {}
        }

        let return_address = cpu.regs[31];
        cpu.pc = return_address;
        cpu.next_pc = return_address.wrapping_add(4);
    }

    pub(super) fn enter_interrupt(
        &mut self,
        cpu: &mut Cpu,
        pc: u32,
        cause: u32,
        bus: &CpuBusAccess,
    ) {
        if cpu.trace_interrupts {
            eprintln!(
                "interrupt pc={pc:#010x} next_pc={:#010x} instr={:#010x} cause={cause:#010x} hook={:?} ra={:#010x}",
                cpu.next_pc,
                bus.peek32(pc).unwrap_or(0),
                self.interrupt_hook,
                cpu.regs[31]
            );
        }
        cpu.cop0[12] = pc;
        cpu.cop0[13] = cause;
        let status = cpu.cop0[COP0_STATUS];
        cpu.cop0[COP0_STATUS] = (status & !0x3f) | ((status << 2) & 0x3f);
        if let Some(hook) = self.interrupt_hook {
            self.interrupt_saved_registers = Some((cpu.regs, cpu.hi, cpu.lo));
            cpu.regs[16..24].copy_from_slice(&hook.saved);
            cpu.regs[28] = hook.gp;
            cpu.regs[29] = hook.sp;
            cpu.regs[30] = hook.fp;
            self.interrupt_return_pc = Some(pc);
            cpu.pc = hook.pc;
            cpu.next_pc = hook.pc.wrapping_add(4);
        } else {
            let vector = exception_vector(cpu.cop0[COP0_STATUS]);
            cpu.pc = vector;
            cpu.next_pc = vector.wrapping_add(4);
        }
    }

    pub(super) fn fast_forward_startup_copy(
        &mut self,
        cpu: &mut Cpu,
        bus: &mut CpuBusAccess,
    ) -> Option<u32> {
        if cpu.pc != BIOS_STARTUP_COPY_PC {
            return None;
        }

        for (index, expected) in BIOS_STARTUP_COPY_PATTERN.iter().copied().enumerate() {
            if bus
                .peek32(BIOS_STARTUP_COPY_PC.wrapping_add((index as u32) * 4))
                .ok()?
                != expected
            {
                return None;
            }
        }

        let count = cpu.regs[6];
        if count == 0 {
            cpu.pc = BIOS_STARTUP_COPY_EXIT_PC;
            cpu.next_pc = BIOS_STARTUP_COPY_EXIT_PC.wrapping_add(4);
            return Some(1);
        }

        let mut src = cpu.regs[5];
        let mut dst = cpu.regs[4];
        let mut last = 0;

        for _ in 0..count {
            let byte = bus.read8(src).ok()?;
            bus.write8(dst, byte).ok()?;
            last = byte;
            src = src.wrapping_add(1);
            dst = dst.wrapping_add(1);
        }

        cpu.regs[4] = dst;
        cpu.regs[5] = src;
        cpu.regs[6] = 0;
        cpu.regs[14] = last as u32;
        cpu.pc = BIOS_STARTUP_COPY_EXIT_PC;
        cpu.next_pc = BIOS_STARTUP_COPY_EXIT_PC.wrapping_add(4);

        Some(count.saturating_mul(5).max(1))
    }

    fn allocate_bios_heap(&mut self, cpu: &mut Cpu, size: u32) {
        let size = (size + 3) & !3;
        cpu.regs[2] = self.bios_heap;
        self.bios_heap = self.bios_heap.wrapping_add(size);
    }

    fn allocate_kernel_heap(&mut self, cpu: &mut Cpu, size: u32) {
        let size = (size + 3) & !3;
        if self.kernel_heap == 0 {
            cpu.regs[2] = 0;
            return;
        }

        let next = self.kernel_heap.wrapping_add(size);
        if next > self.kernel_heap_limit {
            cpu.regs[2] = 0;
            return;
        }

        cpu.regs[2] = self.kernel_heap;
        self.kernel_heap = next;
    }

    fn init_heap(&mut self, cpu: &mut Cpu, addr: u32, _size: u32) {
        self.bios_heap = addr;
        cpu.regs[2] = 0;
    }

    fn init_kernel_memory(&mut self, addr: u32, size: u32) {
        self.kernel_heap = addr;
        self.kernel_heap_limit = addr.wrapping_add(size);
    }

    fn return_from_exception(&mut self, cpu: &mut Cpu) {
        const COP0_STATUS_EXCEPTION_STACK_MASK: u32 = 0x3f;
        const COP0_EPC: usize = 14;
        let status = cpu.cop0[COP0_STATUS];
        cpu.cop0[COP0_STATUS] = (status & !COP0_STATUS_EXCEPTION_STACK_MASK)
            | ((status >> 2) & (COP0_STATUS_EXCEPTION_STACK_MASK >> 2));
        let return_address = self
            .interrupt_return_pc
            .take()
            .unwrap_or(cpu.cop0[COP0_EPC]);
        if cpu.trace_interrupts {
            eprintln!(
                "return interrupt pc={return_address:#010x} epc={:#010x}",
                cpu.cop0[COP0_EPC]
            );
        }
        if let Some((regs, hi, lo)) = self.interrupt_saved_registers.take() {
            cpu.regs = regs;
            cpu.hi = hi;
            cpu.lo = lo;
        }
        cpu.pc = return_address;
        cpu.next_pc = return_address.wrapping_add(4);
    }
}

fn trace_tty_write(bus: &mut CpuBusAccess, address: u32, length: u32, enabled: bool) {
    if !enabled || length == 0 {
        return;
    }

    let max_len = length.min(1024) as usize;
    let mut text = String::new();

    if let Some(bytes) = bus.ram_slice(address, max_len) {
        text.reserve(bytes.len());
        for &byte in bytes {
            let ch = match byte {
                b'\n' | b'\r' | b'\t' => byte as char,
                0x20..=0x7e => byte as char,
                _ => '.',
            };
            text.push(ch);
        }
    } else {
        for offset in 0..max_len as u32 {
            let byte = bus.read8(address.wrapping_add(offset)).unwrap_or(b'?');
            let ch = match byte {
                b'\n' | b'\r' | b'\t' => byte as char,
                0x20..=0x7e => byte as char,
                _ => '.',
            };
            text.push(ch);
        }
    }
    eprint!("{text}");
}

fn trace_tty_string(bus: &mut CpuBusAccess, address: u32, enabled: bool) -> usize {
    let mut bytes = Vec::new();
    for offset in 0..4096_u32 {
        let byte = match bus.read8(address.wrapping_add(offset)) {
            Ok(byte) => byte,
            Err(_) => break,
        };
        if byte == 0 {
            break;
        }
        bytes.push(byte);
    }

    if enabled && !bytes.is_empty() {
        let text = String::from_utf8_lossy(&bytes);
        eprint!("{text}");
    }

    bytes.len()
}

fn trace_tty_char(ch: u8, enabled: bool) {
    if !enabled {
        return;
    }

    let ch = match ch {
        b'\n' | b'\r' | b'\t' => ch as char,
        0x20..=0x7e => ch as char,
        _ => '.',
    };
    eprint!("{ch}");
}

fn read_c_string_len(bus: &mut CpuBusAccess, address: u32, max_len: usize) -> usize {
    let mut len = 0;
    while len < max_len {
        let byte = match bus.read8(address.wrapping_add(len as u32)) {
            Ok(byte) => byte,
            Err(_) => break,
        };
        if byte == 0 {
            break;
        }
        len += 1;
    }
    len
}

fn read_interrupt_hook(bus: &mut CpuBusAccess, address: u32) -> Option<InterruptHook> {
    let mut saved = [0; 8];
    for (index, value) in saved.iter_mut().enumerate() {
        *value = bus
            .read32(address.wrapping_add(0x0c + (index as u32 * 4)))
            .ok()?;
    }

    Some(InterruptHook {
        pc: bus.read32(address).ok()?,
        sp: bus.read32(address.wrapping_add(0x04)).ok()?,
        fp: bus.read32(address.wrapping_add(0x08)).ok()?,
        saved,
        gp: bus.read32(address.wrapping_add(0x2c)).ok()?,
    })
}

fn install_exception_handlers(bus: &mut CpuBusAccess) {
    const HANDLER_SIZE: u32 = 16;
    for offset in 0..HANDLER_SIZE {
        if let Ok(byte) = bus.read8(ROM_EXCEPTION_VECTOR.wrapping_add(offset)) {
            let _ = bus.write8(EXCEPTION_VECTOR.wrapping_add(offset), byte);
            let _ = bus.write8(0x8000_0000_u32.wrapping_add(offset), byte);
        }
    }
}

fn zero_kernel_variables(bus: &mut CpuBusAccess) {
    for addr in KERNEL_VARIABLES_BASE..KERNEL_VARIABLES_END {
        let _ = bus.write8(addr, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bios::Bios;
    use crate::bus::Bus;

    #[test]
    fn fast_forwards_startup_copy_loop() {
        let mut bios_bytes = vec![0; crate::bios::BIOS_SIZE];
        let copy_offset = (BIOS_STARTUP_COPY_PC - 0xbfc0_0000) as usize;
        let source_offset = (0xbfc1_ec45u32 - 0xbfc0_0000u32) as usize;
        let pattern = BIOS_STARTUP_COPY_PATTERN;

        for (index, word) in pattern.into_iter().enumerate() {
            let bytes = word.to_le_bytes();
            let offset = copy_offset + index * 4;
            bios_bytes[offset..offset + 4].copy_from_slice(&bytes);
        }
        bios_bytes[source_offset..source_offset + 4].copy_from_slice(b"ABCD");

        let bios = Bios::from_bytes(bios_bytes).unwrap();
        let mut bus = Bus::new(bios);
        let mut cpu = Cpu::new();
        cpu.set_pc(BIOS_STARTUP_COPY_PC);
        cpu.set_reg(4, 0x8003_6c45);
        cpu.set_reg(5, 0xbfc1_ec45);
        cpu.set_reg(6, 4);
        cpu.set_reg(14, 0);

        let cycles = cpu.step(&mut bus).expect("fast path should execute");

        assert_eq!(cycles, 20);
        assert_eq!(cpu.state().pc, BIOS_STARTUP_COPY_EXIT_PC);
        assert_eq!(cpu.state().next_pc, BIOS_STARTUP_COPY_EXIT_PC + 4);
        assert_eq!(cpu.reg(4), 0x8003_6c49);
        assert_eq!(cpu.reg(5), 0xbfc1_ec49);
        assert_eq!(cpu.reg(6), 0);
        assert_eq!(cpu.reg(14), b'D' as u32);
        assert_eq!(bus.read8(0x8003_6c45).unwrap(), b'A');
        assert_eq!(bus.read8(0x8003_6c46).unwrap(), b'B');
        assert_eq!(bus.read8(0x8003_6c47).unwrap(), b'C');
        assert_eq!(bus.read8(0x8003_6c48).unwrap(), b'D');
    }

    #[test]
    fn reads_interrupt_hook_from_expected_words() {
        let bios = Bios::from_bytes(vec![0; crate::bios::BIOS_SIZE]).unwrap();
        let mut bus = Bus::new(bios);
        let base = 0x8000_2000;
        let expected_saved = [
            0x1111_0001,
            0x1111_0002,
            0x1111_0003,
            0x1111_0004,
            0x1111_0005,
            0x1111_0006,
            0x1111_0007,
            0x1111_0008,
        ];

        bus.write32(base, 0x8000_1234).unwrap();
        bus.write32(base + 0x04, 0x801f_ff00).unwrap();
        bus.write32(base + 0x08, 0x801f_fef0).unwrap();
        for (index, value) in expected_saved.iter().copied().enumerate() {
            bus.write32(base + 0x0c + (index as u32 * 4), value)
                .unwrap();
        }
        bus.write32(base + 0x2c, 0xaabb_ccdd).unwrap();

        let hook = read_interrupt_hook(&mut bus, base).expect("hook should decode");

        assert_eq!(hook.pc, 0x8000_1234);
        assert_eq!(hook.sp, 0x801f_ff00);
        assert_eq!(hook.fp, 0x801f_fef0);
        assert_eq!(hook.saved, expected_saved);
        assert_eq!(hook.gp, 0xaabb_ccdd);
    }
}
