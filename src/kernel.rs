use crate::allocator::heap_stats;
use crate::exec::{
    expand_wbsh_line, format_wbo_inspection, install_system_binaries, WboExecutable, WboSubsystem,
};
use crate::fs::FileSystem;
use crate::gui::{
    rgba, Canvas, Rect, Window, CHAR_H, CHAR_W, FB_HEIGHT, FB_WIDTH, TASKBAR_H, TITLEBAR_H,
};
use crate::hal::Hardware;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

const TERM_COLS: usize = 58;
const TERM_ROWS: usize = 19;

pub struct Kernel {
    framebuffer: Vec<u32>,
    windows: Vec<Window>,
    next_win_id: usize,
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

    // Hierarchical C:/ Filesystem & Explorer state
    fs: FileSystem,
    explorer_dir: String,
    selected_path: Option<String>,

    boot_time_ms: u64,
}

impl Kernel {
    pub fn boot(hw: &dyn Hardware) -> Self {
        let mut fs = FileSystem::new();
        install_system_binaries(&mut fs);

        let mut kernel = Self {
            framebuffer: vec![rgba(14, 19, 27); FB_WIDTH * FB_HEIGHT],
            windows: Vec::new(),
            next_win_id: 1,
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
            fs,
            explorer_dir: String::from("C:/usr"),
            selected_path: Some(String::from("C:/usr/readme.txt")),
            boot_time_ms: hw.uptime_ms(),
        };

        // Boot startup executables from C:/WebOS/bin/Apps/
        let _ = kernel.exec_wbo("C:/WebOS/bin/Apps/Files.wbo", Some((790, 56)));
        let _ = kernel.exec_wbo("C:/WebOS/bin/Apps/SysMon.wbo", Some((790, 410)));
        let _ = kernel.exec_wbo("C:/WebOS/bin/Apps/Terminal.wbo", Some((44, 44)));

        hw.serial_log("[WebOS] Booted C:/ filesystem, .wbo loader, and .wbsh shell.");
        kernel.println("WebOS v0.3.0 (#![no_std] Rust)");
        kernel.println("Apps: C:/WebOS/bin/Apps/*.wbo | PATH: C:/WebOS/bin/PATH/*.wbsh");
        kernel.println("Type 'help' for commands.");
        kernel.println("");
        kernel.print_prompt();
        kernel
    }

    fn print_prompt(&mut self) {
        let prompt = format!("{}> ", self.fs.cwd);
        self.print(&prompt);
    }

    fn focus_window(&mut self, id: usize) {
        if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
            let mut win = self.windows.remove(pos);
            win.open = true;
            self.windows.push(win);
        }
    }

    pub fn exec_wbo(&mut self, path: &str, pos_override: Option<(i32, i32)>) -> Result<(), String> {
        let resolved = self.fs.resolve_path(path);
        let bytes = self
            .fs
            .read_bytes(&resolved)
            .ok_or_else(|| format!("Executable not found: {}", resolved))?
            .to_vec();

        let exe = WboExecutable::parse(&bytes).map_err(|e| format!("{}: {}", resolved, e))?;

        // If an instance of this executable is already loaded, focus and re-open it
        if let Some(existing_id) = self
            .windows
            .iter()
            .find(|w| w.exe_path == resolved)
            .map(|w| w.id)
        {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == existing_id) {
                w.title = exe.title;
                w.custom_lines = exe.body_lines;
            }
            self.focus_window(existing_id);
            return Ok(());
        }

        let id = self.next_win_id;
        self.next_win_id += 1;
        let offset = ((id as i32) * 28) % 160;
        let (x, y) = pos_override.unwrap_or((140 + offset, 80 + offset));

        self.windows.push(Window {
            id,
            exe_path: resolved,
            subsystem: exe.subsystem,
            title: exe.title,
            x,
            y,
            w: exe.width,
            h: exe.height,
            open: true,
            pinned: exe.pinned,
            custom_lines: exe.body_lines,
        });

        Ok(())
    }

    fn toggle_taskbar_window(&mut self, id: usize) {
        if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
            let is_top_open = self.windows[pos].open && pos == self.windows.len() - 1;
            if is_top_open {
                self.windows[pos].open = false;
            } else {
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

        // 1. Check bottom taskbar buttons (sorted by window id for stable positions)
        if y >= FB_HEIGHT as i32 - TASKBAR_H {
            let btn_y = FB_HEIGHT as i32 - TASKBAR_H + 5;
            let btn_h = TASKBAR_H - 10;
            if y >= btn_y && y < btn_y + btn_h {
                let mut taskbar_wins: Vec<(usize, bool)> = self
                    .windows
                    .iter()
                    .filter(|w| w.pinned || w.open)
                    .map(|w| (w.id, true))
                    .collect();
                taskbar_wins.sort_by_key(|(id, _)| *id);

                let mut bx = 128;
                for (win_id, _) in taskbar_wins {
                    if x >= bx && x < bx + 132 {
                        self.toggle_taskbar_window(win_id);
                        return;
                    }
                    bx += 140;
                }
            }
            return;
        }

        // 2. Hit-test open windows from top to bottom
        let mut hit: Option<(usize, WboSubsystem, (i32, i32), Rect, bool, bool)> = None;
        for win in self.windows.iter().rev() {
            if !win.open {
                continue;
            }
            if win.frame_rect().contains(x, y) {
                hit = Some((
                    win.id,
                    win.subsystem,
                    (win.x, win.y),
                    win.client_rect(),
                    win.close_btn_rect().contains(x, y),
                    win.titlebar_rect().contains(x, y),
                ));
                break;
            }
        }

        if let Some((id, subsystem, win_origin, client, close_clicked, titlebar_clicked)) = hit {
            if close_clicked {
                if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
                    if self.windows[pos].pinned {
                        self.windows[pos].open = false;
                    } else {
                        self.windows.remove(pos);
                    }
                }
                return;
            }

            self.focus_window(id);

            if titlebar_clicked {
                self.dragging_win_id = Some(id);
                self.drag_offset_x = x - win_origin.0;
                self.drag_offset_y = y - win_origin.1;
            } else if subsystem == WboSubsystem::Files {
                self.handle_files_click(x, y, client);
            }
        }
    }

    fn handle_files_click(&mut self, x: i32, y: i32, client: Rect) {
        // [Up] button at top-right of File Explorer
        let up_btn = Rect::new(client.x + client.w - 58, client.y + 5, 50, 20);
        if up_btn.contains(x, y) {
            self.explorer_dir = FileSystem::parent_of(&self.explorer_dir);
            self.selected_path = None;
            return;
        }

        // [Run] button in preview panel
        let preview_y = client.y + client.h - 106;
        let run_btn = Rect::new(client.x + client.w - 72, preview_y + 6, 56, 20);
        if run_btn.contains(x, y) {
            if let Some(sel) = self.selected_path.clone() {
                if sel.ends_with(".wbo") {
                    if let Err(err) = self.exec_wbo(&sel, None) {
                        self.println(&format!("\nExec error: {}", err));
                        self.print_prompt();
                    }
                } else if sel.ends_with(".wbsh") {
                    self.println("");
                    self.run_wbsh_file(&sel, &[]);
                    self.print_prompt();
                    if let Some(tid) = self
                        .windows
                        .iter()
                        .find(|w| w.subsystem == WboSubsystem::Terminal)
                        .map(|w| w.id)
                    {
                        self.focus_window(tid);
                    }
                }
            }
            return;
        }

        // Directory / file rows
        let rel_y = y - (client.y + 32);
        if rel_y >= 0 && y < preview_y - 4 {
            let row_idx = (rel_y / 22) as usize;
            if let Ok(entries) = self.fs.list_dir(&self.explorer_dir) {
                if let Some((name, is_dir, _)) = entries.get(row_idx) {
                    let full_path = if self.explorer_dir == "C:/" {
                        format!("C:/{}", name)
                    } else {
                        format!("{}/{}", self.explorer_dir, name)
                    };

                    if *is_dir {
                        self.explorer_dir = full_path;
                        self.selected_path = None;
                    } else {
                        self.selected_path = Some(full_path);
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
        let terminal_focused = self
            .windows
            .last()
            .map(|w| w.open && w.subsystem == WboSubsystem::Terminal)
            .unwrap_or(false);

        if !terminal_focused {
            return;
        }

        match code {
            13 => {
                self.println("");
                let cmd_line = self.input_buffer.trim().to_string();
                self.input_buffer.clear();
                self.execute_shell_line(&cmd_line, hw);
                self.print_prompt();
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
                    if self.input_buffer.len() < TERM_COLS - 14 {
                        self.input_buffer.push(ch);
                        self.put_char(ch);
                    }
                }
            }
            _ => {}
        }
    }

    fn execute_shell_line(&mut self, line: &str, hw: &dyn Hardware) {
        if line.is_empty() {
            return;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        let cmd = tokens[0];
        let args = &tokens[1..];

        // 1. Core Shell Intrinsics (must run inside the shell process itself)
        match cmd {
            "pwd" => {
                let cwd = self.fs.cwd.clone();
                self.println(&cwd);
                return;
            }
            "cd" => {
                let target = args.first().copied().unwrap_or("C:/usr");
                let resolved = self.fs.resolve_path(target);
                if self.fs.is_dir(&resolved) {
                    self.fs.cwd = resolved;
                } else {
                    self.println(&format!("cd: {}: No such directory", target));
                }
                return;
            }
            "clear" => {
                self.term_grid.fill(' ');
                self.cursor_x = 0;
                self.cursor_y = 0;
                return;
            }
            "run" => {
                if let Some(target) = args.first() {
                    let resolved = self.fs.resolve_path(target);
                    if resolved.ends_with(".wbo") {
                        if let Err(e) = self.exec_wbo(&resolved, None) {
                            self.println(&e);
                        }
                    } else if resolved.ends_with(".wbsh") {
                        self.run_wbsh_file(&resolved, &args[1..]);
                    } else {
                        self.println("run: file must end in .wbo or .wbsh");
                    }
                } else {
                    self.println("Usage: run <file.wbo | file.wbsh>");
                }
                return;
            }
            _ => {}
        }

        // 2. Direct path execution (e.g., `./hello.wbsh` or `C:/WebOS/bin/Apps/SysMon.wbo`)
        if cmd.contains('/') || cmd.contains('\\') || cmd.ends_with(".wbo") || cmd.ends_with(".wbsh") {
            let resolved = self.fs.resolve_path(cmd);
            if resolved.ends_with(".wbo") {
                if let Err(e) = self.exec_wbo(&resolved, None) {
                    self.println(&e);
                }
                return;
            } else if resolved.ends_with(".wbsh") {
                self.run_wbsh_file(&resolved, args);
                return;
            }
        }

        // 3. Search C:/WebOS/bin/PATH/<cmd>.wbsh
        let path_script = format!("C:/WebOS/bin/PATH/{}.wbsh", cmd);
        if self.fs.read_str(&path_script).is_some() {
            self.run_wbsh_file(&path_script, args);
            return;
        }

        // 4. Search C:/WebOS/bin/Apps/<cmd>.wbo
        let app_bin = format!("C:/WebOS/bin/Apps/{}.wbo", cmd);
        if self.fs.read_bytes(&app_bin).is_some() {
            if let Err(e) = self.exec_wbo(&app_bin, None) {
                self.println(&e);
            }
            return;
        }

        let _ = hw;
        self.println(&format!(
            "wbsh: '{}': not found in C:/WebOS/bin/PATH or Apps",
            cmd
        ));
    }

    fn run_wbsh_file(&mut self, script_path: &str, args: &[&str]) {
        let script_source = match self.fs.read_str(script_path) {
            Some(s) => s.to_string(),
            None => {
                self.println(&format!("wbsh: cannot open '{}'", script_path));
                return;
            }
        };

        let lines: Vec<String> = script_source.lines().map(|l| l.to_string()).collect();
        for raw_line in lines {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let expanded = expand_wbsh_line(trimmed, args, &self.fs.cwd);
            self.execute_wbsh_statement(&expanded);
        }
    }

    fn execute_wbsh_statement(&mut self, stmt: &str) {
        if let Some(msg) = stmt.strip_prefix("echo ") {
            self.println(msg);
            return;
        }
        if stmt == "echo" {
            self.println("");
            return;
        }

        // Kernel Syscall Bridge invoked by .wbsh scripts
        let mut parts = stmt.splitn(4, ' ');
        let syscall = parts.next().unwrap_or("");
        let a1 = parts.next().unwrap_or("");
        let a2 = parts.next().unwrap_or("");
        let a3 = parts.next().unwrap_or("");

        match syscall {
            "@sys.ls" => {
                let target = if a1.is_empty() {
                    self.fs.cwd.clone()
                } else {
                    self.fs.resolve_path(a1)
                };
                match self.fs.list_dir(&target) {
                    Ok(entries) => {
                        if entries.is_empty() {
                            self.println("(empty directory)");
                        }
                        for (name, is_dir, size) in entries {
                            if is_dir {
                                self.println(&format!("  [DIR]  {}/", name));
                            } else {
                                self.println(&format!("  [FILE] {:<20} ({} B)", name, size));
                            }
                        }
                    }
                    Err(e) => self.println(&format!("ls: {}: {}", target, e)),
                }
            }
            "@sys.cat" => {
                if a1.is_empty() {
                    self.println("Usage: cat <file>");
                    return;
                }
                let resolved = self.fs.resolve_path(a1);
                if let Some(bytes) = self.fs.read_bytes(&resolved) {
                    if resolved.ends_with(".wbo") {
                        let info = format_wbo_inspection(bytes);
                        self.println(&info);
                    } else if let Ok(text) = core::str::from_utf8(bytes) {
                        let owned = text.to_string();
                        for l in owned.lines() {
                            self.println(l);
                        }
                    } else {
                        self.println(&format!("<binary file: {} bytes>", bytes.len()));
                    }
                } else {
                    self.println(&format!("cat: {}: No such file", a1));
                }
            }
            "@sys.write" => {
                let content = if a3.is_empty() {
                    a2.to_string()
                } else {
                    format!("{} {}", a2, a3)
                };
                if a1.is_empty() || content.is_empty() {
                    self.println("Usage: write <file> <text>");
                    return;
                }
                match self.fs.write_str(a1, &content) {
                    Ok(()) => self.println(&format!("Wrote {} bytes to {}", content.len(), a1)),
                    Err(e) => self.println(&format!("write: {}", e)),
                }
            }
            "@sys.mkdir" => {
                if a1.is_empty() {
                    self.println("Usage: mkdir <dir>");
                    return;
                }
                match self.fs.mkdir(a1) {
                    Ok(()) => self.println(&format!("Created directory {}", a1)),
                    Err(e) => self.println(&format!("mkdir: {}", e)),
                }
            }
            "@sys.rm" => {
                if a1.is_empty() {
                    self.println("Usage: rm <path>");
                    return;
                }
                match self.fs.remove(a1) {
                    Ok(()) => self.println(&format!("Removed {}", a1)),
                    Err(e) => self.println(&format!("rm: {}", e)),
                }
            }
            "@sys.mem" => {
                let (used, total) = heap_stats();
                self.println(&format!(
                    "Kernel Heap: {} / {} KiB ({:.1}%)",
                    used / 1024,
                    total / 1024,
                    (used as f64 / total as f64) * 100.0
                ));
            }
            "@sys.uptime" => {
                self.println(&format!("Boot Timestamp: {} ms", self.boot_time_ms));
            }
            "@sys.theme" => match a1 {
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
            "@sys.open" => {
                if a1.is_empty() {
                    self.println("Usage: open <AppName.wbo>");
                    return;
                }
                let candidate = if a1.contains('/') {
                    a1.to_string()
                } else if a1.ends_with(".wbo") {
                    format!("C:/WebOS/bin/Apps/{}", a1)
                } else {
                    format!("C:/WebOS/bin/Apps/{}.wbo", a1)
                };
                if let Err(e) = self.exec_wbo(&candidate, None) {
                    self.println(&e);
                }
            }
            "@sys.mkwbo" => {
                if a1.is_empty() || a2.is_empty() || a3.is_empty() {
                    self.println("Usage: mkwbo <path.wbo> <Title> <BodyText>");
                    return;
                }
                let bin = WboExecutable::pack(WboSubsystem::CustomApp, 420, 210, false, a2, a3);
                match self.fs.write_bytes(a1, bin) {
                    Ok(()) => self.println(&format!("Compiled .wbo executable: {}", a1)),
                    Err(e) => self.println(&format!("mkwbo: {}", e)),
                }
            }
            _ => self.println(&format!("wbsh: unknown directive '{}'", syscall)),
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

        // Prepare taskbar list sorted by window ID so button order stays fixed
        let mut taskbar_items: Vec<(usize, String, bool)> = self
            .windows
            .iter()
            .filter(|w| w.pinned || w.open)
            .map(|w| {
                let short_name = w
                    .exe_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&w.title)
                    .trim_end_matches(".wbo")
                    .to_string();
                (w.id, short_name, w.open)
            })
            .collect();
        taskbar_items.sort_by_key(|(id, _, _)| *id);

        let dir_entries = self.fs.list_dir(&self.explorer_dir).unwrap_or_default();
        let selected_preview: Option<(String, bool)> = self.selected_path.as_ref().and_then(|p| {
            let runnable = p.ends_with(".wbo") || p.ends_with(".wbsh");
            let text = if p.ends_with(".wbo") {
                self.fs.read_bytes(p).map(format_wbo_inspection)
            } else {
                self.fs.read_str(p).map(|s| s.to_string())
            };
            text.map(|t| (t, runnable))
        });

        let mut canvas = Canvas::new(&mut self.framebuffer);

        // 1. Desktop Wallpaper
        canvas.draw_wallpaper();

        // 2. Render open windows
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

            canvas.reset_clip();
            canvas.fill_rect(
                Rect::new(frame.x + 5, frame.y + 5, frame.w, frame.h),
                rgba(7, 10, 14),
            );
            canvas.fill_rect(frame, rgba(16, 21, 29));
            canvas.stroke_rect(frame, border_col);

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

            canvas.set_clip(client);
            match win.subsystem {
                WboSubsystem::Terminal => {
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
                WboSubsystem::Files => {
                    let panel_bg = rgba(16, 21, 29);
                    canvas.fill_rect(client, panel_bg);

                    // Current directory bar & [Up] button
                    canvas.draw_text(
                        client.x + 10,
                        client.y + 8,
                        &self.explorer_dir,
                        rgba(86, 212, 255),
                        panel_bg,
                    );
                    let up_btn = Rect::new(client.x + client.w - 58, client.y + 5, 50, 20);
                    canvas.fill_rect(up_btn, rgba(33, 44, 61));
                    canvas.stroke_rect(up_btn, rgba(88, 166, 255));
                    canvas.draw_text(
                        up_btn.x + 13,
                        up_btn.y + 3,
                        "Up",
                        rgba(230, 237, 243),
                        rgba(33, 44, 61),
                    );

                    // Folder & file entries
                    for (idx, (name, is_dir, size)) in dir_entries.iter().enumerate() {
                        let row_y = client.y + 32 + (idx as i32) * 22;
                        let full = if self.explorer_dir == "C:/" {
                            format!("C:/{}", name)
                        } else {
                            format!("{}/{}", self.explorer_dir, name)
                        };
                        let is_selected = self.selected_path.as_deref() == Some(full.as_str());
                        let row_bg = if is_selected {
                            rgba(31, 61, 97)
                        } else {
                            panel_bg
                        };

                        canvas.fill_rect(
                            Rect::new(client.x + 6, row_y - 2, client.w - 12, 20),
                            row_bg,
                        );
                        let label = if *is_dir {
                            format!("[DIR]  {}/", name)
                        } else {
                            format!("       {:<18} {:>4} B", name, size)
                        };
                        let fg = if *is_dir {
                            rgba(255, 184, 77)
                        } else if name.ends_with(".wbo") || name.ends_with(".wbsh") {
                            rgba(126, 231, 135)
                        } else {
                            rgba(230, 237, 243)
                        };
                        canvas.draw_text(client.x + 10, row_y + 1, &label, fg, row_bg);
                    }

                    // Preview box & [Run] button
                    let preview_y = client.y + client.h - 106;
                    let preview_rect =
                        Rect::new(client.x + 8, preview_y, client.w - 16, 98);
                    canvas.fill_rect(preview_rect, rgba(11, 15, 21));
                    canvas.stroke_rect(preview_rect, rgba(48, 54, 61));

                    if let Some((body, runnable)) = &selected_preview {
                        if *runnable {
                            let run_btn = Rect::new(client.x + client.w - 72, preview_y + 6, 56, 20);
                            canvas.fill_rect(run_btn, rgba(35, 134, 54));
                            canvas.draw_text(
                                run_btn.x + 10,
                                run_btn.y + 3,
                                "Run",
                                rgba(255, 255, 255),
                                rgba(35, 134, 54),
                            );
                        }

                        for (line_idx, chunk) in body.as_bytes().chunks(28).take(3).enumerate() {
                            if let Ok(s) = core::str::from_utf8(chunk) {
                                canvas.draw_text(
                                    preview_rect.x + 8,
                                    preview_rect.y + 10 + (line_idx as i32) * 22,
                                    s,
                                    rgba(201, 209, 217),
                                    rgba(11, 15, 21),
                                );
                            }
                        }
                    }
                }
                WboSubsystem::SysMon => {
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
                        &format!("Running .wbo Processes: {}", open_count),
                        rgba(139, 148, 158),
                        panel_bg,
                    );
                    canvas.draw_text(
                        client.x + 12,
                        client.y + 104,
                        "Exe Format: WBO\\x01 | Shell: .wbsh",
                        rgba(86, 212, 255),
                        panel_bg,
                    );
                }
                WboSubsystem::CustomApp => {
                    let panel_bg = rgba(16, 21, 29);
                    canvas.fill_rect(client, panel_bg);
                    for (idx, line) in win.custom_lines.iter().enumerate() {
                        canvas.draw_text(
                            client.x + 14,
                            client.y + 14 + (idx as i32) * 24,
                            line,
                            rgba(230, 237, 243),
                            panel_bg,
                        );
                    }
                }
            }
        }

        // 3. Bottom Taskbar
        canvas.reset_clip();
        let tb_y = FB_HEIGHT as i32 - TASKBAR_H;
        let tb_bg = rgba(22, 27, 34);
        canvas.fill_rect(Rect::new(0, tb_y, FB_WIDTH as i32, TASKBAR_H), tb_bg);
        canvas.fill_rect(Rect::new(0, tb_y, FB_WIDTH as i32, 1), rgba(48, 54, 61));

        canvas.fill_rect(Rect::new(8, tb_y + 5, 108, TASKBAR_H - 10), rgba(35, 134, 54));
        canvas.draw_text(18, tb_y + 11, "WebOS", rgba(255, 255, 255), rgba(35, 134, 54));

        let mut bx = 128;
        for (_, label, is_open) in taskbar_items {
            let btn_bg = if is_open {
                rgba(33, 44, 61)
            } else {
                rgba(16, 21, 29)
            };
            let btn_rect = Rect::new(bx, tb_y + 5, 132, TASKBAR_H - 10);
            canvas.fill_rect(btn_rect, btn_bg);
            canvas.stroke_rect(
                btn_rect,
                if is_open {
                    rgba(88, 166, 255)
                } else {
                    rgba(48, 54, 61)
                },
            );
            canvas.draw_text(bx + 10, tb_y + 11, &label, rgba(230, 237, 243), btn_bg);
            bx += 140;
        }

        let total_secs = elapsed_ms / 1000;
        let clock_str = format!("UP {:02}:{:02}", total_secs / 60, total_secs % 60);
        canvas.draw_text(
            FB_WIDTH as i32 - 122,
            tb_y + 11,
            &clock_str,
            rgba(139, 148, 158),
            tb_bg,
        );

        // 4. Mouse Cursor
        canvas.draw_cursor(self.mouse_x, self.mouse_y);
    }

    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.framebuffer.as_ptr()
    }
}