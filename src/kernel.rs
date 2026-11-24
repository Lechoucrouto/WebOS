use crate::allocator::heap_stats;
use crate::gui::{
    rgba, AppKind, Canvas, Rect, Window, CHAR_H, CHAR_W, FB_HEIGHT, FB_WIDTH, TASKBAR_H,
    TITLEBAR_H,
};
use crate::hal::Hardware;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

const TERM_COLS: usize = 58;
const TERM_ROWS: usize = 19;

pub struct Kernel {
    framebuffer: Vec<u32>,
    windows: Vec<Window>,
    dragging_win_id: Option<usize>,
    drag_offset_x: i32,
    drag_offset_y: i32,
    mouse_x: i32,
    mouse_y: i32,

    // Terminal state
    term_grid: Vec<char>,
    cursor_x: usize,
    cursor_y: usize,
    term_fg: u32,
    term_bg: u32,
    input_buffer: String,

    // Virtual filesystem & File Explorer state
    vfs: BTreeMap<String, String>,
    selected_file: Option<String>,

    boot_time_ms: u64,
}

impl Kernel {
    pub fn boot(hw: &dyn Hardware) -> Self {
        let mut vfs = BTreeMap::new();
        vfs.insert(
            String::from("readme.txt"),
            String::from("Welcome to WebOS Desktop! Drag titlebars, click taskbar buttons, or use the terminal."),
        );
        vfs.insert(
            String::from("arch.info"),
            String::from("Kernel: #![no_std] Rust | Target: wasm32-unknown-unknown | GUI: 1280x720 RGBA"),
        );
        vfs.insert(
            String::from("notes.md"),
            String::from("Try running: write todo.txt Hello GUI! -- then click Files on the taskbar."),
        );

        let windows = vec![
            Window {
                id: 1,
                kind: AppKind::Terminal,
                title: String::from("Terminal - ferrios"),
                x: 44,
                y: 44,
                w: 720,
                h: 440,
                open: true,
            },
            Window {
                id: 2,
                kind: AppKind::Files,
                title: String::from("File Explorer (/vfs)"),
                x: 790,
                y: 64,
                w: 446,
                h: 320,
                open: true,
            },
            Window {
                id: 3,
                kind: AppKind::SysMon,
                title: String::from("System Monitor"),
                x: 790,
                y: 408,
                w: 446,
                h: 240,
                open: true,
            },
        ];

        let mut kernel = Self {
            framebuffer: vec![rgba(14, 19, 27); FB_WIDTH * FB_HEIGHT],
            windows,
            dragging_win_id: None,
            drag_offset_x: 0,
            drag_offset_y: 0,
            mouse_x: (FB_WIDTH / 2) as i32,
            mouse_y: (FB_HEIGHT / 2) as i32,
            term_grid: vec![' '; TERM_COLS * TERM_ROWS],
            cursor_x: 0,
            cursor_y: 0,
            term_fg: rgba(88, 242, 132),
            term_bg: rgba(13, 17, 23),
            input_buffer: String::new(),
            vfs,
            selected_file: Some(String::from("readme.txt")),
            boot_time_ms: hw.uptime_ms(),
        };

        // Bring Terminal to front initially
        kernel.focus_window(1);
        hw.serial_log("[WebOS] Graphical desktop booted.");
        kernel.println("WebOS Desktop v0.2.0 (#![no_std] Rust)");
        kernel.println("Type 'help' for commands or click windows.");
        kernel.println("");
        kernel.print("ferrios> ");
        kernel
    }

    fn focus_window(&mut self, id: usize) {
        if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
            let mut win = self.windows.remove(pos);
            win.open = true;
            self.windows.push(win);
        }
    }

    fn toggle_or_open_app(&mut self, kind: AppKind) {
        if let Some(pos) = self.windows.iter().position(|w| w.kind == kind) {
            let is_top_and_open = self.windows[pos].open && pos == self.windows.len() - 1;
            if is_top_and_open {
                self.windows[pos].open = false;
            } else {
                let id = self.windows[pos].id;
                self.focus_window(id);
            }
        }
    }

    pub fn on_mouse_move(&mut self, x: i32, y: i32) {
        self.mouse_x = x.clamp(0, FB_WIDTH as i32 - 1);
        self.mouse_y = y.clamp(0, FB_HEIGHT as i32 - 1);

        if let Some(drag_id) = self.dragging_win_id {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == drag_id) {
                let max_x = FB_WIDTH as i32 - 80;
                let max_y = FB_HEIGHT as i32 - TASKBAR_H - TITLEBAR_H;
                win.x = (self.mouse_x - self.drag_offset_x).clamp(-win.w + 80, max_x);
                win.y = (self.mouse_y - self.drag_offset_y).clamp(0, max_y);
            }
        }
    }

    pub fn on_mouse_down(&mut self, x: i32, y: i32, button: u32) {
        self.on_mouse_move(x, y);
        if button != 0 {
            return;
        }

        // 1. Check bottom taskbar clicks
        if y >= FB_HEIGHT as i32 - TASKBAR_H {
            let btn_y = FB_HEIGHT as i32 - TASKBAR_H + 5;
            let btn_h = TASKBAR_H - 10;
            if y >= btn_y && y < btn_y + btn_h {
                if (128..252).contains(&x) {
                    self.toggle_or_open_app(AppKind::Terminal);
                } else if (262..374).contains(&x) {
                    self.toggle_or_open_app(AppKind::Files);
                } else if (384..508).contains(&x) {
                    self.toggle_or_open_app(AppKind::SysMon);
                }
            }
            return;
        }

        // 2. Hit-test open windows from top-most (end of Vec) to bottom
        let mut clicked_win_id = None;
        let mut close_clicked = false;
        let mut titlebar_clicked = false;
        let mut win_origin = (0, 0);
        let mut clicked_kind = AppKind::Terminal;
        let mut client_origin = (0, 0);

        for win in self.windows.iter().rev() {
            if !win.open {
                continue;
            }
            if win.frame_rect().contains(x, y) {
                clicked_win_id = Some(win.id);
                clicked_kind = win.kind;
                win_origin = (win.x, win.y);
                let client = win.client_rect();
                client_origin = (client.x, client.y);

                if win.close_btn_rect().contains(x, y) {
                    close_clicked = true;
                } else if win.titlebar_rect().contains(x, y) {
                    titlebar_clicked = true;
                }
                break;
            }
        }

        if let Some(id) = clicked_win_id {
            if close_clicked {
                if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
                    w.open = false;
                }
                return;
            }

            self.focus_window(id);

            if titlebar_clicked {
                self.dragging_win_id = Some(id);
                self.drag_offset_x = x - win_origin.0;
                self.drag_offset_y = y - win_origin.1;
            } else if clicked_kind == AppKind::Files {
                // Check if user clicked a file row inside File Explorer
                let rel_y = y - client_origin.1 - 32;
                if rel_y >= 0 {
                    let row_idx = (rel_y / 22) as usize;
                    let file_name = self.vfs.keys().nth(row_idx).cloned();
                    if file_name.is_some() {
                        self.selected_file = file_name;
                    }
                }
            }
        }
    }

    pub fn on_mouse_up(&mut self, _x: i32, _y: i32, button: u32) {
        if button == 0 {
            self.dragging_win_id = None;
        }
    }

    pub fn on_key(&mut self, code: u32, hw: &dyn Hardware) {
        // Route keyboard input to Terminal if it is the active top window
        let terminal_focused = self
            .windows
            .last()
            .map(|w| w.open && w.kind == AppKind::Terminal)
            .unwrap_or(false);

        if !terminal_focused {
            return;
        }

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
                        self.term_grid[self.cursor_y * TERM_COLS + self.cursor_x] = ' ';
                    }
                }
            }
            32..=126 => {
                if let Some(ch) = char::from_u32(code) {
                    if self.input_buffer.len() < TERM_COLS - 10 {
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
                self.println("Commands: help, ls, cat <f>, write <f> <txt>, rm <f>,");
                self.println("          open <files|sysmon>, theme <green|amber|cyan>,");
                self.println("          mem, uptime, clear");
            }
            "open" => match arg1 {
                "files" => {
                    if let Some(id) = self.windows.iter().find(|w| w.kind == AppKind::Files).map(|w| w.id) {
                        self.focus_window(id);
                    }
                }
                "sysmon" => {
                    if let Some(id) = self.windows.iter().find(|w| w.kind == AppKind::SysMon).map(|w| w.id) {
                        self.focus_window(id);
                    }
                }
                _ => self.println("Usage: open <files|sysmon>"),
            },
            "uptime" => {
                let elapsed = hw.uptime_ms().saturating_sub(self.boot_time_ms);
                self.println(&format!("Uptime: {}.{:03}s", elapsed / 1000, elapsed % 1000));
            }
            "mem" => {
                let (used, total) = heap_stats();
                self.println(&format!(
                    "Heap: {} / {} KiB ({:.1}%)",
                    used / 1024,
                    total / 1024,
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
                    self.selected_file = Some(arg1.to_string());
                    self.println(&format!("Wrote {} bytes to '{}'", arg2.len(), arg1));
                }
            }
            "rm" => {
                if self.vfs.remove(arg1).is_some() {
                    if self.selected_file.as_deref() == Some(arg1) {
                        self.selected_file = self.vfs.keys().next().cloned();
                    }
                    self.println(&format!("Removed '{}'", arg1));
                } else {
                    self.println(&format!("rm: {}: No such file", arg1));
                }
            }
            "theme" => match arg1 {
                "green" => {
                    self.term_fg = rgba(88, 242, 132);
                    self.term_bg = rgba(13, 17, 23);
                }
                "amber" => {
                    self.term_fg = rgba(255, 184, 77);
                    self.term_bg = rgba(24, 16, 8);
                }
                "cyan" => {
                    self.term_fg = rgba(86, 212, 255);
                    self.term_bg = rgba(10, 18, 28);
                }
                _ => self.println("Usage: theme <green|amber|cyan>"),
            },
            "clear" => {
                self.term_grid.fill(' ');
                self.cursor_x = 0;
                self.cursor_y = 0;
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
        if self.cursor_x >= TERM_COLS {
            self.new_line();
        }
        self.term_grid[self.cursor_y * TERM_COLS + self.cursor_x] = ch;
        self.cursor_x += 1;
    }

    fn new_line(&mut self) {
        self.cursor_x = 0;
        if self.cursor_y + 1 < TERM_ROWS {
            self.cursor_y += 1;
        } else {
            self.term_grid.copy_within(TERM_COLS.., 0);
            let last_row = (TERM_ROWS - 1) * TERM_COLS;
            self.term_grid[last_row..].fill(' ');
        }
    }

    pub fn render(&mut self, hw: &dyn Hardware) {
        let elapsed_ms = hw.uptime_ms().saturating_sub(self.boot_time_ms);
        let blink_on = (hw.uptime_ms() / 500) % 2 == 0;
        let top_open_id = self.windows.iter().rev().find(|w| w.open).map(|w| w.id);

        let mut canvas = Canvas::new(&mut self.framebuffer);

        // 1. Desktop Wallpaper
        canvas.draw_wallpaper();

        // 2. Render open windows from back to front
        for win in &self.windows {
            if !win.open {
                continue;
            }
            let is_focused = Some(win.id) == top_open_id;
            let frame = win.frame_rect();
            let titlebar = win.titlebar_rect();
            let close_btn = win.close_btn_rect();
            let client = win.client_rect();

            let border_col = if is_focused {
                rgba(88, 166, 255)
            } else {
                rgba(48, 54, 61)
            };
            let title_bg = if is_focused {
                rgba(28, 37, 52)
            } else {
                rgba(22, 27, 34)
            };

            // Window drop shadow & outer frame
            canvas.reset_clip();
            canvas.fill_rect(
                Rect::new(frame.x + 5, frame.y + 5, frame.w, frame.h),
                rgba(7, 10, 14),
            );
            canvas.fill_rect(frame, rgba(16, 21, 29));
            canvas.stroke_rect(frame, border_col);

            // Title bar & Close button
            canvas.set_clip(titlebar);
            canvas.fill_rect(titlebar, title_bg);
            canvas.draw_text(
                titlebar.x + 10,
                titlebar.y + 7,
                &win.title,
                rgba(230, 237, 243),
                title_bg,
            );
            canvas.fill_rect(close_btn, rgba(218, 54, 51));
            canvas.draw_text(
                close_btn.x + 5,
                close_btn.y + 3,
                "X",
                rgba(255, 255, 255),
                rgba(218, 54, 51),
            );

            // Window App Content
            canvas.set_clip(client);
            match win.kind {
                AppKind::Terminal => {
                    canvas.fill_rect(client, self.term_bg);
                    for r in 0..TERM_ROWS {
                        for c in 0..TERM_COLS {
                            let ch = self.term_grid[r * TERM_COLS + c];
                            if ch != ' ' {
                                canvas.draw_char(
                                    client.x + 8 + (c * CHAR_W) as i32,
                                    client.y + 8 + (r * CHAR_H) as i32,
                                    ch,
                                    self.term_fg,
                                    self.term_bg,
                                );
                            }
                        }
                    }
                    if is_focused && blink_on && self.cursor_x < TERM_COLS && self.cursor_y < TERM_ROWS {
                        canvas.fill_rect(
                            Rect::new(
                                client.x + 8 + (self.cursor_x * CHAR_W) as i32,
                                client.y + 8 + (self.cursor_y * CHAR_H) as i32,
                                9,
                                15,
                            ),
                            self.term_fg,
                        );
                    }
                }
                AppKind::Files => {
                    let panel_bg = rgba(16, 21, 29);
                    canvas.fill_rect(client, panel_bg);
                    canvas.draw_text(
                        client.x + 10,
                        client.y + 8,
                        "Click a file to preview:",
                        rgba(139, 148, 158),
                        panel_bg,
                    );

                    for (idx, (name, content)) in self.vfs.iter().enumerate() {
                        let row_y = client.y + 32 + (idx as i32) * 22;
                        let is_selected = self.selected_file.as_deref() == Some(name.as_str());
                        let row_bg = if is_selected {
                            rgba(31, 61, 97)
                        } else {
                            panel_bg
                        };
                        canvas.fill_rect(
                            Rect::new(client.x + 6, row_y - 2, client.w - 12, 20),
                            row_bg,
                        );
                        let label = format!("{:<18} {:>5} B", name, content.len());
                        canvas.draw_text(
                            client.x + 12,
                            row_y + 1,
                            &label,
                            rgba(230, 237, 243),
                            row_bg,
                        );
                    }

                    // Preview box at bottom of File Explorer
                    let preview_y = client.y + client.h - 110;
                    let preview_rect =
                        Rect::new(client.x + 8, preview_y, client.w - 16, 100);
                    canvas.fill_rect(preview_rect, rgba(11, 15, 21));
                    canvas.stroke_rect(preview_rect, rgba(48, 54, 61));

                    if let Some(sel) = &self.selected_file {
                        if let Some(body) = self.vfs.get(sel) {
                            // Wrap preview text across 3 lines (33 chars per line)
                            for (line_idx, chunk) in body.as_bytes().chunks(33).take(3).enumerate() {
                                if let Ok(s) = core::str::from_utf8(chunk) {
                                    canvas.draw_text(
                                        preview_rect.x + 8,
                                        preview_rect.y + 10 + (line_idx as i32) * 22,
                                        s,
                                        rgba(126, 231, 135),
                                        rgba(11, 15, 21),
                                    );
                                }
                            }
                        }
                    }
                }
                AppKind::SysMon => {
                    let panel_bg = rgba(16, 21, 29);
                    canvas.fill_rect(client, panel_bg);

                    let (used, total) = heap_stats();
                    let pct = ((used as f64 / total as f64) * 100.0).min(100.0);
                    let open_count = self.windows.iter().filter(|w| w.open).count();

                    canvas.draw_text(
                        client.x + 12,
                        client.y + 12,
                        &format!("Kernel Heap: {} / {} KiB ({:.1}%)", used / 1024, total / 1024, pct),
                        rgba(230, 237, 243),
                        panel_bg,
                    );

                    // Heap Progress Bar
                    let bar_rect = Rect::new(client.x + 12, client.y + 38, client.w - 24, 22);
                    canvas.fill_rect(bar_rect, rgba(11, 15, 21));
                    canvas.stroke_rect(bar_rect, rgba(48, 54, 61));
                    let fill_w = (((bar_rect.w - 4) as f64) * (pct / 100.0)) as i32;
                    canvas.fill_rect(
                        Rect::new(bar_rect.x + 2, bar_rect.y + 2, fill_w.max(4), bar_rect.h - 4),
                        rgba(88, 242, 132),
                    );

                    canvas.draw_text(
                        client.x + 12,
                        client.y + 78,
                        &format!("Active Windows: {} / {}", open_count, self.windows.len()),
                        rgba(139, 148, 158),
                        panel_bg,
                    );
                    canvas.draw_text(
                        client.x + 12,
                        client.y + 104,
                        "Display: 1280x720 RGBA @ 60Hz",
                        rgba(139, 148, 158),
                        panel_bg,
                    );
                    canvas.draw_text(
                        client.x + 12,
                        client.y + 130,
                        "Arch: wasm32-unknown-unknown",
                        rgba(86, 212, 255),
                        panel_bg,
                    );
                }
            }
        }

        // 3. Bottom Taskbar
        canvas.reset_clip();
        let tb_y = FB_HEIGHT as i32 - TASKBAR_H;
        let tb_bg = rgba(22, 27, 34);
        canvas.fill_rect(Rect::new(0, tb_y, FB_WIDTH as i32, TASKBAR_H), tb_bg);
        canvas.fill_rect(Rect::new(0, tb_y, FB_WIDTH as i32, 1), rgba(48, 54, 61));

        // OS Badge
        canvas.fill_rect(Rect::new(8, tb_y + 5, 108, TASKBAR_H - 10), rgba(35, 134, 54));
        canvas.draw_text(
            18,
            tb_y + 11,
            "FerriOS",
            rgba(255, 255, 255),
            rgba(35, 134, 54),
        );

        // Taskbar App Buttons
        let buttons = [
            (128, 124, "Terminal", AppKind::Terminal),
            (262, 112, "Files", AppKind::Files),
            (384, 124, "SysMon", AppKind::SysMon),
        ];

        for (bx, bw, label, kind) in buttons {
            let is_open = self.windows.iter().any(|w| w.kind == kind && w.open);
            let btn_bg = if is_open {
                rgba(33, 44, 61)
            } else {
                rgba(16, 21, 29)
            };
            let btn_rect = Rect::new(bx, tb_y + 5, bw, TASKBAR_H - 10);
            canvas.fill_rect(btn_rect, btn_bg);
            canvas.stroke_rect(
                btn_rect,
                if is_open {
                    rgba(88, 166, 255)
                } else {
                    rgba(48, 54, 61)
                },
            );
            canvas.draw_text(bx + 12, tb_y + 11, label, rgba(230, 237, 243), btn_bg);
        }

        // Live Clock on Right
        let total_secs = elapsed_ms / 1000;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        let clock_str = format!("UP {:02}:{:02}", mins, secs);
        canvas.draw_text(
            FB_WIDTH as i32 - 122,
            tb_y + 11,
            &clock_str,
            rgba(139, 148, 158),
            tb_bg,
        );

        // 4. Mouse Cursor (drawn on top of everything)
        canvas.draw_cursor(self.mouse_x, self.mouse_y);
    }

    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.framebuffer.as_ptr()
    }
}