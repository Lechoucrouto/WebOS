use crate::exec::{DeclarativeRuntime, WboSubsystem};
use crate::font::glyph_for;
use alloc::string::String;
use alloc::vec::Vec;

pub const FB_WIDTH: usize = 1280;
pub const FB_HEIGHT: usize = 720;
pub const TASKBAR_H: i32 = 42;
pub const TITLEBAR_H: i32 = 30;

pub const CHAR_W: usize = 11;
pub const CHAR_H: usize = 19;

pub const fn rgba(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | (0xFF << 24)
}

pub fn blend_rgba(fg: u32, bg: u32, alpha: u8) -> u32 {
    let a = alpha as u32;
    let inv_a = 255 - a;

    let fr = fg & 0xFF;
    let fg_g = (fg >> 8) & 0xFF;
    let fb = (fg >> 16) & 0xFF;

    let br = bg & 0xFF;
    let bg_g = (bg >> 8) & 0xFF;
    let bb = (bg >> 16) & 0xFF;

    let r = (fr * a + br * inv_a) / 255;
    let g = (fg_g * a + bg_g * inv_a) / 255;
    let b = (fb * a + bb * inv_a) / 255;

    r | (g << 8) | (b << 16) | (0xFF << 24)
}

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    pub fn intersect(&self, other: Rect) -> Rect {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        Rect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Terminal,
    Files,
    Editor,
    SysMon,
    AppBinary,
    Blueprint,
    Script,
    Folder,
    FileDoc,
    WebOsLogo,
}

pub struct EditorState {
    pub file_path: String,
    pub lines: Vec<String>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub scroll_y: usize,
    pub scroll_x: usize,
    pub status: String,
    pub built_wbo_path: Option<String>,
}

pub struct Window {
    pub id: usize,
    pub exe_path: String,
    pub subsystem: WboSubsystem,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub minimized: bool,
    pub maximized: bool,
    pub saved_bounds: Option<Rect>,
    pub editor: Option<EditorState>,
    pub runtime: Option<DeclarativeRuntime>,
}

impl Window {
    pub fn frame_rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }

    pub fn titlebar_rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, 30)
    }

    pub fn close_btn_rect(&self) -> Rect {
        Rect::new(self.x + self.w - 42, self.y + 1, 41, 28)
    }

    pub fn max_btn_rect(&self) -> Rect {
        Rect::new(self.x + self.w - 78, self.y + 1, 36, 28)
    }

    pub fn min_btn_rect(&self) -> Rect {
        Rect::new(self.x + self.w - 114, self.y + 1, 36, 28)
    }

    pub fn resize_grip_rect(&self) -> Rect {
        Rect::new(self.x + self.w - 14, self.y + self.h - 14, 14, 14)
    }

    pub fn client_rect(&self) -> Rect {
        Rect::new(
            self.x + 1,
            self.y + 30,
            (self.w - 2).max(0),
            (self.h - 31).max(0),
        )
    }
}

fn glyph_bit(bitmap: u64, gx: isize, gy: isize) -> bool {
    if !(0..5).contains(&gx) || !(0..7).contains(&gy) {
        return false;
    }
    let bit_index = (gy as usize) * 5 + (4 - gx as usize);
    ((bitmap >> bit_index) & 1) == 1
}

pub struct Canvas<'a> {
    buf: &'a mut [u32],
    clip: Rect,
}

impl<'a> Canvas<'a> {
    pub fn new(buf: &'a mut [u32]) -> Self {
        Self {
            buf,
            clip: Rect::new(0, 0, FB_WIDTH as i32, FB_HEIGHT as i32),
        }
    }

    pub fn draw_win10_button(
        &mut self,
        r: Rect,
        label: &str,
        hovered: bool,
        primary: bool,
        text_color: u32,
    ) {
        let (bg, border) = if primary {
            if hovered {
                (rgba(24, 144, 240), rgba(96, 180, 255))
            } else {
                (rgba(0, 120, 215), rgba(38, 148, 235))
            }
        } else if hovered {
            (rgba(45, 52, 64), rgba(110, 122, 140))
        } else {
            (rgba(33, 38, 46), rgba(62, 70, 82))
        };

        self.fill_rect(r, bg);
        self.stroke_rect(r, border);

        let tx = r.x + (r.w - (label.chars().count() as i32 * CHAR_W as i32)) / 2;
        let ty = r.y + (r.h - CHAR_H as i32) / 2;
        self.draw_text(tx.max(r.x + 4), ty, label, text_color, bg);
    }

    pub fn draw_win10_window_chrome(
        &mut self,
        bounds: Rect,
        title: &str,
        icon: IconKind,
        focused: bool,
        maximized: bool,
        hover_btn: u8, // 0 = none, 1 = min, 2 = max, 3 = close
    ) {
        self.reset_clip();

        if !maximized {
            self.fill_rect(
                Rect::new(bounds.x + 3, bounds.y + 3, bounds.w, bounds.h),
                rgba(4, 6, 10),
            );
        }

        self.fill_rect(bounds, rgba(16, 20, 26));

        let title_rect = Rect::new(bounds.x, bounds.y, bounds.w, 30);
        let title_bg = if focused {
            rgba(24, 31, 42)
        } else {
            rgba(19, 23, 30)
        };
        self.fill_rect(title_rect, title_bg);
        self.fill_rect(
            Rect::new(bounds.x, bounds.y + 29, bounds.w, 1),
            rgba(46, 54, 66),
        );

        if focused {
            self.fill_rect(
                Rect::new(bounds.x, bounds.y, 3, 30),
                rgba(0, 120, 215),
            );
        }

        self.draw_icon(bounds.x + 10, bounds.y + 7, icon);

        let title_col = if focused {
            rgba(240, 246, 252)
        } else {
            rgba(145, 155, 170)
        };
        self.draw_text(bounds.x + 32, bounds.y + 7, title, title_col, title_bg);

        let close_r = Rect::new(bounds.x + bounds.w - 42, bounds.y + 1, 41, 28);
        let max_r = Rect::new(bounds.x + bounds.w - 78, bounds.y + 1, 36, 28);
        let min_r = Rect::new(bounds.x + bounds.w - 114, bounds.y + 1, 36, 28);

        if hover_btn == 1 {
            self.fill_rect(min_r, rgba(44, 52, 64));
        }
        self.fill_rect(
            Rect::new(min_r.x + 13, min_r.y + 15, 10, 1),
            rgba(220, 226, 234),
        );

        if hover_btn == 2 {
            self.fill_rect(max_r, rgba(44, 52, 64));
        }
        if maximized {
            self.stroke_rect(
                Rect::new(max_r.x + 15, max_r.y + 9, 8, 8),
                rgba(220, 226, 234),
            );
            self.fill_rect(
                Rect::new(max_r.x + 13, max_r.y + 11, 8, 8),
                title_bg,
            );
            self.stroke_rect(
                Rect::new(max_r.x + 13, max_r.y + 11, 8, 8),
                rgba(220, 226, 234),
            );
        } else {
            self.stroke_rect(
                Rect::new(max_r.x + 13, max_r.y + 9, 10, 10),
                rgba(220, 226, 234),
            );
        }

        if hover_btn == 3 {
            self.fill_rect(close_r, rgba(232, 17, 35));
        }
        let cx = close_r.x + 16;
        let cy = close_r.y + 10;
        for d in 0..9 {
            self.put_pixel(cx + d, cy + d, rgba(240, 245, 250));
            self.put_pixel(cx + 8 - d, cy + d, rgba(240, 245, 250));
        }

        let border_col = if focused {
            rgba(0, 120, 215)
        } else {
            rgba(52, 60, 72)
        };
        self.stroke_rect(bounds, border_col);
    }

    pub fn set_clip(&mut self, rect: Rect) {
        let screen = Rect::new(0, 0, FB_WIDTH as i32, FB_HEIGHT as i32);
        self.clip = screen.intersect(rect);
    }

    pub fn reset_clip(&mut self) {
        self.clip = Rect::new(0, 0, FB_WIDTH as i32, FB_HEIGHT as i32);
    }

    #[inline]
    pub fn put_pixel(&mut self, x: i32, y: i32, color: u32) {
        if self.clip.contains(x, y) {
            self.buf[(y as usize) * FB_WIDTH + (x as usize)] = color;
        }
    }

    #[inline]
    pub fn blend_pixel(&mut self, x: i32, y: i32, color: u32, alpha: u8) {
        if self.clip.contains(x, y) {
            let idx = (y as usize) * FB_WIDTH + (x as usize);
            self.buf[idx] = blend_rgba(color, self.buf[idx], alpha);
        }
    }

    pub fn fill_rect(&mut self, rect: Rect, color: u32) {
        let r = self.clip.intersect(rect);
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        for py in r.y..(r.y + r.h) {
            let row_start = (py as usize) * FB_WIDTH + (r.x as usize);
            let row_end = row_start + (r.w as usize);
            self.buf[row_start..row_end].fill(color);
        }
    }

    pub fn fill_rect_alpha(&mut self, rect: Rect, color: u32, alpha: u8) {
        let r = self.clip.intersect(rect);
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        for py in r.y..(r.y + r.h) {
            let row_off = (py as usize) * FB_WIDTH;
            for px in r.x..(r.x + r.w) {
                let idx = row_off + (px as usize);
                self.buf[idx] = blend_rgba(color, self.buf[idx], alpha);
            }
        }
    }

    pub fn fill_gradient_v(&mut self, rect: Rect, top_col: u32, bot_col: u32) {
        let r = self.clip.intersect(rect);
        if r.w <= 0 || r.h <= 0 || rect.h <= 0 {
            return;
        }
        let denom = (rect.h - 1).max(1) as u32;
        for py in r.y..(r.y + r.h) {
            let t = ((py - rect.y).clamp(0, rect.h - 1) as u32 * 255) / denom;
            let row_col = blend_rgba(bot_col, top_col, t as u8);
            let row_start = (py as usize) * FB_WIDTH + (r.x as usize);
            let row_end = row_start + (r.w as usize);
            self.buf[row_start..row_end].fill(row_col);
        }
    }

    pub fn stroke_rect(&mut self, rect: Rect, color: u32) {
        if rect.w <= 0 || rect.h <= 0 {
            return;
        }
        self.fill_rect(Rect::new(rect.x, rect.y, rect.w, 1), color);
        self.fill_rect(Rect::new(rect.x, rect.y + rect.h - 1, rect.w, 1), color);
        self.fill_rect(Rect::new(rect.x, rect.y, 1, rect.h), color);
        self.fill_rect(Rect::new(rect.x + rect.w - 1, rect.y, 1, rect.h), color);
    }

    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: i32, color: u32) {
        if rect.w <= 0 || rect.h <= 0 {
            return;
        }
        let r = radius.min(rect.w / 2).min(rect.h / 2).max(0);
        if r <= 0 {
            self.fill_rect(rect, color);
            return;
        }
        let clipped = self.clip.intersect(rect);
        if clipped.w <= 0 || clipped.h <= 0 {
            return;
        }
        let r2 = r * r;
        for py in clipped.y..(clipped.y + clipped.h) {
            let dy = py - rect.y;
            let corner_dy = if dy < r {
                r - dy
            } else if dy >= rect.h - r {
                dy - (rect.h - r - 1)
            } else {
                0
            };

            let inset = if corner_dy > 0 {
                let mut dx = r;
                while dx > 0 && (dx * dx + corner_dy * corner_dy) > r2 {
                    dx -= 1;
                }
                r - dx
            } else {
                0
            };

            let x_start = (rect.x + inset).max(clipped.x);
            let x_end = (rect.x + rect.w - inset).min(clipped.x + clipped.w);
            if x_start < x_end {
                let row_off = (py as usize) * FB_WIDTH;
                self.buf[row_off + x_start as usize..row_off + x_end as usize].fill(color);
            }
        }
    }

    pub fn stroke_rounded_rect(&mut self, rect: Rect, radius: i32, color: u32) {
        if rect.w <= 1 || rect.h <= 1 {
            return;
        }
        let r = radius.min(rect.w / 2).min(rect.h / 2).max(0);
        if r <= 0 {
            self.stroke_rect(rect, color);
            return;
        }
        let r_out2 = r * r;
        let r_in = (r - 1).max(0);
        let r_in2 = r_in * r_in;

        for dy in 0..rect.h {
            let py = rect.y + dy;
            let corner_dy = if dy < r {
                r - dy
            } else if dy >= rect.h - r {
                dy - (rect.h - r - 1)
            } else {
                0
            };

            if corner_dy == 0 {
                self.put_pixel(rect.x, py, color);
                self.put_pixel(rect.x + rect.w - 1, py, color);
            } else {
                for dx in 0..rect.w {
                    let corner_dx = if dx < r {
                        r - dx
                    } else if dx >= rect.w - r {
                        dx - (rect.w - r - 1)
                    } else {
                        0
                    };
                    let d2 = corner_dx * corner_dx + corner_dy * corner_dy;
                    if d2 <= r_out2 && (corner_dx == 0 || corner_dy == 0 || d2 >= r_in2) {
                        self.put_pixel(rect.x + dx, py, color);
                    }
                }
            }
        }
    }

    pub fn draw_shadow(&mut self, rect: Rect, radius: i32) {
        for i in (1..=4).rev() {
            let spread = i * 3;
            let alpha = (42 - i * 8).max(6) as u8;
            let srect = Rect::new(
                rect.x - spread / 2,
                rect.y + i * 2,
                rect.w + spread,
                rect.h + spread,
            );
            let clipped = self.clip.intersect(srect);
            if clipped.w <= 0 || clipped.h <= 0 {
                continue;
            }
            let r = (radius + i).min(srect.w / 2).min(srect.h / 2);
            let r2 = r * r;
            let shadow_col = rgba(3, 5, 9);
            for py in clipped.y..(clipped.y + clipped.h) {
                let dy = py - srect.y;
                let cdy = if dy < r {
                    r - dy
                } else if dy >= srect.h - r {
                    dy - (srect.h - r - 1)
                } else {
                    0
                };
                let inset = if cdy > 0 {
                    let mut dx = r;
                    while dx > 0 && (dx * dx + cdy * cdy) > r2 {
                        dx -= 1;
                    }
                    r - dx
                } else {
                    0
                };
                let xs = (srect.x + inset).max(clipped.x);
                let xe = (srect.x + srect.w - inset).min(clipped.x + clipped.w);
                if xs < xe {
                    let row_off = (py as usize) * FB_WIDTH;
                    for px in xs..xe {
                        let idx = row_off + px as usize;
                        self.buf[idx] = blend_rgba(shadow_col, self.buf[idx], alpha);
                    }
                }
            }
        }
    }

    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, color: u32) {
        if radius <= 0 {
            return;
        }
        let r2 = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= r2 {
                    self.put_pixel(cx + dx, cy + dy, color);
                }
            }
        }
    }

    pub fn draw_line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: u32) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put_pixel(x0, y0, color);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    pub fn draw_char_scaled(&mut self, x: i32, y: i32, ch: char, fg: u32, scale: i32) {
        if ch == ' ' {
            return;
        }
        let bitmap = glyph_for(ch);
        let s = scale.max(1);
        for gy in 0..7isize {
            for gx in 0..5isize {
                let px = x + (gx as i32) * s;
                let py = y + (gy as i32) * s;

                if glyph_bit(bitmap, gx, gy) {
                    for dy in 0..s {
                        for dx in 0..s {
                            self.put_pixel(px + dx, py + dy, fg);
                        }
                    }
                } else if s >= 2 {
                    let top = glyph_bit(bitmap, gx, gy - 1);
                    let bottom = glyph_bit(bitmap, gx, gy + 1);
                    let left = glyph_bit(bitmap, gx - 1, gy);
                    let right = glyph_bit(bitmap, gx + 1, gy);

                    if top && left {
                        self.blend_pixel(px, py, fg, 85);
                    }
                    if top && right {
                        self.blend_pixel(px + s - 1, py, fg, 85);
                    }
                    if bottom && left {
                        self.blend_pixel(px, py + s - 1, fg, 85);
                    }
                    if bottom && right {
                        self.blend_pixel(px + s - 1, py + s - 1, fg, 85);
                    }
                }
            }
        }
    }

    pub fn draw_char(&mut self, x: i32, y: i32, ch: char, fg: u32, _bg: u32) {
        self.draw_char_scaled(x, y, ch, fg, 2);
    }

    pub fn draw_text(&mut self, mut x: i32, y: i32, text: &str, fg: u32, _bg: u32) {
        for ch in text.chars() {
            self.draw_char_scaled(x, y, ch, fg, 2);
            x += CHAR_W as i32;
        }
    }

    pub fn draw_text_scaled(&mut self, mut x: i32, y: i32, text: &str, fg: u32, scale: i32) {
        let step = 5 * scale + scale.max(1);
        for ch in text.chars() {
            self.draw_char_scaled(x, y, ch, fg, scale);
            x += step;
        }
    }

    pub fn draw_icon(&mut self, x: i32, y: i32, kind: IconKind) {
        match kind {
            IconKind::Terminal => {
                self.fill_rounded_rect(Rect::new(x, y, 16, 16), 3, rgba(22, 31, 46));
                self.stroke_rounded_rect(Rect::new(x, y, 16, 16), 3, rgba(88, 242, 132));
                self.draw_line(x + 4, y + 5, x + 7, y + 8, rgba(88, 242, 132));
                self.draw_line(x + 7, y + 8, x + 4, y + 11, rgba(88, 242, 132));
                self.fill_rect(Rect::new(x + 8, y + 11, 5, 2), rgba(88, 242, 132));
            }
            IconKind::Files | IconKind::Folder => {
                self.fill_rounded_rect(Rect::new(x + 1, y + 2, 7, 4), 1, rgba(235, 165, 58));
                self.fill_rounded_rect(Rect::new(x + 1, y + 4, 14, 10), 2, rgba(255, 194, 92));
            }
            IconKind::Editor | IconKind::Blueprint => {
                self.fill_rounded_rect(Rect::new(x + 1, y + 1, 14, 14), 3, rgba(24, 54, 88));
                self.stroke_rounded_rect(Rect::new(x + 1, y + 1, 14, 14), 3, rgba(88, 166, 255));
                self.fill_rect(Rect::new(x + 4, y + 5, 8, 2), rgba(121, 192, 255));
                self.fill_rect(Rect::new(x + 4, y + 9, 6, 2), rgba(126, 231, 135));
            }
            IconKind::SysMon => {
                self.fill_rounded_rect(Rect::new(x + 1, y + 1, 14, 14), 3, rgba(20, 46, 44));
                self.stroke_rounded_rect(Rect::new(x + 1, y + 1, 14, 14), 3, rgba(63, 185, 160));
                self.draw_line(x + 3, y + 10, x + 6, y + 10, rgba(88, 242, 132));
                self.draw_line(x + 6, y + 10, x + 8, y + 5, rgba(88, 242, 132));
                self.draw_line(x + 8, y + 5, x + 10, y + 12, rgba(88, 242, 132));
                self.draw_line(x + 10, y + 12, x + 13, y + 8, rgba(88, 242, 132));
            }
            IconKind::AppBinary => {
                self.fill_rounded_rect(Rect::new(x + 1, y + 1, 14, 14), 4, rgba(35, 105, 62));
                self.stroke_rounded_rect(Rect::new(x + 1, y + 1, 14, 14), 4, rgba(126, 231, 135));
                self.fill_rect(Rect::new(x + 6, y + 5, 5, 6), rgba(255, 255, 255));
            }
            IconKind::Script => {
                self.fill_rounded_rect(Rect::new(x + 2, y + 1, 12, 14), 2, rgba(38, 44, 58));
                self.stroke_rounded_rect(Rect::new(x + 2, y + 1, 12, 14), 2, rgba(210, 168, 255));
                self.fill_rect(Rect::new(x + 5, y + 5, 6, 2), rgba(210, 168, 255));
                self.fill_rect(Rect::new(x + 5, y + 9, 4, 2), rgba(210, 168, 255));
            }
            IconKind::FileDoc => {
                self.fill_rounded_rect(Rect::new(x + 2, y + 1, 12, 14), 2, rgba(33, 39, 51));
                self.stroke_rounded_rect(Rect::new(x + 2, y + 1, 12, 14), 2, rgba(139, 148, 158));
                self.fill_rect(Rect::new(x + 5, y + 5, 6, 1), rgba(201, 209, 217));
                self.fill_rect(Rect::new(x + 5, y + 8, 6, 1), rgba(201, 209, 217));
                self.fill_rect(Rect::new(x + 5, y + 11, 4, 1), rgba(201, 209, 217));
            }
            IconKind::WebOsLogo => {
                self.fill_rounded_rect(Rect::new(x + 1, y + 1, 6, 6), 2, rgba(88, 166, 255));
                self.fill_rounded_rect(Rect::new(x + 9, y + 1, 6, 6), 2, rgba(126, 231, 135));
                self.fill_rounded_rect(Rect::new(x + 1, y + 9, 6, 6), 2, rgba(210, 168, 255));
                self.fill_rounded_rect(Rect::new(x + 9, y + 9, 6, 6), 2, rgba(255, 184, 77));
            }
        }
    }

    pub fn draw_wallpaper(&mut self) {
        let desk_h = FB_HEIGHT as i32 - TASKBAR_H;
        self.fill_gradient_v(
            Rect::new(0, 0, FB_WIDTH as i32, desk_h),
            rgba(10, 15, 26),
            rgba(18, 27, 44),
        );

        self.fill_rect_alpha(
            Rect::new(0, desk_h - 160, FB_WIDTH as i32, 160),
            rgba(36, 68, 118),
            26,
        );

        let dot = rgba(46, 63, 88);
        let mut y = 28;
        while y < desk_h {
            let mut x = 28;
            while x < FB_WIDTH as i32 {
                self.put_pixel(x, y, dot);
                x += 36;
            }
            y += 36;
        }
    }

    pub fn draw_cursor(&mut self, mx: i32, my: i32) {
        #[rustfmt::skip]
        const CURSOR: [[u8; 12]; 18] = [
            [1,0,0,0,0,0,0,0,0,0,0,0],
            [1,1,0,0,0,0,0,0,0,0,0,0],
            [1,2,1,0,0,0,0,0,0,0,0,0],
            [1,2,2,1,0,0,0,0,0,0,0,0],
            [1,2,2,2,1,0,0,0,0,0,0,0],
            [1,2,2,2,2,1,0,0,0,0,0,0],
            [1,2,2,2,2,2,1,0,0,0,0,0],
            [1,2,2,2,2,2,2,1,0,0,0,0],
            [1,2,2,2,2,2,2,2,1,0,0,0],
            [1,2,2,2,2,2,2,2,2,1,0,0],
            [1,2,2,2,2,2,2,2,2,2,1,0],
            [1,2,2,2,2,2,2,1,1,1,1,1],
            [1,2,2,2,1,2,2,1,0,0,0,0],
            [1,2,2,1,0,1,2,2,1,0,0,0],
            [1,2,1,0,0,1,2,2,1,0,0,0],
            [1,1,0,0,0,0,1,2,2,1,0,0],
            [0,0,0,0,0,0,1,2,2,1,0,0],
            [0,0,0,0,0,0,0,1,1,0,0,0],
        ];

        self.reset_clip();
        let border = rgba(10, 12, 16);
        let fill = rgba(248, 250, 252);

        for (dy, row) in CURSOR.iter().enumerate() {
            for (dx, &px) in row.iter().enumerate() {
                if px == 1 {
                    self.put_pixel(mx + dx as i32, my + dy as i32, border);
                } else if px == 2 {
                    self.put_pixel(mx + dx as i32, my + dy as i32, fill);
                }
            }
        }
    }
}