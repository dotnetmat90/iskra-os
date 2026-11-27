//! iskra-vmm-timer: a guest driven by a timer interrupt (hypervisor step 5).
//!
//! The guest installs its own GDT and IDT. The first tick is injected while the guest still has
//! interrupts disabled, so the kernel must hold it and deliver it right after STI (interrupt
//! window). Then the guest enables interrupts and halts; whenever it halts with
//! interrupts on, this VMM waits one tick (100 ms), steps it past HLT and injects interrupt 0x20,
//! the way a real VMM emulates a timer. The guest's handler prints `*` and counts; after five ticks
//! the guest disables interrupts and halts for good. Must run as root.

use std::{
    io::{self, Write},
    thread::sleep,
    time::{Duration, Instant},
};

use iskra_vmm::{COM1, EXIT_HLT, EXIT_INTR, EXIT_IO, RFLAGS_IF, Regs, Seg, Vm};

const TIMER_VECTOR: u8 = 0x20;
const TICK: Duration = Duration::from_millis(100);
const TICKS: u8 = 5;

// Guest-physical layout: 0x000 main, 0x020 handler, 0x210 tick counter, 0x300 GDT, 0x400 IDT,
// 0x1000 page directory, 0x2000-0x2FFF stack.
const MAIN: [u8; 17] = [
    0xFA, //                              0: cli
    0xE6, 0x80, //                        1: out 0x80, al           ; "ready": the VMM injects now,
    //                                       while interrupts are off -> kernel opens a window
    0xFB, //                              3: sti
    0x90, //                              4: nop                    ; window interrupt lands here
    0xF4, //                              5: hlt                    ; woken by the timer
    0x80, 0x3D, 0x10, 0x02, 0x00, 0x00, TICKS, // 6: cmp byte [0x210], TICKS
    0x72, 0xF6, //                       13: jb 5
    0xFA, //                             15: cli
    0xF4, //                             16: hlt                    ; done
];
const READY_PORT: u64 = 0x80;
const HANDLER_AT: u64 = 0x20;
const HANDLER: [u8; 18] = [
    0x50, //                              push eax
    0x52, //                              push edx
    0xFE, 0x05, 0x10, 0x02, 0x00, 0x00, // inc byte [0x210]
    0x66, 0xBA, 0xF8, 0x03, //            mov dx, 0x3F8
    0xB0, b'*', //                        mov al, '*'
    0xEE, //                              out dx, al
    0x5A, //                              pop edx
    0x58, //                              pop eax
    0xCF, //                              iret
];
const GDT_AT: u64 = 0x300;
const GDT: [u64; 3] = [
    0,
    0x00CF_9A00_0000_FFFF, // 0x08: 32-bit code, base 0, 4 GiB
    0x00CF_9200_0000_FFFF, // 0x10: data, base 0, 4 GiB
];
const IDT_AT: u64 = 0x400;

fn main() -> io::Result<()> {
    let vm = Vm::new()?;
    vm.write_mem(0, &MAIN)?;
    vm.write_mem(HANDLER_AT, &HANDLER)?;
    for (i, d) in GDT.iter().enumerate() {
        vm.write_mem(GDT_AT + 8 * i as u64, &d.to_le_bytes())?;
    }
    // IDT entry 0x20: 32-bit interrupt gate to the handler, code selector 0x08.
    let gate: u64 = (HANDLER_AT & 0xFFFF) | (0x08 << 16) | (0x8E00u64 << 32) | ((HANDLER_AT >> 16) << 48);
    vm.write_mem(IDT_AT + 8 * TIMER_VECTOR as u64, &gate.to_le_bytes())?;
    vm.write_mem(IDT_AT + 0x7F8, &[0; 8])?; // populate the rest of the IDT page
    vm.write_mem(0x1000, &0x83u32.to_le_bytes())?; // PDE 0: 4 MiB identity page
    vm.write_mem(0x2000, &[0; 4096])?; // stack page

    let mut regs = Regs::flat32(0x1000);
    regs.seg[6] = Seg { limit: 8 * GDT.len() as u32 - 1, base: GDT_AT, ..Default::default() };
    regs.seg[8] = Seg { limit: 0x7FF, base: IDT_AT, ..Default::default() };
    regs.gpr[4] = 0x3000; // esp
    vm.set_regs(&regs)?;

    let mut out = io::stdout().lock();
    let start = Instant::now();
    let mut injected = 0;
    loop {
        let exit = vm.run()?;
        match exit.code {
            EXIT_IO if exit.io() == (true, COM1) => {
                out.write_all(&[vm.regs()?.gpr[0] as u8])?;
                out.flush()?;
                vm.set_rip(exit.info2)?;
            }
            EXIT_IO if exit.io() == (true, READY_PORT) => {
                // First tick arrives while the guest has interrupts disabled: the kernel must
                // hold it until the guest executes STI (interrupt window) and deliver it then.
                vm.set_rip(exit.info2)?;
                vm.inject(TIMER_VECTOR)?;
                injected += 1;
            }
            EXIT_INTR => {}
            EXIT_HLT => {
                let mut r = vm.regs()?;
                if r.rflags & RFLAGS_IF == 0 {
                    writeln!(
                        out,
                        "\n[iskra-vmm-timer] guest took {injected} timer interrupts in {:.1} s and halted",
                        start.elapsed().as_secs_f64()
                    )?;
                    return Ok(());
                }
                // Halted waiting for an interrupt: wait one tick, then wake it with the timer.
                sleep(TICK);
                r.rip += 1; // HLT is one byte; the guest continues after it once the handler returns
                vm.set_regs(&r)?;
                vm.inject(TIMER_VECTOR)?;
                injected += 1;
            }
            code => {
                writeln!(out, "\n[iskra-vmm-timer] unexpected exit {code:#x} {exit:?}")?;
                std::process::exit(1);
            }
        }
    }
}
