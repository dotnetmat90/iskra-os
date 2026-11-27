//! iskra-vmm-hello: the smallest possible VMM for Iskra's hypervisor (`/scheme/sys/vmm`).
//!
//! Creates a VM, loads a tiny 32-bit guest that prints a message to the serial port, sets the
//! vCPU registers, then runs it: every serial write traps back here, gets printed, and the guest
//! continues at the next instruction, until it halts. Must run as root.

use std::{
    fs::OpenOptions,
    io::{self, Write},
    os::unix::fs::FileExt,
};

/// Offsets with special meaning on a /scheme/sys/vmm handle (below 1 TiB = guest memory).
const REGS: u64 = 1 << 48;
const RUN: u64 = REGS + 0x1000;

// SVM exit codes.
const EXIT_INTR: u64 = 0x60;
const EXIT_HLT: u64 = 0x78;
const EXIT_IO: u64 = 0x7B;

const COM1: u64 = 0x3F8;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Seg {
    selector: u16,
    attrib: u16,
    limit: u32,
    base: u64,
}

/// Mirrors the kernel's VmRegs. gpr: rax rcx rdx rbx rsp rbp rsi rdi r8..r15;
/// seg: es cs ss ds fs gs gdtr ldtr idtr tr.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Regs {
    gpr: [u64; 16],
    rip: u64,
    rflags: u64,
    cr0: u64,
    cr2: u64,
    cr3: u64,
    cr4: u64,
    efer: u64,
    seg: [Seg; 10],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Exit {
    code: u64,
    info1: u64,
    info2: u64,
}

fn bytes<T>(v: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts((v as *const T).cast(), size_of::<T>()) }
}
fn bytes_mut<T>(v: &mut T) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut((v as *mut T).cast(), size_of::<T>()) }
}

/// Guest at guest-physical 0, 32-bit protected mode with paging:
///   mov esi, 0x40 ; mov dx, 0x3F8 ; loop: lodsb ; test al, al ; jz end ; out dx, al ; jmp loop
///   end: hlt
const CODE: [u8; 18] = [
    0xBE, 0x40, 0x00, 0x00, 0x00, 0x66, 0xBA, 0xF8, 0x03, 0xAC, 0x84, 0xC0, 0x74, 0x03, 0xEE,
    0xEB, 0xF8, 0xF4,
];
const MESSAGE: &[u8] = b"Hello from a VM started by a userspace program on Iskra!\n\0";

fn main() -> io::Result<()> {
    let vm = OpenOptions::new().read(true).write(true).open("/scheme/sys/vmm")?;

    // Guest memory: code at 0, message at 0x40, page directory at 0x1000 with one 4 MiB
    // identity page (present | writable | page size).
    vm.write_all_at(&CODE, 0)?;
    vm.write_all_at(MESSAGE, 0x40)?;
    vm.write_all_at(&0x83u32.to_le_bytes(), 0x1000)?;

    let mut regs = Regs {
        rflags: 0x2,
        cr0: 0x8000_0011, // PG | ET | PE
        cr3: 0x1000,
        cr4: 0x10, // PSE
        ..Default::default()
    };
    let flat_data = Seg { selector: 0x10, attrib: 0x0C93, limit: 0xFFFF_FFFF, base: 0 };
    for i in [0, 2, 3, 4, 5] {
        regs.seg[i] = flat_data;
    }
    regs.seg[1] = Seg { selector: 0x08, attrib: 0x0C9B, limit: 0xFFFF_FFFF, base: 0 };
    regs.seg[6] = Seg { limit: 0xFFFF, ..Default::default() }; // gdtr
    regs.seg[7] = Seg { attrib: 0x0082, limit: 0xFFFF, ..Default::default() }; // ldtr
    regs.seg[8] = Seg { limit: 0xFFFF, ..Default::default() }; // idtr
    regs.seg[9] = Seg { attrib: 0x008B, limit: 0xFFFF, ..Default::default() }; // tr
    vm.write_all_at(bytes(&regs), REGS)?;

    let mut out = io::stdout().lock();
    let mut exits = 0u64;
    loop {
        let mut exit = Exit::default();
        vm.read_exact_at(bytes_mut(&mut exit), RUN)?;
        exits += 1;
        match exit.code {
            EXIT_IO if exit.info1 & 1 == 0 && exit.info1 >> 16 == COM1 => {
                vm.read_exact_at(bytes_mut(&mut regs), REGS)?;
                out.write_all(&[regs.gpr[0] as u8])?;
                regs.rip = exit.info2; // next instruction after OUT
                vm.write_all_at(bytes(&regs), REGS)?;
            }
            EXIT_INTR => {} // a host interrupt was taken; just resume
            EXIT_HLT => {
                writeln!(out, "[iskra-vmm-hello] guest halted after {exits} exits")?;
                return Ok(());
            }
            code => {
                writeln!(
                    out,
                    "[iskra-vmm-hello] unexpected exit {code:#x} info1 {:#x} info2 {:#x}",
                    exit.info1, exit.info2
                )?;
                std::process::exit(1);
            }
        }
    }
}
