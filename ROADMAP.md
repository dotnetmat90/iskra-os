# Iskra roadmap

Revised 2026-11-27 (hypervisor steps 1–5 done the same day) after reviewing the "Iskra OS Master Blueprint" against what Redox can do today.
The blueprint's vision stays: an OS built for AI agents, where the microkernel decides what an agent
can see. Its 12-week schedule does not: it packs years of systems work (a hypervisor, IOMMU passthrough,
a Linux driver VM, GPU gaming, power management) into weeks 5–12. This roadmap keeps the ambition but
orders the work so every phase ends in something that runs and can be shown.

## What makes Iskra worth building

1. **Agents that physically cannot see your secrets.** Redox gives each process its own namespace;
   an agent launched with only the project directory gets "no such file" for `~/.ssh`. Upstream
   already ships the mechanism (`contain`, see `config/desktop-contain.toml`). Nobody has built an
   agent product on it.
2. **An AI-native desktop:** Super+Space bar to launch apps or ask, push-to-talk, permission prompts
   in the same bar.
3. **Rust all the way down**, MIT-licensed, small enough to understand.

Everything else (snapshots, self-healing, hypervisor, gaming) supports these or comes later.

## Hardware and what each machine is for

| Machine | Role | Facts that shape the plan |
|---|---|---|
| MacBook M1 Pro | Editing, AI work, quick builds (OrbStack VM) | x86 Iskra is emulated there: usable for checks, slow for daily use. |
| Proxmox server | Main build and test host (see `PROXMOX.md`) | KVM: Iskra at native speed, web console as screen, e1000/virtio network that Redox drives. Nested virtualization for hypervisor work. |
| ASUS Vivobook 15X (Ryzen 9 5900HX + NVIDIA) | Occasional bare-metal check; AMD SVM target for the hypervisor | Redox has **no** driver for its Wi-Fi, the TP-Link UE300C USB Ethernet (RTL8153), the Radeon iGPU or the NVIDIA dGPU (drivers in the image: e1000, rtl8139/8168, ixgbe, virtio, Intel HD graphics, xHCI, USB HID/storage, HDA audio). The display works through the UEFI framebuffer; keyboard and NVMe should work; touchpad and network will not. |

Consequences:
- No "OTA push over the 1 Gbps cable" to a running Iskra on the laptop: Iskra can't drive that adapter.
  What works instead is **network boot**: if the laptop's UEFI can PXE through the USB adapter, iPXE
  (`redox.ipxe` in this repo) loads a fresh image into RAM on each reboot. Otherwise, a USB stick.
- The laptop is not the daily test machine; Proxmox is.

## Phase 0 — this week

- [ ] Proxmox: build VM + Iskra VM working (`PROXMOX.md`). Needs SSH access or the user's hands.
- [ ] **Sandbox spike (1–2 hours, the most important unknown):** in an Iskra VM, run a shell under
      `contain` with only `/tmp/project` visible and confirm `ls ~/.ssh` fails while project files
      work. Record exactly which namespace/scheme settings do it on today's Redox.
- [ ] `CLAUDE.md` in the repo: build commands, layout, rules (one recipe at a time, no GPL code in
      Iskra crates, `#![forbid(unsafe_code)]` outside kernel work).
- [ ] Remove Firefox from `config/iskra.toml` (crashes at start; 290 MB) until upstream fixes it.

## Phase 1 — demo v0.1 (about 6–8 weeks of evenings)

Goal: the 60-second demo, minus snapshots.

1. **`iskra` CLI agent (Rust)**, replacing the `ai` shell script: providers as `ai` (Claude native API;
   OpenAI-compatible for OpenAI, Gemini, OpenRouter, Groq, LM Studio, Ollama), a tool loop (read,
   write, run, search), memory in Markdown files, skills in the agentskills.io format.
   **Tier 1:** every tool call runs in a child process under `contain` with only the project granted.
   Demo scene: prompt injection asks for `~/.ssh/id_rsa`; the OS answers "no such file".
2. **`iskra-shell`, native on Orbital** (iced, the same toolkit as the COSMIC apps already in the image):
   thin top bar (workspaces, clock, status) and the **Super+Space bar** (app search + streaming AI answer
   + permission prompt). Floating windows; no tiling manager, per the user's preference. Super+Space
   needs a global hotkey in Orbital: the first real Orbital patch.
3. **`healer-d` v0:** restart crashed services within about a second and show it in the bar. On Redox
   this is a supervisor over init services; no VMs involved.
4. Record the demo on Proxmox (native speed).

## Phase 2 — v0.2 (months 2–4)

- **Undo for agent changes, first without filesystem work:** the agent records every file it changes
  and `iskra undo` restores them. That gives the demo's "undo" scene honestly.
- **RedoxFS snapshots, research:** RedoxFS is transactional (header generations) but frees old blocks
  immediately; snapshots need reference-tracking in the allocator. Prototype, measure, propose upstream.
- **Iskra Store:** COSMIC Store with its Redox `pkgar` backend (recipe exists upstream, unpublished).
- **Installer:** brand upstream's `installer-gui` rather than writing one.
- **Voice:** first confirm microphone capture works on Redox (HDA input). Only then Whisper (Candle).
- **Bare-metal report for the Vivobook:** what boots, what doesn't, logged for upstream's HARDWARE list.
- **Funding:** publish the demo; NLnet proposal (calls every two months) for "capability-sandboxed AI
  agents on Redox", with small milestones. That is fundable now; a hypervisor port is a later proposal.

## Phase 3 — hypervisor research (months 4–12+)

The blueprint's `iskra-vmm`, modelled on FreeBSD bhyve (BSD-2-Clause, compatible). Realistic shape:
1. Ask upstream Redox first; aim to contribute, not to fork the kernel long-term.
2. AMD SVM first (the Vivobook is AMD; SVM is simpler than VT-x). Develop under nested KVM on Proxmox.
3. Milestones: a guest executes `hlt` → nested paging → serial output from a tiny guest → a `/scheme/vmm`
   interface → a minimal Linux guest with virtio console and block.
   - [x] **Step 1 done (2026-11-27):** the kernel enables SVM at boot, runs a real-mode guest
     (`mov ax, 0x1234; hlt`) with VMRUN and gets a HLT exit with AX = 0x1234; boot then continues
     normally. Verified in QEMU's SVM emulation (`ISKRA_CONFIG=iskra-hv ISKRA_QEMU_CPU=EPYC
     ISKRA_BOOT_MARKER="iskra-svm: io exits: PASS" ./iskra.sh test`). Code: `recipes/iskra/kernel/`.
   - [x] **Step 2 done (2026-11-27): nested paging.** A 32-bit paged guest runs in its own
     guest-physical address space through 4-level nested page tables: its writes land in the
     host frame behind its address, an unmapped address exits to the host as a nested page fault
     with the right address, and after the host maps a page and resumes, the guest finishes with
     the new data. (QEMU 8.2's SVM emulation skips nested paging for guests with paging off, so
     the test guest runs with its own paging on, as Linux does.)
   - [x] **Step 3 done (2026-11-27): I/O exits.** The guest prints "Hello from a guest inside
     Iskra" to COM1; every `out` traps to the host through an all-ports I/O permission map, the host
     collects the byte and resumes the guest after the instruction (the basis of device emulation).
   - [x] **Step 4 done (2026-11-27): VMs for userspace.** `/scheme/sys/vmm` (root): opening it
     creates a VM; pwrite/pread below 1 TiB is guest memory, offset `1 << 48` the vCPU registers,
     `(1 << 48) + 4 KiB` runs until the next exit. Guests can't reach host hardware or MSRs (all
     intercepted); full register switching including VMSAVE/VMLOAD. `iskra-vmm-hello` (package
     `iskra-vmm`) runs a guest that prints through the serial port into the Iskra terminal.
     (It lives under `sys` because a new top-level kernel scheme also needs Redox's userspace
     bootstrap to know its name; a proper `/scheme/vmm` is an upstream change.)
   - [x] **Step 5 done (2026-11-27): interrupts.** `/scheme/sys/vmm` offset `INJECT` queues an
     external interrupt; the kernel injects it when the guest can take it, or opens an interrupt
     window and delivers it right after the guest's STI; events cut off by an exit are re-delivered.
     `iskra-vmm-timer` runs a guest with its own GDT/IDT that takes five 100 ms timer ticks
     (the first through the interrupt window). The timer itself lives in the VMM, as in real VMMs.
   - [ ] Step 6: 64-bit long-mode guests and the Linux boot protocol (load bzImage + initramfs,
     boot params, serial console).
   - [ ] Step 7: run Linux: either a small Rust VMM (CPUID/MSR answers, serial, PIT/PIC or LAPIC
     timer) or a QEMU accelerator backend on `/scheme/sys/vmm` (QEMU supplies all devices).
   - [ ] Later: IOMMU (AMD-Vi) and device passthrough to the Linux VM.
   - [ ] Not yet tried on real AMD hardware or under nested KVM on Proxmox.
4. Only after that: `dev-bridge` (.NET/Node/Docker ports over virtio-vsock), and a Linux "driver VM".

## Parked (not on the roadmap until Phase 3 works)

| Blueprint item | Why parked |
|---|---|
| IOMMU/PCI passthrough of Wi-Fi, USB, GPU to a Linux driver VM | Needs a working hypervisor first; Redox has no IOMMU support today. |
| Steam/Proton gaming at 95–99%, IVSHMEM 120–240 Hz | Laptop NVIDIA GPUs have no display of their own (frames go through the iGPU), and Redox can only draw to the UEFI framebuffer in software. |
| Local CUDA LLMs, Gaming ↔ LLM VRAM switch | Depends on GPU passthrough. CPU-only local models (Candle) can be tried in Phase 2 if they build for Redox. |
| `power-d` (P-states, NVIDIA D3cold 0 W), "battery better than Windows" | Redox has no CPU frequency scaling or device power management yet; this is upstream-sized work. |
| Tiling window manager | User asked for no tiling; keyboard snapping in the shell instead. |
| Wasm plugins (Wasmtime/Extism) | Wasmtime's JIT on Redox is unverified; revisit with `wasmi` (interpreter) if plugins become a need. |
| A/B bootloader with watchdog and surviving panic buffer | Useful for bare metal later; not needed while testing in VMs. |
| Apple Silicon bare metal, mobile app, cloud sync, recall, multi-agent swarms | v2.0 ideas; no dependency on them now. |

## Notes on the blueprint's claims

- **Licensing:** the reasoning that a GPLv2 Linux kernel inside a separate VM does not make Iskra GPL is
  sound (separate programs). Distributing that VM image still means offering its complete kernel source
  (not just the config) and following each `linux-firmware` file's own licence; some are not freely
  redistributable.
- **Numbers** (<2 s desktop start, <120 MB RAM, 1.2 s VM respawn, <1 ms snapshots) are targets, not
  measurements. Measure on Proxmox before quoting them anywhere public.
- **Branding:** keep saying "based on Redox OS" and never use the Redox name or logo as Iskra's
  (`TRADEMARK.md`).
- **"Redox namespaces / `redox_namespace::mkns`":** namespaces are now a file-descriptor capability
  managed by a userspace `nsmgr` (2025–26 upstream work). Build on `contain` and that API, not on the
  older `mkns` call.
