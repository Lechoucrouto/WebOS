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
}

impl FileSystem {
    pub fn new() -> Self {
        let mut fs = Self {
            nodes: BTreeMap::new(),
            cwd: String::from("C:/usr"),
        };

        // Core directory hierarchy
        fs.nodes.insert(String::from("C:/"), FsNode::Dir);
        let _ = fs.mkdir("C:/usr");
        let _ = fs.mkdir("C:/WebOS");
        let _ = fs.mkdir("C:/WebOS/bin");
        let _ = fs.mkdir("C:/WebOS/bin/PATH");
        let _ = fs.mkdir("C:/WebOS/bin/Apps");

        // User files & sample .wbml app source code in C:/usr/
        let _ = fs.write_str(
            "C:/usr/readme.txt",
            "Welcome to WebOS v0.4.0!\n- Click Files or Editor in the bottom-right WebOS menu.\n- Open counter.wbml in Editor and click 'Build .wbo' to compile your own app!",
        );
        let _ = fs.write_str(
            "C:/usr/counter.wbml",
            "app \"Counter\" 340 205\nvar count = 0\nvar step = 1\n\nrect 16 14 304 54 dark\nlabel title 28 24 \"Count: {count}\"\nlabel sub 28 46 \"Step size: {step}\"\n\nbutton inc 16 82 94 34 \"+Step\" {\n    count = count + step\n}\nbutton dec 120 82 94 34 \"-Step\" {\n    count = count - step\n}\nbutton rst 224 82 94 34 \"Reset\" {\n    count = 0\n}\nbutton dbl 16 126 302 30 \"Double Step\" {\n    step = step * 2\n}\n",
        );
        let _ = fs.write_str(
            "C:/usr/calculator.wbml",
            "app \"Calculator\" 296 285\nvar acc = 0\nvar cur = 0\nvar op = 0\n\nrect 16 12 260 44 dark\nlabel disp 26 24 \"Val: {cur} (Mem: {acc})\"\n\nbutton b7 16 68 58 34 \"7\" { cur = cur * 10 + 7 }\nbutton b8 82 68 58 36 \"8\" { cur = cur * 10 + 8 }\nbutton b9 148 68 58 36 \"9\" { cur = cur * 10 + 9 }\nbutton bdiv 216 68 60 36 \"/\" { acc = cur; cur = 0; op = 4 }\n\nbutton b4 16 112 58 36 \"4\" { cur = cur * 10 + 4 }\nbutton b5 82 112 58 36 \"5\" { cur = cur * 10 + 5 }\nbutton b6 148 112 58 36 \"6\" { cur = cur * 10 + 6 }\nbutton bmul 216 112 60 36 \"*\" { acc = cur; cur = 0; op = 3 }\n\nbutton b1 16 156 58 36 \"1\" { cur = cur * 10 + 1 }\nbutton b2 82 156 58 36 \"2\" { cur = cur * 10 + 2 }\nbutton b3 148 156 58 36 \"3\" { cur = cur * 10 + 3 }\nbutton bsub 216 156 60 36 \"-\" { acc = cur; cur = 0; op = 2 }\n\nbutton bclr 16 200 58 36 \"C\" { acc = 0; cur = 0; op = 0 }\nbutton b0 82 200 58 36 \"0\" { cur = cur * 10 }\nbutton beq 148 200 58 36 \"=\" {\n    if op == 1 { cur = acc + cur }\n    if op == 2 { cur = acc - cur }\n    if op == 3 { cur = acc * cur }\n    if op == 4 { cur = acc / cur }\n    op = 0\n}\nbutton badd 216 200 60 36 \"+\" { acc = cur; cur = 0; op = 1 }\n",
        );
        let _ = fs.write_str(
            "C:/usr/hello.wbsh",
            "# Sample WebOS shell script\necho Running hello.wbsh from C:/usr/\n@sys.mem\n",
        );

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
                Ok(())
            }
            Some(FsNode::Dir) => {
                let prefix = format!("{}/", resolved);
                let has_children = self.nodes.keys().any(|k| k.starts_with(&prefix));
                if has_children {
                    Err("Directory is not empty")
                } else {
                    self.nodes.remove(&resolved);
                    Ok(())
                }
            }
        }
    }

    /// Returns direct children as `(name, is_dir, byte_len)`
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
}