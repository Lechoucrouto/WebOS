use crate::allocator::heap_stats;
use crate::font::glyph_for;
use crate::hal::Hardware;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

pub const FB_WIDTH: usize = 1280;
pub const FB_HEIGHT: usize = 720;

const CHAR_W: usize = 14;
const CHAR_H: usize = 24;
const COLS: usize = (FB_WIDTH - 32) / CHAR_W;   // 89 columns
const ROWS: usize = (FB_HEIGHT - 32) / CHAR_H;  // 28 rows

const fn rgba(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | (0xFF << 24)
}

fn blend_rgba(fg: u32, bg: u32, alpha: u8) -> u32 {
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

fn glyph_bit(bitmap: u64, gx: isize, gy: isize) -> bool {
    if !(0..5).contains(&gx) || !(0..7).contains(&gy) {
        return false;
    }
    let bit_index = (gy as usize) * 5 + (4 - gx as usize);
    ((bitmap >> bit_index) & 1) == 1
}

pub struct Kernel {
    framebuffer: Vec<u32>,
    text_grid: Vec<char>,
    cursor_x: usize,
    cursor_y: usize,
    fg_color: u32,
    bg_color: u32,
    input_buffer: String,
    vfs: BTreeMap<String, String>,
    boot_time_ms: u64,
    dirty: bool,
}

impl Kernel {
    pub fn boot(hw: &dyn Hardware) -> Self {
        let mut vfs = BTreeMap::new();
        vfs.insert(
            String::from("readme.txt"),
            String::from("Welcome to FerriOS! This #![no_std] Rust kernel runs inside WebAssembly."),
        );
        vfs.insert(
            String::from("arch.info"),
            String::from("Target: wasm32-unknown-unknown | Allocator: BumpAllocator (16 MiB)"),
        );

        let mut kernel = Self {
            framebuffer: vec![rgba(13, 17, 23); FB_WIDTH * FB_HEIGHT],
            text_grid: vec![' '; COLS * ROWS],
            cursor_x: 0,
            cursor_y: 0,
            fg_color: rgba(88, 242, 132), // Phosphor green
            bg_color: rgba(13, 17, 23),   // Deep slate
            input_buffer: String::new(),
            vfs,
            boot_time_ms: hw.uptime_ms(),
            dirty: true,
        };

        hw.serial_log("[FerriOS] Kernel booted successfully.");
        kernel.println("FerriOS v0.1.0 (#![no_std] Rust Kernel on WebAssembly)");
        kernel.println("Type 'help' to see available kernel commands.");
        kernel.println("");
        kernel.print("ferrios> ");
        kernel
    }

    pub fn on_key(&mut self, code: u32, hw: &dyn Hardware) {
        match code {
            13 => {
                self.println("");
                let cmd_line = self.input_buffer.trim().to_string();
                self.input_buffer.clear();
                self.execute_command(&cmd_line, hw);
                self.print("ferrios> ");
            }
            8 => {
                if !self.input_buffer.is_empty() {
                    self.input_buffer.pop();
                    if self.cursor_x > 0 {
                        self.cursor_x -= 1;
                        self.text_grid[self.cursor_y * COLS + self.cursor_x] = ' ';
                        self.dirty = true;
                    }
                }
            }
            32..=126 => {
                if let Some(ch) = char::from_u32(code) {
                    if self.input_buffer.len() < COLS - 12 {
                        self.input_buffer.push(ch);
                        self.put_char(ch);
                    }
                }
            }
            _ => {}
        }
    }

    fn execute_command(&mut self, line: &str, hw: &dyn Hardware) {
        if line.is_empty() {
            return;
        }

        let mut parts = line.splitn(3, ' ');
        let cmd = parts.next().unwrap_or("");
        let arg1 = parts.next().unwrap_or("");
        let arg2 = parts.next().unwrap_or("");

        match cmd {
            "help" => {
                self.println("Commands: help, uname, uptime, mem, ls, cat <file>,");
                self.println("          write <file> <text>, rm <file>, theme <green|amber|cyan>, clear");
            }
            "uname" => {
                self.println("FerriOS 0.1.0 wasm32-unknown-unknown (#![no_std] + alloc)");
            }
            "uptime" => {
                let elapsed = hw.uptime_ms().saturating_sub(self.boot_time_ms);
                self.println(&format!("Uptime: {}.{:03}s", elapsed / 1000, elapsed % 1000));
            }
            "mem" => {
                let (used, total) = heap_stats();
                self.println(&format!(
                    "Kernel Heap: {} / {} bytes used ({:.1}%)",
                    used,
                    total,
                    (used as f64 / total as f64) * 100.0
                ));
            }
            "ls" => {
                let entries: Vec<String> = self
                    .vfs
                    .iter()
                    .map(|(name, content)| format!("  {:<16} ({} bytes)", name, content.len()))
                    .collect();
                for entry in entries {
                    self.println(&entry);
                }
            }
            "cat" => {
                if arg1.is_empty() {
                    self.println("Usage: cat <filename>");
                } else if let Some(content) = self.vfs.get(arg1) {
                    let out = content.clone();
                    self.println(&out);
                } else {
                    self.println(&format!("cat: {}: No such file", arg1));
                }
            }
            "write" => {
                if arg1.is_empty() || arg2.is_empty() {
                    self.println("Usage: write <filename> <text>");
                } else {
                    self.vfs.insert(arg1.to_string(), arg2.to_string());
                    self.println(&format!("Wrote {} bytes to '{}'", arg2.len(), arg1));
                }
            }
            "rm" => {
                if self.vfs.remove(arg1).is_some() {
                    self.println(&format!("Removed '{}'", arg1));
                } else {
                    self.println(&format!("rm: {}: No such file", arg1));
                }
            }
            "theme" => match arg1 {
                "green" => {
                    self.fg_color = rgba(88, 242, 132);
                    self.bg_color = rgba(13, 17, 23);
                    self.dirty = true;
                }
                "amber" => {
                    self.fg_color = rgba(255, 184, 77);
                    self.bg_color = rgba(24, 16, 8);
                    self.dirty = true;
                }
                "cyan" => {
                    self.fg_color = rgba(86, 212, 255);
                    self.bg_color = rgba(10, 18, 28);
                    self.dirty = true;
                }
                _ => self.println("Usage: theme <green|amber|cyan>"),
            },
            "clear" => {
                self.text_grid.fill(' ');
                self.cursor_x = 0;
                self.cursor_y = 0;
                self.dirty = true;
            }
            _ => {
                self.println(&format!("Unknown command: '{}'. Type 'help'.", cmd));
            }
        }
    }

    fn print(&mut self, s: &str) {
        for ch in s.chars() {
            self.put_char(ch);
        }
    }

    fn println(&mut self, s: &str) {
        self.print(s);
        self.new_line();
    }

    fn put_char(&mut self, ch: char) {
        if ch == '\n' {
            self.new_line();
            return;
        }
        if self.cursor_x >= COLS {
            self.new_line();
        }
        self.text_grid[self.cursor_y * COLS + self.cursor_x] = ch;
        self.cursor_x += 1;
        self.dirty = true;
    }

    fn new_line(&mut self) {
        self.cursor_x = 0;
        if self.cursor_y + 1 < ROWS {
            self.cursor_y += 1;
        } else {
            self.text_grid.copy_within(COLS.., 0);
            let last_row_start = (ROWS - 1) * COLS;
            self.text_grid[last_row_start..].fill(' ');
        }
        self.dirty = true;
    }

    pub fn render(&mut self, hw: &dyn Hardware) {
        let blink_on = (hw.uptime_ms() / 500) % 2 == 0;
        if !self.dirty {
            self.draw_cursor(blink_on);
            return;
        }

        self.framebuffer.fill(self.bg_color);

        for row in 0..ROWS {
            for col in 0..COLS {
                let ch = self.text_grid[row * COLS + col];
                if ch != ' ' {
                    self.draw_glyph(col, row, ch);
                }
            }
        }

        self.draw_cursor(blink_on);
        self.dirty = false;
    }

    fn draw_glyph(&mut self, col: usize, row: usize, ch: char) {
        let bitmap = glyph_for(ch);
        let base_x = 16 + col * CHAR_W;
        let base_y = 16 + row * CHAR_H;
        let aa_color = blend_rgba(self.fg_color, self.bg_color, 95);

        for gy in 0..7isize {
            for gx in 0..5isize {
                let px = base_x + (gx as usize) * 2;
                let py = base_y + (gy as usize) * 2;

                if glyph_bit(bitmap, gx, gy) {
                    self.framebuffer[py * FB_WIDTH + px] = self.fg_color;
                    self.framebuffer[py * FB_WIDTH + (px + 1)] = self.fg_color;
                    self.framebuffer[(py + 1) * FB_WIDTH + px] = self.fg_color;
                    self.framebuffer[(py + 1) * FB_WIDTH + (px + 1)] = self.fg_color;
                } else {
                    // Anti-alias diagonal corners with 37% alpha blend
                    let top = glyph_bit(bitmap, gx, gy - 1);
                    let bottom = glyph_bit(bitmap, gx, gy + 1);
                    let left = glyph_bit(bitmap, gx - 1, gy);
                    let right = glyph_bit(bitmap, gx + 1, gy);

                    if top && left {
                        self.framebuffer[py * FB_WIDTH + px] = aa_color;
                    }
                    if top && right {
                        self.framebuffer[py * FB_WIDTH + (px + 1)] = aa_color;
                    }
                    if bottom && left {
                        self.framebuffer[(py + 1) * FB_WIDTH + px] = aa_color;
                    }
                    if bottom && right {
                        self.framebuffer[(py + 1) * FB_WIDTH + (px + 1)] = aa_color;
                    }
                }
            }
        }
    }

    fn draw_cursor(&mut self, visible: bool) {
        if self.cursor_x >= COLS || self.cursor_y >= ROWS {
            return;
        }
        let color = if visible { self.fg_color } else { self.bg_color };
        let base_x = 16 + self.cursor_x * CHAR_W;
        let base_y = 16 + self.cursor_y * CHAR_H;

        for py in base_y..(base_y + 15) {
            for px in base_x..(base_x + 10) {
                self.framebuffer[py * FB_WIDTH + px] = color;
            }
        }
    }

    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.framebuffer.as_ptr()
    }
}