//! iskra-panel: Iskra's floating frosted bottom panel (DESIGN.md "Panel") and the Super+Space
//! command bar (DESIGN.md "Command bar"), replacing Redox's `launcher`. It is the session
//! process: exiting it (Log out) returns to the greeter.

use event::{user_data, EventQueue};
use iskra_shell::*;
use orbclient::image::Image;
use orbclient::{Color, EventOption, Renderer, Window, WindowFlag};
use std::os::unix::io::AsRawFd;
use std::process::{Child, Command};

#[derive(Clone)]
struct App {
    name: String,
    exec: String,
    glyph: Glyph,
    color: Color,
}

impl App {
    fn new(name: &str, exec: &str) -> App {
        let (glyph, color) = glyph_for(name, exec);
        App { name: name.into(), exec: exec.into(), glyph, color }
    }
}

fn exists(bin: &str) -> bool {
    std::path::Path::new(&format!("/usr/bin/{}", bin)).exists()
}

/// Every launchable app: orbutils' /usr/share/ui/apps and freedesktop .desktop files.
fn all_apps() -> Vec<App> {
    let mut apps: Vec<App> = Vec::new();
    let mut add = |name: Option<String>, exec: Option<String>| {
        if let (Some(n), Some(e)) = (name, exec) {
            if !apps.iter().any(|a| a.name == n) {
                apps.push(App::new(&n, &e));
            }
        }
    };
    for dir in ["/usr/share/ui/apps", "/usr/share/applications"] {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for entry in rd.flatten() {
            let Ok(s) = std::fs::read_to_string(entry.path()) else { continue };
            let (mut name, mut exec, mut hidden) = (None, None, false);
            for line in s.lines() {
                match line.split_once('=') {
                    Some(("name" | "Name", v)) if name.is_none() => name = Some(v.to_string()),
                    Some(("binary", v)) => exec = Some(v.to_string()),
                    Some(("Exec", v)) if exec.is_none() => {
                        let v: Vec<&str> = v.split_whitespace().filter(|a| !a.starts_with('%')).collect();
                        exec = Some(v.join(" "));
                    }
                    Some(("NoDisplay", "true")) => hidden = true,
                    _ => {}
                }
                if line.starts_with('[') && line != "[Desktop Entry]" {
                    break; // actions sections
                }
            }
            if !hidden {
                add(name, exec);
            }
        }
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

fn pinned() -> Vec<App> {
    let mut v: Vec<App> = [("Files", "cosmic-files"), ("Terminal", "cosmic-term"), ("Browser", "netsurf-fb"), ("Editor", "cosmic-edit")]
        .iter()
        .filter(|(_, e)| exists(e))
        .map(|(n, e)| App::new(n, e))
        .collect();
    if !exists("cosmic-term") && exists("orbterm") {
        v.insert(1.min(v.len()), App::new("Terminal", "orbterm"));
    }
    v.push(App { name: "Ask AI".into(), exec: "@ask".into(), glyph: Glyph::Ai, color: VIOLET });
    v
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[derive(Clone)]
enum Action {
    Exec(String),
    Ask(String),
    Logout,
    Restart,
    Shutdown,
}

#[derive(Clone)]
struct Row {
    section: &'static str,
    title: String,
    kind: String,
    glyph: Glyph,
    color: Color,
    action: Action,
}

struct CmdBar {
    win: Window,
    query: String,
    sel: usize,
    rows: Vec<Row>,
}

const BAR_W: u32 = 460;

struct Panel {
    sw: u32,
    sh: u32,
    s: f32,
    win: Window,
    backdrop: Image,
    fonts: Fonts,
    pins: Vec<App>,
    apps: Vec<App>,
    recent: Vec<App>,
    children: Vec<(String, Child)>,
    hover: i32,
    hits: Vec<(f32, f32, f32, f32)>, // 0 = spark, 1.. = pins
    cmd: Option<CmdBar>,
}

impl Panel {
    fn geometry(sw: u32, sh: u32, s: f32) -> (i32, i32, u32, u32) {
        let w = (sw as f32 * 0.84) as u32;
        let h = (48.0 * s) as u32;
        ((sw - w) as i32 / 2, sh as i32 - (12.0 * s) as i32 - h as i32, w, h)
    }

    fn running(&self, exec: &str) -> bool {
        self.children.iter().any(|(e, _)| e == exec)
    }

    fn draw(&mut self) {
        let s = self.s;
        let (w, h) = (self.win.width() as f32, self.win.height() as f32);
        let (ox, oy) = (self.win.x(), self.win.y());
        for c in self.win.data_mut() {
            *c = Color::rgba(0, 0, 0, 0);
        }
        let running: Vec<bool> = self.pins.iter().map(|a| self.running(&a.exec)).collect();
        let win = &mut self.win;
        let f = &self.fonts;
        glass(win, &self.backdrop, ox, oy, 0.0, 0.0, w, h, 16.0 * s, Color::rgba(16, 18, 26, 120));
        self.hits.clear();

        // Left: the spark launcher button, a divider, then the app tiles.
        let tile = 34.0 * s;
        let ty = (h - tile) / 2.0;
        let mut x = 8.0 * s;
        if self.hover == 0 {
            rrect(win, x - 3.0 * s, ty - 3.0 * s, tile + 6.0 * s, tile + 6.0 * s, 10.0 * s, white(0.12));
        }
        spark(win, x + tile / 2.0, h / 2.0, tile * 1.9, 0.0, false);
        self.hits.push((x, 0.0, tile, h));
        x += tile + 10.0 * s;
        rrect(win, x, h / 2.0 - 12.0 * s, 1.0, 24.0 * s, 0.5, white(0.16));
        x += 11.0 * s;
        for (i, app) in self.pins.iter().enumerate() {
            if running[i] || self.hover == i as i32 + 1 {
                let a = if self.hover == i as i32 + 1 { 0.16 } else { 0.10 };
                rrect(win, x - 4.0 * s, ty - 4.0 * s, tile + 8.0 * s, tile + 8.0 * s, 11.0 * s, white(a));
            }
            app_icon(win, f, x, ty, tile, app.glyph, app.color);
            if running[i] {
                rrect(win, x + tile / 2.0 - 5.0 * s, h - 4.0 * s, 10.0 * s, 2.5 * s, 1.2 * s, white(0.85));
            }
            self.hits.push((x - 4.0 * s, 0.0, tile + 8.0 * s, h));
            x += tile + 10.0 * s;
        }

        // Right: agent dot, Wi-Fi, volume, battery, two-line clock.
        let clock = now();
        let (t, d) = (clock.hm(), clock.short_date());
        let tw = text_w(&f.semibold, &t, 13.0 * s).max(text_w(&f.regular, &d, 11.0 * s));
        let mut rx = w - 14.0 * s - tw;
        text(win, &f.semibold, &t, 13.0 * s, w - 14.0 * s - text_w(&f.semibold, &t, 13.0 * s), h / 2.0 - 16.0 * s, WHITE);
        text(win, &f.regular, &d, 11.0 * s, w - 14.0 * s - text_w(&f.regular, &d, 11.0 * s), h / 2.0 + 1.0 * s, white(0.7));
        rx -= 18.0 * s;
        let cy = h / 2.0;
        battery_glyph(win, rx - 8.0, cy, white(0.88));
        rx -= 30.0 * s;
        volume_glyph(win, rx - 6.0, cy, white(0.88));
        rx -= 28.0 * s;
        wifi_glyph(win, rx - 8.0, cy - 3.0, white(0.88));
        rx -= 28.0 * s;
        circle(win, rx - 4.0 * s, cy, 7.0 * s, Color::rgba(61, 174, 233, 50));
        circle(win, rx - 4.0 * s, cy, 4.0 * s, ACCENT);
        win.sync();
    }

    fn launch(&mut self, app: &App) {
        if app.exec == "@ask" {
            self.open_cmd();
            return;
        }
        if let Some(child) = spawn(&app.exec) {
            self.children.push((app.exec.clone(), child));
        }
        self.recent.retain(|a| a.name != app.name);
        self.recent.insert(0, app.clone());
        self.recent.truncate(5);
    }

    // ---- command bar ----

    fn open_cmd(&mut self) {
        if self.cmd.is_some() {
            return;
        }
        let x = (self.sw - (BAR_W as f32 * self.s) as u32) as i32 / 2;
        let y = (self.sh as f32 * 0.16) as i32;
        let Some(win) = Window::new_flags(x, y, (BAR_W as f32 * self.s) as u32, 100, "Command bar",
            &[WindowFlag::Async, WindowFlag::Borderless, WindowFlag::Transparent, WindowFlag::Front]) else { return };
        self.cmd = Some(CmdBar { win, query: String::new(), sel: 0, rows: Vec::new() });
        self.update_rows();
    }

    fn update_rows(&mut self) {
        let Some(cmd) = self.cmd.as_mut() else { return };
        let q = cmd.query.trim().to_lowercase();
        let mut rows = Vec::new();
        let app_row = |section, a: &App| Row { section, title: a.name.clone(), kind: "Application".into(), glyph: a.glyph, color: a.color, action: Action::Exec(a.exec.clone()) };
        if q.is_empty() {
            let src = if self.recent.is_empty() { &self.pins } else { &self.recent };
            for a in src.iter().filter(|a| a.exec != "@ask") {
                rows.push(app_row("Recent", a));
            }
        } else {
            for a in self.apps.iter().chain(self.pins.iter()).filter(|a| a.exec != "@ask" && a.name.to_lowercase().contains(&q)).take(6) {
                if !rows.iter().any(|r: &Row| matches!(&r.action, Action::Exec(e) if *e == a.exec)) {
                    rows.push(app_row("Apps", a));
                }
            }
            for (path, is_dir) in find_files(&q, 4) {
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                let home = std::env::var("HOME").unwrap_or_default();
                let shown = path.replacen(&home, "~", 1);
                let exec = if is_dir { format!("cosmic-files {}", path) } else { format!("launcher {}", path) };
                let (glyph, color) = if is_dir { (Glyph::Files, ACCENT) } else { (Glyph::Editor, Color::rgb(0x4b, 0x55, 0x63)) };
                rows.push(Row { section: "Files", title: name, kind: shown, glyph, color, action: Action::Exec(exec) });
            }
            for (title, action) in [("Log out", Action::Logout), ("Restart", Action::Restart), ("Shut down", Action::Shutdown)] {
                if title.to_lowercase().contains(&q) {
                    rows.push(Row { section: "Actions", title: title.into(), kind: "System".into(), glyph: Glyph::Settings, color: Color::rgb(0x6b, 0x72, 0x80), action });
                }
            }
            rows.push(Row { section: "Ask AI", title: format!("Ask AI: {}", cmd.query.trim()), kind: "Tab".into(), glyph: Glyph::Ai, color: VIOLET, action: Action::Ask(cmd.query.trim().into()) });
        }
        cmd.sel = cmd.sel.min(rows.len().saturating_sub(1));
        cmd.rows = rows;
        self.draw_cmd();
    }

    fn draw_cmd(&mut self) {
        let s = self.s;
        let Some(cmd) = self.cmd.as_mut() else { return };
        let f = &self.fonts;
        // Height from content: input row, rows with section headers, footer.
        let (input_h, row_h, head_h, foot_h) = (52.0 * s, 36.0 * s, 26.0 * s, 34.0 * s);
        let mut list_h = 8.0 * s;
        let mut last = "";
        for r in &cmd.rows {
            if r.section != last { list_h += head_h; last = r.section; }
            list_h += row_h;
        }
        list_h += 6.0 * s;
        let total = (input_h + list_h + foot_h + 12.0 * s).ceil();
        let w = cmd.win.width() as f32;
        if cmd.win.height() != total as u32 {
            cmd.win.set_size(w as u32, total as u32);
        }
        let (ox, oy) = (cmd.win.x(), cmd.win.y());
        let win = &mut cmd.win;
        for c in win.data_mut() { *c = Color::rgba(0, 0, 0, 0); }
        glass(win, &self.backdrop, ox, oy, 0.0, 0.0, w, total, 14.0 * s, Color::rgba(16, 18, 26, 125));

        // Input row.
        search_glyph(win, 22.0 * s, input_h / 2.0, white(0.75));
        let tx = 40.0 * s;
        if cmd.query.is_empty() {
            text(win, &f.regular, "Search apps, files, or ask anything", 15.0 * s, tx, input_h / 2.0 - 10.0 * s, white(0.55));
            rrect(win, tx - 1.0, input_h / 2.0 - 9.0 * s, 1.5 * s, 18.0 * s, 0.5, ACCENT);
        } else {
            let qw = text(win, &f.regular, &cmd.query, 15.0 * s, tx, input_h / 2.0 - 10.0 * s, WHITE);
            rrect(win, tx + qw + 2.0, input_h / 2.0 - 9.0 * s, 1.5 * s, 18.0 * s, 0.5, ACCENT);
        }

        // Results on a solid surface.
        let (lx, ly, lw) = (6.0 * s, input_h, w - 12.0 * s);
        rrect(win, lx, ly, lw, list_h, 10.0 * s, Color::rgba(22, 24, 32, 245));
        let mut y = ly + 8.0 * s;
        let mut last = "";
        for (i, r) in cmd.rows.iter().enumerate() {
            if r.section != last {
                text(win, &f.semibold, r.section, 11.0 * s, lx + 12.0 * s, y + 8.0 * s, white(0.55));
                y += head_h;
                last = r.section;
            }
            if i == cmd.sel {
                rrect(win, lx + 4.0 * s, y, lw - 8.0 * s, row_h, 8.0 * s, Color::rgba(61, 174, 233, 64));
            }
            app_icon(win, f, lx + 12.0 * s, y + 7.0 * s, 22.0 * s, r.glyph, r.color);
            let mut title = r.title.clone();
            while title.len() > 4 && text_w(&f.regular, &title, 13.0 * s) > lw * 0.6 { title.pop(); }
            text(win, &f.regular, &title, 13.0 * s, lx + 44.0 * s, y + 10.0 * s, WHITE);
            let kw = text_w(&f.regular, &r.kind, 11.0 * s);
            text(win, &f.regular, &r.kind, 11.0 * s, lx + lw - 14.0 * s - kw, y + 12.0 * s, white(0.55));
            y += row_h;
        }

        // Footer: shortcuts.
        let mut fx = 14.0 * s;
        let fy = total - foot_h + 4.0 * s;
        for (key, label) in [("Enter", "Open"), ("Tab", "Ask AI"), ("Esc", "Close")] {
            let kw = text_w(&f.medium, key, 11.0 * s) + 12.0 * s;
            rrect(win, fx, fy + 4.0 * s, kw, 20.0 * s, 5.0 * s, white(0.10));
            text(win, &f.medium, key, 11.0 * s, fx + 6.0 * s, fy + 7.0 * s, white(0.85));
            fx += kw + 6.0 * s;
            fx += text(win, &f.regular, label, 11.0 * s, fx, fy + 7.0 * s, white(0.7)) + 16.0 * s;
        }
        win.sync();
    }

    /// Returns false when the session should end.
    fn run(&mut self, action: Action) -> bool {
        self.cmd = None;
        match action {
            Action::Exec(e) => {
                let app = self.apps.iter().chain(self.pins.iter()).find(|a| a.exec == e).cloned().unwrap_or_else(|| App::new(&e, &e));
                self.launch(&app);
            }
            Action::Ask(q) if !q.is_empty() => {
                let term = if exists("orbterm") { "orbterm" } else { "cosmic-term" };
                let script = format!("ai {}; echo; printf 'Press Enter to close'; read x", quote(&q));
                if let Ok(c) = Command::new(term).args(["sh", "-c", &script]).spawn() {
                    self.children.push(("@ask".into(), c));
                }
            }
            Action::Ask(_) => {}
            Action::Logout => return false,
            Action::Restart => { spawn("shutdown -r"); }
            Action::Shutdown => { spawn("shutdown"); }
        }
        true
    }

    /// Handles a key in the command bar; returns false to end the session.
    fn cmd_event(&mut self, e: EventOption) -> bool {
        let Some(cmd) = self.cmd.as_mut() else { return true };
        match e {
            EventOption::TextInput(t) if !t.character.is_control() => {
                cmd.query.push(t.character);
                cmd.sel = 0;
                self.update_rows();
            }
            EventOption::Key(k) if k.pressed => match k.scancode {
                orbclient::K_ESC => self.cmd = None,
                orbclient::K_BKSP => { cmd.query.pop(); cmd.sel = 0; self.update_rows(); }
                orbclient::K_UP => { cmd.sel = cmd.sel.saturating_sub(1); self.draw_cmd(); }
                orbclient::K_DOWN => { cmd.sel = (cmd.sel + 1).min(cmd.rows.len().saturating_sub(1)); self.draw_cmd(); }
                orbclient::K_ENTER => {
                    if let Some(r) = cmd.rows.get(cmd.sel).cloned() { return self.run(r.action); }
                }
                orbclient::K_TAB => {
                    let q = cmd.query.trim().to_string();
                    return self.run(Action::Ask(q));
                }
                _ => {}
            },
            EventOption::Focus(f) if !f.focused => self.cmd = None,
            _ => {}
        }
        true
    }
}

/// Files and folders under $HOME (two levels) whose name contains `q`.
fn find_files(q: &str, max: usize) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let Ok(home) = std::env::var("HOME") else { return out };
    let mut dirs = vec![(home, 0)];
    while let Some((d, depth)) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { continue; }
            let path = format!("{}/{}", d, name);
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if name.to_lowercase().contains(q) {
                out.push((path.clone(), is_dir));
                if out.len() >= max { return out; }
            }
            if is_dir && depth < 1 { dirs.push((path, depth + 1)); }
        }
    }
    out
}

fn main() {
    let (sw, sh) = orbclient::get_display_size().expect("iskra-panel: no display");
    let s = ((sh / 1600) + 1) as f32;
    let (x, y, w, h) = Panel::geometry(sw, sh, s);
    // The panel window must exist before any other window of the session: Orbital sends
    // Super+key events to the oldest window.
    let win = Window::new_flags(x, y, w, h, "Panel", &[WindowFlag::Async, WindowFlag::Borderless, WindowFlag::Transparent])
        .expect("iskra-panel: failed to open window");
    let wp = wallpaper_path(None);
    let mut children = Vec::new();
    if let Ok(c) = Command::new("background").args([&wp, "zoom"]).spawn() {
        children.push(("background".to_string(), c));
    }
    let backdrop = frost(&wallpaper(&wp, sw, sh), 20.0, 0.85);
    let mut p = Panel {
        sw, sh, s, win, backdrop, fonts: Fonts::load(), pins: pinned(), apps: all_apps(), recent: Vec::new(),
        children, hover: -1, hits: Vec::new(), cmd: None,
    };
    p.draw();

    user_data! { enum Ev { Panel, Cmd, Tick } }
    let queue = EventQueue::<Ev>::new().expect("event queue");
    let mut timer = Timer::new().expect("timer");
    queue.subscribe(p.win.as_raw_fd() as usize, Ev::Panel, event::EventFlags::READ).unwrap();
    queue.subscribe(timer.0.as_raw_fd() as usize, Ev::Tick, event::EventFlags::READ).unwrap();
    timer.arm(1000);
    let mut cmd_fd: Option<usize> = None;
    let mut last_min = now().min;
    let (mut mx, mut my, mut left) = (-1, -1, false);

    'events: for ev in queue.iter() {
        let Ok(ev) = ev else { break };
        match ev.user_data {
            Ev::Tick => {
                // Reap finished apps; redraw only when something visible changed.
                let before = p.children.len();
                p.children.retain_mut(|(_, c)| matches!(c.try_wait(), Ok(None)));
                if p.children.len() != before || now().min != last_min {
                    last_min = now().min;
                    p.draw();
                }
                timer.arm(1000);
            }
            Ev::Panel => {
                let events: Vec<_> = p.win.events().collect();
                for e in events {
                    if e.code >= 0x1000_0000 {
                        // Super+key, forwarded by Orbital (see orbital's scheme.rs).
                        let mut se = e;
                        se.code -= 0x1000_0000;
                        if let EventOption::Key(k) = se.to_option() {
                            if k.pressed {
                                match k.scancode {
                                    orbclient::K_SPACE => if p.cmd.is_some() { p.cmd = None } else { p.open_cmd() },
                                    orbclient::K_T => { let a = App::new("Terminal", if exists("cosmic-term") { "cosmic-term" } else { "orbterm" }); p.launch(&a) }
                                    orbclient::K_F => p.launch(&App::new("Files", "cosmic-files")),
                                    orbclient::K_B => p.launch(&App::new("Browser", "netsurf-fb")),
                                    _ => {}
                                }
                            }
                        }
                        continue;
                    }
                    match e.to_option() {
                        EventOption::Mouse(m) => {
                            (mx, my) = (m.x, m.y);
                            let hv = p.hits.iter().position(|r| (mx as f32) >= r.0 && (mx as f32) < r.0 + r.2).map_or(-1, |i| i as i32);
                            if hv != p.hover { p.hover = hv; p.draw(); }
                        }
                        EventOption::Hover(hv) if !hv.entered => { if p.hover != -1 { p.hover = -1; p.draw(); } }
                        EventOption::Button(b) => {
                            if b.left && !left && my >= 0 {
                                match p.hover {
                                    0 => if p.cmd.is_some() { p.cmd = None } else { p.open_cmd() },
                                    i if i > 0 => { let a = p.pins[i as usize - 1].clone(); p.launch(&a); p.draw(); }
                                    _ => {}
                                }
                            }
                            left = b.left;
                        }
                        EventOption::Screen(se) => {
                            p.sw = se.width;
                            p.sh = se.height;
                            let (x, y, w, h) = Panel::geometry(se.width, se.height, p.s);
                            p.win.set_pos(x, y);
                            p.win.set_size(w, h);
                            p.backdrop = frost(&wallpaper(&wallpaper_path(None), se.width, se.height), 20.0, 0.85);
                            p.draw();
                        }
                        EventOption::Quit(_) => break 'events,
                        _ => {}
                    }
                }
            }
            Ev::Cmd => {
                let events: Vec<EventOption> = match p.cmd.as_mut() {
                    Some(c) => c.win.events().map(|e| e.to_option()).collect(),
                    None => Vec::new(),
                };
                for e in events {
                    if !p.cmd_event(e) { break 'events; }
                }
            }
        }
        // Keep the command bar's window subscribed while it is open.
        let fd = p.cmd.as_ref().map(|c| c.win.as_raw_fd() as usize);
        if fd != cmd_fd {
            if let Some(old) = cmd_fd { let _ = queue.unsubscribe(old); }
            if let Some(new) = fd { let _ = queue.subscribe(new, Ev::Cmd, event::EventFlags::READ); }
            cmd_fd = fd;
            p.draw();
        }
    }

    // Log out: close everything this session started.
    for (_, c) in p.children.iter_mut() {
        let _ = c.kill();
        let _ = c.wait();
    }
}
