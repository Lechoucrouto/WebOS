use crate::allocator::heap_stats;
use crate::exec::{
    compile_wbml_to_wbo, expand_wbsh_line, format_wbo_inspection, install_system_binaries,
    ButtonStyle, DeclarativeRuntime, RenderedWidget, RenderedWidgetKind, TextAlign, Value,
    WboExecutable, WboSubsystem,
};
use crate::fs::FileSystem;
use crate::gui::{
    rgba, Canvas, EditorState, IconKind, Rect, Window, CHAR_H, CHAR_W, FB_HEIGHT, FB_WIDTH,
    TASKBAR_H, TITLEBAR_H,
};
use crate::hal::Hardware;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

const TERM_COLS: usize = 56;
const TERM_ROWS: usize = 19;

fn char_to_byte_idx(s: &str, char_col: usize) -> usize {
    s.char_indices()
        .nth(char_col)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

pub struct Kernel {
    framebuffer: Vec<u32>,
    windows: Vec<Window>,
    next_win_id: usize,
    dragging_win_id: Option<usize>,
    resizing_win_id: Option<usize>,
    drag_offset_x: i32,
    drag_offset_y: i32,
    mouse_x: i32,
    mouse_y: i32,

    launcher_open: bool,
    launcher_query: String,

    term_grid: Vec<char>,
    cursor_x: usize,
    cursor_y: usize,
    term_fg: u32,
    term_bg: u32,
    input_buffer: String,

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
            framebuffer: vec![rgba(10, 15, 26); FB_WIDTH * FB_HEIGHT],
            windows: Vec::new(),
            next_win_id: 1,
            dragging_win_id: None,
            resizing_win_id: None,
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
            term_bg: rgba(11, 15, 23),
            input_buffer: String::new(),
            fs,
            explorer_dir: String::from("C:/usr"),
            selected_path: Some(String::from("C:/usr/snake.wbml")),
            boot_time_ms: hw.uptime_ms(),
        };

        let _ = kernel.exec_wbo("C:/WebOS/bin/Apps/Editor.wbo", Some((135, 36)));

        hw.serial_log("[WebOS] Booted v0.5.0 Win10 Chrome + WBML v2.");
        kernel.println("WebOS v0.5.0 (#![no_std] Rust)");
        kernel.println("Press Super/Win or click 'WebOS' (bottom-right) to launch apps.");
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
            win.minimized = false;
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
        let offset = ((id as i32) * 30) % 150;
        let (x, y) = pos_override.unwrap_or((150 + offset, 48 + offset));

        self.windows.push(Window {
            id,
            exe_path: resolved,
            subsystem: exe.subsystem,
            title: exe.title,
            x,
            y,
            w: exe.width,
            h: exe.height,
            minimized: false,
            maximized: false,
            saved_bounds: None,
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
            .unwrap_or("app \"MyApp\" 320 220 {\n    state {\n        count = 0\n    }\n    col pad=16 gap=12 {\n        text \"Count: {count}\" size=lg color=#58F284\n        btn \"Increment\" style=primary { count = count + 1 }\n    }\n}\n");
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
            scroll_x: 0,
            status: String::from("Ctrl+S Save | Ctrl+R Build & Run"),
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
            let _ = self.exec_wbo("C:/WebOS/bin/Apps/Editor.wbo", Some((140, 42)));
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
                    Ok(()) => ed.status = format!("Saved {} ({} B)", path, content.len()),
                    Err(e) => ed.status = format!("Save error: {}", e),
                }
            }
        }
    }

    fn build_and_run_active_editor(&mut self, auto_run: bool) {
        self.save_active_editor();
        let src_path = self
            .windows
            .last()
            .and_then(|w| w.editor.as_ref())
            .map(|ed| ed.file_path.clone());

        if let Some(path) = src_path {
            let result = self.compile_wbml_path(&path, None);
            let mut launch_path = None;
            if let Some(ed) = self.windows.last_mut().and_then(|w| w.editor.as_mut()) {
                match result {
                    Ok(out_wbo) => {
                        ed.status = format!("Built -> {}", out_wbo);
                        ed.built_wbo_path = Some(out_wbo.clone());
                        if auto_run {
                            launch_path = Some(out_wbo);
                        }
                    }
                    Err(err) => {
                        ed.status = format!("Compile Error: {}", err);
                    }
                }
            }
            if let Some(wbo_path) = launch_path {
                let _ = self.exec_wbo(&wbo_path, None);
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
                if !win.maximized {
                    let max_x = FB_WIDTH as i32 - 80;
                    let max_y = FB_HEIGHT as i32 - TASKBAR_H - TITLEBAR_H;
                    win.x = (self.mouse_x - self.drag_offset_x).clamp(-win.w + 80, max_x);
                    win.y = (self.mouse_y - self.drag_offset_y).clamp(0, max_y);
                }
            }
        } else if let Some(res_id) = self.resizing_win_id {
            if let Some(win) = self.windows.iter_mut().find(|w| w.id == res_id) {
                if !win.maximized {
                    win.w = (self.mouse_x - win.x).clamp(280, FB_WIDTH as i32 - win.x);
                    win.h = (self.mouse_y - win.y).clamp(180, FB_HEIGHT as i32 - TASKBAR_H - win.y);
                }
            }
        }
    }

    fn launcher_rect(&self) -> Rect {
        Rect::new(FB_WIDTH as i32 - 360, FB_HEIGHT as i32 - TASKBAR_H - 352, 348, 344)
    }

    fn launcher_btn_rect(&self) -> Rect {
        Rect::new(FB_WIDTH as i32 - 122, FB_HEIGHT as i32 - TASKBAR_H + 6, 112, TASKBAR_H - 12)
    }

    fn desktop_shortcuts(&self) -> [(&'static str, &'static str, IconKind); 5] {
        [
            ("Terminal", "C:/WebOS/bin/Apps/Terminal.wbo", IconKind::Terminal),
            ("Editor", "C:/WebOS/bin/Apps/Editor.wbo", IconKind::Editor),
            ("Files", "C:/WebOS/bin/Apps/Files.wbo", IconKind::Files),
            ("Counter", "C:/WebOS/bin/Apps/Counter.wbo", IconKind::AppBinary),
            ("Arcade", "C:/WebOS/bin/Apps/Arcade_Runner.wbo", IconKind::AppBinary),
        ]
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

        // 2. Launcher Popup
        if self.launcher_open {
            let lrect = self.launcher_rect();
            if lrect.contains(x, y) {
                let list_top = lrect.y + 84;
                if y >= list_top {
                    let row = ((y - list_top) / 30) as usize;
                    let matches = self.matching_launcher_apps();
                    if let Some((_, exe_path)) = matches.get(row) {
                        let path_clone = exe_path.clone();
                        self.launcher_open = false;
                        let _ = self.exec_wbo(&path_clone, None);
                    }
                }
                return;
            } else {
                self.launcher_open = false;
            }
        }

        // 3. Taskbar running app tabs
        if y >= FB_HEIGHT as i32 - TASKBAR_H {
            let btn_y = FB_HEIGHT as i32 - TASKBAR_H + 6;
            let btn_h = TASKBAR_H - 12;
            if y >= btn_y && y < btn_y + btn_h {
                let mut running_ids: Vec<usize> = self.windows.iter().map(|w| w.id).collect();
                running_ids.sort_unstable();

                let mut bx = 12;
                for win_id in running_ids {
                    if bx + 148 > self.launcher_btn_rect().x - 140 {
                        break;
                    }
                    if x >= bx && x < bx + 142 {
                        let is_top = self
                            .windows
                            .last()
                            .map(|w| w.id == win_id && !w.minimized)
                            .unwrap_or(false);
                        if is_top {
                            if let Some(w) = self.windows.iter_mut().find(|w| w.id == win_id) {
                                w.minimized = true;
                            }
                        } else {
                            self.focus_window(win_id);
                        }
                        return;
                    }
                    bx += 148;
                }
            }
            return;
        }

        // 4. Hit-test visible open windows from top to bottom
        let mut hit: Option<(usize, WboSubsystem, (i32, i32), Rect, bool, bool, bool, bool, bool)> =
            None;
        for win in self.windows.iter().rev() {
            if win.minimized {
                continue;
            }
            if win.frame_rect().contains(x, y) {
                hit = Some((
                    win.id,
                    win.subsystem,
                    (win.x, win.y),
                    win.client_rect(),
                    win.close_btn_rect().contains(x, y),
                    win.max_btn_rect().contains(x, y),
                    win.min_btn_rect().contains(x, y),
                    win.titlebar_rect().contains(x, y),
                    win.resize_grip_rect().contains(x, y),
                ));
                break;
            }
        }

        if let Some((
            id,
            subsystem,
            win_origin,
            client,
            close_hit,
            max_hit,
            min_hit,
            title_hit,
            resize_hit,
        )) = hit
        {
            if close_hit {
                if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
                    self.windows.remove(pos);
                }
                return;
            }

            if min_hit {
                if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
                    w.minimized = true;
                }
                return;
            }

            if max_hit {
                self.focus_window(id);
                if let Some(w) = self.windows.last_mut() {
                    if w.maximized {
                        if let Some(sb) = w.saved_bounds {
                            w.x = sb.x;
                            w.y = sb.y;
                            w.w = sb.w;
                            w.h = sb.h;
                        }
                        w.maximized = false;
                    } else {
                        w.saved_bounds = Some(w.frame_rect());
                        w.x = 0;
                        w.y = 0;
                        w.w = FB_WIDTH as i32;
                        w.h = FB_HEIGHT as i32 - TASKBAR_H;
                        w.maximized = true;
                    }
                }
                return;
            }

            self.focus_window(id);

            if resize_hit {
                self.resizing_win_id = Some(id);
                return;
            }

            if title_hit {
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
            return;
        }

        // 5. Desktop shortcut icons
        for (idx, (_, exe_path, _)) in self.desktop_shortcuts().iter().enumerate() {
            let icon_rect = Rect::new(18, 22 + (idx as i32) * 78, 76, 66);
            if icon_rect.contains(x, y) {
                let _ = self.exec_wbo(exe_path, None);
                return;
            }
        }
    }

    fn handle_files_click(&mut self, x: i32, y: i32, client: Rect) {
        let new_file_btn = Rect::new(client.x + client.w - 184, client.y + 6, 58, 22);
        let new_dir_btn = Rect::new(client.x + client.w - 120, client.y + 6, 54, 22);
        let up_btn = Rect::new(client.x + client.w - 60, client.y + 6, 50, 22);

        if new_file_btn.contains(x, y) {
            let mut idx = 1;
            loop {
                let candidate = if idx == 1 {
                    format!("{}/new_app.wbml", self.explorer_dir)
                } else {
                    format!("{}/new_app_{}.wbml", self.explorer_dir, idx)
                };
                if !self.fs.exists(&candidate) {
                    let starter = "app \"NewApp\" 320 210 {\n    state {\n        n = 0\n    }\n    col pad=16 gap=12 {\n        text \"Value: {n}\" size=lg color=#58F284\n        btn \"Add 1\" style=primary { n = n + 1 }\n    }\n}\n";
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

        let preview_y = client.y + client.h - 110;
        let del_btn = Rect::new(client.x + client.w - 64, preview_y + 8, 48, 22);
        let action_btn = Rect::new(client.x + client.w - 124, preview_y + 8, 54, 22);

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

        let rel_y = y - (client.y + 36);
        if rel_y >= 0 && y < preview_y - 4 {
            let row_idx = (rel_y / 24) as usize;
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
        let save_btn = Rect::new(client.x + client.w - 248, client.y + 5, 62, 22);
        let build_btn = Rect::new(client.x + client.w - 180, client.y + 5, 108, 22);
        let run_btn = Rect::new(client.x + client.w - 66, client.y + 5, 56, 22);

        if save_btn.contains(x, y) {
            self.save_active_editor();
            return;
        }

        if build_btn.contains(x, y) {
            self.build_and_run_active_editor(false);
            return;
        }

        if run_btn.contains(x, y) {
            self.build_and_run_active_editor(true);
            return;
        }

        // Click inside code area to position cursor (accounting for scroll_x & scroll_y)
        let text_top = client.y + 38;
        let text_left = client.x + 52;
        if y >= text_top && y < client.y + client.h - 24 && x >= text_left {
            if let Some(ed) = self.windows.last_mut().and_then(|w| w.editor.as_mut()) {
                let clicked_row = ((y - text_top) / (CHAR_H as i32)) as usize;
                let clicked_col = ed.scroll_x + ((x - text_left) / (CHAR_W as i32)) as usize;
                let target_line = (ed.scroll_y + clicked_row).min(ed.lines.len().saturating_sub(1));
                ed.cursor_line = target_line;
                ed.cursor_col = clicked_col.min(char_len(&ed.lines[target_line]));
            }
        }
    }

    fn dispatch_runtime_click(
        rt: &mut DeclarativeRuntime,
        bounds: Rect,
        x: i32,
        y: i32,
        show_guides: bool,
    ) {
        let rendered = rt.layout(bounds, show_guides);
        rt.focused_input = None;
        for w in rendered.iter().rev() {
            if !w.rect.contains(x, y) {
                continue;
            }
            match &w.kind {
                RenderedWidgetKind::Button { script, .. } => {
                    let s = script.clone();
                    rt.run_script(&s);
                    break;
                }
                RenderedWidgetKind::Input { bind, .. } => {
                    rt.focused_input = Some(bind.clone());
                    break;
                }
                RenderedWidgetKind::Toggle { bind, checked, .. } => {
                    rt.vars.insert(bind.clone(), Value::Bool(!checked));
                    break;
                }
                RenderedWidgetKind::Slider { bind, min, max, .. } => {
                    let ratio = ((x - w.rect.x).clamp(0, w.rect.w) as i64) * (*max - *min);
                    let new_val = *min + ratio / (w.rect.w.max(1) as i64);
                    rt.vars.insert(bind.clone(), Value::Int(new_val));
                    break;
                }
                _ => {}
            }
        }
    }

    fn handle_declarative_click(&mut self, x: i32, y: i32, client: Rect) {
        if let Some(win) = self.windows.last_mut() {
            if let Some(rt) = win.runtime.as_mut() {
                Self::dispatch_runtime_click(rt, client, x, y, false);
            }
        }
    }

    fn flush_runtime_effects(&mut self) {
        let mut effects = Vec::new();
        for w in &mut self.windows {
            if let Some(rt) = w.runtime.as_mut() {
                effects.append(&mut rt.pending_effects);
            }
        }

        for eff in effects {
            match eff.kind.as_str() {
                "open" => {
                    let candidate = if eff.arg1.contains('/') {
                        eff.arg1
                    } else if eff.arg1.ends_with(".wbo") {
                        format!("C:/WebOS/bin/Apps/{}", eff.arg1)
                    } else {
                        format!("C:/WebOS/bin/Apps/{}.wbo", eff.arg1)
                    };
                    let _ = self.exec_wbo(&candidate, None);
                }
                "fs_write" => {
                    let _ = self.fs.write_str(&eff.arg1, &eff.arg2);
                }
                _ => {}
            }
        }
    }

    pub fn on_mouse_up(&mut self, _x: i32, _y: i32, button: u32) {
        if button == 0 {
            self.dragging_win_id = None;
            self.resizing_win_id = None;
        }
    }

    pub fn on_key(&mut self, code: u32, hw: &dyn Hardware) {
        if code == 1005 {
            self.toggle_launcher();
            return;
        }

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

        let focused_subsys = self
            .windows
            .last()
            .filter(|w| !w.minimized)
            .map(|w| w.subsystem);

        match focused_subsys {
            Some(WboSubsystem::Editor) => self.on_editor_key(code),
            Some(WboSubsystem::Terminal) => self.on_terminal_key(code, hw),
            Some(WboSubsystem::DeclarativeApp) => self.on_declarative_key(code),
            _ => {}
        }
    }

    fn key_code_to_name(code: u32) -> Option<String> {
        match code {
            8 => Some("Backspace".into()),
            9 => Some("Tab".into()),
            13 => Some("Enter".into()),
            27 => Some("Escape".into()),
            1001 => Some("ArrowUp".into()),
            1002 => Some("ArrowDown".into()),
            1003 => Some("ArrowLeft".into()),
            1004 => Some("ArrowRight".into()),
            32..=126 => char::from_u32(code).map(|c| c.to_string()),
            _ => None,
        }
    }

    fn on_declarative_key(&mut self, code: u32) {
        if let Some(kname) = Self::key_code_to_name(code) {
            if let Some(win) = self.windows.last_mut() {
                if let Some(rt) = win.runtime.as_mut() {
                    rt.on_key_event(&kname);
                }
            }
        }
    }

    fn on_editor_key(&mut self, code: u32) {
        if code == 2001 {
            self.save_active_editor();
            return;
        }
        if code == 2002 {
            self.build_and_run_active_editor(true);
            return;
        }

        let (visible_rows, visible_cols) = self
            .windows
            .last()
            .map(|w| {
                let rows = ((w.h - TITLEBAR_H - 68).max(40) as usize) / CHAR_H;
                let cols = ((w.w - 64).max(40) as usize) / CHAR_W;
                (rows, cols)
            })
            .unwrap_or((18, 70));

        let Some(ed) = self.windows.last_mut().and_then(|w| w.editor.as_mut()) else {
            return;
        };

        match code {
            // Vertical Wheel Up / Down
            1010 => {
                ed.scroll_y = ed.scroll_y.saturating_sub(2);
                return;
            }
            1011 => {
                if ed.scroll_y + 2 < ed.lines.len() {
                    ed.scroll_y += 2;
                }
                return;
            }
            // Horizontal Wheel Left / Right
            1012 => {
                ed.scroll_x = ed.scroll_x.saturating_sub(4);
                return;
            }
            1013 => {
                ed.scroll_x += 4;
                return;
            }
            // ArrowUp
            1001 => {
                if ed.cursor_line > 0 {
                    ed.cursor_line -= 1;
                    ed.cursor_col = ed.cursor_col.min(char_len(&ed.lines[ed.cursor_line]));
                }
            }
            // ArrowDown
            1002 => {
                if ed.cursor_line + 1 < ed.lines.len() {
                    ed.cursor_line += 1;
                    ed.cursor_col = ed.cursor_col.min(char_len(&ed.lines[ed.cursor_line]));
                }
            }
            // ArrowLeft
            1003 => {
                if ed.cursor_col > 0 {
                    ed.cursor_col -= 1;
                } else if ed.cursor_line > 0 {
                    ed.cursor_line -= 1;
                    ed.cursor_col = char_len(&ed.lines[ed.cursor_line]);
                }
            }
            // ArrowRight
            1004 => {
                if ed.cursor_col < char_len(&ed.lines[ed.cursor_line]) {
                    ed.cursor_col += 1;
                } else if ed.cursor_line + 1 < ed.lines.len() {
                    ed.cursor_line += 1;
                    ed.cursor_col = 0;
                }
            }
            1008 => ed.cursor_col = 0,
            1009 => ed.cursor_col = char_len(&ed.lines[ed.cursor_line]),
            // Enter
            13 => {
                let b_idx = char_to_byte_idx(&ed.lines[ed.cursor_line], ed.cursor_col);
                let tail = ed.lines[ed.cursor_line].split_off(b_idx);
                ed.cursor_line += 1;
                ed.lines.insert(ed.cursor_line, tail);
                ed.cursor_col = 0;
            }
            // Backspace
            8 => {
                if ed.cursor_col > 0 {
                    ed.cursor_col -= 1;
                    let b_idx = char_to_byte_idx(&ed.lines[ed.cursor_line], ed.cursor_col);
                    ed.lines[ed.cursor_line].remove(b_idx);
                } else if ed.cursor_line > 0 {
                    let current = ed.lines.remove(ed.cursor_line);
                    ed.cursor_line -= 1;
                    ed.cursor_col = char_len(&ed.lines[ed.cursor_line]);
                    ed.lines[ed.cursor_line].push_str(&current);
                }
            }
            // Delete
            127 => {
                if ed.cursor_col < char_len(&ed.lines[ed.cursor_line]) {
                    let b_idx = char_to_byte_idx(&ed.lines[ed.cursor_line], ed.cursor_col);
                    ed.lines[ed.cursor_line].remove(b_idx);
                } else if ed.cursor_line + 1 < ed.lines.len() {
                    let next = ed.lines.remove(ed.cursor_line + 1);
                    ed.lines[ed.cursor_line].push_str(&next);
                }
            }
            // Tab -> 4 spaces
            9 => {
                let b_idx = char_to_byte_idx(&ed.lines[ed.cursor_line], ed.cursor_col);
                ed.lines[ed.cursor_line].insert_str(b_idx, "    ");
                ed.cursor_col += 4;
            }
            // Printable ASCII
            32..=126 => {
                if let Some(ch) = char::from_u32(code) {
                    let b_idx = char_to_byte_idx(&ed.lines[ed.cursor_line], ed.cursor_col);
                    ed.lines[ed.cursor_line].insert(b_idx, ch);
                    ed.cursor_col += 1;
                }
            }
            _ => {}
        }

        if ed.cursor_line < ed.scroll_y {
            ed.scroll_y = ed.cursor_line;
        } else if ed.cursor_line >= ed.scroll_y + visible_rows.max(1) {
            ed.scroll_y = ed.cursor_line + 1 - visible_rows.max(1);
        }

        if ed.cursor_col < ed.scroll_x {
            ed.scroll_x = ed.cursor_col;
        } else if ed.cursor_col >= ed.scroll_x + visible_cols.max(1) {
            ed.scroll_x = ed.cursor_col + 1 - visible_cols.max(1);
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
                    self.term_bg = rgba(11, 15, 23);
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

    fn subsys_icon(sub: WboSubsystem) -> IconKind {
        match sub {
            WboSubsystem::Terminal => IconKind::Terminal,
            WboSubsystem::Files => IconKind::Files,
            WboSubsystem::SysMon => IconKind::SysMon,
            WboSubsystem::Editor => IconKind::Editor,
            WboSubsystem::DeclarativeApp => IconKind::AppBinary,
        }
    }

    fn syntax_color_for_line(line: &str) -> u32 {
        let t = line.trim_start();
        if t.starts_with('#') || t.starts_with("//") {
            rgba(110, 118, 129)
        } else if t.starts_with("app ")
            || t.starts_with("state")
            || t.starts_with("on_tick")
            || t.starts_with("on_key")
            || t.starts_with("on_init")
        {
            rgba(210, 168, 255)
        } else if t.starts_with("col")
            || t.starts_with("row")
            || t.starts_with("grid")
            || t.starts_with("card")
            || t.starts_with("canvas")
        {
            rgba(121, 192, 255)
        } else if t.starts_with("btn ")
            || t.starts_with("button ")
            || t.starts_with("text ")
            || t.starts_with("input")
            || t.starts_with("slider")
            || t.starts_with("toggle")
            || t.starts_with("progress")
        {
            rgba(126, 231, 135)
        } else if t.starts_with("if ") || t.starts_with("for ") || t.starts_with("else") {
            rgba(255, 166, 87)
        } else {
            rgba(230, 237, 243)
        }
    }

    fn render_declarative_widgets(
        canvas: &mut Canvas,
        widgets: &[RenderedWidget],
        focused_input: Option<&str>,
        mx: i32,
        my: i32,
        blink_on: bool,
    ) {
        for w in widgets {
            let hovered = w.rect.contains(mx, my);
            match &w.kind {
                RenderedWidgetKind::GuideBox => {
                    canvas.stroke_rect(w.rect, rgba(42, 62, 92));
                }
                RenderedWidgetKind::Card { bg, border, radius } => {
                    canvas.fill_rounded_rect(w.rect, *radius, *bg);
                    canvas.stroke_rounded_rect(w.rect, *radius, *border);
                }
                RenderedWidgetKind::Text {
                    text,
                    scale,
                    align,
                    color,
                } => {
                    let char_step = 5 * *scale + (*scale).max(1);
                    let text_w = (text.chars().count() as i32) * char_step;
                    let tx = match align {
                        TextAlign::Left => w.rect.x,
                        TextAlign::Center => w.rect.x + ((w.rect.w - text_w) / 2).max(0),
                        TextAlign::Right => w.rect.x + (w.rect.w - text_w).max(0),
                    };
                    let ty = w.rect.y + ((w.rect.h - 7 * scale) / 2).max(0);
                    canvas.draw_text_scaled(tx, ty, text, *color, *scale);
                }
                RenderedWidgetKind::Button { text, style, .. } => {
                    let is_primary = matches!(style, ButtonStyle::Primary | ButtonStyle::Accent);
                    let text_col = if *style == ButtonStyle::Danger {
                        rgba(255, 123, 114)
                    } else {
                        rgba(255, 255, 255)
                    };
                    canvas.draw_win10_button(w.rect, text, hovered, is_primary, text_col);
                }
                RenderedWidgetKind::Input {
                    bind,
                    text,
                    placeholder,
                } => {
                    let is_active = focused_input == Some(bind.as_str());
                    let bg = rgba(10, 14, 20);
                    canvas.fill_rect(w.rect, bg);
                    canvas.stroke_rect(
                        w.rect,
                        if is_active {
                            rgba(0, 120, 215)
                        } else {
                            rgba(62, 70, 82)
                        },
                    );
                    let display = if text.is_empty() && !is_active {
                        placeholder.as_str()
                    } else {
                        text.as_str()
                    };
                    let fg = if text.is_empty() && !is_active {
                        rgba(110, 118, 129)
                    } else {
                        rgba(230, 237, 243)
                    };
                    canvas.draw_text(w.rect.x + 8, w.rect.y + 8, display, fg, bg);
                    if is_active && blink_on {
                        let cx = w.rect.x + 8 + (text.chars().count() * CHAR_W) as i32;
                        canvas.fill_rect(Rect::new(cx, w.rect.y + 7, 2, 17), rgba(0, 120, 215));
                    }
                }
                RenderedWidgetKind::Slider {
                    min, max, current, ..
                } => {
                    let track =
                        Rect::new(w.rect.x + 4, w.rect.y + w.rect.h / 2 - 3, w.rect.w - 8, 6);
                    canvas.fill_rect(track, rgba(13, 17, 23));
                    let span = (*max - *min).max(1);
                    let fill_w =
                        (((*current - *min).clamp(0, span) * (track.w as i64)) / span) as i32;
                    canvas.fill_rect(
                        Rect::new(track.x, track.y, fill_w.max(4), track.h),
                        rgba(0, 120, 215),
                    );
                    canvas.fill_circle(track.x + fill_w, track.y + 3, 7, rgba(240, 246, 252));
                }
                RenderedWidgetKind::Toggle { label, checked, .. } => {
                    let pill = Rect::new(w.rect.x, w.rect.y + 4, 38, 20);
                    let pill_col = if *checked {
                        rgba(0, 120, 215)
                    } else {
                        rgba(48, 54, 61)
                    };
                    canvas.fill_rounded_rect(pill, 10, pill_col);
                    let knob_x = if *checked { pill.x + 28 } else { pill.x + 10 };
                    canvas.fill_circle(knob_x, pill.y + 10, 7, rgba(255, 255, 255));
                    canvas.draw_text(
                        w.rect.x + 48,
                        w.rect.y + 6,
                        label,
                        rgba(230, 237, 243),
                        pill_col,
                    );
                }
                RenderedWidgetKind::Progress { ratio_pct, color } => {
                    canvas.fill_rect(w.rect, rgba(13, 17, 23));
                    canvas.stroke_rect(w.rect, rgba(62, 70, 82));
                    let fw = ((w.rect.w - 4) * ratio_pct) / 100;
                    if fw > 0 {
                        canvas.fill_rect(
                            Rect::new(w.rect.x + 2, w.rect.y + 2, fw, w.rect.h - 4),
                            *color,
                        );
                    }
                }
                RenderedWidgetKind::CanvasBox { bg, cmds } => {
                    canvas.fill_rect(w.rect, *bg);
                    canvas.stroke_rect(w.rect, rgba(62, 70, 82));
                    for &(op, a, b, c, d, col) in cmds {
                        match op {
                            0 => canvas.fill_rect(w.rect, col),
                            1 => canvas.fill_rect(Rect::new(w.rect.x + a, w.rect.y + b, c, d), col),
                            2 => canvas.fill_circle(w.rect.x + a, w.rect.y + b, c, col),
                            3 => canvas.draw_line(
                                w.rect.x + a,
                                w.rect.y + b,
                                w.rect.x + c,
                                w.rect.y + d,
                                col,
                            ),
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    pub fn render(&mut self, hw: &dyn Hardware) {
        let now_ms = hw.uptime_ms();
        let elapsed_ms = now_ms.saturating_sub(self.boot_time_ms);
        let blink_on = (now_ms / 500) % 2 == 0;

        // Advance timers for running DeclarativeApps
        for win in &mut self.windows {
            if let Some(rt) = win.runtime.as_mut() {
                rt.tick(now_ms);
            }
        }
        self.flush_runtime_effects();

        let top_win_id = self
            .windows
            .iter()
            .rev()
            .find(|w| !w.minimized)
            .map(|w| w.id);

        let mut taskbar_items: Vec<(usize, String, WboSubsystem, bool)> = self
            .windows
            .iter()
            .map(|w| (w.id, w.title.clone(), w.subsystem, Some(w.id) == top_win_id))
            .collect();
        taskbar_items.sort_by_key(|(id, _, _, _)| *id);

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
        let shortcuts = self.desktop_shortcuts();
        let mx = self.mouse_x;
        let my = self.mouse_y;

        let mut canvas = Canvas::new(&mut self.framebuffer);

        // 1. Desktop Wallpaper & Shortcut Icons
        canvas.draw_wallpaper();
        for (idx, (label, _, icon)) in shortcuts.iter().enumerate() {
            let srect = Rect::new(18, 22 + (idx as i32) * 78, 76, 66);
            if srect.contains(mx, my) {
                canvas.fill_rect_alpha(srect, rgba(0, 120, 215), 40);
                canvas.stroke_rect(srect, rgba(0, 120, 215));
            }
            canvas.draw_icon(srect.x + 30, srect.y + 12, *icon);
            let tw = (label.chars().count() as i32) * 6;
            canvas.draw_text_scaled(
                srect.x + ((srect.w - tw) / 2).max(2),
                srect.y + 38,
                label,
                rgba(220, 228, 238),
                1,
            );
        }

        // 2. Render Visible Running Windows
        let total_windows = self.windows.len();
        for win in &mut self.windows {
            if win.minimized {
                continue;
            }
            let is_focused = Some(win.id) == top_win_id;
            let frame = win.frame_rect();
            let close_btn = win.close_btn_rect();
            let max_btn = win.max_btn_rect();
            let min_btn = win.min_btn_rect();
            let client = win.client_rect();

            let hover_btn = if close_btn.contains(mx, my) {
                3
            } else if max_btn.contains(mx, my) {
                2
            } else if min_btn.contains(mx, my) {
                1
            } else {
                0
            };

            canvas.draw_win10_window_chrome(
                frame,
                &win.title,
                Self::subsys_icon(win.subsystem),
                is_focused,
                win.maximized,
                hover_btn,
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
                                    client.x + 10 + (c * CHAR_W) as i32,
                                    client.y + 8 + (r * CHAR_H) as i32,
                                    ch,
                                    self.term_fg,
                                    self.term_bg,
                                );
                            }
                        }
                    }
                    if is_focused
                        && blink_on
                        && self.cursor_x < TERM_COLS
                        && self.cursor_y < TERM_ROWS
                    {
                        canvas.fill_rect(
                            Rect::new(
                                client.x + 10 + (self.cursor_x * CHAR_W) as i32,
                                client.y + 8 + (self.cursor_y * CHAR_H) as i32,
                                9,
                                15,
                            ),
                            self.term_fg,
                        );
                    }
                }
                WboSubsystem::Files => {
                    let panel_bg = rgba(15, 20, 29);
                    canvas.fill_rect(client, panel_bg);

                    canvas.draw_icon(client.x + 10, client.y + 9, IconKind::Folder);
                    canvas.draw_text(
                        client.x + 32,
                        client.y + 9,
                        &self.explorer_dir,
                        rgba(121, 192, 255),
                        panel_bg,
                    );

                    let new_file_btn = Rect::new(client.x + client.w - 184, client.y + 6, 58, 22);
                    let new_dir_btn = Rect::new(client.x + client.w - 120, client.y + 6, 54, 22);
                    let up_btn = Rect::new(client.x + client.w - 60, client.y + 6, 50, 22);

                    for (btn, lbl) in [
                        (new_file_btn, "+File"),
                        (new_dir_btn, "+Dir"),
                        (up_btn, "Up"),
                    ] {
                        canvas.draw_win10_button(
                            btn,
                            lbl,
                            btn.contains(mx, my),
                            false,
                            rgba(230, 237, 243),
                        );
                    }

                    for (idx, (name, is_dir, size)) in dir_entries.iter().enumerate() {
                        let row_y = client.y + 36 + (idx as i32) * 24;
                        let full = if self.explorer_dir == "C:/" {
                            format!("C:/{}", name)
                        } else {
                            format!("{}/{}", self.explorer_dir, name)
                        };
                        let row_rect = Rect::new(client.x + 8, row_y - 2, client.w - 16, 22);
                        let is_selected = self.selected_path.as_deref() == Some(full.as_str());
                        let row_bg = if is_selected {
                            rgba(0, 90, 158)
                        } else if row_rect.contains(mx, my) {
                            rgba(28, 36, 48)
                        } else {
                            panel_bg
                        };

                        canvas.fill_rect(row_rect, row_bg);
                        let icon = if *is_dir {
                            IconKind::Folder
                        } else if name.ends_with(".wbo") {
                            IconKind::AppBinary
                        } else if name.ends_with(".wbml") {
                            IconKind::Blueprint
                        } else if name.ends_with(".wbsh") {
                            IconKind::Script
                        } else {
                            IconKind::FileDoc
                        };
                        canvas.draw_icon(row_rect.x + 6, row_y + 1, icon);

                        let label = if *is_dir {
                            format!("{}/", name)
                        } else {
                            format!("{:<20} {:>5} B", name, size)
                        };
                        let fg = if *is_dir {
                            rgba(255, 194, 92)
                        } else if name.ends_with(".wbo") || name.ends_with(".wbsh") {
                            rgba(126, 231, 135)
                        } else if name.ends_with(".wbml") {
                            rgba(121, 192, 255)
                        } else {
                            rgba(230, 237, 243)
                        };
                        canvas.draw_text(row_rect.x + 28, row_y + 2, &label, fg, row_bg);
                    }

                    let preview_y = client.y + client.h - 110;
                    let preview_rect = Rect::new(client.x + 8, preview_y, client.w - 16, 102);
                    canvas.fill_rect(preview_rect, rgba(11, 15, 21));
                    canvas.stroke_rect(preview_rect, rgba(48, 54, 61));

                    if let Some((body, is_exec)) = &selected_preview {
                        let del_btn = Rect::new(client.x + client.w - 64, preview_y + 8, 48, 22);
                        let action_btn =
                            Rect::new(client.x + client.w - 124, preview_y + 8, 54, 22);

                        canvas.draw_win10_button(
                            action_btn,
                            if *is_exec { "Run" } else { "Edit" },
                            action_btn.contains(mx, my),
                            true,
                            rgba(255, 255, 255),
                        );

                        canvas.draw_win10_button(
                            del_btn,
                            "Del",
                            del_btn.contains(mx, my),
                            false,
                            rgba(255, 123, 114),
                        );

                        for (line_idx, line_txt) in body.lines().take(3).enumerate() {
                            let clipped: String = line_txt.chars().take(28).collect();
                            canvas.draw_text(
                                preview_rect.x + 10,
                                preview_rect.y + 12 + (line_idx as i32) * 22,
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

                    canvas.fill_rect(Rect::new(client.x, client.y, client.w, 32), bar_bg);
                    if let Some(ed) = win.editor.as_mut() {
                        canvas.draw_icon(client.x + 10, client.y + 8, IconKind::Blueprint);
                        canvas.draw_text(
                            client.x + 32,
                            client.y + 8,
                            &ed.file_path,
                            rgba(121, 192, 255),
                            bar_bg,
                        );

                        let save_btn = Rect::new(client.x + client.w - 248, client.y + 5, 62, 22);
                        let build_btn = Rect::new(client.x + client.w - 180, client.y + 5, 108, 22);
                        let run_btn = Rect::new(client.x + client.w - 66, client.y + 5, 56, 22);

                        canvas.draw_win10_button(
                            save_btn,
                            "Save",
                            save_btn.contains(mx, my),
                            false,
                            rgba(230, 237, 243),
                        );
                        canvas.draw_win10_button(
                            build_btn,
                            "Build .wbo",
                            build_btn.contains(mx, my),
                            false,
                            rgba(126, 231, 135),
                        );
                        canvas.draw_win10_button(
                            run_btn,
                            "Run",
                            run_btn.contains(mx, my),
                            true,
                            rgba(255, 255, 255),
                        );

                        let visible_rows = ((client.h - 60).max(20) as usize) / CHAR_H;
                        let visible_cols = ((client.w - 58).max(40) as usize) / CHAR_W;

                        // Line Number Gutter
                        canvas.fill_rect(
                            Rect::new(client.x, client.y + 32, 46, client.h - 56),
                            rgba(16, 21, 29),
                        );

                        for row in 0..visible_rows {
                            let line_idx = ed.scroll_y + row;
                            if line_idx >= ed.lines.len() {
                                break;
                            }
                            let py = client.y + 38 + (row * CHAR_H) as i32;
                            if line_idx == ed.cursor_line {
                                canvas.fill_rect(
                                    Rect::new(
                                        client.x + 46,
                                        py - 2,
                                        client.w - 46,
                                        CHAR_H as i32,
                                    ),
                                    rgba(20, 27, 38),
                                );
                            }
                            let lnum = format!("{:>3}", line_idx + 1);
                            canvas.draw_text(
                                client.x + 6,
                                py,
                                &lnum,
                                if line_idx == ed.cursor_line {
                                    rgba(121, 192, 255)
                                } else {
                                    rgba(110, 118, 129)
                                },
                                rgba(16, 21, 29),
                            );

                            let col_fg = Self::syntax_color_for_line(&ed.lines[line_idx]);
                            let visible_slice: String = ed.lines[line_idx]
                                .chars()
                                .skip(ed.scroll_x)
                                .take(visible_cols)
                                .collect();
                            canvas.draw_text(client.x + 52, py, &visible_slice, col_fg, bg);
                        }

                        // Blinking Text Cursor (respecting scroll_x and scroll_y)
                        if is_focused
                            && blink_on
                            && ed.cursor_line >= ed.scroll_y
                            && ed.cursor_line < ed.scroll_y + visible_rows
                            && ed.cursor_col >= ed.scroll_x
                            && ed.cursor_col <= ed.scroll_x + visible_cols
                        {
                            let vis_row = ed.cursor_line - ed.scroll_y;
                            let vis_col = ed.cursor_col - ed.scroll_x;
                            let cx = client.x + 52 + (vis_col * CHAR_W) as i32;
                            let cy = client.y + 38 + (vis_row * CHAR_H) as i32;
                            canvas.fill_rect(Rect::new(cx, cy, 2, 16), rgba(88, 242, 132));
                        }

                        // Status Footer
                        let status_y = client.y + client.h - 24;
                        canvas.fill_rect(Rect::new(client.x, status_y, client.w, 24), bar_bg);
                        let st_col = if ed.status.starts_with("Syntax")
                            || ed.status.starts_with("Compile")
                            || ed.status.starts_with("Save error")
                        {
                            rgba(255, 123, 114)
                        } else {
                            rgba(126, 231, 135)
                        };
                        canvas.draw_text(client.x + 10, status_y + 4, &ed.status, st_col, bar_bg);

                        let pos_label = format!(
                            "Ln {}, Col {}",
                            ed.cursor_line + 1,
                            ed.cursor_col + 1
                        );
                        let pos_x =
                            client.x + client.w - (pos_label.chars().count() as i32 * CHAR_W as i32) - 14;
                        canvas.draw_text(
                            pos_x,
                            status_y + 4,
                            &pos_label,
                            rgba(139, 148, 158),
                            bar_bg,
                        );
                    }
                }
                WboSubsystem::SysMon => {
                    let panel_bg = rgba(15, 20, 29);
                    canvas.fill_rect(client, panel_bg);

                    let (used, total) = heap_stats();
                    let pct = ((used as f64 / total as f64) * 100.0).min(100.0);

                    canvas.draw_text(
                        client.x + 14,
                        client.y + 14,
                        &format!(
                            "Kernel Heap: {} / {} KiB ({:.1}%)",
                            used / 1024,
                            total / 1024,
                            pct
                        ),
                        rgba(230, 237, 243),
                        panel_bg,
                    );

                    let bar_rect = Rect::new(client.x + 14, client.y + 42, client.w - 28, 22);
                    canvas.fill_rect(bar_rect, rgba(10, 14, 20));
                    canvas.stroke_rect(bar_rect, rgba(48, 54, 61));
                    let fill_w = (((bar_rect.w - 4) as f64) * (pct / 100.0)) as i32;
                    canvas.fill_rect(
                        Rect::new(bar_rect.x + 2, bar_rect.y + 2, fill_w.max(6), bar_rect.h - 4),
                        rgba(0, 120, 215),
                    );

                    canvas.draw_text(
                        client.x + 14,
                        client.y + 82,
                        &format!("Running Processes: {}", total_windows),
                        rgba(139, 148, 158),
                        panel_bg,
                    );
                    canvas.draw_text(
                        client.x + 14,
                        client.y + 108,
                        "Exe Format: WBO\\x02 (WBML v2 Layout + VM)",
                        rgba(121, 192, 255),
                        panel_bg,
                    );
                }
                WboSubsystem::DeclarativeApp => {
                    let panel_bg = rgba(15, 20, 29);
                    canvas.fill_rect(client, panel_bg);

                    if let Some(rt) = win.runtime.as_mut() {
                        let widgets = rt.layout(client, false);
                        Self::render_declarative_widgets(
                            &mut canvas,
                            &widgets,
                            rt.focused_input.as_deref(),
                            mx,
                            my,
                            blink_on,
                        );
                    }
                }
            }

            // Bottom-right window resize grip indicator
            if !win.maximized {
                canvas.reset_clip();
                let gx = win.x + win.w - 10;
                let gy = win.y + win.h - 10;
                canvas.draw_line(gx, gy + 6, gx + 6, gy, rgba(110, 118, 129));
                canvas.draw_line(gx + 3, gy + 6, gx + 6, gy + 3, rgba(110, 118, 129));
            }
        }

        // 3. Bottom-Right Searchable WebOS Launcher Popup
        if self.launcher_open {
            canvas.reset_clip();
            let l_bg = rgba(20, 26, 36);
            canvas.fill_rect(launcher_rect, l_bg);
            canvas.stroke_rect(launcher_rect, rgba(0, 120, 215));

            canvas.draw_icon(
                launcher_rect.x + 14,
                launcher_rect.y + 12,
                IconKind::WebOsLogo,
            );
            canvas.draw_text(
                launcher_rect.x + 38,
                launcher_rect.y + 11,
                "WebOS Applications",
                rgba(235, 241, 248),
                l_bg,
            );

            let search_box = Rect::new(
                launcher_rect.x + 14,
                launcher_rect.y + 40,
                launcher_rect.w - 28,
                30,
            );
            let s_bg = rgba(11, 15, 22);
            canvas.fill_rect(search_box, s_bg);
            canvas.stroke_rect(search_box, rgba(0, 120, 215));

            let prompt_txt = if self.launcher_query.is_empty() {
                "Type to filter apps..."
            } else {
                &self.launcher_query
            };
            let prompt_col = if self.launcher_query.is_empty() {
                rgba(110, 118, 129)
            } else {
                rgba(230, 237, 243)
            };
            canvas.draw_text(search_box.x + 10, search_box.y + 7, prompt_txt, prompt_col, s_bg);

            for (idx, (app_name, _)) in launcher_apps.iter().take(8).enumerate() {
                let row_y = launcher_rect.y + 84 + (idx as i32) * 30;
                let row_rect = Rect::new(launcher_rect.x + 14, row_y, launcher_rect.w - 28, 26);
                let row_bg = if idx == 0 || row_rect.contains(mx, my) {
                    rgba(0, 90, 158)
                } else {
                    l_bg
                };
                canvas.fill_rect(row_rect, row_bg);
                let icon = match app_name.as_str() {
                    "Terminal" => IconKind::Terminal,
                    "Files" => IconKind::Files,
                    "Editor" => IconKind::Editor,
                    "SysMon" => IconKind::SysMon,
                    _ => IconKind::AppBinary,
                };
                canvas.draw_icon(row_rect.x + 8, row_rect.y + 5, icon);
                canvas.draw_text(
                    row_rect.x + 32,
                    row_rect.y + 5,
                    app_name,
                    rgba(230, 237, 243),
                    row_bg,
                );
            }
        }

        // 4. Bottom Windows 10 Taskbar
        canvas.reset_clip();
        let tb_y = FB_HEIGHT as i32 - TASKBAR_H;
        let tb_bg = rgba(16, 20, 26);
        canvas.fill_rect(Rect::new(0, tb_y, FB_WIDTH as i32, TASKBAR_H), tb_bg);
        canvas.fill_rect(Rect::new(0, tb_y, FB_WIDTH as i32, 1), rgba(46, 54, 66));

        let mut bx = 12;
        for (_, title, subsys, is_focused) in taskbar_items {
            if bx + 148 > launcher_btn.x - 140 {
                break;
            }
            let btn_rect = Rect::new(bx, tb_y + 4, 142, TASKBAR_H - 8);
            let btn_bg = if is_focused {
                rgba(36, 44, 56)
            } else if btn_rect.contains(mx, my) {
                rgba(28, 34, 44)
            } else {
                rgba(22, 27, 35)
            };
            canvas.fill_rect(btn_rect, btn_bg);
            canvas.stroke_rect(
                btn_rect,
                if is_focused {
                    rgba(0, 120, 215)
                } else {
                    rgba(46, 54, 66)
                },
            );
            canvas.draw_icon(bx + 8, tb_y + 13, Self::subsys_icon(subsys));
            let short_title: String = title.chars().take(8).collect();
            canvas.draw_text(bx + 30, tb_y + 13, &short_title, rgba(235, 241, 248), btn_bg);
            if is_focused {
                canvas.fill_rect(
                    Rect::new(bx, tb_y + TASKBAR_H - 3, 142, 3),
                    rgba(0, 120, 215),
                );
            }
            bx += 148;
        }

        let total_secs = elapsed_ms / 1000;
        let clock_str = format!("UP {:02}:{:02}", total_secs / 60, total_secs % 60);
        canvas.draw_text(
            launcher_btn.x - 118,
            tb_y + 13,
            &clock_str,
            rgba(150, 162, 178),
            tb_bg,
        );

        let wb_bg = if self.launcher_open || launcher_btn.contains(mx, my) {
            rgba(24, 144, 240)
        } else {
            rgba(0, 120, 215)
        };
        canvas.fill_rect(launcher_btn, wb_bg);
        canvas.stroke_rect(launcher_btn, rgba(38, 148, 235));
        canvas.draw_icon(launcher_btn.x + 10, launcher_btn.y + 7, IconKind::WebOsLogo);
        canvas.draw_text(
            launcher_btn.x + 34,
            launcher_btn.y + 7,
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