use crate::allocator::heap_stats;
use crate::font::glyph_for;
use crate::hal::Hardware;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

pub const FB_WIDTH: usize = 640;
pub const FB_HEIGHT: usize = 360;

const CHAR_W: usize = 8;
const CHAR_H: usize = 14;
const COLS: usize = (FB_WIDTH - 16) / CHAR_W;   // 78 columns
const ROWS: usize = (FB_HEIGHT - 16) / CHAR_H;  // 24 rows

const fn rgba(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | (0xFF << 24)
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
            String::from("Target: wasm32-unknown-unknown | Allocator: BumpAllocator (4 MiB)"),
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
            // Enter
            13 => {
                self.println("");
                let cmd_line = self.input_buffer.trim().to_string();
                self.input_buffer.clear();
                self.execute_command(&cmd_line, hw);
                self.print("ferrios> ");
            }
            // Backspace
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
            // Printable ASCII
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
            // Scroll terminal rows upward by 1 line
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
        let base_x = 8 + col * CHAR_W;
        let base_y = 8 + row * CHAR_H;

        for gy in 0..7 {
            for gx in 0..5 {
                if (bitmap >> (gy * 5 + gx)) & 1 == 1 {
                    let px = base_x + gx;
                    let py = base_y + gy;
                    self.framebuffer[py * FB_WIDTH + px] = self.fg_color;
                }
            }
        }
    }

    fn draw_cursor(&mut self, visible: bool) {
        if self.cursor_x >= COLS || self.cursor_y >= ROWS {
            return;
        }
        let color = if visible { self.fg_color } else { self.bg_color };
        let base_x = 8 + self.cursor_x * CHAR_W;
        let base_y = 8 + self.cursor_y * CHAR_H;

        for py in base_y..(base_y + 8) {
            for px in base_x..(base_x + 5) {
                self.framebuffer[py * FB_WIDTH + px] = color;
            }
        }
    }

    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.framebuffer.as_ptr()
    }
}