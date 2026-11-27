# Changes to upstream files

Every upstream file Iskra modifies is listed here, with the reason. New Iskra-only files are not listed.

| File | Change | Why |
|---|---|---|
| `README.md` | Notice at the top pointing to `ISKRA.md` | So the repository front page does not present Iskra as Redox OS (see `TRADEMARK.md`). |
| `recipes/gui/orbdata/recipe.toml` | After copying upstream's data, copy `branding/share/` over `/usr/share/` if that folder exists | Iskra wallpaper, login screen, start-button icon and Orbital colours. Config `[[files]]` entries can only hold text, and a second package shipping the same paths would conflict. No effect on builds without `branding/share`. |
| `recipes/core/base/recipe.toml` | `rev` pinned to 6cf80970 (the commit of the prebuilt package) and `patches = ["../../iskra/base/fbbootlogd-splash.patch"]` | Boot splash from the start of userspace (DESIGN.md); `config/iskra.toml` builds base from source. |
| `recipes/wip/x11/xfce4/xfconf/recipe.toml` | 4.21.0 → 4.21.2 | xfwl4 needs xfconf ≥ 4.21.2. Its meson build has a race on a generated D-Bus header; a retry passes. |
| `recipes/wip/x11/xfce4/libxfce4ui/recipe.toml` (+ `xdt-gen-visibility` next to it) | 4.21.2 → 4.21.4; custom script that puts the bundled `xdt-gen-visibility` helper on `PATH` | xfwl4 needs the 4.21.4 keyboard-shortcut API. The 4.21.4 tarball does not bundle that helper; the copy comes from xfconf 4.21.2. |

Patches to code outside this repository (kept in `recipes/iskra/<name>/patches/`, applied by that folder's `prepare.sh`):

| Project | Change | Why |
|---|---|---|
| smithay @ d09642cc | Redox fork's patches (jackpot51/smithay, branch `redox`) rebased; Orbital window case added to the refactored winit backend | Redox support at the exact smithay commit xfwl4 pins |
| smithay | `reis` made optional, only with `backend_libei` | its socket flags do not exist on Redox; upstream bug worth reporting |
| smithay | winit backend: Redox key-name → Linux key-code table | winit has no scancode extension on Redox; without it keyboards send nothing (the cause of cosmic-comp's "no keyboard input") |
| winit, winit-orbital 0.31.0-beta.4; redox_event 0.4.8 | Orbital event loop exposes its event-queue fd | smithay polls winit's loop through that fd; only Linux had it |
| Redox kernel @ 8d5f7753 (`recipes/iskra/kernel/`) | `src/arch/x86_shared/svm.rs`: AMD SVM self-tests at boot and the `Vm` object; `src/scheme/sys/vmm.rs` + `sys/mod.rs`: `/scheme/sys/vmm` for userspace VMMs. Behind feature `iskra_svm_selftest` | Hypervisor steps 1–7 (ROADMAP.md Phase 3). Built only by `config/iskra-hv.toml`. |
| Redox base @ 6cf80970 (`recipes/iskra/base/fbbootlogd-splash.patch`) | `console-draw`: `V2DisplayMap::draw_raw` for direct framebuffer drawing; `fbbootlogd`: draws Iskra's spark on black instead of the boot log, keeps the log in memory and shows it on Esc; the animation stops after 60 s | Iskra boot splash. Upstream's fbbootlogd already suggests a quiet-mode splash; a configurable version could go upstream. |
| xfwl4 (in its recipe script) | `WindowSurface` import and `X11Surface` match ungated; pixman renderer enabled; `PREFIX=/usr`; defaults file installed | upstream assumes its default Linux features; reportable |

Iskra-only files: `ISKRA.md`, `PATCHES.md`, `iskra.sh`, `config/iskra.toml`, and `branding/`:

- `branding/share/` mirrors `/usr/share/`; every file in it replaces upstream's.
- `branding/wallpaper.sh` renders the default wallpaper (DESIGN.md: navy, indigo, plum, soft glows) with ImageMagick 7.
- `branding/synthwave.sh` (old look, no longer the default) renders the synthwave wallpaper and login art with ImageMagick 7.
- `branding/shots.py` boots the image headless and saves screenshots (login, desktop, and any keyboard/mouse steps you pass).
- `recipes/iskra/`: `font-inter` (Inter, the UI font), `iskra-shell` (`iskra-greeter` login screen and `iskra-panel` floating panel + Super+Space command bar, replacing orblogin and launcher via `/usr/lib/init.d/20_orbital` in `config/iskra.toml`; see DESIGN.md), `iskra-tools` (the `ai` command), `kernel` (hypervisor patches, `prepare.sh`), `iskra-vmm` (userspace VMMs: `iskra-vmm-hello`, `-timer`, `-long`, `-linux`, plus the Linux guest's initramfs sources) and `xfwl4` (experiment, see `config/iskra-xfwl4.toml`).
