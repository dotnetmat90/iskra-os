//! iskra-vmm-hello: the smallest possible VMM for Iskra's hypervisor (`/scheme/sys/vmm`).
//!
//! Creates a VM, loads a tiny 32-bit guest that prints a message to the serial port, sets the
//! vCPU registers, then runs it: every serial write traps back here, gets printed, and the guest
//! continues at the next instruction, until it halts. Must run as root.

use std::io::{self, Write};

use iskra_vmm::{COM1, EXIT_HLT, EXIT_INTR, EXIT_IO, Regs, Vm};

/// Guest at guest-physical 0:
///   mov esi, 0x40 ; mov dx, 0x3F8 ; loop: lodsb ; test al, al ; jz end ; out dx, al ; jmp loop
///   end: hlt
const CODE: [u8; 18] = [
    0xBE, 0x40, 0x00, 0x00, 0x00, 0x66, 0xBA, 0xF8, 0x03, 0xAC, 0x84, 0xC0, 0x74, 0x03, 0xEE,
    0xEB, 0xF8, 0xF4,
];
const MESSAGE: &[u8] = b"Hello from a VM started by a userspace program on Iskra!\n\0";

fn main() -> io::Result<()> {
    let vm = Vm::new()?;
    vm.write_mem(0, &CODE)?;
    vm.write_mem(0x40, MESSAGE)?;
    vm.write_mem(0x1000, &0x83u32.to_le_bytes())?; // PDE 0: 4 MiB identity page
    vm.set_regs(&Regs::flat32(0x1000))?;

    let mut out = io::stdout().lock();
    let mut exits = 0u64;
    loop {
        let exit = vm.run()?;
        exits += 1;
        match exit.code {
            EXIT_IO if exit.io() == (true, COM1) => {
                out.write_all(&[vm.regs()?.gpr[0] as u8])?;
                vm.set_rip(exit.info2)?; // next instruction after OUT
            }
            EXIT_INTR => {} // a host interrupt was taken; just resume
            EXIT_HLT => {
                writeln!(out, "[iskra-vmm-hello] guest halted after {exits} exits")?;
                return Ok(());
            }
            code => {
                writeln!(out, "[iskra-vmm-hello] unexpected exit {code:#x} {exit:?}")?;
                std::process::exit(1);
            }
        }
    }
}
