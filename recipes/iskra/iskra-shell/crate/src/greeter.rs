//! iskra-greeter: Iskra's login screen (DESIGN.md "Login screen"), replacing orblogin.
//! Usage: iskra-greeter <session command> [args...]   (e.g. `iskra-greeter iskra-panel`)
//! Authenticates with redox_users and starts the session in the user's scheme namespace,
//! exactly like orblogin; shows again when the session exits.

use event::{user_data, EventQueue};
use iskra_shell::*;
use orbclient::image::Image;
use orbclient::{Color, EventOption, Renderer, Window, WindowFlag};
use redox_users::{All, AllUsers, Config};
use std::process::Command;
use std::time::Instant;

#[derive(PartialEq, Clone, Copy)]
enum SparkMode {
    On,
    Still,
    Off,
}

/// /etc/iskra/greeter.toml: `spark = "on" | "still" | "off"`. "reduce_motion = true" also stops it.
fn spark_mode() -> SparkMode {
    let s = std::fs::read_to_string("/etc/iskra/greeter.toml").unwrap_or_default();
    let get = |k: &str| {
        s.lines()
            .filter_map(|l| l.split_once('='))
            .find(|(a, _)| a.trim() == k)
            .map(|(_, b)| b.trim().trim_matches('"').to_string())
    };
    if get("reduce_motion").as_deref() == Some("true") {
        return SparkMode::Still;
    }
    match get("spark").as_deref() {
        Some("still") => SparkMode::Still,
        Some("off") => SparkMode::Off,
        _ => SparkMode::On,
    }
}

type Hit = (f32, f32, f32, f32);
fn inside(h: Hit, x: i32, y: i32) -> bool {
    let (x, y) = (x as f32, y as f32);
    x >= h.0 && y >= h.1 && x < h.0 + h.2 && y < h.1 + h.3
}

struct Greeter {
    win: Window,
    wall: Image,
    frosted: Image,
    fonts: Fonts,
    user: String,
    password: String,
    error: bool,
    shake: Option<Instant>,
    spark: SparkMode,
    t0: Instant,
    s: f32,
    // hit areas from the last layout
    go: Hit,
    restart: Hit,
    shutdown: Hit,
    spark_box: Hit,
    pill_box: Hit,
    hover: i32,
}

impl Greeter {
    fn draw(&mut self) {
        let (w, h, s) = (self.win.width() as f32, self.win.height() as f32, self.s);
        self.win.data_mut().copy_from_slice(self.frosted.data());
        let f = &self.fonts;
        let win = &mut self.win;

        // 1. Top row: wordmark left, status pill right.
        let m = 24.0 * s;
        text(win, &f.semibold, "Iskra", 13.0 * s, m, m + 6.0 * s, white(0.92));
        let pill_h = 30.0 * s;
        let lw = text_w(&f.medium, "EN", 12.0 * s);
        let pill_w = 14.0 * s + lw + 16.0 * s + 18.0 * s + 14.0 * s + 20.0 * s + 14.0 * s;
        let (px, py) = (w - m - pill_w, m);
        glass(win, &self.frosted, 0, 0, px, py, pill_w, pill_h, pill_h / 2.0, white(0.08));
        let cy = py + pill_h / 2.0;
        text(win, &f.medium, "EN", 12.0 * s, px + 14.0 * s, cy - 7.5 * s, white(0.9));
        let mut gx = px + 14.0 * s + lw + 16.0 * s + 9.0 * s;
        wifi_glyph(win, gx, cy - 2.0, white(0.9));
        gx += 18.0 * s + 14.0 * s;
        battery_glyph(win, gx, cy, white(0.9));

        // 2. Centre stack: spark, time, date, user card.
        let clock = now();
        let card_h = (20.0 + 54.0 + 12.0 + 20.0 + 14.0 + 36.0 + 20.0 + if self.error { 22.0 } else { 0.0 }) * s;
        let spark_h = if self.spark == SparkMode::Off { 0.0 } else { 52.0 * s + 14.0 * s };
        let total = spark_h + 76.0 * s + 8.0 * s + 18.0 * s + 26.0 * s + card_h;
        let mut y = ((h - total) / 2.0 - 24.0 * s).max(m + pill_h);
        let cx = w / 2.0;
        if self.spark != SparkMode::Off {
            self.spark_box = (cx - 26.0 * s, y, 52.0 * s, 52.0 * s);
            spark(win, cx, y + 26.0 * s, 52.0 * s, self.t0.elapsed().as_secs_f32(), self.spark == SparkMode::On);
            y += spark_h;
        }
        let hm = clock.hm();
        let tw = text_tab(win, &f.light, &hm, 76.0 * s, 0.0, 0.0, -2.0 * s, WHITE, false);
        // Inter's digits sit ~0.2 em below the top of the line box; pull the clock up by that.
        text_tab(win, &f.light, &hm, 76.0 * s, cx - tw / 2.0, y - 12.0 * s, -2.0 * s, WHITE, true);
        y += 76.0 * s + 8.0 * s;
        let date = clock.long_date();
        let dw = text_w(&f.regular, &date, 14.0 * s);
        text(win, &f.regular, &date, 14.0 * s, cx - dw / 2.0, y, white(0.72));
        y += 18.0 * s + 26.0 * s;

        // User card.
        let cw = 250.0 * s;
        let (cx0, cy0) = (cx - cw / 2.0, y);
        glass(win, &self.frosted, 0, 0, cx0, cy0, cw, card_h, 20.0 * s, white(0.07));
        let mut yy = cy0 + 20.0 * s;
        let ar = 27.0 * s;
        let (ax, ay) = (cx, yy + ar);
        fill_sdf(win, (ax - ar, ay - ar, ax + ar, ay + ar),
            |px, py| lerp(ACCENT, VIOLET, (((px - ax + ar) + (py - ay + ar)) / (4.0 * ar)).clamp(0.0, 1.0)),
            |px, py| (px - ax).hypot(py - ay) - ar);
        let initial: String = self.user.chars().take(1).collect::<String>().to_uppercase();
        let iw = text_w(&f.semibold, &initial, 22.0 * s);
        text(win, &f.semibold, &initial, 22.0 * s, ax - iw / 2.0, ay - 13.0 * s, WHITE);
        yy += 54.0 * s + 12.0 * s;
        let mut name = self.user.clone();
        if let Some(c) = name.get_mut(0..1) {
            c.make_ascii_uppercase();
        }
        let nw = text_w(&f.semibold, &name, 15.0 * s);
        text(win, &f.semibold, &name, 15.0 * s, cx - nw / 2.0, yy, WHITE);
        yy += 20.0 * s + 14.0 * s;

        // Password pill, shaking after a wrong password.
        let dx = match self.shake {
            Some(t) if t.elapsed().as_secs_f32() < 0.45 => {
                let e = t.elapsed().as_secs_f32();
                9.0 * s * (e * 45.0).sin() * (1.0 - e / 0.45)
            }
            _ => 0.0,
        };
        let (fx, fw, fh) = (cx0 + 20.0 * s + dx, cw - 40.0 * s, 36.0 * s);
        self.pill_box = (cx0, yy, cw, fh);
        rrect(win, fx, yy, fw, fh, fh / 2.0, black(0.28));
        rrect_border(win, fx, yy, fw, fh, fh / 2.0, if self.error { Color::rgba(255, 120, 120, 150) } else { Color::rgba(61, 174, 233, 140) });
        if self.password.is_empty() {
            text(win, &f.regular, "Password", 13.0 * s, fx + 16.0 * s, yy + 10.0 * s, white(0.6));
        } else {
            for i in 0..self.password.chars().count().min(14) {
                circle(win, fx + 20.0 * s + i as f32 * 11.0 * s, yy + fh / 2.0, 3.2 * s, WHITE);
            }
        }
        let (bx, by, br) = (fx + fw - 18.0 * s, yy + fh / 2.0, 13.0 * s);
        circle(win, bx, by, br, if self.hover == 0 { lerp(ACCENT, WHITE, 0.15) } else { ACCENT });
        seg(win, bx - 5.0 * s, by, bx + 5.0 * s, by, 2.0 * s, WHITE);
        seg(win, bx + 5.0 * s, by, bx + 1.0 * s, by - 4.0 * s, 2.0 * s, WHITE);
        seg(win, bx + 5.0 * s, by, bx + 1.0 * s, by + 4.0 * s, 2.0 * s, WHITE);
        self.go = (bx - br, by - br, br * 2.0, br * 2.0);
        if self.error {
            let msg = "Wrong password. Try again.";
            let ew = text_w(&f.regular, msg, 12.0 * s);
            text(win, &f.regular, msg, 12.0 * s, cx - ew / 2.0, yy + fh + 8.0 * s, Color::rgb(255, 170, 170));
        }

        // 3. Bottom row: power pills. Sleep is left out: Redox has no suspend yet.
        let ph = 32.0 * s;
        let labels = ["Restart", "Shut down"];
        let widths: Vec<f32> = labels.iter().map(|l| 14.0 * s + 14.0 * s + 8.0 * s + text_w(&f.medium, l, 12.0 * s) + 16.0 * s).collect();
        let row_w = widths.iter().sum::<f32>() + 12.0 * s;
        let mut x = cx - row_w / 2.0;
        let y = h - 28.0 * s - ph;
        for (i, l) in labels.iter().enumerate() {
            let tint = if self.hover == i as i32 + 1 { white(0.16) } else { white(0.07) };
            glass(win, &self.frosted, 0, 0, x, y, widths[i], ph, ph / 2.0, tint);
            let (gx, gy) = (x + 14.0 * s + 7.0 * s, y + ph / 2.0);
            if i == 0 { restart_glyph(win, gx, gy, white(0.9)) } else { power_glyph(win, gx, gy, white(0.9)) }
            text(win, &f.medium, l, 12.0 * s, x + 14.0 * s + 14.0 * s + 8.0 * s, y + 9.0 * s, white(0.9));
            if i == 0 { self.restart = (x, y, widths[i], ph) } else { self.shutdown = (x, y, widths[i], ph) }
            x += widths[i] + 12.0 * s;
        }
        win.sync();
    }

    /// Redraws only the spark (and its backdrop) for an animation frame.
    fn draw_spark(&mut self) {
        let (x, y, bw, bh) = self.spark_box;
        let w = self.win.width() as usize;
        let (x0, y0, x1, y1) = (x as usize, y as usize, (x + bw) as usize, (y + bh) as usize);
        let src = self.frosted.data();
        let dst = self.win.data_mut();
        for yy in y0..y1 {
            dst[yy * w + x0..yy * w + x1].copy_from_slice(&src[yy * w + x0..yy * w + x1]);
        }
        let s = self.s;
        spark(&mut self.win, x + bw / 2.0, y + bh / 2.0, 52.0 * s, self.t0.elapsed().as_secs_f32(), true);
        self.win.update_rects(&[(x as i32, y as i32, bw as u32, bh as u32)]);
    }

    /// Unlock: the blur and darkening melt away to the plain wallpaper (~300 ms).
    fn melt(&mut self) {
        let start = Instant::now();
        loop {
            let a = (start.elapsed().as_secs_f32() / 0.3).min(1.0);
            let ia = (a * 256.0) as u32;
            for ((d, f), w) in self.win.data_mut().iter_mut().zip(self.frosted.data()).zip(self.wall.data()) {
                let m = |p: u8, q: u8| ((p as u32 * (256 - ia) + q as u32 * ia) >> 8) as u8;
                *d = Color::rgb(m(f.r(), w.r()), m(f.g(), w.g()), m(f.b(), w.b()));
            }
            self.win.sync();
            if a >= 1.0 {
                break;
            }
        }
    }
}

fn login_command(username: &str, pass: &str, cmd: &[String]) -> Option<Command> {
    let users = AllUsers::authenticator(Config::default()).ok()?;
    let user = users.get_by_name(username)?;
    if !user.verify_passwd(pass) {
        return None;
    }
    let mut command = user.login_cmd(&cmd[0]);
    command.args(&cmd[1..]);
    Some(command)
}

fn first_user() -> (String, Option<String>) {
    if let Ok(users) = AllUsers::basic(Config::default()) {
        let mut normal: Vec<_> = users.iter().filter(|u| u.uid >= 1000).map(|u| (u.user.clone(), u.home.clone())).collect();
        normal.sort();
        if let Some((name, home)) = normal.into_iter().next() {
            return (name, Some(home));
        }
    }
    ("user".into(), None)
}

/// Shows the greeter until a login succeeds; returns the session command.
fn greet(cmd: &[String], fonts: Fonts) -> Option<Command> {
    let (w, h) = orbclient::get_display_size().ok()?;
    let (user, home) = first_user();
    let wall = wallpaper(&wallpaper_path(home), w, h);
    let frosted = frost(&wall, 26.0, 0.5);
    let win = Window::new_flags(0, 0, w, h, "Iskra", &[WindowFlag::Borderless, WindowFlag::Async])?;
    let mut g = Greeter {
        win, wall, frosted, fonts, user, password: String::new(), error: false, shake: None,
        spark: spark_mode(), t0: Instant::now(), s: ((h / 1600) + 1) as f32,
        go: (0., 0., 0., 0.), restart: (0., 0., 0., 0.), shutdown: (0., 0., 0., 0.),
        spark_box: (0., 0., 0., 0.), pill_box: (0., 0., 0., 0.), hover: -1,
    };
    g.draw();

    user_data! { enum Ev { Win, Tick } }
    let queue = EventQueue::<Ev>::new().ok()?;
    let mut timer = Timer::new().ok()?;
    use std::os::unix::io::AsRawFd;
    queue.subscribe(g.win.as_raw_fd() as usize, Ev::Win, event::EventFlags::READ).ok()?;
    queue.subscribe(timer.0.as_raw_fd() as usize, Ev::Tick, event::EventFlags::READ).ok()?;
    timer.arm(66);
    let mut last_min = now().min;
    let (mut mx, mut my, mut left) = (0, 0, false);

    for ev in queue {
        match ev.ok()?.user_data {
            Ev::Tick => {
                let shaking = g.shake.map_or(false, |t| t.elapsed().as_secs_f32() < 0.5);
                if now().min != last_min || shaking {
                    last_min = now().min;
                    g.draw();
                } else if g.spark == SparkMode::On {
                    g.draw_spark();
                }
                // ~15 fps while anything moves, otherwise once a second for the clock.
                timer.arm(if g.spark == SparkMode::On || shaking { 66 } else { 1000 });
            }
            Ev::Win => {
                let mut submit = false;
                let mut redraw = false;
                for e in g.win.events() {
                    match e.to_option() {
                        EventOption::TextInput(t) if !t.character.is_control() => {
                            g.password.push(t.character);
                            redraw = true;
                        }
                        EventOption::Key(k) if k.pressed => match k.scancode {
                            orbclient::K_BKSP => { g.password.pop(); redraw = true; }
                            orbclient::K_ENTER => submit = true,
                            orbclient::K_ESC => { g.password.clear(); redraw = true; }
                            _ => {}
                        },
                        EventOption::Mouse(m) => {
                            (mx, my) = (m.x, m.y);
                            let hv = if inside(g.go, mx, my) { 0 } else if inside(g.restart, mx, my) { 1 } else if inside(g.shutdown, mx, my) { 2 } else { -1 };
                            if hv != g.hover { g.hover = hv; redraw = true; }
                        }
                        EventOption::Button(b) => {
                            if b.left && !left {
                                if inside(g.go, mx, my) { submit = true }
                                else if inside(g.restart, mx, my) { spawn("shutdown -r"); }
                                else if inside(g.shutdown, mx, my) { spawn("shutdown"); }
                            }
                            left = b.left;
                        }
                        _ => {}
                    }
                }
                if submit {
                    if let Some(c) = login_command(&g.user, &g.password, cmd) {
                        g.melt();
                        return Some(c);
                    }
                    g.password.clear();
                    g.error = true;
                    g.shake = Some(Instant::now());
                    timer.arm(30);
                    redraw = true;
                }
                if redraw { g.draw(); }
            }
        }
    }
    None
}

fn main() {
    let cmd: Vec<String> = std::env::args().skip(1).collect();
    if cmd.is_empty() {
        eprintln!("usage: iskra-greeter <session command> [args...]");
        std::process::exit(2);
    }
    loop {
        let Some(mut command) = greet(&cmd, Fonts::load()) else {
            eprintln!("iskra-greeter: greeter failed; retrying");
            std::thread::sleep(std::time::Duration::from_secs(1));
            continue;
        };
        // The greeter window is gone now, so its animation has stopped for the session.
        let user = first_user().0;
        let Ok(before_ns) = ns::apply_login_schemes(&user) else { continue };
        match command.spawn() {
            Ok(mut child) => { let _ = child.wait(); }
            Err(e) => eprintln!("iskra-greeter: failed to start {}: {}", cmd[0], e),
        }
        let _ = libredox::call::fcntl(before_ns.raw(), syscall::F_SETFD, 0);
        let _ = libredox::call::close(libredox::call::setns(before_ns.into_raw()).expect("failed to restore namespace"));
    }
}

/// Per-user scheme namespace, as orblogin does (orbutils, MIT). TODO upstream: move to redox_users.
mod ns {
    use serde::Deserialize;
    use std::collections::BTreeMap;

    const DEFAULT_SCHEMES: [&str; 27] = [
        "debug", "event", "memory", "pipe", "serio", "irq", "time", "sys", "rand", "null", "zero", "log",
        "ip", "icmp", "tcp", "udp", "shm", "chan", "uds_stream", "uds_dgram", "file", "display.vesa",
        "display*", "pty", "sudo", "audio", "orbital",
    ];

    #[derive(Deserialize)]
    struct UserSchemes { schemes: Vec<String> }
    #[derive(Deserialize)]
    struct LoginConfig { user_schemes: BTreeMap<String, UserSchemes> }

    pub fn apply_login_schemes(user: &str) -> libredox::error::Result<libredox::Fd> {
        let schemes: Vec<String> = std::fs::read_to_string("/etc/login_schemes.toml")
            .ok()
            .and_then(|s| toml::from_str::<LoginConfig>(&s).ok())
            .and_then(|mut c| c.user_schemes.remove(user))
            .map(|u| u.schemes)
            .unwrap_or_else(|| DEFAULT_SCHEMES.iter().map(|s| s.to_string()).collect());
        let names: Vec<ioslice::IoSlice> = schemes.iter().map(|s| ioslice::IoSlice::new(s.as_bytes())).collect();
        let ns_fd = libredox::call::mkns(&names)?;
        let before = libredox::Fd::new(libredox::call::setns(ns_fd)?);
        let _ = libredox::call::fcntl(before.raw(), syscall::F_SETFD, libredox::flag::O_CLOEXEC as usize);
        Ok(before)
    }
}
