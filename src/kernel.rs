use crate::allocator::heap_stats;
use crate::exec::{
    compile_wbml_to_wbo, expand_wbsh_line, format_wbo_inspection, install_system_binaries,
    UiWidgetKind, WboExecutable, WboSubsystem,
};
use crate::fs::FileSystem;
use crate::gui::{
    rgba, Canvas, EditorState, Rect, Window, CHAR_H, CHAR_W, FB_HEIGHT, FB_WIDTH, TASKBAR_H,
    TITLEBAR_H,
};
use crate::hal::Hardware;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

const TERM_COLS: usize = 54;
const TERM_ROWS: usize = 18;

const EDITOR_VISIBLE_ROWS: usize = 17;
const EDITOR_VISIBLE_COLS: usize = 55;

pub struct Kernel {
    framebuffer: Vec<u32>,
    windows: Vec<Window>,
    next_win_id: usize,
    dragging_win_id: Option<usize>,
    drag_offset_x: i32,
    drag_offset_y: i32,
    mouse_x: i32,
    mouse_y: i32,

    // Bottom-right WebOS Launcher Menu
    launcher_open: bool,
    launcher_query: String,

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
            launcher_open: false,
            launcher_query: String::new(),
            term_grid: vec![' '; TERM_COLS * TERM_ROWS],
            cursor_x: 0,
            cursor_y: 0,
            term_fg: rgba(88, 242, 132),
            term_bg: rgba(13, 17, 23),
            input_buffer: String::new(),
            fs,
            explorer_dir: String::from("C:/usr"),
            selected_path: Some(String::from("C:/usr/counter.wbml")),
            boot_time_ms: hw.uptime_ms(),
        };

        // Start with Terminal running (closing it leaves an empty taskbar + WebOS launcher)
        let _ = kernel.exec_wbo("C:/WebOS/bin/Apps/Terminal.wbo", Some((44, 44)));

        hw.serial_log("[WebOS] Booted v0.4.0 with Launcher, Code Editor, and .wbml compiler.");
        kernel.println("WebOS v0.4.0 (#![no_std] Rust)");
        kernel.println("Press Super/Win key or click 'WebOS' (bottom-right) to launch apps.");
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
            let win = self.windows.remove(pos);
            self.windows.push(win);
        }
    }

    fn toggle_launcher(&mut self) {
        self.launcher_open = !self.launcher_open;
        if self.launcher_open {
            self.launcher_query.clear();
        }
    }

    fn matching_launcher_apps(&self) -> Vec<(String, String)> {
        let q = self.launcher_query.to_ascii_lowercase();
        let mut apps = Vec::new();
        if let Ok(entries) = self.fs.list_dir("C:/WebOS/bin/Apps") {
            for (name, is_dir, _) in entries {
                if !is_dir && name.ends_with(".wbo") {
                    let display = name.trim_end_matches(".wbo").to_string();
                    if q.is_empty() || display.to_ascii_lowercase().contains(&q) {
                        apps.push((display, format!("C:/WebOS/bin/Apps/{}", name)));
                    }
                }
            }
        }
        apps
    }

    pub fn exec_wbo(&mut self, path: &str, pos_override: Option<(i32, i32)>) -> Result<(), String> {
        let resolved = self.fs.resolve_path(path);
        let bytes = self
            .fs
            .read_bytes(&resolved)
            .ok_or_else(|| format!("Executable not found: {}", resolved))?
            .to_vec();

        let exe = WboExecutable::parse(&bytes).map_err(|e| format!("{}: {}", resolved, e))?;

        // Focus existing singleton system window if already running
        if exe.subsystem != WboSubsystem::DeclarativeApp {
            if let Some(existing_id) = self
                .windows
                .iter()
                .find(|w| w.exe_path == resolved)
                .map(|w| w.id)
            {
                self.focus_window(existing_id);
                return Ok(());
            }
        }

        let editor_state = if exe.subsystem == WboSubsystem::Editor {
            Some(self.make_editor_state("C:/usr/counter.wbml"))
        } else {
            None
        };

        let id = self.next_win_id;
        self.next_win_id += 1;
        let offset = ((id as i32) * 32) % 180;
        let (x, y) = pos_override.unwrap_or((120 + offset, 64 + offset));

        self.windows.push(Window {
            id,
            exe_path: resolved,
            subsystem: exe.subsystem,
            title: exe.title,
            x,
            y,
            w: exe.width,
            h: exe.height,
            editor: editor_state,
            runtime: exe.runtime,
        });

        Ok(())
    }

    fn make_editor_state(&self, path: &str) -> EditorState {
        let resolved = self.fs.resolve_path(path);
        let content = self
            .fs
            .read_str(&resolved)
            .unwrap_or("app \"MyApp\" 340 200\nvar count = 0\nlabel lbl 20 20 \"Count: {count}\"\nbutton btn 20 60 110 32 \"Click Me\" {\n    count = count + 1\n}\n");
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        EditorState {
            file_path: resolved,
            lines,
            cursor_line: 0,
            cursor_col: 0,
            scroll_y: 0,
            status: String::from("Ctrl+S to Save | Click 'Build .wbo' for .wbml apps"),
            built_wbo_path: None,
        }
    }

    pub fn open_in_editor(&mut self, path: &str) {
        let resolved = self.fs.resolve_path(path);
        let new_state = self.make_editor_state(&resolved);

        if let Some(existing_id) = self
            .windows
            .iter()
            .find(|w| w.subsystem == WboSubsystem::Editor)
            .map(|w| w.id)
        {
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == existing_id) {
                w.editor = Some(new_state);
            }
            self.focus_window(existing_id);
        } else {
            let _ = self.exec_wbo("C:/WebOS/bin/Apps/Editor.wbo", Some((260, 70)));
            if let Some(w) = self.windows.last_mut() {
                w.editor = Some(new_state);
            }
        }
    }

    fn save_active_editor(&mut self) {
        let save_info = self
            .windows
            .last()
            .and_then(|w| w.editor.as_ref())
            .map(|ed| (ed.file_path.clone(), ed.lines.join("\n")));

        if let Some((path, content)) = save_info {
            let res = self.fs.write_str(&path, &content);
            if let Some(ed) = self.windows.last_mut().and_then(|w| w.editor.as_mut()) {
                match res {
                    Ok(()) => ed.status = format!("Saved {} ({} bytes)", path, content.len()),
                    Err(e) => ed.status = format!("Save error: {}", e),
                }
            }
        }
    }

    fn compile_wbml_path(&mut self, src_path: &str, out_override: Option<&str>) -> Result<String, String> {
        let resolved_src = self.fs.resolve_path(src_path);
        let source = self
            .fs
            .read_str(&resolved_src)
            .ok_or_else(|| format!("Source not found: {}", resolved_src))?
            .to_string();

        let (app_title, wbo_bytes) = compile_wbml_to_wbo(&source)?;
        let out_path = if let Some(custom) = out_override {
            self.fs.resolve_path(custom)
        } else {
            let clean_name: String = app_title
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            let final_name = if clean_name.is_empty() {
                String::from("CustomApp")
            } else {
                clean_name
            };
            format!("C:/WebOS/bin/Apps/{}.wbo", final_name)
        };

        self.fs
            .write_bytes(&out_path, wbo_bytes)
            .map_err(|e| format!("Write failed: {}", e))?;
        Ok(out_path)
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

    fn launcher_rect(&self) -> Rect {
        Rect::new(FB_WIDTH as i32 - 352, FB_HEIGHT as i32 - TASKBAR_H - 344, 344, 340)
    }

    fn launcher_btn_rect(&self) -> Rect {
        Rect::new(FB_WIDTH as i32 - 124, FB_HEIGHT as i32 - TASKBAR_H + 5, 116, TASKBAR_H - 10)
    }

    pub fn on_mouse_down(&mut self, x: i32, y: i32, button: u32) {
        self.on_mouse_move(x, y);
        if button != 0 {
            return;
        }

        // 1. Bottom-right WebOS Launcher Button
        if self.launcher_btn_rect().contains(x, y) {
            self.toggle_launcher();
            return;
        }

        // 2. If Launcher Menu is open, check clicks inside it
        if self.launcher_open {
            let lrect = self.launcher_rect();
            if lrect.contains(x, y) {
                let list_top = lrect.y + 78;
                if y >= list_top {
                    let row = ((y - list_top) / 28) as usize;
                    let matches = self.matching_launcher_apps();
                    if let Some((_, exe_path)) = matches.get(row) {
                        let path_clone = exe_path.clone();
                        self.launcher_open = false;
                        let _ = self.exec_wbo(&path_clone, None);
                    }
                }
                return;
            } else {
                // Clicking outside closes the launcher
                self.launcher_open = false;
            }
        }

        // 3. Taskbar running app tabs (ONLY currently running windows)
        if y >= FB_HEIGHT as i32 - TASKBAR_H {
            let btn_y = FB_HEIGHT as i32 - TASKBAR_H + 5;
            let btn_h = TASKBAR_H - 10;
            if y >= btn_y && y < btn_y + btn_h {
                let mut running_ids: Vec<usize> = self.windows.iter().map(|w| w.id).collect();
                running_ids.sort_unstable();

                let mut bx = 10;
                for win_id in running_ids {
                    if bx + 136 > self.launcher_btn_rect().x - 130 {
                        break;
                    }
                    if x >= bx && x < bx + 132 {
                        self.focus_window(win_id);
                        return;
                    }
                    bx += 138;
                }
            }
            return;
        }

        // 4. Hit-test open windows from top to bottom
        let mut hit: Option<(usize, WboSubsystem, (i32, i32), Rect, bool, bool)> = None;
        for win in self.windows.iter().rev() {
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
                // Closing terminates the running app and removes it from the taskbar
                if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
                    self.windows.remove(pos);
                }
                return;
            }

            self.focus_window(id);

            if titlebar_clicked {
                self.dragging_win_id = Some(id);
                self.drag_offset_x = x - win_origin.0;
                self.drag_offset_y = y - win_origin.1;
            } else {
                match subsystem {
                    WboSubsystem::Files => self.handle_files_click(x, y, client),
                    WboSubsystem::Editor => self.handle_editor_click(x, y, client),
                    WboSubsystem::DeclarativeApp => self.handle_declarative_click(x, y, client),
                    _ => {}
                }
            }
        }
    }

    fn handle_files_click(&mut self, x: i32, y: i32, client: Rect) {
        // Toolbar buttons: [+File] [+Dir] [Up]
        let new_file_btn = Rect::new(client.x + client.w - 178, client.y + 5, 56, 20);
        let new_dir_btn = Rect::new(client.x + client.w - 116, client.y + 5, 52, 20);
        let up_btn = Rect::new(client.x + client.w - 58, client.y + 5, 50, 20);

        if new_file_btn.contains(x, y) {
            let mut idx = 1;
            loop {
                let candidate = if idx == 1 {
                    format!("{}/new_app.wbml", self.explorer_dir)
                } else {
                    format!("{}/new_app_{}.wbml", self.explorer_dir, idx)
                };
                if !self.fs.exists(&candidate) {
                    let starter = "app \"NewApp\" 320 190\nvar n = 0\nlabel lbl 20 20 \"Value: {n}\"\nbutton add 20 60 100 32 \"Add 1\" {\n    n = n + 1\n}\n";
                    let _ = self.fs.write_str(&candidate, starter);
                    self.selected_path = Some(candidate.clone());
                    self.open_in_editor(&candidate);
                    break;
                }
                idx += 1;
            }
            return;
        }

        if new_dir_btn.contains(x, y) {
            let mut idx = 1;
            loop {
                let candidate = if idx == 1 {
                    format!("{}/folder", self.explorer_dir)
                } else {
                    format!("{}/folder_{}", self.explorer_dir, idx)
                };
                if !self.fs.exists(&candidate) {
                    let _ = self.fs.mkdir(&candidate);
                    break;
                }
                idx += 1;
            }
            return;
        }

        if up_btn.contains(x, y) {
            self.explorer_dir = FileSystem::parent_of(&self.explorer_dir);
            self.selected_path = None;
            return;
        }

        // Preview panel action buttons: [Edit] [Run] [Del]
        let preview_y = client.y + client.h - 106;
        let del_btn = Rect::new(client.x + client.w - 62, preview_y + 6, 46, 20);
        let action_btn = Rect::new(client.x + client.w - 120, preview_y + 6, 52, 20);

        if let Some(sel) = self.selected_path.clone() {
            if del_btn.contains(x, y) {
                let _ = self.fs.remove(&sel);
                self.selected_path = None;
                return;
            }

            if action_btn.contains(x, y) {
                if sel.ends_with(".wbo") {
                    let _ = self.exec_wbo(&sel, None);
                } else if sel.ends_with(".wbsh") {
                    let _ = self.exec_wbo("C:/WebOS/bin/Apps/Terminal.wbo", None);
                    self.println("");
                    self.run_wbsh_file(&sel, &[]);
                    self.print_prompt();
                } else {
                    self.open_in_editor(&sel);
                }
                return;
            }
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

    fn handle_editor_click(&mut self, x: i32, y: i32, client: Rect) {
        let save_btn = Rect::new(client.x + client.w - 246, client.y + 5, 62, 22);
        let build_btn = Rect::new(client.x + client.w - 178, client.y + 5, 108, 22);
        let run_btn = Rect::new(client.x + client.w - 64, client.y + 5, 54, 22);

        if save_btn.contains(x, y) {
            self.save_active_editor();
            return;
        }

        if build_btn.contains(x, y) {
            self.save_active_editor();
            let src_path = self
                .windows
                .last()
                .and_then(|w| w.editor.as_ref())
                .map(|ed| ed.file_path.clone());

            if let Some(path) = src_path {
                let result = self.compile_wbml_path(&path, None);
                if let Some(ed) = self.windows.last_mut().and_then(|w| w.editor.as_mut()) {
                    match result {
                        Ok(out_wbo) => {
                            ed.status = format!("Compiled -> {}", out_wbo);
                            ed.built_wbo_path = Some(out_wbo);
                        }
                        Err(err) => {
                            ed.status = format!("Compile Error: {}", err);
                        }
                    }
                }
            }
            return;
        }

        if run_btn.contains(x, y) {
            let target_wbo = self
                .windows
                .last()
                .and_then(|w| w.editor.as_ref())
                .and_then(|ed| ed.built_wbo_path.clone());
            if let Some(wbo_path) = target_wbo {
                let _ = self.exec_wbo(&wbo_path, None);
            }
            return;
        }

        // Click inside text area to move cursor
        let text_top = client.y + 36;
        let text_left = client.x + 54;
        if y >= text_top && x >= text_left {
            if let Some(ed) = self.windows.last_mut().and_then(|w| w.editor.as_mut()) {
                let clicked_row = ((y - text_top) / (CHAR_H as i32)) as usize;
                let clicked_col = ((x - text_left) / (CHAR_W as i32)) as usize;
                let target_line = (ed.scroll_y + clicked_row).min(ed.lines.len().saturating_sub(1));
                ed.cursor_line = target_line;
                ed.cursor_col = clicked_col.min(ed.lines[target_line].len());
            }
        }
    }

    fn handle_declarative_click(&mut self, x: i32, y: i32, client: Rect) {
        if let Some(win) = self.windows.last_mut() {
            if let Some(rt) = win.runtime.as_mut() {
                let rel_x = x - client.x;
                let rel_y = y - client.y;
                let mut script_to_run = None;

                for w in &rt.widgets {
                    if matches!(w.kind, UiWidgetKind::Button) {
                        let btn_rect = Rect::new(w.x, w.y, w.w, w.h);
                        if btn_rect.contains(rel_x, rel_y) {
                            script_to_run = Some(w.script.clone());
                            break;
                        }
                    }
                }

                if let Some(script) = script_to_run {
                    rt.run_script(&script);
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
        // 1. Windows / Super / Meta / F1 key toggles the WebOS Launcher
        if code == 1005 {
            self.toggle_launcher();
            return;
        }

        // 2. If Launcher Menu is open, route keys to Launcher search
        if self.launcher_open {
            match code {
                27 => self.launcher_open = false,
                13 => {
                    let matches = self.matching_launcher_apps();
                    if let Some((_, exe_path)) = matches.first() {
                        let path_clone = exe_path.clone();
                        self.launcher_open = false;
                        let _ = self.exec_wbo(&path_clone, None);
                    }
                }
                8 => {
                    self.launcher_query.pop();
                }
                32..=126 => {
                    if let Some(ch) = char::from_u32(code) {
                        if self.launcher_query.len() < 22 {
                            self.launcher_query.push(ch);
                        }
                    }
                }
                _ => {}
            }
            return;
        }

        // 3. Route keys to the focused top-most window
        let focused_subsys = self.windows.last().map(|w| w.subsystem);
        match focused_subsys {
            Some(WboSubsystem::Editor) => self.on_editor_key(code),
            Some(WboSubsystem::Terminal) => self.on_terminal_key(code, hw),
            _ => {}
        }
    }

    fn on_editor_key(&mut self, code: u32) {
        if code == 2001 {
            // Ctrl+S
            self.save_active_editor();
            return;
        }

        let Some(ed) = self.windows.last_mut().and_then(|w| w.editor.as_mut()) else {
            return;
        };

        match code {
            // ArrowUp
            1001 => {
                if ed.cursor_line > 0 {
                    ed.cursor_line -= 1;
                    ed.cursor_col = ed.cursor_col.min(ed.lines[ed.cursor_line].len());
                }
            }
            // ArrowDown
            1002 => {
                if ed.cursor_line + 1 < ed.lines.len() {
                    ed.cursor_line += 1;
                    ed.cursor_col = ed.cursor_col.min(ed.lines[ed.cursor_line].len());
                }
            }
            // ArrowLeft
            1003 => {
                if ed.cursor_col > 0 {
                    ed.cursor_col -= 1;
                } else if ed.cursor_line > 0 {
                    ed.cursor_line -= 1;
                    ed.cursor_col = ed.lines[ed.cursor_line].len();
                }
            }
            // ArrowRight
            1004 => {
                if ed.cursor_col < ed.lines[ed.cursor_line].len() {
                    ed.cursor_col += 1;
                } else if ed.cursor_line + 1 < ed.lines.len() {
                    ed.cursor_line += 1;
                    ed.cursor_col = 0;
                }
            }
            // Enter
            13 => {
                let tail = ed.lines[ed.cursor_line].split_off(ed.cursor_col);
                ed.cursor_line += 1;
                ed.lines.insert(ed.cursor_line, tail);
                ed.cursor_col = 0;
            }
            // Backspace
            8 => {
                if ed.cursor_col > 0 {
                    ed.cursor_col -= 1;
                    ed.lines[ed.cursor_line].remove(ed.cursor_col);
                } else if ed.cursor_line > 0 {
                    let current = ed.lines.remove(ed.cursor_line);
                    ed.cursor_line -= 1;
                    ed.cursor_col = ed.lines[ed.cursor_line].len();
                    ed.lines[ed.cursor_line].push_str(&current);
                }
            }
            // Tab -> 4 spaces
            9 => {
                ed.lines[ed.cursor_line].insert_str(ed.cursor_col, "    ");
                ed.cursor_col += 4;
            }
            // Printable ASCII
            32..=126 => {
                if let Some(ch) = char::from_u32(code) {
                    ed.lines[ed.cursor_line].insert(ed.cursor_col, ch);
                    ed.cursor_col += 1;
                }
            }
            _ => {}
        }

        // Keep cursor within visible scroll viewport
        if ed.cursor_line < ed.scroll_y {
            ed.scroll_y = ed.cursor_line;
        } else if ed.cursor_line >= ed.scroll_y + EDITOR_VISIBLE_ROWS {
            ed.scroll_y = ed.cursor_line + 1 - EDITOR_VISIBLE_ROWS;
        }
    }

    fn on_terminal_key(&mut self, code: u32, hw: &dyn Hardware) {
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

        let path_script = format!("C:/WebOS/bin/PATH/{}.wbsh", cmd);
        if self.fs.read_str(&path_script).is_some() {
            self.run_wbsh_file(&path_script, args);
            return;
        }

        let app_bin = format!("C:/WebOS/bin/Apps/{}.wbo", cmd);
        if self.fs.read_bytes(&app_bin).is_some() {
            if let Err(e) = self.exec_wbo(&app_bin, None) {
                self.println(&e);
            }
            return;
        }

        let _ = hw;
        self.println(&format!("wbsh: '{}': not found in PATH or Apps", cmd));
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

        let mut parts = stmt.splitn(3, ' ');
        let syscall = parts.next().unwrap_or("");
        let a1 = parts.next().unwrap_or("");
        let a2 = parts.next().unwrap_or("");

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
                if a1.is_empty() || a2.is_empty() {
                    self.println("Usage: write <file> <text>");
                    return;
                }
                match self.fs.write_str(a1, a2) {
                    Ok(()) => self.println(&format!("Wrote {} bytes to {}", a2.len(), a1)),
                    Err(e) => self.println(&format!("write: {}", e)),
                }
            }
            "@sys.edit" => {
                let target = if a1.is_empty() {
                    "C:/usr/counter.wbml"
                } else {
                    a1
                };
                self.open_in_editor(target);
            }
            "@sys.build" => {
                if a1.is_empty() {
                    self.println("Usage: build <source.wbml> [output.wbo]");
                    return;
                }
                let out_opt = if a2.is_empty() { None } else { Some(a2) };
                match self.compile_wbml_path(a1, out_opt) {
                    Ok(out_path) => self.println(&format!("Compiled -> {}", out_path)),
                    Err(e) => self.println(&format!("Build error: {}", e)),
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
        let top_win_id = self.windows.last().map(|w| w.id);

        // Taskbar ONLY shows currently running windows
        let mut taskbar_items: Vec<(usize, String, bool)> = self
            .windows
            .iter()
            .map(|w| (w.id, w.title.clone(), Some(w.id) == top_win_id))
            .collect();
        taskbar_items.sort_by_key(|(id, _, _)| *id);

        let dir_entries = self.fs.list_dir(&self.explorer_dir).unwrap_or_default();
        let selected_preview: Option<(String, bool)> = self.selected_path.as_ref().map(|p| {
            let is_exec = p.ends_with(".wbo") || p.ends_with(".wbsh");
            let text = if p.ends_with(".wbo") {
                self.fs
                    .read_bytes(p)
                    .map(format_wbo_inspection)
                    .unwrap_or_default()
            } else {
                self.fs.read_str(p).unwrap_or("").to_string()
            };
            (text, is_exec)
        });

        let launcher_apps = if self.launcher_open {
            self.matching_launcher_apps()
        } else {
            Vec::new()
        };
        let launcher_rect = self.launcher_rect();
        let launcher_btn = self.launcher_btn_rect();

        let mut canvas = Canvas::new(&mut self.framebuffer);

        // 1. Desktop Wallpaper
        canvas.draw_wallpaper();

        // 2. Render running windows
        for win in &self.windows {
            let is_focused = Some(win.id) == top_win_id;
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

                    // Path header & [+File] [+Dir] [Up]
                    canvas.draw_text(
                        client.x + 10,
                        client.y + 8,
                        &self.explorer_dir,
                        rgba(86, 212, 255),
                        panel_bg,
                    );

                    let new_file_btn = Rect::new(client.x + client.w - 178, client.y + 5, 56, 20);
                    let new_dir_btn = Rect::new(client.x + client.w - 116, client.y + 5, 52, 20);
                    let up_btn = Rect::new(client.x + client.w - 58, client.y + 5, 50, 20);

                    for (btn, lbl) in [
                        (new_file_btn, "+File"),
                        (new_dir_btn, "+Dir"),
                        (up_btn, "Up"),
                    ] {
                        canvas.fill_rect(btn, rgba(33, 44, 61));
                        canvas.stroke_rect(btn, rgba(88, 166, 255));
                        canvas.draw_text(
                            btn.x + 4,
                            btn.y + 3,
                            lbl,
                            rgba(230, 237, 243),
                            rgba(33, 44, 61),
                        );
                    }

                    // Directory entries
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
                        } else if name.ends_with(".wbml") {
                            rgba(86, 212, 255)
                        } else {
                            rgba(230, 237, 243)
                        };
                        canvas.draw_text(client.x + 10, row_y + 1, &label, fg, row_bg);
                    }

                    // Preview panel with [Edit]/[Run] and [Del]
                    let preview_y = client.y + client.h - 106;
                    let preview_rect = Rect::new(client.x + 8, preview_y, client.w - 16, 98);
                    canvas.fill_rect(preview_rect, rgba(11, 15, 21));
                    canvas.stroke_rect(preview_rect, rgba(48, 54, 61));

                    if let Some((body, is_exec)) = &selected_preview {
                        let del_btn = Rect::new(client.x + client.w - 62, preview_y + 6, 46, 20);
                        let action_btn = Rect::new(client.x + client.w - 120, preview_y + 6, 52, 20);

                        canvas.fill_rect(action_btn, rgba(35, 134, 54));
                        canvas.draw_text(
                            action_btn.x + 6,
                            action_btn.y + 3,
                            if *is_exec { "Run" } else { "Edit" },
                            rgba(255, 255, 255),
                            rgba(35, 134, 54),
                        );

                        canvas.fill_rect(del_btn, rgba(155, 42, 42));
                        canvas.draw_text(
                            del_btn.x + 6,
                            del_btn.y + 3,
                            "Del",
                            rgba(255, 255, 255),
                            rgba(155, 42, 42),
                        );

                        for (line_idx, line_txt) in body.lines().take(3).enumerate() {
                            let clipped: String = line_txt.chars().take(24).collect();
                            canvas.draw_text(
                                preview_rect.x + 8,
                                preview_rect.y + 10 + (line_idx as i32) * 22,
                                &clipped,
                                rgba(201, 209, 217),
                                rgba(11, 15, 21),
                            );
                        }
                    }
                }
                WboSubsystem::Editor => {
                    let bg = rgba(13, 17, 23);
                    let bar_bg = rgba(22, 27, 34);
                    canvas.fill_rect(client, bg);

                    // Editor top toolbar
                    canvas.fill_rect(Rect::new(client.x, client.y, client.w, 32), bar_bg);
                    if let Some(ed) = &win.editor {
                        canvas.draw_text(
                            client.x + 10,
                            client.y + 8,
                            &ed.file_path,
                            rgba(86, 212, 255),
                            bar_bg,
                        );

                        let save_btn = Rect::new(client.x + client.w - 246, client.y + 5, 62, 22);
                        let build_btn = Rect::new(client.x + client.w - 178, client.y + 5, 108, 22);
                        let run_btn = Rect::new(client.x + client.w - 64, client.y + 5, 54, 22);

                        canvas.fill_rect(save_btn, rgba(33, 44, 61));
                        canvas.stroke_rect(save_btn, rgba(88, 166, 255));
                        canvas.draw_text(
                            save_btn.x + 8,
                            save_btn.y + 4,
                            "Save",
                            rgba(230, 237, 243),
                            rgba(33, 44, 61),
                        );

                        canvas.fill_rect(build_btn, rgba(35, 134, 54));
                        canvas.draw_text(
                            build_btn.x + 6,
                            build_btn.y + 4,
                            "Build .wbo",
                            rgba(255, 255, 255),
                            rgba(35, 134, 54),
                        );

                        let run_bg = if ed.built_wbo_path.is_some() {
                            rgba(31, 111, 235)
                        } else {
                            rgba(33, 38, 45)
                        };
                        canvas.fill_rect(run_btn, run_bg);
                        canvas.draw_text(
                            run_btn.x + 10,
                            run_btn.y + 4,
                            "Run",
                            rgba(255, 255, 255),
                            run_bg,
                        );

                        // Gutter & source lines
                        let gutter_w = 46;
                        canvas.fill_rect(
                            Rect::new(client.x, client.y + 32, gutter_w, client.h - 58),
                            rgba(16, 21, 29),
                        );

                        for row in 0..EDITOR_VISIBLE_ROWS {
                            let line_idx = ed.scroll_y + row;
                            if line_idx >= ed.lines.len() {
                                break;
                            }
                            let py = client.y + 36 + (row * CHAR_H) as i32;
                            let lnum = format!("{:>3}", line_idx + 1);
                            canvas.draw_text(
                                client.x + 6,
                                py,
                                &lnum,
                                rgba(110, 118, 129),
                                rgba(16, 21, 29),
                            );

                            let visible_slice: String =
                                ed.lines[line_idx].chars().take(EDITOR_VISIBLE_COLS).collect();
                            canvas.draw_text(
                                client.x + 54,
                                py,
                                &visible_slice,
                                rgba(230, 237, 243),
                                bg,
                            );
                        }

                        // Blinking text cursor
                        if is_focused
                            && blink_on
                            && ed.cursor_line >= ed.scroll_y
                            && ed.cursor_line < ed.scroll_y + EDITOR_VISIBLE_ROWS
                        {
                            let vis_row = ed.cursor_line - ed.scroll_y;
                            let vis_col = ed.cursor_col.min(EDITOR_VISIBLE_COLS);
                            let cx = client.x + 54 + (vis_col * CHAR_W) as i32;
                            let cy = client.y + 36 + (vis_row * CHAR_H) as i32;
                            canvas.fill_rect(Rect::new(cx, cy, 2, 16), rgba(88, 242, 132));
                        }

                        // Status footer bar
                        let status_y = client.y + client.h - 24;
                        canvas.fill_rect(Rect::new(client.x, status_y, client.w, 24), bar_bg);
                        canvas.draw_text(
                            client.x + 10,
                            status_y + 4,
                            &ed.status,
                            rgba(126, 231, 135),
                            bar_bg,
                        );
                    }
                }
                WboSubsystem::SysMon => {
                    let panel_bg = rgba(16, 21, 29);
                    canvas.fill_rect(client, panel_bg);

                    let (used, total) = heap_stats();
                    let pct = ((used as f64 / total as f64) * 100.0).min(100.0);

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
                        &format!("Running Processes: {}", self.windows.len()),
                        rgba(139, 148, 158),
                        panel_bg,
                    );
                    canvas.draw_text(
                        client.x + 12,
                        client.y + 104,
                        "Exe Format: WBO\\x01 (.wbml -> .wbo)",
                        rgba(86, 212, 255),
                        panel_bg,
                    );
                }
                WboSubsystem::DeclarativeApp => {
                    let panel_bg = rgba(16, 21, 29);
                    canvas.fill_rect(client, panel_bg);

                    if let Some(rt) = &win.runtime {
                        for w in &rt.widgets {
                            let wx = client.x + w.x;
                            let wy = client.y + w.y;
                            match w.kind {
                                UiWidgetKind::Rect(col) => {
                                    let r = Rect::new(wx, wy, w.w, w.h);
                                    canvas.fill_rect(r, col);
                                    canvas.stroke_rect(r, rgba(48, 54, 61));
                                }
                                UiWidgetKind::Label => {
                                    let rendered = rt.interpolate(&w.text);
                                    canvas.draw_text(
                                        wx,
                                        wy,
                                        &rendered,
                                        rgba(230, 237, 243),
                                        panel_bg,
                                    );
                                }
                                UiWidgetKind::Button => {
                                    let r = Rect::new(wx, wy, w.w, w.h);
                                    let btn_bg = rgba(33, 44, 61);
                                    canvas.fill_rect(r, btn_bg);
                                    canvas.stroke_rect(r, rgba(88, 166, 255));
                                    let rendered = rt.interpolate(&w.text);
                                    let text_w = (rendered.chars().count() * CHAR_W) as i32;
                                    let tx = wx + ((w.w - text_w) / 2).max(6);
                                    let ty = wy + ((w.h - 14) / 2).max(4);
                                    canvas.draw_text(
                                        tx,
                                        ty,
                                        &rendered,
                                        rgba(255, 255, 255),
                                        btn_bg,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Bottom-Right Searchable WebOS Launcher Popup
        if self.launcher_open {
            canvas.reset_clip();
            canvas.fill_rect(
                Rect::new(launcher_rect.x - 4, launcher_rect.y - 4, launcher_rect.w + 8, launcher_rect.h + 8),
                rgba(7, 10, 14),
            );
            let l_bg = rgba(22, 27, 34);
            canvas.fill_rect(launcher_rect, l_bg);
            canvas.stroke_rect(launcher_rect, rgba(88, 166, 255));

            canvas.draw_text(
                launcher_rect.x + 12,
                launcher_rect.y + 10,
                "WebOS App Launcher",
                rgba(230, 237, 243),
                l_bg,
            );

            // Search input box
            let search_box = Rect::new(launcher_rect.x + 12, launcher_rect.y + 36, launcher_rect.w - 24, 28);
            let s_bg = rgba(13, 17, 23);
            canvas.fill_rect(search_box, s_bg);
            canvas.stroke_rect(search_box, rgba(88, 242, 132));

            let prompt_txt = if self.launcher_query.is_empty() {
                "Search Apps/..."
            } else {
                &self.launcher_query
            };
            let prompt_col = if self.launcher_query.is_empty() {
                rgba(110, 118, 129)
            } else {
                rgba(230, 237, 243)
            };
            canvas.draw_text(search_box.x + 8, search_box.y + 7, prompt_txt, prompt_col, s_bg);

            // Matching .wbo apps list
            for (idx, (app_name, _)) in launcher_apps.iter().take(8).enumerate() {
                let row_y = launcher_rect.y + 78 + (idx as i32) * 28;
                let row_rect = Rect::new(launcher_rect.x + 12, row_y, launcher_rect.w - 24, 24);
                let row_bg = if idx == 0 { rgba(31, 61, 97) } else { l_bg };
                canvas.fill_rect(row_rect, row_bg);
                canvas.draw_text(
                    row_rect.x + 10,
                    row_rect.y + 4,
                    &format!("{}.wbo", app_name),
                    rgba(126, 231, 135),
                    row_bg,
                );
            }
        }

        // 4. Bottom Taskbar (ONLY running apps on left; Clock + WebOS button on bottom-right)
        canvas.reset_clip();
        let tb_y = FB_HEIGHT as i32 - TASKBAR_H;
        let tb_bg = rgba(22, 27, 34);
        canvas.fill_rect(Rect::new(0, tb_y, FB_WIDTH as i32, TASKBAR_H), tb_bg);
        canvas.fill_rect(Rect::new(0, tb_y, FB_WIDTH as i32, 1), rgba(48, 54, 61));

        let mut bx = 10;
        for (_, title, is_focused) in taskbar_items {
            if bx + 136 > launcher_btn.x - 130 {
                break;
            }
            let btn_bg = if is_focused {
                rgba(33, 44, 61)
            } else {
                rgba(16, 21, 29)
            };
            let btn_rect = Rect::new(bx, tb_y + 5, 132, TASKBAR_H - 10);
            canvas.fill_rect(btn_rect, btn_bg);
            canvas.stroke_rect(
                btn_rect,
                if is_focused {
                    rgba(88, 166, 255)
                } else {
                    rgba(48, 54, 61)
                },
            );
            let short_title: String = title.chars().take(9).collect();
            canvas.draw_text(bx + 10, tb_y + 11, &short_title, rgba(230, 237, 243), btn_bg);
            bx += 138;
        }

        // Clock to the left of the bottom-right WebOS Launcher button
        let total_secs = elapsed_ms / 1000;
        let clock_str = format!("UP {:02}:{:02}", total_secs / 60, total_secs % 60);
        canvas.draw_text(
            launcher_btn.x - 114,
            tb_y + 11,
            &clock_str,
            rgba(139, 148, 158),
            tb_bg,
        );

        // Bottom-right WebOS Start/Launcher Button
        let wb_bg = if self.launcher_open {
            rgba(46, 160, 67)
        } else {
            rgba(35, 134, 54)
        };
        canvas.fill_rect(launcher_btn, wb_bg);
        canvas.draw_text(
            launcher_btn.x + 26,
            launcher_btn.y + 6,
            "WebOS",
            rgba(255, 255, 255),
            wb_bg,
        );

        // 5. Mouse Cursor
        canvas.draw_cursor(self.mouse_x, self.mouse_y);
    }

    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.framebuffer.as_ptr()
    }
}