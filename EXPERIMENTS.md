# Iskra experiments

Parked work, written down so it can be picked up later without starting over.

## Desktop environments on Redox (parked 2026-11-26)

Goal: an Omarchy-like desktop (thin top bar, fast launcher, keyboard-first, one theme) without
writing a panel and window manager from scratch. Two existing desktops were tried. Neither is
usable yet, so Iskra's desktop will be built natively on Orbital (see `ISKRA.md`).

### 1. XFCE on X11 (`config/iskra-xfce.toml`) — does not work

Upstream Redox's `x11.toml` session, plus Iskra.

- X.Org starts inside an Orbital window and draws: `xterm` shows and works.
- `xfwm4`, `xfce4-panel` and `xfdesktop` exit immediately **without printing anything**, even
  when started directly instead of through `xfce4-session`. Three separate programs dying silently
  points at a crash in the shared GTK/XFCE layer on Redox, not at configuration.
- Upstream builds these packages in CI (`config/x86_64/ci.toml`) but CI only checks that they
  compile, not that they run.

Retest after upstream changes its X11 session or XFCE recipes.

### 2. xfwl4, Xfce's Rust Wayland compositor (`config/iskra-xfwl4.toml`) — builds and starts, fails in use

[xfwl4](https://gitlab.xfce.org/xfce/xfwl4) (main branch, 4.21.1+dev, November 2026) is Xfce's new
compositor, written in Rust on smithay. It can run nested inside another window through smithay's
winit backend, which is how upstream Redox runs cosmic-comp inside Orbital.

**What works** (headless QEMU, 2026-11-26):
- Cross-compiles for `x86_64-unknown-redox`.
- Starts inside Orbital: Redox EGL platform, Mesa OpenGL ES 3.2 (llvmpipe, software), Wayland
  socket `wayland-1`, xfconf settings, keyboard shortcuts, US keymap, output 1280×752, main loop.
- Draws its window in Orbital (title "smithay", empty dark desktop).

**What doesn't:**
- The test client (`winit-wayland_window`, from upstream's Wayland demos) never appeared inside it.
- In UTM on an M1 Mac (x86 emulated, 6 cores, 8 GB, Force Multicore) the user reported it fails.
  Details were not captured; the next attempt should start by recording the serial log in UTM.
- Not tested: mouse and keyboard input inside xfwl4, any panel.
- One `EGL BAD_SURFACE` error from `eglQuerySurface` right after start-up; may be related.

**How to rebuild it:**

```bash
recipes/iskra/xfwl4/prepare.sh                 # patched smithay, winit, winit-orbital, redox_event
ISKRA_CONFIG=iskra-xfwl4 ./iskra.sh build       # builds xfconf/libxfce4ui from source, then xfwl4
ISKRA_CONFIG=iskra-xfwl4 SHOTS_NO_LOGIN=1 SHOTS_DESKTOP_WAIT=240 \
  branding/shots.py /tmp/xfwl4-shots 90 shot:later   # headless boot, screenshots + serial.log
```

`xfconf` occasionally fails on a generated-header race in its meson build; run the build again.

**Everything it took** (details in `PATCHES.md`, patches in `recipes/iskra/xfwl4/patches/`):

| Problem | Fix |
|---|---|
| xfwl4 pins smithay `d09642cc`; Redox support lives in a fork of an older smithay | Redox fork's 11 commits (jackpot51/smithay `redox`) rebased onto `d09642cc`; Orbital window case re-added to the refactored winit backend |
| smithay requires `reis`, whose socket flags don't exist on Redox | `reis` made optional, only with `backend_libei` |
| winit has no scancodes on Redox, so smithay sent no keys | Redox-only table from winit key names to Linux key codes in smithay's winit backend (likely also why upstream's cosmic-comp port notes "no keyboard input") |
| smithay polls winit's loop through its fd; only Linux exposed one | `redox_event` gets an fd accessor; `winit-orbital` and `winit` implement `AsFd` for Orbital |
| `drm` 0.14.1 uses `rustix::fs::major/minor`, absent on Redox | pinned 0.14.2 (what cosmic-comp uses) |
| xfwl4 needs libxfce4ui ≥ 4.21.4 and xfconf ≥ 4.21.2; Redox had 4.21.2 / 4.21.0 | recipes bumped; libxfce4ui needs the `xdt-gen-visibility` helper its tarball omits |
| proc macros linked Redox's GLib instead of the build host's | host-target `pkg-config` overrides in the recipe |
| xfwl4 breaks without its default Linux features (`WindowSurface`, `X11Surface`, pixman) | ungated in the recipe script |
| D-Bus service activation fails on Redox ("Failed to read from child pipe") | session script starts `xfconfd` itself |
| `dbus-daemon --system` needs the `messagebus` user | user added in the config |
| `cargo install` copies only the binary | recipe sets `PREFIX=/usr`, installs `defaults`; `xfwm4` package provides the "Default" theme |

**Upstream bugs found, worth reporting:**
- smithay: `reis` is a hard dependency though only `backend_libei` uses it.
- winit: Orbital's event loop doesn't implement `AsFd`, so it can't be polled by calloop.
- xfwl4: building without default features (`xwayland`, `udev`) fails to compile.
- xfconf 4.21.2: meson build race on `xfconf-gdbus-bindings.h`.
- libxfce4ui 4.21.4: release tarball doesn't ship `xdt-gen-visibility`.

**Where to start when resuming:**
1. Boot `iskra-xfwl4` in UTM with a serial console attached and capture why it fails there.
2. Get one Wayland client visible inside xfwl4 (try a GTK3 app with `GDK_BACKEND=wayland`, or
   `smallvil`'s clients). Check `WAYLAND_DISPLAY` and `XDG_RUNTIME_DIR` reach the session command.
3. Test input inside the nested window.
4. Only then: `xfce4-panel` on GTK3's Wayland backend.
