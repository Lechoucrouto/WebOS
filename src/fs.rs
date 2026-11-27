use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub enum FsNode {
    Dir,
    File(Vec<u8>),
}

pub struct FileSystem {
    nodes: BTreeMap<String, FsNode>,
    pub cwd: String,
    pub dirty: bool,
}

impl FileSystem {
    pub fn new() -> Self {
        let mut fs = Self {
            nodes: BTreeMap::new(),
            cwd: String::from("C:/usr"),
            dirty: false,
        };

        fs.nodes.insert(String::from("C:/"), FsNode::Dir);
        let _ = fs.mkdir("C:/usr");
        let _ = fs.mkdir("C:/WebOS");
        let _ = fs.mkdir("C:/WebOS/bin");
        let _ = fs.mkdir("C:/WebOS/bin/PATH");
        let _ = fs.mkdir("C:/WebOS/bin/Apps");

        let _ = fs.write_str(
            "C:/usr/readme.txt",
            "Welcome to WebOS v0.6.0!\n- All files in C:/usr/ and Settings now persist in browser storage.\n- Open Settings to adjust Brightness, Volume, or perform a Factory Reset.",
        );

        let _ = fs.write_str(
            "C:/usr/counter.wbml",
            r#"app "Counter" 340 245 {
    state {
        count = 0
        step = 1
    }

    col pad=16 gap=12 {
        card bg=#0D1117 border=#30363D pad=12 radius=6 {
            text "Count: {count}" size=lg color=#58F284
            text "Step size: {step}" size=sm color=#8B949E
        }

        row gap=8 {
            btn "+Step" style=primary { count = count + step }
            btn "-Step" { count = count - step }
            btn "Reset" style=danger { count = 0 }
        }

        btn "Double Step" style=accent { step = step * 2 }
    }
}
"#,
        );

        let _ = fs.write_str(
            "C:/usr/calculator.wbml",
            r#"app "Calculator" 310 330 {
    state {
        acc = 0
        cur = 0
        op = ""
    }

    col pad=14 gap=10 {
        card bg=#0D1117 border=#30363D pad=12 radius=6 {
            text "{cur}" size=lg align=right color=#58F284
            text "MEM: {acc} {op}" size=sm align=right color=#8B949E
        }

        grid cols=4 gap=8 {
            btn "7" { cur = cur * 10 + 7 }
            btn "8" { cur = cur * 10 + 8 }
            btn "9" { cur = cur * 10 + 9 }
            btn "/" style=accent { acc = cur; cur = 0; op = "/" }

            btn "4" { cur = cur * 10 + 4 }
            btn "5" { cur = cur * 10 + 5 }
            btn "6" { cur = cur * 10 + 6 }
            btn "*" style=accent { acc = cur; cur = 0; op = "*" }

            btn "1" { cur = cur * 10 + 1 }
            btn "2" { cur = cur * 10 + 2 }
            btn "3" { cur = cur * 10 + 3 }
            btn "-" style=accent { acc = cur; cur = 0; op = "-" }

            btn "C" style=danger { acc = 0; cur = 0; op = "" }
            btn "0" { cur = cur * 10 }
            btn "=" style=primary {
                if op == "+" { cur = acc + cur }
                if op == "-" { cur = acc - cur }
                if op == "*" { cur = acc * cur }
                if op == "/" {
                    if cur != 0 { cur = acc / cur }
                }
                op = ""
            }
            btn "+" style=accent { acc = cur; cur = 0; op = "+" }
        }
    }
}
"#,
        );

        let _ = fs.write_str(
            "C:/usr/snake.wbml",
            r#"app "Arcade Runner" 320 330 {
    state {
        px = 140
        py = 170
        vx = 10
        vy = 0
        fx = 60
        fy = 60
        score = 0
        alive = 1
    }

    on_tick(140) {
        if alive == 1 {
            px = px + vx
            py = py + vy
            if px < 0 { alive = 0 }
            if px > 270 { alive = 0 }
            if py < 0 { alive = 0 }
            if py > 180 { alive = 0 }
            if px == fx {
                if py == fy {
                    score = score + 10
                    fx = sys.rand(1, 26) * 10
                    fy = sys.rand(1, 17) * 10
                }
            }
        }
    }

    on_key(k) {
        if k == "ArrowUp" { vx = 0; vy = -10 }
        if k == "ArrowDown" { vx = 0; vy = 10 }
        if k == "ArrowLeft" { vx = -10; vy = 0 }
        if k == "ArrowRight" { vx = 10; vy = 0 }
        if k == "r" {
            px = 140; py = 170; vx = 10; vy = 0; score = 0; alive = 1
        }
    }

    col pad=14 gap=10 {
        row gap=12 {
            text "Score: {score}" color=#58F284
            if alive == 0 {
                text "GAME OVER (Press R)" color=#F85149
            }
        }

        canvas w=288 h=196 bg=#0A0E14 {
            rect fx fy 10 10 #FF7B72
            rect px py 10 10 #58F284
        }

        row gap=6 {
            btn "Left" { vx = -10; vy = 0 }
            btn "Up" { vx = 0; vy = -10 }
            btn "Down" { vx = 0; vy = 10 }
            btn "Right" { vx = 10; vy = 0 }
        }
    }
}
"#,
        );

        let _ = fs.write_str(
            "C:/usr/hello.wbsh",
            "# Sample WebOS shell script\necho Running hello.wbsh from C:/usr/\n@sys.mem\n",
        );

        fs.dirty = false;
        fs
    }

    pub fn resolve_path(&self, input: &str) -> String {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return self.cwd.clone();
        }

        let normalized_slashes = trimmed.replace('\\', "/");
        let raw_combined = if normalized_slashes.starts_with("C:/")
            || normalized_slashes.starts_with("c:/")
        {
            normalized_slashes[3..].to_string()
        } else if normalized_slashes.eq_ignore_ascii_case("C:") {
            String::new()
        } else if let Some(stripped) = normalized_slashes.strip_prefix('/') {
            stripped.to_string()
        } else if self.cwd == "C:/" {
            normalized_slashes
        } else {
            format!("{}/{}", &self.cwd[3..], normalized_slashes)
        };

        let mut parts: Vec<&str> = Vec::new();
        for part in raw_combined.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }

        if parts.is_empty() {
            String::from("C:/")
        } else {
            format!("C:/{}", parts.join("/"))
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        let resolved = self.resolve_path(path);
        self.nodes.contains_key(&resolved)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), &'static str> {
        let resolved = self.resolve_path(path);
        if self.nodes.contains_key(&resolved) {
            return Err("Path already exists");
        }
        let parent = Self::parent_of(&resolved);
        match self.nodes.get(&parent) {
            Some(FsNode::Dir) => {
                self.nodes.insert(resolved, FsNode::Dir);
                self.dirty = true;
                Ok(())
            }
            _ => Err("Parent directory does not exist"),
        }
    }

    pub fn write_bytes(&mut self, path: &str, data: Vec<u8>) -> Result<(), &'static str> {
        let resolved = self.resolve_path(path);
        if resolved == "C:/" {
            return Err("Cannot overwrite root directory");
        }
        if matches!(self.nodes.get(&resolved), Some(FsNode::Dir)) {
            return Err("Target is a directory");
        }
        let parent = Self::parent_of(&resolved);
        match self.nodes.get(&parent) {
            Some(FsNode::Dir) => {
                self.nodes.insert(resolved, FsNode::File(data));
                self.dirty = true;
                Ok(())
            }
            _ => Err("Parent directory does not exist"),
        }
    }

    pub fn write_str(&mut self, path: &str, text: &str) -> Result<(), &'static str> {
        self.write_bytes(path, text.as_bytes().to_vec())
    }

    pub fn read_bytes(&self, path: &str) -> Option<&[u8]> {
        let resolved = self.resolve_path(path);
        match self.nodes.get(&resolved) {
            Some(FsNode::File(bytes)) => Some(bytes.as_slice()),
            _ => None,
        }
    }

    pub fn read_str(&self, path: &str) -> Option<&str> {
        self.read_bytes(path)
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    pub fn is_dir(&self, path: &str) -> bool {
        let resolved = self.resolve_path(path);
        matches!(self.nodes.get(&resolved), Some(FsNode::Dir))
    }

    pub fn remove(&mut self, path: &str) -> Result<(), &'static str> {
        let resolved = self.resolve_path(path);
        if resolved == "C:/" {
            return Err("Cannot remove root directory");
        }
        match self.nodes.get(&resolved) {
            None => Err("No such file or directory"),
            Some(FsNode::File(_)) => {
                self.nodes.remove(&resolved);
                self.dirty = true;
                Ok(())
            }
            Some(FsNode::Dir) => {
                let prefix = format!("{}/", resolved);
                let has_children = self.nodes.keys().any(|k| k.starts_with(&prefix));
                if has_children {
                    Err("Directory is not empty")
                } else {
                    self.nodes.remove(&resolved);
                    self.dirty = true;
                    Ok(())
                }
            }
        }
    }

    pub fn list_dir(&self, dir_path: &str) -> Result<Vec<(String, bool, usize)>, &'static str> {
        let resolved = self.resolve_path(dir_path);
        if !matches!(self.nodes.get(&resolved), Some(FsNode::Dir)) {
            return Err("Not a directory");
        }

        let prefix = if resolved == "C:/" {
            String::from("C:/")
        } else {
            format!("{}/", resolved)
        };

        let mut entries = Vec::new();
        for (path, node) in &self.nodes {
            if path.starts_with(&prefix) && path.len() > prefix.len() {
                let rest = &path[prefix.len()..];
                if !rest.contains('/') {
                    match node {
                        FsNode::Dir => entries.push((rest.to_string(), true, 0)),
                        FsNode::File(data) => entries.push((rest.to_string(), false, data.len())),
                    }
                }
            }
        }
        Ok(entries)
    }

    pub fn parent_of(resolved_path: &str) -> String {
        if resolved_path == "C:/" {
            return String::from("C:/");
        }
        match resolved_path.rfind('/') {
            Some(idx) if idx <= 2 => String::from("C:/"),
            Some(idx) => resolved_path[..idx].to_string(),
            None => String::from("C:/"),
        }
    }

    pub fn export_persistent_blob(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"WBFS\x01");

        let mut records: Vec<(&String, &FsNode)> = self
            .nodes
            .iter()
            .filter(|(k, _)| {
                k.starts_with("C:/usr/")
                    || *k == "C:/WebOS/settings.cfg"
                    || (k.starts_with("C:/WebOS/bin/Apps/")
                        && !matches!(
                            k.as_str(),
                            "C:/WebOS/bin/Apps/Terminal.wbo"
                                | "C:/WebOS/bin/Apps/Files.wbo"
                                | "C:/WebOS/bin/Apps/SysMon.wbo"
                                | "C:/WebOS/bin/Apps/Editor.wbo"
                                | "C:/WebOS/bin/Apps/Settings.wbo"
                        ))
            })
            .collect();

        records.sort_by_key(|(k, _)| k.len());

        let count = records.len() as u32;
        out.extend_from_slice(&count.to_le_bytes());

        for (path, node) in records {
            let pbytes = path.as_bytes();
            let plen = pbytes.len().min(u16::MAX as usize) as u16;
            match node {
                FsNode::Dir => {
                    out.push(0);
                    out.extend_from_slice(&plen.to_le_bytes());
                    out.extend_from_slice(&pbytes[..plen as usize]);
                }
                FsNode::File(data) => {
                    out.push(1);
                    out.extend_from_slice(&plen.to_le_bytes());
                    out.extend_from_slice(&pbytes[..plen as usize]);
                    let dlen = data.len() as u32;
                    out.extend_from_slice(&dlen.to_le_bytes());
                    out.extend_from_slice(data);
                }
            }
        }
        out
    }

    pub fn import_persistent_blob(&mut self, blob: &[u8]) -> bool {
        if blob.len() < 9 || &blob[0..5] != b"WBFS\x01" {
            return false;
        }
        let count = u32::from_le_bytes([blob[5], blob[6], blob[7], blob[8]]) as usize;
        let mut cur = 9usize;

        self.nodes.retain(|k, _| !k.starts_with("C:/usr/"));

        for _ in 0..count {
            if cur + 3 > blob.len() {
                return false;
            }
            let kind = blob[cur];
            let plen = u16::from_le_bytes([blob[cur + 1], blob[cur + 2]]) as usize;
            cur += 3;
            if cur + plen > blob.len() {
                return false;
            }
            let Ok(path_str) = core::str::from_utf8(&blob[cur..cur + plen]) else {
                return false;
            };
            cur += plen;

            if kind == 0 {
                self.nodes.insert(path_str.to_string(), FsNode::Dir);
            } else if kind == 1 {
                if cur + 4 > blob.len() {
                    return false;
                }
                let dlen = u32::from_le_bytes([
                    blob[cur],
                    blob[cur + 1],
                    blob[cur + 2],
                    blob[cur + 3],
                ]) as usize;
                cur += 4;
                if cur + dlen > blob.len() {
                    return false;
                }
                self.nodes
                    .insert(path_str.to_string(), FsNode::File(blob[cur..cur + dlen].to_vec()));
                cur += dlen;
            } else {
                return false;
            }
        }
        self.dirty = false;
        true
    }
}
