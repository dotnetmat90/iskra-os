//! iskra-vmm-long: a 64-bit long-mode guest (hypervisor step 6).
//!
//! The guest runs with its own 4-level page tables, reads EFER with RDMSR (answered inside the
//! kernel from the VMCB) and reports it through I/O port 0xE9, then prints a greeting through the
//! serial port. Checks that long mode is really active (EFER.LME|LMA = 0x500). Must run as root.

use std::io::{self, Write};

use iskra_vmm::{COM1, EXIT_HLT, EXIT_INTR, EXIT_IO, Regs, Vm};

const DEBUG_PORT: u64 = 0xE9;
const CODE: [u8; 30] = [
    0xB9, 0x80, 0x00, 0x00, 0xC0, //  0: mov ecx, 0xC0000080   ; EFER
    0x0F, 0x32, //                     5: rdmsr
    0xC1, 0xE8, 0x08, //               7: shr eax, 8
    0xE6, 0xE9, //                    10: out 0xE9, al          ; report EFER bits 8-15
    0xBE, 0x80, 0x00, 0x00, 0x00, // 12: mov esi, 0x80        ; message
    0x66, 0xBA, 0xF8, 0x03, //        17: mov dx, 0x3F8
    0xAC, //                          21: lodsb
    0x84, 0xC0, //                    22: test al, al
    0x74, 0x03, //                    24: jz 29
    0xEE, //                          26: out dx, al
    0xEB, 0xF8, //                    27: jmp 21
    0xF4, //                          29: hlt
];
const MESSAGE: &[u8] = b"Hello from a 64-bit guest on Iskra!\n\0";

fn main() -> io::Result<()> {
    let vm = Vm::new()?;
    vm.write_mem(0, &CODE)?;
    vm.write_mem(0x80, MESSAGE)?;
    let pml4 = vm.identity_map_2m(0x1000, 1)?;
    vm.set_regs(&Regs::long64(pml4))?;

    let mut out = io::stdout().lock();
    let mut efer_hi = None;
    loop {
        let exit = vm.run()?;
        match exit.code {
            EXIT_IO if exit.io() == (true, COM1) => {
                out.write_all(&[vm.regs()?.gpr[0] as u8])?;
                vm.set_rip(exit.info2)?;
            }
            EXIT_IO if exit.io() == (true, DEBUG_PORT) => {
                efer_hi = Some(vm.regs()?.gpr[0] as u8);
                vm.set_rip(exit.info2)?;
            }
            EXIT_INTR => {}
            EXIT_HLT => {
                let ok = efer_hi == Some(0x05);
                writeln!(
                    out,
                    "[iskra-vmm-long] guest EFER bits 8-15 = {:#04x} (expect 0x05: LME|LMA) -> {}",
                    efer_hi.unwrap_or(0),
                    if ok { "PASS" } else { "FAIL" }
                )?;
                std::process::exit(if ok { 0 } else { 1 });
            }
            code => {
                writeln!(out, "[iskra-vmm-long] unexpected exit {code:#x} {exit:?}")?;
                std::process::exit(1);
            }
        }
    }
}
