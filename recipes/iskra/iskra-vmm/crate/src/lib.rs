//! Userspace side of Iskra's hypervisor: a thin wrapper over `/scheme/sys/vmm`.
//!
//! The kernel file is driven by offsets: below 1 TiB is guest memory, [`REGS`] the vCPU registers,
//! [`RUN`] runs the guest until its next exit, [`INJECT`] queues an external interrupt.

use std::{
    fs::{File, OpenOptions},
    io,
    os::unix::fs::FileExt,
};

pub const REGS: u64 = 1 << 48;
pub const RUN: u64 = REGS + 0x1000;
pub const INJECT: u64 = REGS + 0x2000;

// SVM exit codes (AMD APM vol. 2, appendix C).
pub const EXIT_INTR: u64 = 0x60;
pub const EXIT_HLT: u64 = 0x78;
pub const EXIT_IO: u64 = 0x7B;

pub const COM1: u64 = 0x3F8;
pub const RFLAGS_IF: u64 = 1 << 9;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Seg {
    pub selector: u16,
    pub attrib: u16,
    pub limit: u32,
    pub base: u64,
}

/// Mirrors the kernel's VmRegs. gpr: rax rcx rdx rbx rsp rbp rsi rdi r8..r15;
/// seg: es cs ss ds fs gs gdtr ldtr idtr tr.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Regs {
    pub gpr: [u64; 16],
    pub rip: u64,
    pub rflags: u64,
    pub cr0: u64,
    pub cr2: u64,
    pub cr3: u64,
    pub cr4: u64,
    pub efer: u64,
    pub seg: [Seg; 10],
}

impl Regs {
    /// 32-bit protected mode with paging: flat code (selector 0x08) and data (0x10) segments,
    /// page directory at guest-physical `page_dir` using 4 MiB pages.
    pub fn flat32(page_dir: u64) -> Self {
        let mut r = Regs {
            rflags: 0x2,
            cr0: 0x8000_0011, // PG | ET | PE
            cr3: page_dir,
            cr4: 0x10, // PSE
            ..Default::default()
        };
        let data = Seg { selector: 0x10, attrib: 0x0C93, limit: 0xFFFF_FFFF, base: 0 };
        for i in [0, 2, 3, 4, 5] {
            r.seg[i] = data;
        }
        r.seg[1] = Seg { selector: 0x08, attrib: 0x0C9B, limit: 0xFFFF_FFFF, base: 0 };
        r.seg[6] = Seg { limit: 0xFFFF, ..Default::default() }; // gdtr
        r.seg[7] = Seg { attrib: 0x0082, limit: 0xFFFF, ..Default::default() }; // ldtr
        r.seg[8] = Seg { limit: 0xFFFF, ..Default::default() }; // idtr
        r.seg[9] = Seg { attrib: 0x008B, limit: 0xFFFF, ..Default::default() }; // tr
        r
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Exit {
    pub code: u64,
    pub info1: u64,
    pub info2: u64,
}

impl Exit {
    /// For an I/O exit: (is a write, port).
    pub fn io(&self) -> (bool, u64) {
        (self.info1 & 1 == 0, self.info1 >> 16)
    }
}

fn bytes<T>(v: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts((v as *const T).cast(), size_of::<T>()) }
}
fn bytes_mut<T>(v: &mut T) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut((v as *mut T).cast(), size_of::<T>()) }
}

/// One VM with one vCPU. Dropping it destroys the VM.
pub struct Vm(File);

impl Vm {
    pub fn new() -> io::Result<Self> {
        Ok(Vm(OpenOptions::new().read(true).write(true).open("/scheme/sys/vmm")?))
    }
    pub fn write_mem(&self, gpa: u64, data: &[u8]) -> io::Result<()> {
        self.0.write_all_at(data, gpa)
    }
    pub fn regs(&self) -> io::Result<Regs> {
        let mut r = Regs::default();
        self.0.read_exact_at(bytes_mut(&mut r), REGS)?;
        Ok(r)
    }
    pub fn set_regs(&self, r: &Regs) -> io::Result<()> {
        self.0.write_all_at(bytes(r), REGS)
    }
    pub fn run(&self) -> io::Result<Exit> {
        let mut e = Exit::default();
        self.0.read_exact_at(bytes_mut(&mut e), RUN)?;
        Ok(e)
    }
    /// Deliver external interrupt `vector` as soon as the guest can take it.
    pub fn inject(&self, vector: u8) -> io::Result<()> {
        self.0.write_all_at(&(vector as usize).to_ne_bytes(), INJECT)
    }
    /// Move the guest past an instruction that exited (e.g. after emulating it).
    pub fn set_rip(&self, rip: u64) -> io::Result<()> {
        let mut r = self.regs()?;
        r.rip = rip;
        self.set_regs(&r)
    }
}
