use crate::fs::FileSystem;
use crate::gui::rgba;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub const WBO_MAGIC: &[u8; 4] = b"WBO\x01";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WboSubsystem {
    Terminal = 1,
    Files = 2,
    SysMon = 3,
    Editor = 4,
    DeclarativeApp = 5,
}

impl WboSubsystem {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::Terminal),
            2 => Some(Self::Files),
            3 => Some(Self::SysMon),
            4 => Some(Self::Editor),
            5 => Some(Self::DeclarativeApp),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub enum UiWidgetKind {
    Label,
    Button,
    Rect(u32),
}

#[derive(Clone)]
pub struct UiWidget {
    pub kind: UiWidgetKind,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub text: String,
    pub script: String,
}

#[derive(Clone)]
pub struct DeclarativeRuntime {
    pub vars: BTreeMap<String, i64>,
    pub widgets: Vec<UiWidget>,
}

impl DeclarativeRuntime {
    pub fn interpolate(&self, template: &str) -> String {
        let mut out = template.to_string();
        for (name, val) in &self.vars {
            let needle = format!("{{{}}}", name);
            if out.contains(&needle) {
                out = out.replace(&needle, &val.to_string());
            }
        }
        out
    }

    pub fn run_script(&mut self, script: &str) {
        for raw_stmt in script.split(';') {
            let stmt = raw_stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            if let Some(rest) = stmt.strip_prefix("if ") {
                if let (Some(brace_start), Some(brace_end)) = (rest.find('{'), rest.rfind('}')) {
                    let cond = rest[..brace_start].trim();
                    let body = &rest[brace_start + 1..brace_end];
                    if self.eval_condition(cond) {
                        self.run_script(body);
                    }
                }
            } else if let Some((lhs, rhs)) = stmt.split_once('=') {
                let var_name = lhs.trim();
                if !var_name.is_empty() {
                    let val = self.eval_expr(rhs.trim());
                    self.vars.insert(var_name.to_string(), val);
                }
            }
        }
    }

    fn eval_condition(&self, cond: &str) -> bool {
        if let Some((a, b)) = cond.split_once("==") {
            return self.eval_expr(a.trim()) == self.eval_expr(b.trim());
        }
        if let Some((a, b)) = cond.split_once("!=") {
            return self.eval_expr(a.trim()) != self.eval_expr(b.trim());
        }
        if let Some((a, b)) = cond.split_once('>') {
            return self.eval_expr(a.trim()) > self.eval_expr(b.trim());
        }
        if let Some((a, b)) = cond.split_once('<') {
            return self.eval_expr(a.trim()) < self.eval_expr(b.trim());
        }
        false
    }

    fn eval_expr(&self, expr: &str) -> i64 {
        let tokens: Vec<&str> = expr.split_whitespace().collect();
        if tokens.is_empty() {
            return 0;
        }
        let mut acc = self.eval_atom(tokens[0]);
        let mut i = 1;
        while i + 1 < tokens.len() {
            let op = tokens[i];
            let rhs = self.eval_atom(tokens[i + 1]);
            match op {
                "+" => acc = acc.saturating_add(rhs),
                "-" => acc = acc.saturating_sub(rhs),
                "*" => acc = acc.saturating_mul(rhs),
                "/" => {
                    if rhs != 0 {
                        acc /= rhs;
                    }
                }
                _ => {}
            }
            i += 2;
        }
        acc
    }

    fn eval_atom(&self, token: &str) -> i64 {
        if let Ok(num) = token.parse::<i64>() {
            num
        } else {
            *self.vars.get(token).unwrap_or(&0)
        }
    }
}

pub struct WboExecutable {
    pub subsystem: WboSubsystem,
    pub width: i32,
    pub height: i32,
    pub title: String,
    pub runtime: Option<DeclarativeRuntime>,
}

impl WboExecutable {
    /// Packs a system `.wbo` binary (`[0..4] "WBO\x01"`, `[4]` subsystem, `[5..7]` w, `[7..9]` h, `[9]` title_len, `[10..]` title)
    pub fn pack_system(subsystem: WboSubsystem, width: u16, height: u16, title: &str) -> Vec<u8> {
        let title_bytes = title.as_bytes();
        let title_len = title_bytes.len().min(255) as u8;
        let mut bin = Vec::with_capacity(10 + title_len as usize);
        bin.extend_from_slice(WBO_MAGIC);
        bin.push(subsystem as u8);
        bin.extend_from_slice(&width.to_le_bytes());
        bin.extend_from_slice(&height.to_le_bytes());
        bin.push(title_len);
        bin.extend_from_slice(&title_bytes[..title_len as usize]);
        bin
    }

    /// Packs a compiled `.wbml` declarative app into a binary `.wbo` executable
    pub fn pack_declarative(
        width: u16,
        height: u16,
        title: &str,
        runtime: &DeclarativeRuntime,
    ) -> Vec<u8> {
        let mut bin = Self::pack_system(WboSubsystem::DeclarativeApp, width, height, title);

        // Variables table
        let var_count = runtime.vars.len().min(u16::MAX as usize) as u16;
        bin.extend_from_slice(&var_count.to_le_bytes());
        for (name, val) in runtime.vars.iter().take(var_count as usize) {
            let nb = name.as_bytes();
            let nlen = nb.len().min(255) as u8;
            bin.push(nlen);
            bin.extend_from_slice(&nb[..nlen as usize]);
            bin.extend_from_slice(&val.to_le_bytes());
        }

        // Widgets table
        let widget_count = runtime.widgets.len().min(u16::MAX as usize) as u16;
        bin.extend_from_slice(&widget_count.to_le_bytes());
        for w in runtime.widgets.iter().take(widget_count as usize) {
            let (kind_tag, color_val) = match w.kind {
                UiWidgetKind::Label => (1u8, 0u32),
                UiWidgetKind::Button => (2u8, 0u32),
                UiWidgetKind::Rect(c) => (3u8, c),
            };
            bin.push(kind_tag);
            bin.extend_from_slice(&(w.x as i16).to_le_bytes());
            bin.extend_from_slice(&(w.y as i16).to_le_bytes());
            bin.extend_from_slice(&(w.w as i16).to_le_bytes());
            bin.extend_from_slice(&(w.h as i16).to_le_bytes());
            bin.extend_from_slice(&color_val.to_le_bytes());

            let tb = w.text.as_bytes();
            let tlen = tb.len().min(u16::MAX as usize) as u16;
            bin.extend_from_slice(&tlen.to_le_bytes());
            bin.extend_from_slice(&tb[..tlen as usize]);

            let sb = w.script.as_bytes();
            let slen = sb.len().min(u16::MAX as usize) as u16;
            bin.extend_from_slice(&slen.to_le_bytes());
            bin.extend_from_slice(&sb[..slen as usize]);
        }

        bin
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 10 || &bytes[0..4] != WBO_MAGIC {
            return Err("Invalid .wbo binary header (expected magic WBO\\x01)");
        }

        let subsystem =
            WboSubsystem::from_u8(bytes[4]).ok_or("Unsupported .wbo subsystem ID")?;
        let width = u16::from_le_bytes([bytes[5], bytes[6]]) as i32;
        let height = u16::from_le_bytes([bytes[7], bytes[8]]) as i32;
        let title_len = bytes[9] as usize;

        if bytes.len() < 10 + title_len {
            return Err("Truncated .wbo title section");
        }

        let title = core::str::from_utf8(&bytes[10..10 + title_len])
            .map_err(|_| "Invalid UTF-8 in .wbo title")?
            .to_string();

        let mut runtime = None;
        if subsystem == WboSubsystem::DeclarativeApp {
            let mut cur = 10 + title_len;
            if cur + 2 > bytes.len() {
                return Err("Truncated .wbo variable table");
            }
            let var_count = u16::from_le_bytes([bytes[cur], bytes[cur + 1]]) as usize;
            cur += 2;

            let mut vars = BTreeMap::new();
            for _ in 0..var_count {
                if cur >= bytes.len() {
                    return Err("Truncated .wbo variable entry");
                }
                let nlen = bytes[cur] as usize;
                cur += 1;
                if cur + nlen + 8 > bytes.len() {
                    return Err("Truncated .wbo variable data");
                }
                let name = core::str::from_utf8(&bytes[cur..cur + nlen])
                    .map_err(|_| "Invalid var name")?
                    .to_string();
                cur += nlen;
                let mut val_bytes = [0u8; 8];
                val_bytes.copy_from_slice(&bytes[cur..cur + 8]);
                cur += 8;
                vars.insert(name, i64::from_le_bytes(val_bytes));
            }

            if cur + 2 > bytes.len() {
                return Err("Truncated .wbo widget table");
            }
            let widget_count = u16::from_le_bytes([bytes[cur], bytes[cur + 1]]) as usize;
            cur += 2;

            let mut widgets = Vec::with_capacity(widget_count);
            for _ in 0..widget_count {
                if cur + 15 > bytes.len() {
                    return Err("Truncated .wbo widget header");
                }
                let kind_tag = bytes[cur];
                let x = i16::from_le_bytes([bytes[cur + 1], bytes[cur + 2]]) as i32;
                let y = i16::from_le_bytes([bytes[cur + 3], bytes[cur + 4]]) as i32;
                let w = i16::from_le_bytes([bytes[cur + 5], bytes[cur + 6]]) as i32;
                let h = i16::from_le_bytes([bytes[cur + 7], bytes[cur + 8]]) as i32;
                let color = u32::from_le_bytes([
                    bytes[cur + 9],
                    bytes[cur + 10],
                    bytes[cur + 11],
                    bytes[cur + 12],
                ]);
                let tlen = u16::from_le_bytes([bytes[cur + 13], bytes[cur + 14]]) as usize;
                cur += 15;

                if cur + tlen + 2 > bytes.len() {
                    return Err("Truncated .wbo widget text");
                }
                let text = core::str::from_utf8(&bytes[cur..cur + tlen])
                    .map_err(|_| "Invalid widget text")?
                    .to_string();
                cur += tlen;

                let slen = u16::from_le_bytes([bytes[cur], bytes[cur + 1]]) as usize;
                cur += 2;
                if cur + slen > bytes.len() {
                    return Err("Truncated .wbo widget script");
                }
                let script = core::str::from_utf8(&bytes[cur..cur + slen])
                    .map_err(|_| "Invalid widget script")?
                    .to_string();
                cur += slen;

                let kind = match kind_tag {
                    1 => UiWidgetKind::Label,
                    2 => UiWidgetKind::Button,
                    3 => UiWidgetKind::Rect(color),
                    _ => UiWidgetKind::Label,
                };

                widgets.push(UiWidget {
                    kind,
                    x,
                    y,
                    w,
                    h,
                    text,
                    script,
                });
            }

            runtime = Some(DeclarativeRuntime { vars, widgets });
        }

        Ok(Self {
            subsystem,
            width: width.max(240),
            height: height.max(150),
            title,
            runtime,
        })
    }
}

fn color_from_name(name: &str) -> u32 {
    match name {
        "dark" => rgba(11, 15, 21),
        "blue" => rgba(31, 61, 97),
        "green" => rgba(35, 134, 54),
        "amber" => rgba(180, 110, 24),
        "cyan" => rgba(24, 118, 148),
        "red" => rgba(185, 45, 45),
        _ => rgba(28, 37, 52),
    }
}

fn extract_quoted(s: &str) -> Option<(&str, &str)> {
    let start = s.find('"')?;
    let rest = &s[start + 1..];
    let end = rest.find('"')?;
    Some((&rest[..end], rest[end + 1..].trim()))
}

/// Compiles `.wbml` declarative source text into `(AppTitle, WboBinaryBytes)`
pub fn compile_wbml_to_wbo(source: &str) -> Result<(String, Vec<u8>), String> {
    let mut title = String::from("Untitled App");
    let mut width: u16 = 380;
    let mut height: u16 = 240;
    let mut vars: BTreeMap<String, i64> = BTreeMap::new();
    let mut widgets: Vec<UiWidget> = Vec::new();

    // Join multi-line `{ ... }` blocks into single logical statements
    let mut logical_lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut brace_depth: i32 = 0;

    for raw in source.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        for ch in trimmed.chars() {
            if ch == '{' {
                brace_depth += 1;
            } else if ch == '}' {
                brace_depth -= 1;
            }
        }
        if !current.is_empty() {
            current.push(';');
            current.push(' ');
        }
        current.push_str(trimmed);
        if brace_depth <= 0 {
            logical_lines.push(current.clone());
            current.clear();
            brace_depth = 0;
        }
    }
    if !current.is_empty() {
        logical_lines.push(current);
    }

    for (line_idx, line) in logical_lines.iter().enumerate() {
        let line_num = line_idx + 1;
        if let Some(rest) = line.strip_prefix("app ") {
            let (t, after) = extract_quoted(rest)
                .ok_or_else(|| format!("Line {}: expected quoted app title", line_num))?;
            title = t.to_string();
            let nums: Vec<&str> = after.split_whitespace().collect();
            if nums.len() >= 2 {
                width = nums[0].parse::<u16>().unwrap_or(380).max(220);
                height = nums[1].parse::<u16>().unwrap_or(240).max(140);
            }
        } else if let Some(rest) = line.strip_prefix("var ") {
            let (name, val_str) = rest
                .split_once('=')
                .ok_or_else(|| format!("Line {}: expected '=' in var declaration", line_num))?;
            let val = val_str
                .trim()
                .trim_end_matches(';')
                .trim()
                .parse::<i64>()
                .map_err(|_| format!("Line {}: var must be an integer", line_num))?;
            vars.insert(name.trim().to_string(), val);
        } else if let Some(rest) = line.strip_prefix("label ") {
            let (text, _) = extract_quoted(rest)
                .ok_or_else(|| format!("Line {}: expected quoted label text", line_num))?;
            let prefix_end = rest.find('"').unwrap_or(0);
            let toks: Vec<&str> = rest[..prefix_end].split_whitespace().collect();
            if toks.len() < 3 {
                return Err(format!("Line {}: usage: label <id> <x> <y> \"text\"", line_num));
            }
            let x = toks[1].parse::<i32>().unwrap_or(16);
            let y = toks[2].parse::<i32>().unwrap_or(16);
            widgets.push(UiWidget {
                kind: UiWidgetKind::Label,
                x,
                y,
                w: 0,
                h: 0,
                text: text.to_string(),
                script: String::new(),
            });
        } else if let Some(rest) = line.strip_prefix("rect ") {
            let toks: Vec<&str> = rest.split_whitespace().collect();
            if toks.len() < 4 {
                return Err(format!("Line {}: usage: rect <x> <y> <w> <h> [color]", line_num));
            }
            let x = toks[0].parse::<i32>().unwrap_or(10);
            let y = toks[1].parse::<i32>().unwrap_or(10);
            let w = toks[2].parse::<i32>().unwrap_or(100);
            let h = toks[3].parse::<i32>().unwrap_or(40);
            let col = color_from_name(toks.get(4).copied().unwrap_or("dark"));
            widgets.push(UiWidget {
                kind: UiWidgetKind::Rect(col),
                x,
                y,
                w,
                h,
                text: String::new(),
                script: String::new(),
            });
        } else if let Some(rest) = line.strip_prefix("button ") {
            let (text, after_quote) = extract_quoted(rest)
                .ok_or_else(|| format!("Line {}: expected quoted button text", line_num))?;
            let prefix_end = rest.find('"').unwrap_or(0);
            let toks: Vec<&str> = rest[..prefix_end].split_whitespace().collect();
            if toks.len() < 5 {
                return Err(format!(
                    "Line {}: usage: button <id> <x> <y> <w> <h> \"text\" {{ ... }}",
                    line_num
                ));
            }
            let x = toks[1].parse::<i32>().unwrap_or(16);
            let y = toks[2].parse::<i32>().unwrap_or(16);
            let w = toks[3].parse::<i32>().unwrap_or(90);
            let h = toks[4].parse::<i32>().unwrap_or(30);

            let script = if let (Some(b0), Some(b1)) = (after_quote.find('{'), after_quote.rfind('}')) {
                after_quote[b0 + 1..b1].trim().to_string()
            } else {
                String::new()
            };

            widgets.push(UiWidget {
                kind: UiWidgetKind::Button,
                x,
                y,
                w,
                h,
                text: text.to_string(),
                script,
            });
        } else {
            return Err(format!("Line {}: unrecognized directive '{}'", line_num, line));
        }
    }

    let runtime = DeclarativeRuntime { vars, widgets };
    let bin = WboExecutable::pack_declarative(width, height, &title, &runtime);
    Ok((title, bin))
}

/// Installs system `.wbo` binaries into `C:/WebOS/bin/Apps/`
/// and `.wbsh` shell commands into `C:/WebOS/bin/PATH/`.
pub fn install_system_binaries(fs: &mut FileSystem) {
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/Terminal.wbo",
        WboExecutable::pack_system(WboSubsystem::Terminal, 680, 420, "Terminal"),
    );
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/Files.wbo",
        WboExecutable::pack_system(WboSubsystem::Files, 450, 340, "File Explorer"),
    );
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/SysMon.wbo",
        WboExecutable::pack_system(WboSubsystem::SysMon, 420, 220, "System Monitor"),
    );
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/Editor.wbo",
        WboExecutable::pack_system(WboSubsystem::Editor, 740, 470, "Code Editor"),
    );

    // Pre-compile Counter.wbo as a ready-to-launch example in Apps/
    if let Some(counter_src) = fs.read_str("C:/usr/counter.wbml").map(|s| s.to_string()) {
        if let Ok((_, bin)) = compile_wbml_to_wbo(&counter_src) {
            let _ = fs.write_bytes("C:/WebOS/bin/Apps/Counter.wbo", bin);
        }
    }

    let scripts: [(&str, &str); 12] = [
        (
            "C:/WebOS/bin/PATH/help.wbsh",
            "echo Intrinsics: cd <dir>, pwd, clear, run <file>\necho PATH Commands (C:/WebOS/bin/PATH/*.wbsh):\necho   help, ls [dir], cat <file>, write <file> <text>,\necho   edit <file>, build <src.wbml> [out.wbo], mkdir <dir>,\necho   rm <path>, open <app.wbo>, mem, uptime, theme <green|amber|cyan>",
        ),
        ("C:/WebOS/bin/PATH/ls.wbsh", "@sys.ls $1"),
        ("C:/WebOS/bin/PATH/cat.wbsh", "@sys.cat $1"),
        ("C:/WebOS/bin/PATH/write.wbsh", "@sys.write $1 $rest"),
        ("C:/WebOS/bin/PATH/edit.wbsh", "@sys.edit $1"),
        ("C:/WebOS/bin/PATH/build.wbsh", "@sys.build $1 $2"),
        ("C:/WebOS/bin/PATH/mkdir.wbsh", "@sys.mkdir $1"),
        ("C:/WebOS/bin/PATH/rm.wbsh", "@sys.rm $1"),
        ("C:/WebOS/bin/PATH/mem.wbsh", "@sys.mem"),
        ("C:/WebOS/bin/PATH/uptime.wbsh", "@sys.uptime"),
        ("C:/WebOS/bin/PATH/theme.wbsh", "@sys.theme $1"),
        ("C:/WebOS/bin/PATH/open.wbsh", "@sys.open $1"),
    ];

    for (path, content) in scripts {
        let _ = fs.write_str(path, content);
    }
}

pub fn expand_wbsh_line(line: &str, args: &[&str], cwd: &str) -> String {
    let a1 = args.first().copied().unwrap_or("");
    let a2 = args.get(1).copied().unwrap_or("");
    let rest = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        String::new()
    };

    let mut out = line.replace("$CWD", cwd);
    out = out.replace("$rest", &rest);
    out = out.replace("$1", a1);
    out = out.replace("$2", a2);
    out
}

pub fn format_wbo_inspection(bytes: &[u8]) -> String {
    match WboExecutable::parse(bytes) {
        Ok(exe) => {
            let widgets_count = exe.runtime.as_ref().map(|r| r.widgets.len()).unwrap_or(0);
            format!(
                "[WBO Binary] \"{}\" | Subsys: {} | {}x{} | Widgets: {}",
                exe.title, exe.subsystem as u8, exe.width, exe.height, widgets_count
            )
        }
        Err(e) => format!("[Invalid WBO]: {}", e),
    }
}