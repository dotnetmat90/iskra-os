# Iskra design

Agreed 2026-11-27. Modern and polished in the style of the best KDE Plasma 6 desktops (floating
panel, frosted glass, rich wallpaper, colourful app icons), with elementary OS's calm and macOS's
typography. Native Rust apps on Orbital (iced, like the COSMIC apps already in the image). This is
the brief for the greeter, panel, command bar and window look.

## Principles

- **Glass for chrome, solid for content.** Panels, headers, widgets and the login card are frosted
  glass (blur ~18–20 px, saturation 1.4, dark tint ~45–55%, 1 px border at 12–20% white). Window
  bodies, text fields and anything you read stay solid so text is always crisp.
- **Colour lives in the wallpaper and the app icons.** The chrome itself is neutral; one accent,
  Plasma blue `#3daee9`, for selection, focus and primary buttons.
- **Readable:** high-contrast text, secondary text at 70% (never fainter than ~55%), nothing below 11 px.
- **Built as layouts** (iced rows and columns), never absolute coordinates, so nothing can overlap
  at any screen size. 8 px spacing rhythm.
- **Compact controls** for mouse and keyboard, keyboard-first with visible shortcuts.
- **Motion with purpose:** 150–200 ms fades and scale. The login spark is the only ambient animation;
  it pauses on battery and with "reduce motion", and the desktop idles at 0 FPS.
- **Fonts:** Inter (needs a recipe; SIL Open Font License), JetBrains Mono for code.
- **Dark by default,** light mode with the same layout and white frosted glass.

## Wallpaper

Default: deep navy `#0d1326` through indigo `#1d2b57` to plum `#3a1f4d`, with soft glows (coral top
right, blue bottom left, violet centre) and a few faint flowing lines. Users can pick their own.

## Login screen (greeter), replaces orblogin

The desktop's own wallpaper, blurred (~26 px) and darkened (~60%), so unlocking feels like the glass
melting away to reveal the desktop. Three rows (space between):

1. **Top row:** "Iskra" wordmark left (12 px, semi-bold); glass pill right with keyboard layout,
   Wi-Fi, battery.
2. **Centre stack, one vertical axis:**
   - the spark: 52 px, a white point breathing slowly (~4 s) with a faint cool-blue halo and ~12 tiny
     particles twinkling and orbiting close around it;
   - time: Inter light (300), ~76 px, tight letter spacing, tabular digits;
   - date: 14 px, 72% white, 8 px below;
   - glass user card (250 px wide, 20 px radius, ~26 px below the date): 54 px avatar (initials on a
     blue-to-violet `#3daee9 → #8b5cf6` circle, or the user's photo), name (15 px semi-bold), password
     pill (36 px, `rgba(0,0,0,.28)` fill, round accent-blue arrow button).
3. **Bottom row, centred:** Sleep, Restart, Shut down as glass pills (icon + label, 12 px).

- **Wrong password:** the field shakes once and a 12 px line under it reads "Wrong password. Try
  again." No dialog.
- **Unlock transition:** blur and darkening fade out over ~300 ms while the card and clock fade, so
  the desktop appears in place.
- **Settings:** theme dark / light / auto; spark on / still / off (also off with "reduce motion").

## Boot splash

Black, with the same breathing spark centred, from the bootloader handoff until the greeter appears;
the greeter then fades its wallpaper in behind the spark.

## Panel, replaces Redox's taskbar

Plasma 6 style: a **floating frosted panel at the bottom**, ~48 px tall, ~84% of the screen width,
16 px radius, 12 px above the screen edge.

- **Left:** the Iskra spark as the launcher button (opens the command bar), then a thin divider.
- **Apps:** pinned and running apps as 34 px rounded-square colour icons; the active app's tile
  gets a light highlight behind it.
- **Right:** agent dot (accent blue, pulses while an agent works, amber when it waits for you),
  Wi-Fi, volume, battery, and a two-line clock (time, then date). One click on the tray opens a
  single quick-settings panel.
- Can auto-hide; optional top placement.

## Desktop widgets (optional)

Glass widgets on the wallpaper; the default is a clock in the top right: thin 34 px time, date and
weather below.

## Command bar (Super+Space)

Reference: Raycast. A frosted panel (460 px wide, 14 px radius), centred near the top; result rows
on a solid surface inside it.

- **Input row:** search icon and "Search apps, files, or ask anything".
- **Empty state = Recent:** recent apps, folders and AI conversations before anything is typed.
- **Results:** one consistent row layout for every type (icon, title, right-aligned type or path),
  grouped as Apps, Files, Actions, Ask AI. Selected row in a light accent tint.
- **AI answer:** streams into its own section in the same panel.
- **Permission prompts:** in the same panel. "Allow edits in `~/projects/api`?" with Deny (text),
  Always (outline), Allow once (accent).
- **Footer:** always shows the shortcuts: ↵ Open, Tab Ask AI, hold Space Speak, Esc Close.

## Windows

- 12 px corners, 1 px border at 16–20% white; the focused window slightly brighter.
- **Frosted header bar** (34 px): app icon and full title on the left ("Terminal — ~/projects/api"),
  minimise / maximise / close on the right (KDE Breeze layout); close turns red.
- **Solid body** (dark `#101218`–`#1e212a`), so content is always crisp.
- Sidebars in apps like Files use a light accent tint for the current item.

## App icons

Rounded squares (9–10 px radius) in distinct colours with a white glyph: Files `#3daee9`, Terminal
graphite `#2c2f39`, Browser `#f2652f`, Editor `#2fbf71`, AI assistant `#8b5cf6`, Store `#e5484d`,
Settings grey.

## Implementation status (2026-11-27)

Built natively on Orbital in `recipes/iskra/iskra-shell/` (`iskra-greeter`, `iskra-panel`) with
orbclient + orbfont, the libraries orblogin and launcher use, instead of iced: it builds in about a
minute, needs no GPU path, and gives pixel control for the glass. Layout is computed from content
sizes (rows/stacks), not fixed screen coordinates. Inter comes from `recipes/iskra/font-inter/`.

Differences from the design, and why:

- **Blur is of the wallpaper only.** Orbital cannot read back other windows, so glass (greeter,
  panel, command bar) blurs the user's wallpaper, which the shell loads and frosts itself. Over an
  app window, the panel and command bar show frosted wallpaper rather than the window behind.
- **No Sleep button:** Redox has no suspend yet. Restart and Shut down work.
- **No battery or Wi-Fi state:** Redox exposes neither; the tray shows static glyphs, and the
  spark cannot pause on battery. It stops after login (the greeter closes) and honours
  `spark = "on" | "still" | "off"` and `reduce_motion = true` in `/etc/iskra/greeter.toml`.
- **Clock is UTC** (no timezone data on Redox yet). Tabular digits are drawn per digit, as rusttype
  cannot enable Inter's `tnum`.
- **Greeter:** first normal user only; no theme setting (dark only, no light mode yet); unlock
  melts the blur away (~300 ms) but the card and clock disappear at once rather than fading.
- **Panel:** pinned Files, Terminal, Browser, Editor and Ask AI; running apps get the highlight and
  a dot (Orbital does not tell clients which window is focused). Agent dot is static; no
  quick-settings panel, auto-hide or top placement yet.
- **Command bar:** Recent, Apps, Files (two levels of `$HOME`), Actions (Log out, Restart, Shut
  down) and Ask AI. Ask AI opens a terminal running `ai "<question>"` instead of streaming into the
  panel; no permission prompts (no agent runtime yet); no "hold Space to speak".
- **Not started:** boot splash, desktop widgets, window decorations (Orbital draws title bars;
  only its colours were changed in `branding/share/ui/orbital.toml`).
