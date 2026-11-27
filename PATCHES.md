# Changes to upstream files

Every upstream file Iskra modifies is listed here, with the reason. New Iskra-only files are not listed.

| File | Change | Why |
|---|---|---|
| `README.md` | Notice at the top pointing to `ISKRA.md` | So the repository front page does not present Iskra as Redox OS (see `TRADEMARK.md`). |
| `recipes/gui/orbdata/recipe.toml` | After copying upstream's data, copy `branding/share/` over `/usr/share/` if that folder exists | Iskra wallpaper, login screen, start-button icon and Orbital colours. Config `[[files]]` entries can only hold text, and a second package shipping the same paths would conflict. No effect on builds without `branding/share`. |
| `recipes/wip/x11/xfce4/xfconf/recipe.toml` | 4.21.0 → 4.21.2 | xfwl4 needs xfconf ≥ 4.21.2. Its meson build has a race on a generated D-Bus header; a retry passes. |
| `recipes/wip/x11/xfce4/libxfce4ui/recipe.toml` (+ `xdt-gen-visibility` next to it) | 4.21.2 → 4.21.4; custom script that puts the bundled `xdt-gen-visibility` helper on `PATH` | xfwl4 needs the 4.21.4 keyboard-shortcut API. The 4.21.4 tarball does not bundle that helper; the copy comes from xfconf 4.21.2. |

Patches to code outside this repository (kept in `recipes/iskra/<name>/patches/`, applied by that folder's `prepare.sh`):

| Project | Change | Why |
|---|---|---|
| smithay @ d09642cc | Redox fork's patches (jackpot51/smithay, branch `redox`) rebased; Orbital window case added to the refactored winit backend | Redox support at the exact smithay commit xfwl4 pins |
| smithay | `reis` made optional, only with `backend_libei` | its socket flags do not exist on Redox; upstream bug worth reporting |
| smithay | winit backend: Redox key-name → Linux key-code table | winit has no scancode extension on Redox; without it keyboards send nothing (the cause of cosmic-comp's "no keyboard input") |
| winit, winit-orbital 0.31.0-beta.4; redox_event 0.4.8 | Orbital event loop exposes its event-queue fd | smithay polls winit's loop through that fd; only Linux had it |
| Redox kernel @ 8d5f7753 (`recipes/iskra/kernel/`) | `src/arch/x86_shared/svm.rs`: AMD SVM bring-up self-test at boot, behind feature `iskra_svm_selftest` | Hypervisor step 1 (ROADMAP.md Phase 3). Built only by `config/iskra-hv.toml`. |
| xfwl4 (in its recipe script) | `WindowSurface` import and `X11Surface` match ungated; pixman renderer enabled; `PREFIX=/usr`; defaults file installed | upstream assumes its default Linux features; reportable |

Iskra-only files: `ISKRA.md`, `PATCHES.md`, `iskra.sh`, `config/iskra.toml`, and `branding/`:

- `branding/share/` mirrors `/usr/share/`; every file in it replaces upstream's.
- `branding/synthwave.sh` renders the wallpaper and login art with ImageMagick 7.
- `branding/shots.py` boots the image headless and saves screenshots (login, desktop, and any keyboard/mouse steps you pass).
- `recipes/iskra/`: `iskra-tools` (the `ai` command), `kernel` (hypervisor patches, `prepare.sh`) and `xfwl4` (experiment, see `config/iskra-xfwl4.toml`).
