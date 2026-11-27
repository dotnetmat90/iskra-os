//! iskra-vmm-linux: boot a Linux kernel in a VM on Iskra's hypervisor (hypervisor step 7).
//!
//! A deliberately small VMM: 64-bit Linux boot protocol (bzImage + initramfs), 128 MiB of RAM,
//! CPUID passthrough minus what it cannot provide, an 8250/16450 serial port wired to this
//! terminal (input and output, with interrupts), an 8259 PIC and an 8254 PIT for the timer,
//! CMOS clock registers, and "nothing here" (0xFF) for every other port. No PCI, no ACPI, no
//! local APIC: the kernel command line tells Linux not to look for them. Must run as root.
//!
//! The default initramfs starts a BusyBox shell on the serial console; type `exit` to power the
//! VM off. Input can also be piped in, e.g. `printf 'uname -a\nexit\n' | iskra-vmm-linux`.
//!
//! Usage: iskra-vmm-linux [bzImage] [initramfs] [extra kernel arguments...]

use std::{
    arch::x86_64::{__cpuid_count, _rdtsc},
    collections::VecDeque,
    env, fs,
    io::{self, Read, Write},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread::{self, sleep},
    time::{Duration, Instant},
};

use iskra_vmm::{
    EXIT_CPUID, EXIT_HLT, EXIT_INTR, EXIT_IO, EXIT_MSR, EXIT_NPF, EXIT_SHUTDOWN, EXIT_XSETBV,
    RFLAGS_IF, Regs, Seg, Vm,
};

const RAM: u64 = 128 << 20;
// Guest-physical layout (below 1 MiB, inside RAM).
const GDT_AT: u64 = 0x6000;
const BOOT_PARAMS_AT: u64 = 0x7000;
const PAGE_TABLES_AT: u64 = 0x9000; // PML4, PDPT, PD (identity map of the first 1 GiB)
const CMDLINE_AT: u64 = 0x20000;
const INITRD_AT: u64 = 96 << 20;

const CMDLINE: &str = "console=ttyS0 earlyprintk=serial,ttyS0,115200 rdinit=/init \
    nolapic noapic acpi=off pci=off nosmp nokaslr hpet=disable \
    tsc=reliable no_timer_check lpj=4000000 \
    i8042.noaux i8042.nokbd i8042.nopnp mitigations=off nopti panic=-1";

const PIT_HZ: f64 = 1_193_182.0;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let kernel_path = args.get(1).map(String::as_str).unwrap_or("/usr/share/iskra-vmm/vmlinuz");
    let initrd_path = args.get(2).map(String::as_str).unwrap_or("/usr/share/iskra-vmm/initramfs.cpio");
    let extra = args.get(3..).map(|a| a.join(" ")).unwrap_or_default();

    // Linux's own quick TSC calibration fails under emulation; measure the rate and pass it on.
    let tsc_khz = measure_tsc_khz();
    let extra = format!("tsc_early_khz={tsc_khz} {extra}");

    let kernel = fs::read(kernel_path)?;
    let initrd = fs::read(initrd_path)?;
    let vm = Vm::new()?;
    let entry = load_linux(&vm, &kernel, &initrd, &format!("{CMDLINE} {extra}"))?;
    eprintln!(
        "[iskra-vmm-linux] {} ({} KiB) + initramfs ({} KiB), {} MiB RAM, entry {entry:#x}",
        kernel_path,
        kernel.len() / 1024,
        initrd.len() / 1024,
        RAM >> 20
    );

    let mut regs = Regs::long64(PAGE_TABLES_AT);
    regs.seg[1].selector = 0x10; // __BOOT_CS
    let data = Seg { selector: 0x18, attrib: 0x0C93, limit: 0xFFFF_FFFF, base: 0 }; // __BOOT_DS
    for i in [0, 2, 3, 4, 5] {
        regs.seg[i] = data;
    }
    regs.seg[6] = Seg { limit: 31, base: GDT_AT, ..Default::default() };
    regs.rip = entry;
    regs.gpr[6] = BOOT_PARAMS_AT; // rsi
    vm.set_regs(&regs)?;

    // Terminal input arrives on its own thread so the VM never blocks on it.
    let (input_tx, input) = mpsc::channel::<u8>();
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut byte = [0u8; 1];
        while let Ok(1) = stdin.read(&mut byte) {
            if input_tx.send(byte[0]).is_err() {
                break;
            }
        }
    });

    let mut m = Machine::new();
    let start = Instant::now();
    let mut exits: u64 = 0;
    loop {
        m.take_input(&input);
        m.deliver(&vm)?;
        let exit = vm.run()?;
        exits += 1;
        match exit.code {
            EXIT_IO => m.io(&vm, &exit)?,
            EXIT_CPUID => {
                let mut r = vm.regs()?;
                let (a, b, c, d) = cpuid(r.gpr[0] as u32, r.gpr[1] as u32);
                r.gpr[0] = a as u64;
                r.gpr[3] = b as u64;
                r.gpr[1] = c as u64;
                r.gpr[2] = d as u64;
                r.rip += 2;
                vm.set_regs(&r)?;
            }
            EXIT_MSR => {
                // MSRs the kernel does not keep in the VMCB: reads return 0, writes are ignored.
                let mut r = vm.regs()?;
                if exit.info1 == 0 {
                    r.gpr[0] = 0;
                    r.gpr[2] = 0;
                }
                r.rip += 2;
                vm.set_regs(&r)?;
            }
            EXIT_XSETBV => {
                // XSAVE is hidden from the guest; skip the instruction if it tries anyway.
                vm.set_rip(vm.regs()?.rip + 3)?;
            }
            EXIT_HLT => {
                let mut r = vm.regs()?;
                if r.rflags & RFLAGS_IF == 0 {
                    eprintln!(
                        "\n[iskra-vmm-linux] guest halted with interrupts off (powered off) after {:.1} s: \
                         {exits} exits, {} timer ticks, {} absent-device pages",
                        start.elapsed().as_secs_f64(),
                        m.ticks,
                        m.absent_pages
                    );
                    eprintln!(
                        "[iskra-vmm-linux] devices: PIT reload {} (timer {}), PIC vector base {:#x} mask {:#04x}",
                        m.pit_reload,
                        if m.next_tick.is_some() { "running" } else { "never programmed" },
                        m.pic_base,
                        m.pic_mask
                    );
                    return Ok(());
                }
                // Idle: wait for the next timer tick or terminal input, whichever comes first.
                r.rip += 1;
                vm.set_regs(&r)?;
                if m.next_irq().is_none() {
                    let wait = m.until_tick().unwrap_or(Duration::from_millis(100));
                    match input.recv_timeout(wait) {
                        Ok(byte) => m.staged.push_back(byte),
                        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
                    }
                }
                m.tick();
            }
            EXIT_INTR => m.tick(), // a host interrupt: a timer tick may be due
            EXIT_NPF => {
                // Outside RAM: hardware Linux probes for but this VMM does not have. Back the page
                // with 0xFF bytes, the PC convention for "no device here", and let it continue.
                let page = exit.info2 & !0xFFF;
                if exit.info2 < RAM || m.absent_pages >= 64 {
                    let r = vm.regs()?;
                    eprintln!(
                        "\n[iskra-vmm-linux] unexpected nested page fault at {:#x} (rip {:#x})",
                        exit.info2, r.rip
                    );
                    std::process::exit(1);
                }
                m.absent_pages += 1;
                vm.write_mem(page, &[0xFF; 4096])?;
            }
            EXIT_SHUTDOWN => {
                let r = vm.regs()?;
                eprintln!("\n[iskra-vmm-linux] guest triple-faulted at rip {:#x}", r.rip);
                std::process::exit(1);
            }
            code => {
                let r = vm.regs()?;
                eprintln!(
                    "\n[iskra-vmm-linux] unhandled exit {code:#x} info1 {:#x} info2 {:#x} rip {:#x}",
                    exit.info1, exit.info2, r.rip
                );
                std::process::exit(1);
            }
        }
    }
}

/// Load a bzImage per the Linux x86 boot protocol (64-bit entry). Returns the entry address.
fn load_linux(vm: &Vm, kernel: &[u8], initrd: &[u8], cmdline: &str) -> io::Result<u64> {
    let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_string());
    if kernel.len() < 0x1000 || &kernel[0x202..0x206] != b"HdrS" {
        return Err(bad("not a bzImage"));
    }
    let u16_at = |o: usize| u16::from_le_bytes([kernel[o], kernel[o + 1]]);
    let u64_at = |o: usize| u64::from_le_bytes(kernel[o..o + 8].try_into().unwrap());
    if u16_at(0x206) < 0x020C || u16_at(0x236) & 1 == 0 {
        return Err(bad("kernel lacks the 64-bit boot entry (boot protocol >= 2.12, XLF_KERNEL_64)"));
    }
    let setup_sects = match kernel[0x1F1] {
        0 => 4,
        n => n as usize,
    };
    let load_at = u64_at(0x258); // pref_address
    let payload = &kernel[(setup_sects + 1) * 512..];

    // All of RAM is populated up front (zeros), including the legacy hole below 1 MiB that
    // Linux scans for BIOS tables.
    let zeros = vec![0u8; 1 << 20];
    for mib in 0..(RAM >> 20) {
        vm.write_mem(mib << 20, &zeros)?;
    }
    vm.write_mem(load_at, payload)?;
    vm.write_mem(INITRD_AT, initrd)?;
    let mut cmd = cmdline.as_bytes().to_vec();
    cmd.push(0);
    vm.write_mem(CMDLINE_AT, &cmd)?;

    // GDT: null, null, 0x10 64-bit code, 0x18 data (flat).
    let gdt: [u64; 4] = [0, 0, 0x00AF_9A00_0000_FFFF, 0x00CF_9200_0000_FFFF];
    for (i, d) in gdt.iter().enumerate() {
        vm.write_mem(GDT_AT + 8 * i as u64, &d.to_le_bytes())?;
    }
    vm.identity_map_2m(PAGE_TABLES_AT, 1)?;

    // Zero page: the setup header copied from the image, then the loader's fields.
    let mut bp = vec![0u8; 4096];
    let hdr_end = 0x202 + kernel[0x201] as usize;
    bp[0x1F1..hdr_end].copy_from_slice(&kernel[0x1F1..hdr_end]);
    bp[0x210] = 0xFF; // type_of_loader: undefined
    bp[0x211] |= 0x80; // loadflags: CAN_USE_HEAP
    bp[0x218..0x21C].copy_from_slice(&(INITRD_AT as u32).to_le_bytes()); // ramdisk_image
    bp[0x21C..0x220].copy_from_slice(&(initrd.len() as u32).to_le_bytes()); // ramdisk_size
    bp[0x228..0x22C].copy_from_slice(&(CMDLINE_AT as u32).to_le_bytes()); // cmd_line_ptr
    // e820 map: low memory and everything from 1 MiB.
    let e820: [(u64, u64, u32); 2] = [(0, 0x9F000, 1), (1 << 20, RAM - (1 << 20), 1)];
    bp[0x1E8] = e820.len() as u8;
    for (i, (addr, size, kind)) in e820.iter().enumerate() {
        let o = 0x2D0 + 20 * i;
        bp[o..o + 8].copy_from_slice(&addr.to_le_bytes());
        bp[o + 8..o + 16].copy_from_slice(&size.to_le_bytes());
        bp[o + 16..o + 20].copy_from_slice(&kind.to_le_bytes());
    }
    vm.write_mem(BOOT_PARAMS_AT, &bp)?;
    Ok(load_at + 0x200) // startup_64
}

/// TSC rate of this CPU in kHz, measured against the wall clock over 50 ms.
fn measure_tsc_khz() -> u64 {
    let (t0, i0) = (unsafe { _rdtsc() }, Instant::now());
    sleep(Duration::from_millis(50));
    let (t1, i1) = (unsafe { _rdtsc() }, Instant::now());
    ((t1 - t0) as f64 / (i1 - i0).as_secs_f64() / 1000.0) as u64
}

/// CPUID as the guest sees it: the host's answers minus what this VMM cannot provide.
fn cpuid(leaf: u32, sub: u32) -> (u32, u32, u32, u32) {
    let host = |l: u32, s: u32| {
        let r = __cpuid_count(l, s);
        (r.eax, r.ebx, r.ecx, r.edx)
    };
    let max_basic = host(0, 0).0.min(0xD);
    let max_ext = host(0x8000_0000, 0).0;
    match leaf {
        0 => {
            let (_, b, c, d) = host(0, 0);
            (max_basic, b, c, d)
        }
        1 => {
            let (a, _, mut c, mut d) = host(1, 0);
            // ECX: drop MONITOR(3), VMX(5), x2APIC(21), TSC-deadline(24), XSAVE(26), OSXSAVE(27),
            // AVX(28), F16C(29); set "running under a hypervisor" (31).
            c &= !((1 << 3) | (1 << 5) | (1 << 21) | (1 << 24) | (1 << 26) | (1 << 27) | (1 << 28) | (1 << 29));
            c |= 1 << 31;
            // EDX: no local APIC (9); this VMM emulates none.
            d &= !(1 << 9);
            // EBX: one logical CPU, APIC id 0, keep CLFLUSH line size.
            let b = (1 << 16) | (host(1, 0).1 & 0xFF00);
            (a, b, c, d)
        }
        2..=0xD if leaf <= max_basic => match leaf {
            7 if sub == 0 => {
                // Keep FSGSBASE(0), BMI1(3), ERMS(9), SMEP(7)/SMAP(20) off for simplicity: only
                // features that need no VMM support and no XSAVE.
                let (_, b, _, _) = host(7, 0);
                (0, b & ((1 << 3) | (1 << 8) | (1 << 9) | (1 << 19)), 0, 0)
            }
            4 | 0xB | 0xD | 6 | 7 => (0, 0, 0, 0),
            _ => host(leaf, sub),
        },
        0x4000_0000 => {
            // Hypervisor signature "IskraIskraIs".
            let sig = b"IskraIskraIs";
            let w = |i: usize| u32::from_le_bytes(sig[i..i + 4].try_into().unwrap());
            (0x4000_0000, w(0), w(4), w(8))
        }
        0x8000_0000 => (max_ext.min(0x8000_0008), 0, 0, 0),
        0x8000_0001 if max_ext >= 0x8000_0001 => {
            let (a, b, mut c, d) = host(0x8000_0001, 0);
            c &= !(1 << 2); // no nested SVM
            (a, b, c, d)
        }
        0x8000_0002..=0x8000_0008 if leaf <= max_ext => host(leaf, sub),
        _ => (0, 0, 0, 0),
    }
}

#[derive(PartialEq)]
enum Icw {
    VectorBase,
    Cascade,
    Mode,
}

/// The emulated PC devices.
struct Machine {
    // 8250/16450 UART at 0x3F8, IRQ 4.
    ier: u8,
    lcr: u8,
    mcr: u8,
    scr: u8,
    rx: VecDeque<u8>,
    /// Terminal input not yet handed to the UART (held until Linux enables receive interrupts,
    /// since the 8250 driver flushes the receiver while it initializes).
    staged: VecDeque<u8>,
    /// Transmitter-empty interrupt pending (set after a write or when it gets enabled).
    thre_pending: bool,
    // 8259 PIC (master): vector base, mask, in-service bits.
    pic_base: u8,
    pic_mask: u8,
    pic_isr: u8,
    /// Initialization bytes still expected on port 0x21 after ICW1, in order; empty = normal mode.
    pic_icw: Vec<Icw>,
    // 8254 PIT channel 0.
    pit_reload: u16,
    pit_lsb: Option<u8>,
    pit_since: Instant,
    next_tick: Option<Instant>,
    timer_pending: bool,
    // CMOS index.
    cmos_index: u8,
    serial_line: Vec<u8>,
    ticks: u64,
    absent_pages: u32,
}

impl Machine {
    fn new() -> Self {
        Machine {
            ier: 0,
            lcr: 0,
            mcr: 0,
            scr: 0,
            rx: VecDeque::new(),
            staged: VecDeque::new(),
            thre_pending: false,
            pic_base: 0x08,
            pic_mask: 0xFF,
            pic_isr: 0,
            pic_icw: Vec::new(),
            pit_reload: 0,
            pit_lsb: None,
            pit_since: Instant::now(),
            next_tick: None,
            timer_pending: false,
            cmos_index: 0,
            serial_line: Vec::new(),
            ticks: 0,
            absent_pages: 0,
        }
    }

    fn tick_period(&self) -> Option<Duration> {
        let reload = if self.pit_reload == 0 { 65536.0 } else { self.pit_reload as f64 };
        (self.pit_reload != 0 || self.next_tick.is_some()).then(|| Duration::from_secs_f64(reload / PIT_HZ))
    }

    fn until_tick(&self) -> Option<Duration> {
        self.next_tick.map(|t| t.saturating_duration_since(Instant::now()))
    }

    /// Mark IRQ 0 pending if the PIT has fired since the last tick.
    fn tick(&mut self) {
        let (Some(due), Some(period)) = (self.next_tick, self.tick_period()) else { return };
        let now = Instant::now();
        if now < due {
            return;
        }
        // Missed ticks collapse into one, like a real interrupt line.
        let mut next = due + period;
        while next <= now {
            next += period;
        }
        self.next_tick = Some(next);
        self.timer_pending = true;
    }

    /// Move terminal input into the UART once Linux listens for it.
    fn take_input(&mut self, input: &Receiver<u8>) {
        while let Ok(byte) = input.try_recv() {
            self.staged.push_back(byte);
        }
        // Receive interrupts on and OUT2 (the PC's IRQ gate) set outside loopback: the driver is
        // past its probe and FIFO flushes, so nothing typed is thrown away.
        if self.ier & 0x01 != 0 && self.mcr & 0x18 == 0x08 {
            self.rx.extend(self.staged.drain(..));
        }
    }

    /// Serial interrupt identification: receive data first, then transmitter empty.
    fn uart_irq(&self) -> Option<u8> {
        if self.ier & 0x01 != 0 && !self.rx.is_empty() {
            Some(0x04)
        } else if self.ier & 0x02 != 0 && self.thre_pending {
            Some(0x02)
        } else {
            None
        }
    }

    /// The IRQ to inject next, if any: one at a time (the PIC has nothing in service, which also
    /// matches the kernel's single pending-interrupt slot), timer before serial, masks respected.
    fn next_irq(&self) -> Option<u8> {
        if self.pic_isr != 0 {
            None
        } else if self.timer_pending && self.pic_mask & 0x01 == 0 {
            Some(0)
        } else if self.uart_irq().is_some() && self.pic_mask & 0x10 == 0 {
            Some(4)
        } else {
            None
        }
    }

    fn deliver(&mut self, vm: &Vm) -> io::Result<()> {
        let Some(irq) = self.next_irq() else { return Ok(()) };
        if irq == 0 {
            self.timer_pending = false;
            self.ticks += 1;
        }
        self.pic_isr |= 1 << irq;
        vm.inject(self.pic_base + irq)
    }

    fn io(&mut self, vm: &Vm, exit: &iskra_vmm::Exit) -> io::Result<()> {
        let mut r = vm.regs()?;
        let is_in = exit.info1 & 1 != 0;
        let port = (exit.info1 >> 16) as u16;
        let size = ((exit.info1 >> 4) & 0x7) as u32; // 1, 2 or 4 bytes
        if exit.info1 & (1 << 2) != 0 {
            eprintln!("\n[iskra-vmm-linux] string I/O on port {port:#x} is not supported (rip {:#x})", r.rip);
            std::process::exit(1);
        }
        let value = r.gpr[0] as u8;
        if is_in {
            let v: u32 = self.read(port).into();
            let v = if size == 1 { v } else { v | 0xFFFF_FF00 };
            let mask: u64 = match size {
                1 => 0xFF,
                2 => 0xFFFF,
                _ => 0xFFFF_FFFF,
            };
            r.gpr[0] = (r.gpr[0] & !mask) | (v as u64 & mask);
            r.rip = exit.info2;
            return vm.set_regs(&r);
        }
        self.write(port, value)?;
        vm.set_rip(exit.info2)
    }

    fn read(&mut self, port: u16) -> u8 {
        match port {
            0x3F8 if self.lcr & 0x80 == 0 => self.rx.pop_front().unwrap_or(0), // RBR
            0x3F9 if self.lcr & 0x80 == 0 => self.ier,
            0x3FA => match self.uart_irq() {
                Some(0x02) => {
                    self.thre_pending = false; // reading IIR clears the THRE interrupt
                    0x02
                }
                Some(id) => id,
                None => 0x01,
            },
            0x3FB => self.lcr,
            0x3FC => self.mcr,
            0x3FD => 0x60 | !self.rx.is_empty() as u8,   // LSR: THRE | TEMT, DR if data waits
            0x3FE if self.mcr & 0x10 != 0 => {
                // Loopback: modem status mirrors the modem control lines (8250 autodetection).
                let m = self.mcr;
                ((m & 0x01) << 5) | ((m & 0x02) << 3) | ((m & 0x04) << 4) | ((m & 0x08) << 4)
            }
            0x3FE => 0xB0,                               // MSR: DCD, DSR, CTS
            0x3FF => self.scr,
            0x20 => self.pic_isr,                        // PIC ISR (OCW3 read-select ignored)
            0x21 => self.pic_mask,
            0x40 => {
                // PIT channel 0 counter, counting down from the reload value.
                let reload = if self.pit_reload == 0 { 65536.0 } else { self.pit_reload as f64 };
                let ticks = self.pit_since.elapsed().as_secs_f64() * PIT_HZ;
                (reload - ticks % reload) as u32 as u8
            }
            0x61 => 0x20,                                // speaker/gate port: OUT2 high
            0x71 => match self.cmos_index & 0x7F {
                0x0A => 0x26,                            // UIP clear
                0x0B => 0x02,                            // 24-hour, BCD
                0x0D => 0x80,                            // valid RAM/time
                _ => 0,
            },
            _ => 0xFF,                                   // nothing here
        }
    }

    fn write(&mut self, port: u16, v: u8) -> io::Result<()> {
        match port {
            0x3F8 if self.lcr & 0x80 == 0 => {
                // Console output, flushed per line so it interleaves well with the host terminal.
                self.serial_line.push(v);
                // Line-buffered while the kernel prints logs; once userspace drives the port with
                // interrupts (a shell prompt has no newline), flush every byte.
                if v == b'\n' || self.serial_line.len() > 512 || self.ier & 0x02 != 0 {
                    let mut out = io::stdout().lock();
                    out.write_all(&self.serial_line)?;
                    out.flush()?;
                    self.serial_line.clear();
                }
                self.thre_pending = true; // transmitted at once: the holding register is empty again
            }
            0x3F9 if self.lcr & 0x80 == 0 => {
                if v & 0x02 != 0 && self.ier & 0x02 == 0 {
                    self.thre_pending = true; // enabling THRE with an empty transmitter interrupts
                }
                self.ier = v & 0x0F;
            }
            0x3FB => self.lcr = v,
            0x3FC => self.mcr = v,
            0x3FF => self.scr = v,
            0x20 if v & 0x10 != 0 => {
                // ICW1: ICW2 (vector base) follows on 0x21, then ICW3 unless single mode (bit 1),
                // then ICW4 if requested (bit 0). Kept in order of arrival.
                self.pic_icw = vec![Icw::VectorBase];
                if v & 0x02 == 0 {
                    self.pic_icw.push(Icw::Cascade);
                }
                if v & 0x01 != 0 {
                    self.pic_icw.push(Icw::Mode);
                }
            }
            0x20 if v & 0x18 == 0 && v & 0x20 != 0 => {
                // OCW2 end of interrupt: specific (bit 6, IRQ in bits 0-2) or the highest in service.
                if v & 0x40 != 0 {
                    self.pic_isr &= !(1 << (v & 7));
                } else {
                    self.pic_isr &= self.pic_isr.wrapping_sub(1);
                }
            }
            0x20 => {} // other OCW2/OCW3 commands need no state here
            0x21 if self.pic_icw.is_empty() => self.pic_mask = v,
            0x21 => {
                if self.pic_icw.remove(0) == Icw::VectorBase {
                    self.pic_base = v & 0xF8;
                }
            }
            0x43 => {
                // Mode/command: channel 0, lobyte/hibyte access starts a new reload value.
                if v >> 6 == 0 && (v >> 4) & 3 == 3 {
                    self.pit_lsb = None;
                }
            }
            0x40 => match self.pit_lsb.take() {
                None => self.pit_lsb = Some(v),
                Some(lsb) => {
                    self.pit_reload = u16::from_le_bytes([lsb, v]);
                    self.pit_since = Instant::now();
                    self.next_tick = Some(Instant::now() + self.tick_period().unwrap());
                }
            },
            0x70 => self.cmos_index = v,
            _ => {}                                      // ignored (0x80 delays, unknown devices)
        }
        Ok(())
    }
}
