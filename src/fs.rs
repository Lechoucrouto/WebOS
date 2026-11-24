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
        fs.mkdir("C:/usr");
        fs.mkdir("C:/WebOS");
        fs.mkdir("C:/WebOS/bin");
        fs.mkdir("C:/WebOS/bin/PATH");
        fs.mkdir("C:/WebOS/bin/Apps");

        // User files in C:/usr/
        fs.write_str(
            "C:/usr/readme.txt",
            "Welcome to WebOS! Programs now live in C:/WebOS/bin/Apps/*.wbo and commands in C:/WebOS/bin/PATH/*.wbsh.",
        );
        fs.write_str(
            "C:/usr/arch.info",
            "Kernel: #![no_std] Rust | Target: wasm32-unknown-unknown | FS: Hierarchical C:/ VFS",
        );
        fs.write_str(
            "C:/usr/notes.md",
            "Try: cat C:/WebOS/bin/PATH/help.wbsh  OR  run C:/usr/hello.wbsh",
        );
        fs.write_str(
            "C:/usr/hello.wbsh",
            "# Sample user shell script\necho Running custom script from C:/usr/hello.wbsh!\n@sys.uptime\n@sys.mem\n",
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