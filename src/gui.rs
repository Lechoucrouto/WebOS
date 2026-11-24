use crate::exec::WboSubsystem;
use crate::font::glyph_for;
use alloc::string::String;
use alloc::vec::Vec;

pub const FB_WIDTH: usize = 1280;
pub const FB_HEIGHT: usize = 720;
pub const TASKBAR_H: i32 = 36;
pub const TITLEBAR_H: i32 = 28;

pub const CHAR_W: usize = 12;
pub const CHAR_H: usize = 20;

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

pub struct Window {
    pub id: usize,
    pub exe_path: String,
    pub subsystem: WboSubsystem,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub open: bool,
    pub pinned: bool,
    pub custom_lines: Vec<String>,
}

impl Window {
    pub fn frame_rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }

    pub fn titlebar_rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, TITLEBAR_H)
    }

    pub fn close_btn_rect(&self) -> Rect {
        Rect::new(self.x + self.w - 24, self.y + 4, 20, 20)
    }

    pub fn client_rect(&self) -> Rect {
        Rect::new(
            self.x + 2,
            self.y + TITLEBAR_H,
            (self.w - 4).max(0),
            (self.h - TITLEBAR_H - 2).max(0),
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

    pub fn stroke_rect(&mut self, rect: Rect, color: u32) {
        if rect.w <= 0 || rect.h <= 0 {
            return;
        }
        self.fill_rect(Rect::new(rect.x, rect.y, rect.w, 1), color);
        self.fill_rect(Rect::new(rect.x, rect.y + rect.h - 1, rect.w, 1), color);
        self.fill_rect(Rect::new(rect.x, rect.y, 1, rect.h), color);
        self.fill_rect(Rect::new(rect.x + rect.w - 1, rect.y, 1, rect.h), color);
    }

    pub fn draw_char(&mut self, x: i32, y: i32, ch: char, fg: u32, bg: u32) {
        if ch == ' ' {
            return;
        }
        let bitmap = glyph_for(ch);
        let aa_color = blend_rgba(fg, bg, 95);

        for gy in 0..7isize {
            for gx in 0..5isize {
                let px = x + (gx as i32) * 2;
                let py = y + (gy as i32) * 2;

                if glyph_bit(bitmap, gx, gy) {
                    self.put_pixel(px, py, fg);
                    self.put_pixel(px + 1, py, fg);
                    self.put_pixel(px, py + 1, fg);
                    self.put_pixel(px + 1, py + 1, fg);
                } else {
                    let top = glyph_bit(bitmap, gx, gy - 1);
                    let bottom = glyph_bit(bitmap, gx, gy + 1);
                    let left = glyph_bit(bitmap, gx - 1, gy);
                    let right = glyph_bit(bitmap, gx + 1, gy);

                    if top && left {
                        self.put_pixel(px, py, aa_color);
                    }
                    if top && right {
                        self.put_pixel(px + 1, py, aa_color);
                    }
                    if bottom && left {
                        self.put_pixel(px, py + 1, aa_color);
                    }
                    if bottom && right {
                        self.put_pixel(px + 1, py + 1, aa_color);
                    }
                }
            }
        }
    }

    pub fn draw_text(&mut self, mut x: i32, y: i32, text: &str, fg: u32, bg: u32) {
        for ch in text.chars() {
            self.draw_char(x, y, ch, fg, bg);
            x += CHAR_W as i32;
        }
    }

    pub fn draw_wallpaper(&mut self) {
        let bg = rgba(14, 19, 27);
        let dot = rgba(34, 45, 62);
        self.fill_rect(
            Rect::new(0, 0, FB_WIDTH as i32, FB_HEIGHT as i32 - TASKBAR_H),
            bg,
        );
        let mut y = 24;
        while y < FB_HEIGHT as i32 - TASKBAR_H {
            let mut x = 24;
            while x < FB_WIDTH as i32 {
                self.put_pixel(x, y, dot);
                self.put_pixel(x + 1, y, dot);
                x += 32;
            }
            y += 32;
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