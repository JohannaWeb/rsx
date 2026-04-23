use super::{BiosCallVector, COP0_STATUS, Cpu, InterruptHook};
use crate::bus::Bus as CpuBusAccess;

const BIOS_A_MALLOC: u32 = 0x33;
const BIOS_A_WRITE: u32 = 0x03;
const BIOS_A_ISATTY: u32 = 0x07;
const BIOS_B_ALLOC_KERNEL_MEMORY: u32 = 0x00;
const BIOS_B_RETURN_FROM_EXCEPTION: u32 = 0x17;
const BIOS_B_RESET_ENTRY_INT: u32 = 0x18;
const BIOS_B_HOOK_ENTRY_INT: u32 = 0x19;
const BIOS_B_WRITE: u32 = 0x35;
const BIOS_B_ISATTY: u32 = 0x39;
const BIOS_HEAP_START: u32 = 0x8001_0000;

#[derive(Clone, Debug)]
pub(super) struct BiosHle {
    pub(super) bios_heap: u32,
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
            (BiosCallVector::B0, BIOS_B_ALLOC_KERNEL_MEMORY) => {
                self.allocate_bios_heap(cpu, cpu.regs[4]);
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
            self.interrupt_return_pc = Some(pc);
            cpu.pc = hook.pc;
            cpu.next_pc = hook.pc.wrapping_add(4);
        } else {
            cpu.pc = 0x8000_0080;
            cpu.next_pc = 0x8000_0084;
        }
    }

    fn allocate_bios_heap(&mut self, cpu: &mut Cpu, size: u32) {
        let size = (size + 3) & !3;
        cpu.regs[2] = self.bios_heap;
        self.bios_heap = self.bios_heap.wrapping_add(size);
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
        fp: bus.read32(address.wrapping_add(0x09)).ok()?,
        saved,
        gp: bus.read32(address.wrapping_add(0x2c)).ok()?,
    })
}
