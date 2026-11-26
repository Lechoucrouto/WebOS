use crate::fs::FileSystem;
use crate::gui::rgba;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub const WBO_MAGIC_V1: &[u8; 4] = b"WBO\x01";
pub const WBO_MAGIC_V2: &[u8; 4] = b"WBO\x02";

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

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Int(i64),
    Str(String),
    List(Vec<Val>),
}

impl Val {
    pub fn as_int(&self) -> i64 {
        match self {
            Val::Int(n) => *n,
            Val::Str(s) => s.trim().parse::<i64>().unwrap_or(0),
            Val::List(l) => l.len() as i64,
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Val::Int(n) => *n != 0,
            Val::Str(s) => !s.is_empty() && s != "false" && s != "0",
            Val::List(l) => !l.is_empty(),
        }
    }

    pub fn display_str(&self) -> String {
        match self {
            Val::Int(n) => n.to_string(),
            Val::Str(s) => s.clone(),
            Val::List(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.display_str()).collect();
                format!("[{}]", parts.join(", "))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum CanvasCmd {
    Clear(u32),
    Rect { x: String, y: String, w: String, h: String, color: u32 },
    Circle { x: String, y: String, r: String, color: u32 },
    Line { x1: String, y1: String, x2: String, y2: String, color: u32 },
}

#[derive(Clone, Debug)]
pub enum UiNode {
    Col {
        pad: i32,
        gap: i32,
        children: Vec<UiNode>,
    },
    Row {
        pad: i32,
        gap: i32,
        children: Vec<UiNode>,
    },
    Grid {
        cols: i32,
        gap: i32,
        children: Vec<UiNode>,
    },
    Card {
        bg: u32,
        border: u32,
        pad: i32,
        radius: i32,
        children: Vec<UiNode>,
    },
    Text {
        text: String,
        size: i32,   // 1 = sm, 2 = md, 3 = lg
        align: u8,   // 0 = left, 1 = center, 2 = right
        color: u32,
        abs: Option<(i32, i32)>,
    },
    Button {
        label: String,
        style: u8, // 0 = default, 1 = primary, 2 = accent, 3 = danger
        script: String,
        abs: Option<(i32, i32, i32, i32)>,
    },
    RectBox {
        color: u32,
        abs: (i32, i32, i32, i32),
    },
    Input {
        bind: String,
        placeholder: String,
    },
    Slider {
        bind: String,
        min: i64,
        max: i64,
    },
    Toggle {
        bind: String,
        label: String,
    },
    Progress {
        val_expr: String,
        max: i64,
        color: u32,
    },
    Canvas {
        w: i32,
        h: i32,
        bg: u32,
        cmds: Vec<CanvasCmd>,
    },
    If {
        cond: String,
        then_nodes: Vec<UiNode>,
        else_nodes: Vec<UiNode>,
    },
    For {
        item_var: String,
        list_var: String,
        body: Vec<UiNode>,
    },
}

#[derive(Clone)]
pub struct LayoutGuide {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub label: &'static str,
}

#[derive(Clone)]
pub enum RenderedWidget {
    Card {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        bg: u32,
        border: u32,
        radius: i32,
    },
    Text {
        x: i32,
        y: i32,
        w: i32,
        text: String,
        size: i32,
        align: u8,
        color: u32,
    },
    Button {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        label: String,
        style: u8,
        script: String,
    },
    Input {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        bind: String,
        placeholder: String,
    },
    Slider {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        bind: String,
        min: i64,
        max: i64,
    },
    Toggle {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        bind: String,
        label: String,
    },
    Progress {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        val: i64,
        max: i64,
        color: u32,
    },
    Canvas {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        bg: u32,
        cmds: Vec<CanvasCmd>,
    },
}

#[derive(Clone)]
pub struct PendingSyscall {
    pub kind: String,
    pub arg1: String,
    pub arg2: String,
}

#[derive(Clone)]
pub struct DeclarativeRuntime {
    pub source: String,
    pub vars: BTreeMap<String, Val>,
    pub tree: Vec<UiNode>,
    pub on_init: String,
    pub on_tick_ms: u64,
    pub on_tick: String,
    pub last_tick_ms: u64,
    pub on_key_var: String,
    pub on_key: String,
    pub focused_input: Option<String>,
    pub rng_state: u64,
    pub syscalls: Vec<PendingSyscall>,
    pub initialized: bool,
}

impl DeclarativeRuntime {
    pub fn ensure_init(&mut self, now_ms: u64) {
        if !self.initialized {
            self.initialized = true;
            self.last_tick_ms = now_ms;
            if !self.on_init.is_empty() {
                let s = self.on_init.clone();
                self.run_script(&s);
            }
        }
    }

    pub fn tick(&mut self, now_ms: u64) {
        self.ensure_init(now_ms);
        if self.on_tick_ms > 0 && !self.on_tick.is_empty() {
            if now_ms.saturating_sub(self.last_tick_ms) >= self.on_tick_ms {
                self.last_tick_ms = now_ms;
                let s = self.on_tick.clone();
                self.run_script(&s);
            }
        }
    }

    pub fn handle_key(&mut self, key_name: &str, ch_opt: Option<char>, is_backspace: bool) {
        if let Some(bind_var) = self.focused_input.clone() {
            let mut cur = self
                .vars
                .get(&bind_var)
                .map(|v| v.display_str())
                .unwrap_or_default();
            if is_backspace {
                cur.pop();
            } else if let Some(c) = ch_opt {
                if cur.len() < 48 {
                    cur.push(c);
                }
            }
            self.vars.insert(bind_var, Val::Str(cur));
        }

        if !self.on_key.is_empty() {
            let var_name = if self.on_key_var.is_empty() {
                String::from("key")
            } else {
                self.on_key_var.clone()
            };
            self.vars.insert(var_name, Val::Str(key_name.to_string()));
            let s = self.on_key.clone();
            self.run_script(&s);
        }
    }

    pub fn interpolate(&self, template: &str) -> String {
        let mut out = String::new();
        let mut chars = template.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '{' {
                let mut expr = String::new();
                let mut closed = false;
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    if nc == '}' {
                        closed = true;
                        break;
                    }
                    expr.push(nc);
                }
                if closed {
                    let val = self.eval_expr(expr.trim());
                    out.push_str(&val.display_str());
                } else {
                    out.push('{');
                    out.push_str(&expr);
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    pub fn run_script(&mut self, script: &str) {
        let stmts = split_top_level_statements(script);
        for raw_stmt in stmts {
            let stmt = raw_stmt.trim();
            if stmt.is_empty() || stmt.starts_with('#') || stmt.starts_with("//") {
                continue;
            }

            if let Some(rest) = stmt.strip_prefix("if ") {
                if let Some((cond, then_body, else_body)) = parse_if_stmt(rest) {
                    if self.eval_expr(&cond).as_bool() {
                        self.run_script(&then_body);
                    } else if let Some(eb) = else_body {
                        self.run_script(&eb);
                    }
                }
                continue;
            }

            if let Some(rest) = stmt.strip_prefix("push(") {
                if let Some(inner) = rest.strip_suffix(')') {
                    if let Some((var_name, val_expr)) = inner.split_once(',') {
                        let vname = var_name.trim().to_string();
                        let item = self.eval_expr(val_expr.trim());
                        let entry = self.vars.entry(vname).or_insert_with(|| Val::List(Vec::new()));
                        if let Val::List(list) = entry {
                            list.push(item);
                        }
                    }
                }
                continue;
            }

            if let Some(rest) = stmt.strip_prefix("clear(") {
                if let Some(var_name) = rest.strip_suffix(')') {
                    let vname = var_name.trim().to_string();
                    self.vars.insert(vname, Val::List(Vec::new()));
                }
                continue;
            }

            if let Some(rest) = stmt.strip_prefix("fs.write(") {
                if let Some(inner) = rest.strip_suffix(')') {
                    if let Some((p_expr, c_expr)) = split_top_level_comma(inner) {
                        let p = self.eval_expr(p_expr.trim()).display_str();
                        let c = self.eval_expr(c_expr.trim()).display_str();
                        self.syscalls.push(PendingSyscall {
                            kind: String::from("fs.write"),
                            arg1: p,
                            arg2: c,
                        });
                    }
                }
                continue;
            }

            if let Some(rest) = stmt.strip_prefix("sys.open(") {
                if let Some(inner) = rest.strip_suffix(')') {
                    let app = self.eval_expr(inner.trim()).display_str();
                    self.syscalls.push(PendingSyscall {
                        kind: String::from("sys.open"),
                        arg1: app,
                        arg2: String::new(),
                    });
                }
                continue;
            }

            if let Some((lhs, rhs)) = split_assignment(stmt) {
                let var_name = lhs.trim();
                if !var_name.is_empty() {
                    let val = self.eval_expr(rhs.trim());
                    self.vars.insert(var_name.to_string(), val);
                }
            }
        }
    }

    pub fn eval_expr(&mut self, expr: &str) -> Val {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return Val::Int(0);
        }
        if trimmed == "[]" {
            return Val::List(Vec::new());
        }
        if trimmed == "true" {
            return Val::Int(1);
        }
        if trimmed == "false" {
            return Val::Int(0);
        }

        // Handle sys.rand(min, max)
        if let Some(rest) = trimmed.strip_prefix("sys.rand(") {
            if let Some(inner) = rest.strip_suffix(')') {
                if let Some((a_s, b_s)) = split_top_level_comma(inner) {
                    let lo = self.eval_expr(a_s).as_int();
                    let hi = self.eval_expr(b_s).as_int();
                    self.rng_state = self
                        .rng_state
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let span = (hi - lo + 1).max(1) as u64;
                    let r = ((self.rng_state >> 32) % span) as i64 + lo;
                    return Val::Int(r);
                }
            }
        }

        // Handle len(var)
        if let Some(rest) = trimmed.strip_prefix("len(") {
            if let Some(inner) = rest.strip_suffix(')') {
                let v = self.eval_expr(inner);
                return match v {
                    Val::List(l) => Val::Int(l.len() as i64),
                    Val::Str(s) => Val::Int(s.chars().count() as i64),
                    Val::Int(_) => Val::Int(1),
                };
            }
        }

        let tokens = tokenize_expr(trimmed);
        if tokens.is_empty() {
            return Val::Int(0);
        }
        let mut pos = 0;
        self.parse_logic_or(&tokens, &mut pos)
    }

    fn parse_logic_or(&mut self, toks: &[ExprTok], pos: &mut usize) -> Val {
        let mut left = self.parse_logic_and(toks, pos);
        while *pos < toks.len() && toks[*pos] == ExprTok::Op("||".into()) {
            *pos += 1;
            let right = self.parse_logic_and(toks, pos);
            left = Val::Int(if left.as_bool() || right.as_bool() { 1 } else { 0 });
        }
        left
    }

    fn parse_logic_and(&mut self, toks: &[ExprTok], pos: &mut usize) -> Val {
        let mut left = self.parse_equality(toks, pos);
        while *pos < toks.len() && toks[*pos] == ExprTok::Op("&&".into()) {
            *pos += 1;
            let right = self.parse_equality(toks, pos);
            left = Val::Int(if left.as_bool() && right.as_bool() { 1 } else { 0 });
        }
        left
    }

    fn parse_equality(&mut self, toks: &[ExprTok], pos: &mut usize) -> Val {
        let mut left = self.parse_relational(toks, pos);
        while *pos < toks.len() {
            match &toks[*pos] {
                ExprTok::Op(op) if op == "==" || op == "!=" => {
                    let is_eq = op == "==";
                    *pos += 1;
                    let right = self.parse_relational(toks, pos);
                    let res = match (&left, &right) {
                        (Val::Str(a), Val::Str(b)) => a == b,
                        _ => left.as_int() == right.as_int(),
                    };
                    left = Val::Int(if res == is_eq { 1 } else { 0 });
                }
                _ => break,
            }
        }
        left
    }

    fn parse_relational(&mut self, toks: &[ExprTok], pos: &mut usize) -> Val {
        let mut left = self.parse_additive(toks, pos);
        while *pos < toks.len() {
            match &toks[*pos] {
                ExprTok::Op(op) if op == "<" || op == ">" || op == "<=" || op == ">=" => {
                    let op_s = op.clone();
                    *pos += 1;
                    let right = self.parse_additive(toks, pos);
                    let a = left.as_int();
                    let b = right.as_int();
                    let ok = match op_s.as_str() {
                        "<" => a < b,
                        ">" => a > b,
                        "<=" => a <= b,
                        ">=" => a >= b,
                        _ => false,
                    };
                    left = Val::Int(if ok { 1 } else { 0 });
                }
                _ => break,
            }
        }
        left
    }

    fn parse_additive(&mut self, toks: &[ExprTok], pos: &mut usize) -> Val {
        let mut left = self.parse_multiplicative(toks, pos);
        while *pos < toks.len() {
            match &toks[*pos] {
                ExprTok::Op(op) if op == "+" || op == "-" => {
                    let is_add = op == "+";
                    *pos += 1;
                    let right = self.parse_multiplicative(toks, pos);
                    if is_add {
                        match (&left, &right) {
                            (Val::Str(a), _) => {
                                left = Val::Str(format!("{}{}", a, right.display_str()));
                            }
                            (_, Val::Str(b)) => {
                                left = Val::Str(format!("{}{}", left.display_str(), b));
                            }
                            _ => {
                                left = Val::Int(left.as_int().saturating_add(right.as_int()));
                            }
                        }
                    } else {
                        left = Val::Int(left.as_int().saturating_sub(right.as_int()));
                    }
                }
                _ => break,
            }
        }
        left
    }

    fn parse_multiplicative(&mut self, toks: &[ExprTok], pos: &mut usize) -> Val {
        let mut left = self.parse_unary(toks, pos);
        while *pos < toks.len() {
            match &toks[*pos] {
                ExprTok::Op(op) if op == "*" || op == "/" || op == "%" => {
                    let op_s = op.clone();
                    *pos += 1;
                    let right = self.parse_unary(toks, pos);
                    let a = left.as_int();
                    let b = right.as_int();
                    left = Val::Int(match op_s.as_str() {
                        "*" => a.saturating_mul(b),
                        "/" => {
                            if b != 0 {
                                a / b
                            } else {
                                0
                            }
                        }
                        "%" => {
                            if b != 0 {
                                a % b
                            } else {
                                0
                            }
                        }
                        _ => a,
                    });
                }
                _ => break,
            }
        }
        left
    }

    fn parse_unary(&mut self, toks: &[ExprTok], pos: &mut usize) -> Val {
        if *pos < toks.len() {
            if toks[*pos] == ExprTok::Op("-".into()) {
                *pos += 1;
                let v = self.parse_unary(toks, pos);
                return Val::Int(-v.as_int());
            }
            if toks[*pos] == ExprTok::Op("!".into()) {
                *pos += 1;
                let v = self.parse_unary(toks, pos);
                return Val::Int(if v.as_bool() { 0 } else { 1 });
            }
        }
        self.parse_primary(toks, pos)
    }

    fn parse_primary(&mut self, toks: &[ExprTok], pos: &mut usize) -> Val {
        if *pos >= toks.len() {
            return Val::Int(0);
        }
        let tok = toks[*pos].clone();
        *pos += 1;
        match tok {
            ExprTok::Num(n) => Val::Int(n),
            ExprTok::Str(s) => Val::Str(s),
            ExprTok::Ident(name) => self.vars.get(&name).cloned().unwrap_or(Val::Int(0)),
            ExprTok::LParen => {
                let inner = self.parse_logic_or(toks, pos);
                if *pos < toks.len() && toks[*pos] == ExprTok::RParen {
                    *pos += 1;
                }
                inner
            }
            _ => Val::Int(0),
        }
    }

    /// Computes concrete pixel rectangles for all UI nodes (used by both live window and Editor Live Preview)
    pub fn build_layout(
        &mut self,
        avail_w: i32,
        avail_h: i32,
    ) -> (Vec<RenderedWidget>, Vec<LayoutGuide>) {
        let mut out = Vec::new();
        let mut guides = Vec::new();
        let tree_clone = self.tree.clone();
        self.layout_nodes(
            &tree_clone,
            0,
            0,
            avail_w,
            avail_h,
            false,
            0,
            &mut out,
            &mut guides,
        );
        (out, guides)
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_nodes(
        &mut self,
        nodes: &[UiNode],
        x: i32,
        y: i32,
        w: i32,
        _h: i32,
        horizontal: bool,
        gap: i32,
        out: &mut Vec<RenderedWidget>,
        guides: &mut Vec<LayoutGuide>,
    ) -> i32 {
        let mut cur_x = x;
        let mut cur_y = y;
        let count = nodes.len().max(1) as i32;
        let item_w_horiz = ((w - gap * (count - 1)) / count).max(24);

        for node in nodes {
            let slot_w = if horizontal { item_w_horiz } else { w };
            let slot_x = if horizontal { cur_x } else { x };
            let slot_y = if horizontal { y } else { cur_y };

            let consumed_h = match node {
                UiNode::Col { pad, gap: g, children } => {
                    guides.push(LayoutGuide {
                        x: slot_x,
                        y: slot_y,
                        w: slot_w,
                        h: 20,
                        label: "col",
                    });
                    let inner_h = self.layout_nodes(
                        children,
                        slot_x + *pad,
                        slot_y + *pad,
                        (slot_w - pad * 2).max(10),
                        0,
                        false,
                        *g,
                        out,
                        guides,
                    );
                    if let Some(last_g) = guides.iter_mut().rev().find(|lg| lg.x == slot_x && lg.y == slot_y && lg.label == "col") {
                        last_g.h = inner_h + pad * 2;
                    }
                    inner_h + pad * 2
                }
                UiNode::Row { pad, gap: g, children } => {
                    let g_idx = guides.len();
                    guides.push(LayoutGuide {
                        x: slot_x,
                        y: slot_y,
                        w: slot_w,
                        h: 36,
                        label: "row",
                    });
                    let inner_h = self.layout_nodes(
                        children,
                        slot_x + *pad,
                        slot_y + *pad,
                        (slot_w - pad * 2).max(10),
                        0,
                        true,
                        *g,
                        out,
                        guides,
                    );
                    let total_h = inner_h + pad * 2;
                    if let Some(lg) = guides.get_mut(g_idx) {
                        lg.h = total_h;
                    }
                    total_h
                }
                UiNode::Grid { cols, gap: g, children } => {
                    let c = (*cols).max(1);
                    let cell_w = ((slot_w - g * (c - 1)) / c).max(24);
                    let cell_h = 36;
                    let rows = ((children.len() as i32) + c - 1) / c;
                    let total_h = (rows * cell_h + (rows - 1).max(0) * g).max(cell_h);

                    guides.push(LayoutGuide {
                        x: slot_x,
                        y: slot_y,
                        w: slot_w,
                        h: total_h,
                        label: "grid",
                    });

                    for (idx, child) in children.iter().enumerate() {
                        let r = (idx as i32) / c;
                        let col_i = (idx as i32) % c;
                        let cx = slot_x + col_i * (cell_w + g);
                        let cy = slot_y + r * (cell_h + g);
                        self.layout_single_fixed(child, cx, cy, cell_w, cell_h, out);
                    }
                    total_h
                }
                UiNode::Card {
                    bg,
                    border,
                    pad,
                    radius,
                    children,
                } => {
                    let card_idx = out.len();
                    out.push(RenderedWidget::Card {
                        x: slot_x,
                        y: slot_y,
                        w: slot_w,
                        h: 24,
                        bg: *bg,
                        border: *border,
                        radius: *radius,
                    });
                    let inner_h = self.layout_nodes(
                        children,
                        slot_x + *pad,
                        slot_y + *pad,
                        (slot_w - pad * 2).max(10),
                        0,
                        false,
                        6,
                        out,
                        guides,
                    );
                    let total_h = inner_h + pad * 2;
                    if let Some(RenderedWidget::Card { h, .. }) = out.get_mut(card_idx) {
                        *h = total_h;
                    }
                    total_h
                }
                UiNode::Text {
                    text,
                    size,
                    align,
                    color,
                    abs,
                } => {
                    let rendered = self.interpolate(text);
                    let line_h = if *size >= 3 { 24 } else { 19 };
                    let (tx, ty) = abs.unwrap_or((slot_x, slot_y));
                    out.push(RenderedWidget::Text {
                        x: tx,
                        y: ty,
                        w: slot_w,
                        text: rendered,
                        size: *size,
                        align: *align,
                        color: *color,
                    });
                    if abs.is_some() { 0 } else { line_h }
                }
                UiNode::Button {
                    label,
                    style,
                    script,
                    abs,
                } => {
                    let rendered = self.interpolate(label);
                    let (bx, by, bw, bh) = abs.unwrap_or((slot_x, slot_y, slot_w, 34));
                    out.push(RenderedWidget::Button {
                        x: bx,
                        y: by,
                        w: bw,
                        h: bh,
                        label: rendered,
                        style: *style,
                        script: script.clone(),
                    });
                    if abs.is_some() { 0 } else { bh }
                }
                UiNode::RectBox { color, abs } => {
                    out.push(RenderedWidget::Card {
                        x: abs.0,
                        y: abs.1,
                        w: abs.2,
                        h: abs.3,
                        bg: *color,
                        border: rgba(48, 54, 61),
                        radius: 4,
                    });
                    0
                }
                UiNode::Input { bind, placeholder } => {
                    out.push(RenderedWidget::Input {
                        x: slot_x,
                        y: slot_y,
                        w: slot_w,
                        h: 32,
                        bind: bind.clone(),
                        placeholder: placeholder.clone(),
                    });
                    32
                }
                UiNode::Slider { bind, min, max } => {
                    out.push(RenderedWidget::Slider {
                        x: slot_x,
                        y: slot_y,
                        w: slot_w,
                        h: 26,
                        bind: bind.clone(),
                        min: *min,
                        max: *max,
                    });
                    26
                }
                UiNode::Toggle { bind, label } => {
                    out.push(RenderedWidget::Toggle {
                        x: slot_x,
                        y: slot_y,
                        w: slot_w,
                        h: 26,
                        bind: bind.clone(),
                        label: label.clone(),
                    });
                    26
                }
                UiNode::Progress {
                    val_expr,
                    max,
                    color,
                } => {
                    let val = self.eval_expr(val_expr).as_int();
                    out.push(RenderedWidget::Progress {
                        x: slot_x,
                        y: slot_y,
                        w: slot_w,
                        h: 20,
                        val,
                        max: (*max).max(1),
                        color: *color,
                    });
                    20
                }
                UiNode::Canvas { w: cw, h: ch, bg, cmds } => {
                    out.push(RenderedWidget::Canvas {
                        x: slot_x,
                        y: slot_y,
                        w: (*cw).min(slot_w),
                        h: *ch,
                        bg: *bg,
                        cmds: cmds.clone(),
                    });
                    *ch
                }
                UiNode::If {
                    cond,
                    then_nodes,
                    else_nodes,
                } => {
                    if self.eval_expr(cond).as_bool() {
                        self.layout_nodes(then_nodes, slot_x, slot_y, slot_w, 0, horizontal, gap, out, guides)
                    } else {
                        self.layout_nodes(else_nodes, slot_x, slot_y, slot_w, 0, horizontal, gap, out, guides)
                    }
                }
                UiNode::For {
                    item_var,
                    list_var,
                    body,
                } => {
                    let items = match self.vars.get(list_var) {
                        Some(Val::List(l)) => l.clone(),
                        _ => Vec::new(),
                    };
                    let mut total_h = 0;
                    for (idx, item) in items.iter().enumerate() {
                        self.vars.insert(item_var.clone(), item.clone());
                        self.vars.insert(format!("{}_idx", item_var), Val::Int(idx as i64));
                        let h_used = self.layout_nodes(
                            body,
                            slot_x,
                            slot_y + total_h,
                            slot_w,
                            0,
                            false,
                            4,
                            out,
                            guides,
                        );
                        total_h += h_used + 4;
                    }
                    total_h.saturating_sub(4)
                }
            };

            if horizontal {
                cur_x += item_w_horiz + gap;
            } else if consumed_h > 0 {
                cur_y += consumed_h + gap;
            }
        }

        if horizontal {
            36
        } else {
            (cur_y - y - gap).max(0)
        }
    }

    fn layout_single_fixed(
        &mut self,
        node: &UiNode,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        out: &mut Vec<RenderedWidget>,
    ) {
        match node {
            UiNode::Button { label, style, script, .. } => {
                let rendered = self.interpolate(label);
                out.push(RenderedWidget::Button {
                    x,
                    y,
                    w,
                    h,
                    label: rendered,
                    style: *style,
                    script: script.clone(),
                });
            }
            UiNode::Text { text, size, align, color, .. } => {
                let rendered = self.interpolate(text);
                out.push(RenderedWidget::Text {
                    x,
                    y: y + 8,
                    w,
                    text: rendered,
                    size: *size,
                    align: *align,
                    color: *color,
                });
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ExprTok {
    Num(i64),
    Str(String),
    Ident(String),
    Op(String),
    LParen,
    RParen,
}

fn tokenize_expr(s: &str) -> Vec<ExprTok> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '"' {
            i += 1;
            let mut lit = String::new();
            while i < chars.len() && chars[i] != '"' {
                lit.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            out.push(ExprTok::Str(lit));
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let num_s: String = chars[start..i].iter().collect();
            out.push(ExprTok::Num(num_s.parse::<i64>().unwrap_or(0)));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            let id: String = chars[start..i].iter().collect();
            out.push(ExprTok::Ident(id));
            continue;
        }
        if c == '(' {
            out.push(ExprTok::LParen);
            i += 1;
            continue;
        }
        if c == ')' {
            out.push(ExprTok::RParen);
            i += 1;
            continue;
        }
        if i + 1 < chars.len() {
            let two: String = [c, chars[i + 1]].iter().collect();
            if matches!(two.as_str(), "==" | "!=" | "<=" | ">=" | "&&" | "||") {
                out.push(ExprTok::Op(two));
                i += 2;
                continue;
            }
        }
        if matches!(c, '+' | '-' | '*' | '/' | '%' | '<' | '>' | '!') {
            out.push(ExprTok::Op(c.to_string()));
            i += 1;
            continue;
        }
        i += 1;
    }
    out
}

fn split_top_level_statements(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    for ch in script.chars() {
        if ch == '"' {
            in_str = !in_str;
            cur.push(ch);
            continue;
        }
        if !in_str {
            if ch == '{' || ch == '(' {
                depth += 1;
            } else if ch == '}' || ch == ')' {
                depth -= 1;
            }
            if depth == 0 && (ch == ';' || ch == '\n') {
                if !cur.trim().is_empty() {
                    out.push(cur.clone());
                }
                cur.clear();
                continue;
            }
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn parse_if_stmt(rest: &str) -> Option<(String, String, Option<String>)> {
    let (open_idx, close_idx) = find_matching_braces(rest)?;
    let cond = rest[..open_idx].trim().to_string();
    let then_body = rest[open_idx + 1..close_idx].trim().to_string();
    let after = rest[close_idx + 1..].trim();
    if let Some(else_rest) = after.strip_prefix("else") {
        let (e_open, e_close) = find_matching_braces(else_rest.trim())?;
        let else_body = else_rest.trim()[e_open + 1..e_close].trim().to_string();
        Some((cond, then_body, Some(else_body)))
    } else {
        Some((cond, then_body, None))
    }
}

fn find_matching_braces(s: &str) -> Option<(usize, usize)> {
    let mut in_str = false;
    let mut depth = 0i32;
    let mut open_pos = None;
    for (idx, ch) in s.char_indices() {
        if ch == '"' {
            in_str = !in_str;
            continue;
        }
        if !in_str {
            if ch == '{' {
                if depth == 0 {
                    open_pos = Some(idx);
                }
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    return open_pos.map(|o| (o, idx));
                }
            }
        }
    }
    None
}

fn split_top_level_comma(s: &str) -> Option<(&str, &str)> {
    let mut in_str = false;
    let mut depth = 0i32;
    for (idx, ch) in s.char_indices() {
        if ch == '"' {
            in_str = !in_str;
        } else if !in_str {
            if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth -= 1;
            } else if ch == ',' && depth == 0 {
                return Some((&s[..idx], &s[idx + 1..]));
            }
        }
    }
    None
}

fn split_assignment(stmt: &str) -> Option<(&str, &str)> {
    let mut in_str = false;
    let bytes = stmt.as_bytes();
    for (idx, ch) in stmt.char_indices() {
        if ch == '"' {
            in_str = !in_str;
        } else if !in_str && ch == '=' {
            let prev = if idx > 0 { bytes[idx - 1] } else { 0 };
            let next = if idx + 1 < bytes.len() { bytes[idx + 1] } else { 0 };
            if prev != b'=' && prev != b'!' && prev != b'<' && prev != b'>' && next != b'=' {
                return Some((&stmt[..idx], &stmt[idx + 1..]));
            }
        }
    }
    None
}

pub fn parse_color_token(tok: &str) -> u32 {
    let t = tok.trim();
    if let Some(hex) = t.strip_prefix('#') {
        if hex.len() == 6 {
            if let Ok(v) = u32::from_str_radix(hex, 16) {
                let r = ((v >> 16) & 0xFF) as u8;
                let g = ((v >> 8) & 0xFF) as u8;
                let b = (v & 0xFF) as u8;
                return rgba(r, g, b);
            }
        }
    }
    match t {
        "dark" => rgba(13, 17, 23),
        "slate" => rgba(22, 27, 34),
        "blue" => rgba(31, 111, 235),
        "green" => rgba(35, 134, 54),
        "amber" => rgba(180, 110, 24),
        "cyan" => rgba(56, 189, 248),
        "red" => rgba(218, 54, 51),
        "white" => rgba(240, 246, 252),
        "muted" => rgba(139, 148, 158),
        _ => rgba(22, 27, 34),
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
    pub fn pack_system(subsystem: WboSubsystem, width: u16, height: u16, title: &str) -> Vec<u8> {
        let title_bytes = title.as_bytes();
        let title_len = title_bytes.len().min(255) as u8;
        let mut bin = Vec::with_capacity(10 + title_len as usize);
        bin.extend_from_slice(WBO_MAGIC_V2);
        bin.push(subsystem as u8);
        bin.extend_from_slice(&width.to_le_bytes());
        bin.extend_from_slice(&height.to_le_bytes());
        bin.push(title_len);
        bin.extend_from_slice(&title_bytes[..title_len as usize]);
        bin
    }

    pub fn pack_declarative(width: u16, height: u16, title: &str, wbml_source: &str) -> Vec<u8> {
        let mut bin = Self::pack_system(WboSubsystem::DeclarativeApp, width, height, title);
        let src_bytes = wbml_source.as_bytes();
        let slen = src_bytes.len().min(u32::MAX as usize) as u32;
        bin.extend_from_slice(&slen.to_le_bytes());
        bin.extend_from_slice(&src_bytes[..slen as usize]);
        bin
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 10 {
            return Err("Truncated .wbo binary header");
        }
        let is_v1 = &bytes[0..4] == WBO_MAGIC_V1;
        let is_v2 = &bytes[0..4] == WBO_MAGIC_V2;
        if !is_v1 && !is_v2 {
            return Err("Invalid .wbo binary header");
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
            let cur = 10 + title_len;
            if cur + 4 > bytes.len() {
                return Err("Truncated .wbo payload");
            }
            let slen = u32::from_le_bytes([
                bytes[cur],
                bytes[cur + 1],
                bytes[cur + 2],
                bytes[cur + 3],
            ]) as usize;
            if cur + 4 + slen > bytes.len() {
                return Err("Truncated .wbo bytecode/source");
            }
            let src = core::str::from_utf8(&bytes[cur + 4..cur + 4 + slen])
                .map_err(|_| "Invalid UTF-8 in .wbo payload")?;
            let parsed = parse_wbml_runtime(src).map_err(|_| "Invalid WBML payload")?;
            runtime = Some(parsed.runtime);
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

pub struct ParsedWbml {
    pub title: String,
    pub width: u16,
    pub height: u16,
    pub runtime: DeclarativeRuntime,
}

fn extract_quoted(s: &str) -> Option<(&str, &str)> {
    let start = s.find('"')?;
    let rest = &s[start + 1..];
    let end = rest.find('"')?;
    Some((&rest[..end], rest[end + 1..].trim()))
}

fn parse_attr_i32(header: &str, key: &str, default: i32) -> i32 {
    let needle = format!("{}=", key);
    if let Some(pos) = header.find(&needle) {
        let rest = &header[pos + needle.len()..];
        let tok = rest.split_whitespace().next().unwrap_or("");
        tok.parse::<i32>().unwrap_or(default)
    } else {
        default
    }
}

fn parse_attr_str<'a>(header: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{}=", key);
    let pos = header.find(&needle)?;
    let rest = &header[pos + needle.len()..];
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some(&stripped[..end])
    } else {
        Some(rest.split_whitespace().next().unwrap_or(""))
    }
}

/// Parses WBML v2 source into an executable DeclarativeRuntime (also used live in Code Editor!)
pub fn parse_wbml_runtime(source: &str) -> Result<ParsedWbml, String> {
    let mut title = String::from("Untitled App");
    let mut width: u16 = 360;
    let mut height: u16 = 260;
    let mut vars: BTreeMap<String, Val> = BTreeMap::new();
    let mut on_init = String::new();
    let mut on_tick_ms = 0u64;
    let mut on_tick = String::new();
    let mut on_key_var = String::from("key");
    let mut on_key = String::new();

    let trimmed = source.trim();
    let body_str = if let Some(rest) = trimmed.strip_prefix("app ") {
        let (t, after_quote) = extract_quoted(rest)
            .ok_or_else(|| String::from("Expected quoted app title after 'app'"))?;
        title = t.to_string();

        if let Some((open_idx, close_idx)) = find_matching_braces(after_quote) {
            let dims: Vec<&str> = after_quote[..open_idx].split_whitespace().collect();
            if dims.len() >= 2 {
                width = dims[0].parse::<u16>().unwrap_or(360).max(200);
                height = dims[1].parse::<u16>().unwrap_or(260).max(140);
            }
            &after_quote[open_idx + 1..close_idx]
        } else {
            // Legacy flat WBML support (no outer braces)
            let first_line_end = after_quote.find('\n').unwrap_or(after_quote.len());
            let dims: Vec<&str> = after_quote[..first_line_end].split_whitespace().collect();
            if dims.len() >= 2 {
                width = dims[0].parse::<u16>().unwrap_or(360).max(200);
                height = dims[1].parse::<u16>().unwrap_or(260).max(140);
            }
            &after_quote[first_line_end..]
        }
    } else {
        trimmed
    };

    let tree = parse_ui_block(
        body_str,
        &mut vars,
        &mut on_init,
        &mut on_tick_ms,
        &mut on_tick,
        &mut on_key_var,
        &mut on_key,
    )?;

    let runtime = DeclarativeRuntime {
        source: source.to_string(),
        vars,
        tree,
        on_init,
        on_tick_ms,
        on_tick,
        last_tick_ms: 0,
        on_key_var,
        on_key,
        focused_input: None,
        rng_state: 0x1234_5678_9ABC_DEF0,
        syscalls: Vec::new(),
        initialized: false,
    };

    Ok(ParsedWbml {
        title,
        width,
        height,
        runtime,
    })
}

fn split_block_items(block: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str = false;

    for raw_line in block.lines() {
        let line = raw_line.trim();
        if depth == 0 && (line.is_empty() || line.starts_with('#') || line.starts_with("//")) {
            continue;
        }
        for ch in line.chars() {
            if ch == '"' {
                in_str = !in_str;
            } else if !in_str {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                }
            }
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(line);
        if depth <= 0 {
            items.push(cur.clone());
            cur.clear();
            depth = 0;
        }
    }
    if !cur.trim().is_empty() {
        items.push(cur);
    }
    items
}

fn parse_ui_block(
    block: &str,
    vars: &mut BTreeMap<String, Val>,
    on_init: &mut String,
    on_tick_ms: &mut u64,
    on_tick: &mut String,
    on_key_var: &mut String,
    on_key: &mut String,
) -> Result<Vec<UiNode>, String> {
    let mut nodes = Vec::new();
    let items = split_block_items(block);

    for item in items {
        let s = item.trim();
        if s.is_empty() || s.starts_with('#') || s.starts_with("//") {
            continue;
        }

        // state { ... }
        if let Some(rest) = s.strip_prefix("state") {
            if let Some((o, c)) = find_matching_braces(rest) {
                for line in rest[o + 1..c].lines() {
                    let l = line.trim();
                    if l.is_empty() || l.starts_with('#') || l.starts_with("//") {
                        continue;
                    }
                    if let Some((k, v)) = l.split_once('=') {
                        let key = k.trim().to_string();
                        let val_s = v.trim();
                        if val_s == "[]" {
                            vars.insert(key, Val::List(Vec::new()));
                        } else if let Some((q, _)) = extract_quoted(val_s) {
                            vars.insert(key, Val::Str(q.to_string()));
                        } else if val_s == "true" {
                            vars.insert(key, Val::Int(1));
                        } else if val_s == "false" {
                            vars.insert(key, Val::Int(0));
                        } else if let Ok(num) = val_s.parse::<i64>() {
                            vars.insert(key, Val::Int(num));
                        } else {
                            return Err(format!("Invalid initial state value '{}'", val_s));
                        }
                    }
                }
                continue;
            }
        }

        // Legacy `var name = 0`
        if let Some(rest) = s.strip_prefix("var ") {
            let (k, v) = rest
                .split_once('=')
                .ok_or_else(|| String::from("Expected '=' in var declaration"))?;
            let val_s = v.trim();
            if let Some((q, _)) = extract_quoted(val_s) {
                vars.insert(k.trim().to_string(), Val::Str(q.to_string()));
            } else if val_s == "[]" {
                vars.insert(k.trim().to_string(), Val::List(Vec::new()));
            } else {
                let n = val_s
                    .parse::<i64>()
                    .map_err(|_| format!("Invalid var value '{}'", val_s))?;
                vars.insert(k.trim().to_string(), Val::Int(n));
            }
            continue;
        }

        // on_init { ... }
        if let Some(rest) = s.strip_prefix("on_init") {
            if let Some((o, c)) = find_matching_braces(rest) {
                *on_init = rest[o + 1..c].trim().to_string();
                continue;
            }
        }

        // on_tick(150) { ... }
        if let Some(rest) = s.strip_prefix("on_tick") {
            let (o, c) = find_matching_braces(rest)
                .ok_or_else(|| String::from("Expected { ... } block in on_tick"))?;
            let hdr = rest[..o].trim();
            let ms = hdr
                .trim_start