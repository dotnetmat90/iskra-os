//! Shared drawing for the Iskra shell (DESIGN.md): anti-aliased shapes, frosted glass,
//! the wallpaper, Inter text, app icons, time. Everything draws straight into an orbclient
//! `Renderer` (a window or an image), with straight (non-premultiplied) alpha like Orbital.

use orbclient::image::ResizeType;
use orbclient::image::Image;
use orbclient::{Color, Renderer};
use orbfont::Font;
use std::fs::File;
use std::io::{Read, Write};
use std::process::Command;

pub const ACCENT: Color = Color::rgb(0x3d, 0xae, 0xe9);
pub const VIOLET: Color = Color::rgb(0x8b, 0x5c, 0xf6);
pub const WHITE: Color = Color::rgb(255, 255, 255);

pub fn white(alpha: f32) -> Color {
    Color::rgba(255, 255, 255, (alpha * 255.0) as u8)
}

pub fn black(alpha: f32) -> Color {
    Color::rgba(0, 0, 0, (alpha * 255.0) as u8)
}

pub fn lerp(a: Color, b: Color, t: f32) -> Color {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color::rgba(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

/// Source-over with extra coverage `cov` (0..1), straight alpha.
#[inline]
pub fn blend(dst: &mut Color, c: Color, cov: f32) {
    let a = c.a() as f32 / 255.0 * cov;
    if a <= 0.0 {
        return;
    }
    let d = *dst;
    let da = d.a() as f32 / 255.0;
    let oa = a + da * (1.0 - a);
    let mix = |s: u8, t: u8| ((s as f32 * a + t as f32 * da * (1.0 - a)) / oa) as u8;
    *dst = Color::rgba(mix(c.r(), d.r()), mix(c.g(), d.g()), mix(c.b(), d.b()), (oa * 255.0) as u8);
}

/// Fills the box (x0,y0)-(x1,y1) using a signed distance function; coverage = 0.5 - distance.
pub fn fill_sdf<R: Renderer + ?Sized>(
    r: &mut R,
    (x0, y0, x1, y1): (f32, f32, f32, f32),
    color: impl Fn(f32, f32) -> Color,
    sd: impl Fn(f32, f32) -> f32,
) {
    let (w, h) = (r.width() as i32, r.height() as i32);
    let data = r.data_mut();
    for y in (y0.floor() as i32 - 1).max(0)..(y1.ceil() as i32 + 1).min(h) {
        for x in (x0.floor() as i32 - 1).max(0)..(x1.ceil() as i32 + 1).min(w) {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let cov = (0.5 - sd(px, py)).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend(&mut data[(y * w + x) as usize], color(px, py), cov);
            }
        }
    }
}

pub fn sd_rrect(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, rad: f32) -> f32 {
    let (hw, hh) = (w / 2.0, h / 2.0);
    let qx = (px - x - hw).abs() - (hw - rad);
    let qy = (py - y - hh).abs() - (hh - rad);
    (qx.max(0.0).hypot(qy.max(0.0))) + qx.max(qy).min(0.0) - rad
}

pub fn sd_seg(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (pax, pay, bax, bay) = (px - ax, py - ay, bx - ax, by - ay);
    let t = ((pax * bax + pay * bay) / (bax * bax + bay * bay).max(1e-6)).clamp(0.0, 1.0);
    (pax - bax * t).hypot(pay - bay * t)
}

pub fn rrect<R: Renderer + ?Sized>(r: &mut R, x: f32, y: f32, w: f32, h: f32, rad: f32, c: Color) {
    fill_sdf(r, (x, y, x + w, y + h), |_, _| c, |px, py| sd_rrect(px, py, x, y, w, h, rad));
}

/// 1 px inner border of a rounded rectangle.
pub fn rrect_border<R: Renderer + ?Sized>(r: &mut R, x: f32, y: f32, w: f32, h: f32, rad: f32, c: Color) {
    fill_sdf(r, (x, y, x + w, y + h), |_, _| c, |px, py| {
        (sd_rrect(px, py, x, y, w, h, rad) + 0.5).abs() - 0.5
    });
}

pub fn circle<R: Renderer + ?Sized>(r: &mut R, cx: f32, cy: f32, rad: f32, c: Color) {
    fill_sdf(r, (cx - rad, cy - rad, cx + rad, cy + rad), |_, _| c, |px, py| {
        (px - cx).hypot(py - cy) - rad
    });
}

/// Line segment with round caps.
pub fn seg<R: Renderer + ?Sized>(r: &mut R, ax: f32, ay: f32, bx: f32, by: f32, t: f32, c: Color) {
    let bb = (ax.min(bx) - t, ay.min(by) - t, ax.max(bx) + t, ay.max(by) + t);
    fill_sdf(r, bb, |_, _| c, |px, py| sd_seg(px, py, ax, ay, bx, by) - t / 2.0);
}

/// Arc of a ring from angle a0 to a1 (radians, 0 = right, clockwise since y points down).
pub fn arc<R: Renderer + ?Sized>(r: &mut R, cx: f32, cy: f32, rad: f32, t: f32, a0: f32, a1: f32, c: Color) {
    let bb = (cx - rad - t, cy - rad - t, cx + rad + t, cy + rad + t);
    fill_sdf(r, bb, |_, _| c, |px, py| {
        let mut a = (py - cy).atan2(px - cx);
        while a < a0 {
            a += std::f32::consts::TAU;
        }
        if a <= a1 {
            ((px - cx).hypot(py - cy) - rad).abs() - t / 2.0
        } else {
            let e = |ang: f32| (px - cx - rad * ang.cos()).hypot(py - cy - rad * ang.sin());
            e(a0).min(e(a1)) - t / 2.0
        }
    });
}

/// Frosted glass: the (already blurred) backdrop `bg`, seen from window position (ox, oy),
/// saturated ×1.4 and tinted, clipped to a rounded rectangle with a light 1 px border.
pub fn glass<R: Renderer + ?Sized>(
    r: &mut R, bg: &Image, ox: i32, oy: i32, x: f32, y: f32, w: f32, h: f32, rad: f32, tint: Color,
) {
    let (bw, bh) = (bg.width() as i32, bg.height() as i32);
    let bgd = bg.data();
    let color = |px: f32, py: f32| {
        let bx = (ox + px as i32).clamp(0, bw - 1);
        let by = (oy + py as i32).clamp(0, bh - 1);
        let mut c = saturate(bgd[(by * bw + bx) as usize], 1.4);
        c = Color::rgb(c.r(), c.g(), c.b());
        blend(&mut c, tint, 1.0);
        c
    };
    fill_sdf(r, (x, y, x + w, y + h), color, |px, py| sd_rrect(px, py, x, y, w, h, rad));
    rrect_border(r, x, y, w, h, rad, white(0.16));
}

pub fn saturate(c: Color, s: f32) -> Color {
    let l = 0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32;
    let f = |v: u8| (l + (v as f32 - l) * s).clamp(0.0, 255.0) as u8;
    Color::rgba(f(c.r()), f(c.g()), f(c.b()), c.a())
}

/// The user's wallpaper (same lookup as orbutils' `background`), scaled to cover w×h.
pub fn wallpaper_path(home: Option<String>) -> String {
    if let Some(home) = home.or_else(|| std::env::var("HOME").ok()) {
        for name in ["background.png", "background.jpg"] {
            let p = format!("{}/{}", home, name);
            if std::path::Path::new(&p).is_file() {
                return p;
            }
        }
    }
    "/usr/share/ui/background.jpg".into()
}

pub fn wallpaper(path: &str, w: u32, h: u32) -> Image {
    let Ok(img) = Image::from_path(path) else {
        return Image::from_color(w, h, Color::rgb(0x0d, 0x13, 0x26));
    };
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let s = (w as f32 / iw).max(h as f32 / ih);
    let (sw, sh) = ((iw * s).ceil() as u32, (ih * s).ceil() as u32);
    let scaled = img.resize(sw.max(w), sh.max(h), ResizeType::Bilinear);
    let (ox, oy) = ((scaled.width() - w) / 2, (scaled.height() - h) / 2);
    let sd = scaled.data();
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let row = ((y + oy) * scaled.width() + ox) as usize;
        out.extend_from_slice(&sd[row..row + w as usize]);
    }
    Image::from_data(w, h, out.into()).unwrap()
}

/// Blur and darken: blur at quarter size (cheap, and a 20+ px blur hides the upscale), then
/// multiply brightness by `keep`.
pub fn frost(img: &Image, radius: f32, keep: f32) -> Image {
    let (w, h) = (img.width(), img.height());
    let mut small = img.resize((w / 4).max(1), (h / 4).max(1), ResizeType::Bilinear);
    let (sw, sh) = (small.width(), small.height());
    small.box_blur(0, 0, sw, sh, (radius / 4.0).max(1.0) as i32);
    let mut big = small.resize(w, h, ResizeType::Bilinear);
    for c in big.data_mut() {
        let f = |v: u8| (v as f32 * keep) as u8;
        *c = Color::rgb(f(c.r()), f(c.g()), f(c.b()));
    }
    big
}

pub struct Fonts {
    pub light: Font,
    pub regular: Font,
    pub medium: Font,
    pub semibold: Font,
}

impl Fonts {
    pub fn load() -> Fonts {
        let f = |name: &str| {
            Font::from_path(format!("/usr/share/fonts/Sans/Inter/Inter-{}.ttf", name))
                .or_else(|_| Font::find(Some("Sans"), None, None))
                .expect("no font")
        };
        Fonts { light: f("Light"), regular: f("Regular"), medium: f("Medium"), semibold: f("SemiBold") }
    }
}

pub fn text_w(font: &Font, s: &str, size: f32) -> f32 {
    font.render(s, size).width() as f32
}

/// Draws text with its top-left at (x, y). Returns the width.
pub fn text<R: Renderer + ?Sized>(r: &mut R, font: &Font, s: &str, size: f32, x: f32, y: f32, c: Color) -> f32 {
    let t = font.render(s, size);
    t.draw(r, x.round() as i32, y.round() as i32, c);
    t.width() as f32
}

/// Text with tabular digits (every digit takes the width of "0") and extra letter spacing.
/// With `draw` false it only measures.
pub fn text_tab<R: Renderer + ?Sized>(
    r: &mut R, font: &Font, s: &str, size: f32, x: f32, y: f32, tracking: f32, c: Color, draw: bool,
) -> f32 {
    let digit = text_w(font, "0", size);
    let mut cx = x;
    for ch in s.chars() {
        let t = font.render(&ch.to_string(), size);
        let adv = if ch.is_ascii_digit() { digit } else { t.width() as f32 };
        if draw {
            t.draw(r, (cx + (adv - t.width() as f32) / 2.0).round() as i32, y.round() as i32, c);
        }
        cx += adv + tracking;
    }
    cx - x - tracking
}

// ---- time ----

pub struct Clock {
    pub hour: u32,
    pub min: u32,
    pub weekday: usize,
    pub day: u32,
    pub month: usize,
}

pub const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
pub const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
    "November", "December",
];

// ponytail: UTC; Redox has no timezone database wired up yet.
pub fn now() -> Clock {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    let s = secs.rem_euclid(86400);
    // Howard Hinnant's civil_from_days
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as usize;
    Clock {
        hour: (s / 3600) as u32,
        min: (s / 60 % 60) as u32,
        weekday: (days + 3).rem_euclid(7) as usize, // 1970-01-01 was a Thursday
        day,
        month,
    }
}

impl Clock {
    pub fn hm(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.min)
    }
    pub fn long_date(&self) -> String {
        format!("{}, {} {}", WEEKDAYS[self.weekday], self.day, MONTHS[self.month - 1])
    }
    pub fn short_date(&self) -> String {
        format!("{} {} {}", &WEEKDAYS[self.weekday][..3], self.day, &MONTHS[self.month - 1][..3])
    }
}

/// Monotonic timer fd for redox_event: `arm(ms)` asks for one wake-up `ms` from now.
pub struct Timer(pub File);

impl Timer {
    pub fn new() -> std::io::Result<Timer> {
        Ok(Timer(File::open(format!("/scheme/time/{}", libredox::flag::CLOCK_MONOTONIC))?))
    }
    pub fn arm(&mut self, ms: u64) {
        let mut buf = [0u8; std::mem::size_of::<libredox::data::TimeSpec>()];
        if self.0.read(&mut buf).unwrap_or(0) < buf.len() {
            return;
        }
        let t = libredox::data::timespec_from_mut_bytes(&mut buf);
        let ns = t.tv_nsec as u64 + ms * 1_000_000;
        t.tv_sec = (t.tv_sec as u64 + ns / 1_000_000_000) as _;
        t.tv_nsec = (ns % 1_000_000_000) as _;
        let _ = self.0.write(&buf);
    }
}

pub fn spawn(cmd: &str) -> Option<std::process::Child> {
    // ponytail: whitespace split, no quoting; every exec line Iskra uses is simple.
    let mut parts = cmd.split_whitespace();
    Command::new(parts.next()?).args(parts).spawn().ok()
}

// ---- icons: white glyphs on coloured rounded squares (DESIGN.md "App icons") ----

#[derive(Clone, Copy, PartialEq)]
pub enum Glyph {
    Files,
    Terminal,
    Browser,
    Editor,
    Ai,
    Settings,
    Viewer,
    Letter(char),
}

pub fn glyph_for(name: &str, exec: &str) -> (Glyph, Color) {
    let k = format!("{} {}", name, exec).to_lowercase();
    if k.contains("file") {
        (Glyph::Files, ACCENT)
    } else if k.contains("term") {
        (Glyph::Terminal, Color::rgb(0x2c, 0x2f, 0x39))
    } else if k.contains("netsurf") || k.contains("firefox") || k.contains("browser") || k.contains("webkit") {
        (Glyph::Browser, Color::rgb(0xf2, 0x65, 0x2f))
    } else if k.contains("edit") || k.contains("text") {
        (Glyph::Editor, Color::rgb(0x2f, 0xbf, 0x71))
    } else if k.contains(" ai") || k.starts_with("ai") || k.contains("assistant") {
        (Glyph::Ai, VIOLET)
    } else if k.contains("setting") {
        (Glyph::Settings, Color::rgb(0x6b, 0x72, 0x80))
    } else if k.contains("view") || k.contains("image") {
        (Glyph::Viewer, Color::rgb(0xe5, 0x48, 0x4d))
    } else {
        let c = name.chars().next().unwrap_or('?').to_ascii_uppercase();
        let palette = [Color::rgb(0x3d, 0x7b, 0xf0), Color::rgb(0x0e, 0xa5, 0x9e), Color::rgb(0xd9, 0x7a, 0x1c), Color::rgb(0xc0, 0x3b, 0x8f)];
        (Glyph::Letter(c), palette[c as usize % palette.len()])
    }
}

/// A `size`×`size` app tile: rounded square (radius ~27% of size) with a white glyph.
pub fn app_icon<R: Renderer + ?Sized>(r: &mut R, fonts: &Fonts, x: f32, y: f32, size: f32, g: Glyph, bg: Color) {
    let top = lerp(bg, WHITE, 0.12);
    fill_sdf(r, (x, y, x + size, y + size), |_, py| lerp(top, bg, ((py - y) / size).clamp(0.0, 1.0)), |px, py| {
        sd_rrect(px, py, x, y, size, size, size * 0.27)
    });
    rrect_border(r, x, y, size, size, size * 0.27, white(0.10));
    let u = size / 34.0; // glyphs are designed on a 34 px tile
    let (cx, cy) = (x + size / 2.0, y + size / 2.0);
    let t = 2.0 * u;
    let w = WHITE;
    match g {
        Glyph::Files => {
            rrect(r, cx - 9.0 * u, cy - 7.0 * u, 8.0 * u, 4.0 * u, 1.5 * u, w);
            rrect(r, cx - 9.0 * u, cy - 5.0 * u, 18.0 * u, 13.0 * u, 2.0 * u, w);
        }
        Glyph::Terminal => {
            seg(r, cx - 8.0 * u, cy - 5.0 * u, cx - 3.0 * u, cy, t, w);
            seg(r, cx - 3.0 * u, cy, cx - 8.0 * u, cy + 5.0 * u, t, w);
            seg(r, cx, cy + 5.0 * u, cx + 8.0 * u, cy + 5.0 * u, t, w);
        }
        Glyph::Browser => {
            arc(r, cx, cy, 9.0 * u, 1.8 * u, 0.0, std::f32::consts::TAU, w);
            seg(r, cx - 9.0 * u, cy, cx + 9.0 * u, cy, 1.6 * u, w);
            fill_sdf(r, (cx - 5.0 * u, cy - 10.0 * u, cx + 5.0 * u, cy + 10.0 * u), |_, _| w, |px, py| {
                // ellipse outline for the meridian
                let (dx, dy) = ((px - cx) / (4.5 * u), (py - cy) / (9.0 * u));
                ((dx * dx + dy * dy).sqrt() - 1.0).abs() * 4.5 * u - 0.8 * u
            });
        }
        Glyph::Editor => {
            for (i, len) in [16.0, 12.0, 16.0, 9.0].iter().enumerate() {
                let yy = cy - 6.0 * u + i as f32 * 4.0 * u;
                seg(r, cx - 8.0 * u, yy, cx - 8.0 * u + len * u, yy, t, w);
            }
        }
        Glyph::Ai => spark_star(r, cx, cy, 10.0 * u, w),
        Glyph::Settings => {
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                seg(r, cx + 6.0 * u * a.cos(), cy + 6.0 * u * a.sin(), cx + 10.0 * u * a.cos(), cy + 10.0 * u * a.sin(), 3.2 * u, w);
            }
            circle(r, cx, cy, 7.5 * u, w);
            circle(r, cx, cy, 3.0 * u, bg);
        }
        Glyph::Viewer => {
            rrect_border(r, cx - 9.0 * u, cy - 7.0 * u, 18.0 * u, 14.0 * u, 2.0 * u, w);
            rrect_border(r, cx - 8.0 * u, cy - 6.0 * u, 16.0 * u, 12.0 * u, 1.5 * u, w);
            circle(r, cx + 3.5 * u, cy - 2.0 * u, 1.8 * u, w);
            seg(r, cx - 6.0 * u, cy + 4.0 * u, cx - 1.0 * u, cy - 1.0 * u, t, w);
            seg(r, cx - 1.0 * u, cy - 1.0 * u, cx + 6.0 * u, cy + 4.0 * u, t, w);
        }
        Glyph::Letter(c) => {
            let s = c.to_string();
            let fs = 18.0 * u;
            let tw = text_w(&fonts.semibold, &s, fs);
            text(r, &fonts.semibold, &s, fs, cx - tw / 2.0, cy - fs * 0.55, w);
        }
    }
}

/// Four-point star (the Iskra spark / AI glyph).
pub fn spark_star<R: Renderer + ?Sized>(r: &mut R, cx: f32, cy: f32, rad: f32, c: Color) {
    fill_sdf(r, (cx - rad, cy - rad, cx + rad, cy + rad), |_, _| c, |px, py| {
        // astroid-like star: |x|^p + |y|^p = rad^p with p < 1
        let (dx, dy) = (((px - cx) / rad).abs(), ((py - cy) / rad).abs());
        let p = 0.55;
        ((dx.powf(p) + dy.powf(p)).powf(1.0 / p) - 1.0) * rad * 0.45
    });
}

/// The breathing spark: white core, cool-blue halo, `particles` twinkling around it.
/// `t` is seconds; a still spark is `t = 0`.
pub fn spark<R: Renderer + ?Sized>(r: &mut R, cx: f32, cy: f32, size: f32, t: f32, particles: bool) {
    let breathe = 0.5 - 0.5 * (t * std::f32::consts::TAU / 4.0).cos(); // 0..1, 4 s period
    let s = size / 52.0;
    let halo = (10.0 + 4.0 * breathe) * s;
    fill_sdf(r, (cx - size / 2.0, cy - size / 2.0, cx + size / 2.0, cy + size / 2.0), |px, py| {
        let d = (px - cx).hypot(py - cy) / (halo * 1.9);
        Color::rgba(150, 200, 255, ((1.0 - d).max(0.0).powi(2) * (70.0 + 40.0 * breathe)) as u8)
    }, |px, py| (px - cx).hypot(py - cy) - halo * 1.9);
    spark_star(r, cx, cy, (8.0 + 3.0 * breathe) * s, white(0.92));
    circle(r, cx, cy, (2.2 + 0.6 * breathe) * s, WHITE);
    if particles {
        for i in 0..12 {
            let fi = i as f32;
            let orbit = (11.0 + (fi * 7.3) % 12.0) * s;
            let speed = 0.25 + (fi * 0.37) % 0.3;
            let a = fi * 2.399 + t * speed * if i % 2 == 0 { 1.0 } else { -1.0 };
            let tw = 0.5 + 0.5 * (t * (1.3 + fi * 0.21) + fi).sin();
            circle(r, cx + orbit * a.cos(), cy + orbit * a.sin(), (0.7 + 0.5 * tw) * s, white(0.25 + 0.6 * tw));
        }
    }
}

// ---- tray glyphs (white, ~16 px) ----

pub fn wifi_glyph<R: Renderer + ?Sized>(r: &mut R, cx: f32, cy: f32, c: Color) {
    use std::f32::consts::PI;
    for rad in [4.0, 8.0] {
        arc(r, cx, cy + 5.0, rad, 1.6, -PI * 0.75, -PI * 0.25, c);
    }
    circle(r, cx, cy + 5.0, 1.4, c);
}

pub fn volume_glyph<R: Renderer + ?Sized>(r: &mut R, cx: f32, cy: f32, c: Color) {
    use std::f32::consts::PI;
    rrect(r, cx - 7.0, cy - 2.5, 3.5, 5.0, 0.8, c);
    fill_sdf(r, (cx - 5.0, cy - 6.0, cx, cy + 6.0), |_, _| c, |px, py| {
        let k = (px - (cx - 5.0)) / 5.0; // 0..1 widening cone
        ((py - cy).abs() - (2.5 + 3.5 * k)).max((px - cx).max(cx - 5.0 - px))
    });
    arc(r, cx + 0.5, cy, 4.0, 1.5, -PI * 0.3, PI * 0.3, c);
    arc(r, cx + 0.5, cy, 7.0, 1.5, -PI * 0.3, PI * 0.3, c);
}

pub fn battery_glyph<R: Renderer + ?Sized>(r: &mut R, cx: f32, cy: f32, c: Color) {
    rrect_border(r, cx - 9.0, cy - 5.0, 16.0, 10.0, 2.5, c);
    rrect_border(r, cx - 8.5, cy - 4.5, 15.0, 9.0, 2.0, c);
    rrect(r, cx - 6.5, cy - 2.5, 9.0, 5.0, 1.0, c);
    rrect(r, cx + 7.5, cy - 2.0, 2.0, 4.0, 0.8, c);
}

pub fn search_glyph<R: Renderer + ?Sized>(r: &mut R, cx: f32, cy: f32, c: Color) {
    arc(r, cx - 1.5, cy - 1.5, 5.5, 1.8, 0.0, std::f32::consts::TAU, c);
    seg(r, cx + 2.5, cy + 2.5, cx + 7.0, cy + 7.0, 2.0, c);
}

pub fn power_glyph<R: Renderer + ?Sized>(r: &mut R, cx: f32, cy: f32, c: Color) {
    use std::f32::consts::PI;
    arc(r, cx, cy + 0.5, 5.5, 1.6, -PI * 0.33, PI * 1.33, c);
    seg(r, cx, cy - 6.5, cx, cy - 0.5, 1.6, c);
}

pub fn restart_glyph<R: Renderer + ?Sized>(r: &mut R, cx: f32, cy: f32, c: Color) {
    use std::f32::consts::PI;
    arc(r, cx, cy, 5.5, 1.6, -PI * 0.25, PI * 1.5, c);
    // arrow head at the top end of the arc
    let (ax, ay) = (cx + 5.5 * (-PI * 0.25f32).cos(), cy + 5.5 * (-PI * 0.25f32).sin());
    seg(r, ax, ay, ax - 3.5, ay - 0.5, 1.6, c);
    seg(r, ax, ay, ax + 0.5, ay + 3.5, 1.6, c);
}
