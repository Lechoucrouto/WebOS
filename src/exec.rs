use crate::fs::FileSystem;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub const WBO_MAGIC: &[u8; 4] = b"WBO\x01";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WboSubsystem {
    Terminal = 1,
    Files = 2,
    SysMon = 3,
    CustomApp = 4,
}

impl WboSubsystem {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::Terminal),
            2 => Some(Self::Files),
            3 => Some(Self::SysMon),
            4 => Some(Self::CustomApp),
            _ => None,
        }
    }
}

pub struct WboExecutable {
    pub subsystem: WboSubsystem,
    pub width: i32,
    pub height: i32,
    pub pinned: bool,
    pub title: String,
    pub body_lines: Vec<String>,
}

impl WboExecutable {
    /// Serializes a `.wbo` binary executable image:
    /// [0..4]   Magic: "WBO\x01"
    /// [4]      Subsystem ID (1=Terminal, 2=Files, 3=SysMon, 4=CustomApp)
    /// [5..7]   Default width (u16 LE)
    /// [7..9]   Default height (u16 LE)
    /// [9]      Taskbar pinned flag (1 = true, 0 = false)
    /// [10]     Title length in bytes (u8)
    /// [11..N]  UTF-8 Window Title
    /// [N..]    Optional UTF-8 payload lines for CustomApp
    pub fn pack(
        subsystem: WboSubsystem,
        width: u16,
        height: u16,
        pinned: bool,
        title: &str,
        body: &str,
    ) -> Vec<u8> {
        let title_bytes = title.as_bytes();
        let title_len = title_bytes.len().min(255) as u8;

        let mut bin = Vec::with_capacity(11 + title_len as usize + body.len());
        bin.extend_from_slice(WBO_MAGIC);
        bin.push(subsystem as u8);
        bin.extend_from_slice(&width.to_le_bytes());
        bin.extend_from_slice(&height.to_le_bytes());
        bin.push(if pinned { 1 } else { 0 });
        bin.push(title_len);
        bin.extend_from_slice(&title_bytes[..title_len as usize]);
        bin.extend_from_slice(body.as_bytes());
        bin
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 11 || &bytes[0..4] != WBO_MAGIC {
            return Err("Invalid .wbo binary header (expected magic WBO\\x01)");
        }

        let subsystem =
            WboSubsystem::from_u8(bytes[4]).ok_or("Unsupported .wbo subsystem ID")?;
        let width = u16::from_le_bytes([bytes[5], bytes[6]]) as i32;
        let height = u16::from_le_bytes([bytes[7], bytes[8]]) as i32;
        let pinned = bytes[9] != 0;
        let title_len = bytes[10] as usize;

        if bytes.len() < 11 + title_len {
            return Err("Truncated .wbo title section");
        }

        let title = core::str::from_utf8(&bytes[11..11 + title_len])
            .map_err(|_| "Invalid UTF-8 in .wbo title")?
            .to_string();

        let body_str = core::str::from_utf8(&bytes[11 + title_len..]).unwrap_or("");
        let body_lines = body_str.lines().map(|l| l.to_string()).collect();

        Ok(Self {
            subsystem,
            width: width.max(260),
            height: height.max(160),
            pinned,
            title,
            body_lines,
        })
    }
}

/// Installs default `.wbo` executables in `C:/WebOS/bin/Apps/`
/// and `.wbsh` shell commands in `C:/WebOS/bin/PATH/`.
pub fn install_system_binaries(fs: &mut FileSystem) {
    // 1. GUI App Executables (.wbo)
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/Terminal.wbo",
        WboExecutable::pack(WboSubsystem::Terminal, 720, 440, true, "Terminal - wbsh", ""),
    );
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/Files.wbo",
        WboExecutable::pack(WboSubsystem::Files, 446, 330, true, "File Explorer", ""),
    );
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/SysMon.wbo",
        WboExecutable::pack(WboSubsystem::SysMon, 446, 230, true, "System Monitor", ""),
    );
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/About.wbo",
        WboExecutable::pack(
            WboSubsystem::CustomApp,
            460,
            220,
            false,
            "About WebOS",
            "WebOS v0.3.0\nExecutable Format: .wbo (WBO\\x01)\nShell Interpreter: .wbsh\nBuilt with #![no_std] Rust on WebAssembly.",
        ),
    );

    // 2. Shell Command Scripts (.wbsh) in C:/WebOS/bin/PATH/
    let scripts: [(&str, &str); 11] = [
        (
            "C:/WebOS/bin/PATH/help.wbsh",
            "echo Shell Intrinsics: cd <dir>, pwd, clear, run <file>\necho PATH Commands (C:/WebOS/bin/PATH/*.wbsh):\necho   help, ls [dir], cat <file>, write <file> <text>,\necho   mkdir <dir>, rm <path>, open <app.wbo>, mkwbo <file> <title> <text>,\necho   mem, uptime, theme <green|amber|cyan>",
        ),
        (
            "C:/WebOS/bin/PATH/ls.wbsh",
            "# Lists directory contents\n@sys.ls $1",
        ),
        (
            "C:/WebOS/bin/PATH/cat.wbsh",
            "# Prints file contents\n@sys.cat $1",
        ),
        (
            "C:/WebOS/bin/PATH/write.wbsh",
            "# Writes text to a file\n@sys.write $1 $rest",
        ),
        (
            "C:/WebOS/bin/PATH/mkdir.wbsh",
            "# Creates a directory\n@sys.mkdir $1",
        ),
        (
            "C:/WebOS/bin/PATH/rm.wbsh",
            "# Deletes a file or empty directory\n@sys.rm $1",
        ),
        (
            "C:/WebOS/bin/PATH/mem.wbsh",
            "# Displays kernel heap statistics\n@sys.mem",
        ),
        (
            "C:/WebOS/bin/PATH/uptime.wbsh",
            "# Displays system uptime\n@sys.uptime",
        ),
        (
            "C:/WebOS/bin/PATH/theme.wbsh",
            "# Changes terminal color palette\n@sys.theme $1",
        ),
        (
            "C:/WebOS/bin/PATH/open.wbsh",
            "# Launches a .wbo executable from C:/WebOS/bin/Apps/\n@sys.open $1",
        ),
        (
            "C:/WebOS/bin/PATH/mkwbo.wbsh",
            "# Compiles a custom .wbo app binary: mkwbo <path> <title> <text>\n@sys.mkwbo $1 $2 $rest2",
        ),
    ];

    for (path, content) in scripts {
        let _ = fs.write_str(path, content);
    }
}

/// Expands `$1`, `$2`, `$rest` (args 2..), `$rest2` (args 3..), and `$CWD`
pub fn expand_wbsh_line(line: &str, args: &[&str], cwd: &str) -> String {
    let a1 = args.first().copied().unwrap_or("");
    let a2 = args.get(1).copied().unwrap_or("");
    let rest = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        String::new()
    };
    let rest2 = if args.len() > 2 {
        args[2..].join(" ")
    } else {
        String::new()
    };

    let mut out = line.replace("$CWD", cwd);
    out = out.replace("$rest2", &rest2);
    out = out.replace("$rest", &rest);
    out = out.replace("$1", a1);
    out = out.replace("$2", a2);
    out
}

pub fn format_wbo_inspection(bytes: &[u8]) -> String {
    match WboExecutable::parse(bytes) {
        Ok(exe) => format!(
            "[WBO Binary] Title: \"{}\" | Subsystem: {:?} | Size: {}x{} | Pinned: {}",
            exe.title,
            exe.subsystem as u8,
            exe.width,
            exe.height,
            exe.pinned
        ),
        Err(e) => format!("[Invalid WBO]: {}", e),
    }
}