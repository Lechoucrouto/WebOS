use crate::fs::FileSystem;
use crate::gui::{rgba, Rect, CHAR_W};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub const WBO_MAGIC: &[u8; 4] = b"WBO\x02";

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
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
    List(Vec<Value>),
}

impl Value {
    pub fn as_int(&self) -> i64 {
        match self {
            Value::Int(n) => *n,
            Value::Bool(b) => i64::from(*b),
            Value::Str(s) => s.parse::<i64>().unwrap_or(0),
            Value::List(l) => l.len() as i64,
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Int(n) => *n != 0,
            Value::Bool(b) => *b,
            Value::Str(s) => !s.is_empty() && s != "false" && s != "0",
            Value::List(l) => !l.is_empty(),
        }
    }

    pub fn as_display_string(&self) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
            Value::Str(s) => s.clone(),
            Value::List(items) => {
                let inner: Vec<String> = items.iter().map(|v| v.as_display_string()).collect();
                format!("[{}]", inner.join(", "))
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Default,
    Primary,
    Accent,
    Danger,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone)]
pub enum CanvasDrawCmd {
    Clear(u32),
    Rect { x: String, y: String, w: String, h: String, color: u32 },
    Circle { x: String, y: String, r: String, color: u32 },
    Line { x1: String, y1: String, x2: String, y2: String, color: u32 },
}

#[derive(Clone)]
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
        content: String,
        scale: i32,
        align: TextAlign,
        color: u32,
        at: Option<(i32, i32)>,
    },
    Button {
        text: String,
        style: ButtonStyle,
        script: String,
        at: Option<(i32, i32, i32, i32)>,
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
        max_expr: String,
        color: u32,
    },
    Canvas {
        w: i32,
        h: i32,
        bg: u32,
        cmds: Vec<CanvasDrawCmd>,
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
pub enum RenderedWidgetKind {
    Card { bg: u32, border: u32, radius: i32 },
    Text { text: String, scale: i32, align: TextAlign, color: u32 },
    Button { text: String, style: ButtonStyle, script: String },
    Input { bind: String, text: String, placeholder: String },
    Slider { bind: String, min: i64, max: i64, current: i64 },
    Toggle { bind: String, label: String, checked: bool },
    Progress { ratio_pct: i32, color: u32 },
    CanvasBox { bg: u32, cmds: Vec<(u8, i32, i32, i32, i32, u32)> },
    GuideBox,
}

#[derive(Clone)]
pub struct RenderedWidget {
    pub rect: Rect,
    pub kind: RenderedWidgetKind,
}

#[derive(Clone)]
pub struct SysEffect {
    pub kind: String,
    pub arg1: String,
    pub arg2: String,
}

#[derive(Clone)]
pub struct DeclarativeRuntime {
    pub vars: BTreeMap<String, Value>,
    pub tree: Vec<UiNode>,
    pub init_script: String,
    pub tick_interval_ms: u64,
    pub tick_script: String,
    pub last_tick_ms: u64,
    pub key_var: String,
    pub key_script: String,
    pub focused_input: Option<String>,
    pub rng_state: u64,
    pub pending_effects: Vec<SysEffect>,
    pub initialized: bool,
}

impl DeclarativeRuntime {
    pub fn ensure_init(&mut self) {
        if !self.initialized {
            self.initialized = true;
            if !self.init_script.is_empty() {
                let s = self.init_script.clone();
                self.run_script(&s);
            }
        }
    }

    pub fn tick(&mut self, now_ms: u64) {
        self.ensure_init();
        if self.tick_interval_ms > 0 && !self.tick_script.is_empty() {
            if self.last_tick_ms == 0 {
                self.last_tick_ms = now_ms;
            } else if now_ms.saturating_sub(self.last_tick_ms) >= self.tick_interval_ms {
                self.last_tick_ms = now_ms;
                let s = self.tick_script.clone();
                self.run_script(&s);
            }
        }
    }

    pub fn on_key_event(&mut self, key_name: &str) {
        if let Some(bind_var) = self.focused_input.clone() {
            let mut current = self
                .vars
                .get(&bind_var)
                .map(|v| v.as_display_string())
                .unwrap_or_default();
            if key_name == "Backspace" {
                current.pop();
            } else if key_name == "Escape" || key_name == "Enter" {
                self.focused_input = None;
            } else if key_name.len() == 1 {
                current.push_str(key_name);
            }
            self.vars.insert(bind_var, Value::Str(current));
            return;
        }

        if !self.key_script.is_empty() {
            if !self.key_var.is_empty() {
                self.vars
                    .insert(self.key_var.clone(), Value::Str(key_name.to_string()));
            }
            let s = self.key_script.clone();
            self.run_script(&s);
        }
    }

    pub fn interpolate(&self, template: &str) -> String {
        let mut out = String::new();
        let mut chars = template.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '{' {
                let mut expr = String::new();
                let mut closed = false;
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    expr.push(c);
                }
                if closed {
                    let val = self.eval_expr_readonly(expr.trim());
                    out.push_str(&val.as_display_string());
                } else {
                    out.push('{');
                    out.push_str(&expr);
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    pub fn eval_expr_readonly(&self, expr: &str) -> Value {
        let mut clone_rt = self.clone();
        clone_rt.eval_expr(expr)
    }

    fn next_rand(&mut self, min: i64, max: i64) -> i64 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        if max <= min {
            return min;
        }
        let span = (max - min + 1) as u64;
        min + ((self.rng_state >> 32) % span) as i64
    }

    pub fn run_script(&mut self, script: &str) {
        let stmts = split_top_level_statements(script);
        for stmt in stmts {
            let s = stmt.trim();
            if s.is_empty() || s.starts_with('#') || s.starts_with("//") {
                continue;
            }

            if let Some(rest) = s.strip_prefix("if ") {
                if let Some((cond, then_body, else_body)) = extract_if_blocks(rest) {
                    if self.eval_expr(&cond).as_bool() {
                        self.run_script(&then_body);
                    } else if let Some(eb) = else_body {
                        self.run_script(&eb);
                    }
                }
                continue;
            }

            if let Some(rest) = s.strip_prefix("sys.open(") {
                if let Some(inner) = rest.strip_suffix(')') {
                    let target = self.eval_expr(inner.trim()).as_display_string();
                    self.pending_effects.push(SysEffect {
                        kind: String::from("open"),
                        arg1: target,
                        arg2: String::new(),
                    });
                }
                continue;
            }

            if let Some(rest) = s.strip_prefix("fs.write(") {
                if let Some(inner) = rest.strip_suffix(')') {
                    if let Some((p_expr, c_expr)) = split_top_level_comma(inner) {
                        let path = self.eval_expr(p_expr.trim()).as_display_string();
                        let content = self.eval_expr(c_expr.trim()).as_display_string();
                        self.pending_effects.push(SysEffect {
                            kind: String::from("fs_write"),
                            arg1: path,
                            arg2: content,
                        });
                    }
                }
                continue;
            }

            if let Some((lhs, rhs)) = split_assignment(s) {
                let var = lhs.trim();
                let val = self.eval_expr(rhs.trim());
                if !var.is_empty() {
                    self.vars.insert(var.to_string(), val);
                }
            } else {
                let _ = self.eval_expr(s);
            }
        }
    }

    pub fn eval_expr(&mut self, expr: &str) -> Value {
        let tokens = tokenize_expr(expr);
        if tokens.is_empty() {
            return Value::Int(0);
        }
        let mut pos = 0;
        self.parse_or(&tokens, &mut pos)
    }

    fn parse_or(&mut self, toks: &[ExprTok], pos: &mut usize) -> Value {
        let mut lhs = self.parse_and(toks, pos);
        while *pos < toks.len() && toks[*pos] == ExprTok::Op("||".into()) {
            *pos += 1;
            let rhs = self.parse_and(toks, pos);
            lhs = Value::Bool(lhs.as_bool() || rhs.as_bool());
        }
        lhs
    }

    fn parse_and(&mut self, toks: &[ExprTok], pos: &mut usize) -> Value {
        let mut lhs = self.parse_cmp(toks, pos);
        while *pos < toks.len() && toks[*pos] == ExprTok::Op("&&".into()) {
            *pos += 1;
            let rhs = self.parse_cmp(toks, pos);
            lhs = Value::Bool(lhs.as_bool() && rhs.as_bool());
        }
        lhs
    }

    fn parse_cmp(&mut self, toks: &[ExprTok], pos: &mut usize) -> Value {
        let mut lhs = self.parse_add(toks, pos);
        while *pos < toks.len() {
            let op = match &toks[*pos] {
                ExprTok::Op(o) if matches!(o.as_str(), "==" | "!=" | "<" | ">" | "<=" | ">=") => {
                    o.clone()
                }
                _ => break,
            };
            *pos += 1;
            let rhs = self.parse_add(toks, pos);
            let res = match op.as_str() {
                "==" => match (&lhs, &rhs) {
                    (Value::Str(a), Value::Str(b)) => a == b,
                    _ => lhs.as_int() == rhs.as_int(),
                },
                "!=" => match (&lhs, &rhs) {
                    (Value::Str(a), Value::Str(b)) => a != b,
                    _ => lhs.as_int() != rhs.as_int(),
                },
                "<" => lhs.as_int() < rhs.as_int(),
                ">" => lhs.as_int() > rhs.as_int(),
                "<=" => lhs.as_int() <= rhs.as_int(),
                ">=" => lhs.as_int() >= rhs.as_int(),
                _ => false,
            };
            lhs = Value::Bool(res);
        }
        lhs
    }

    fn parse_add(&mut self, toks: &[ExprTok], pos: &mut usize) -> Value {
        let mut lhs = self.parse_mul(toks, pos);
        while *pos < toks.len() {
            let op = match &toks[*pos] {
                ExprTok::Op(o) if o == "+" || o == "-" => o.clone(),
                _ => break,
            };
            *pos += 1;
            let rhs = self.parse_mul(toks, pos);
            if op == "+" {
                match (&lhs, &rhs) {
                    (Value::Str(a), _) => {
                        lhs = Value::Str(format!("{}{}", a, rhs.as_display_string()));
                    }
                    (_, Value::Str(b)) => {
                        lhs = Value::Str(format!("{}{}", lhs.as_display_string(), b));
                    }
                    _ => lhs = Value::Int(lhs.as_int().saturating_add(rhs.as_int())),
                }
            } else {
                lhs = Value::Int(lhs.as_int().saturating_sub(rhs.as_int()));
            }
        }
        lhs
    }

    fn parse_mul(&mut self, toks: &[ExprTok], pos: &mut usize) -> Value {
        let mut lhs = self.parse_unary(toks, pos);
        while *pos < toks.len() {
            let op = match &toks[*pos] {
                ExprTok::Op(o) if matches!(o.as_str(), "*" | "/" | "%") => o.clone(),
                _ => break,
            };
            *pos += 1;
            let rhs = self.parse_unary(toks, pos);
            let r = rhs.as_int();
            lhs = match op.as_str() {
                "*" => Value::Int(lhs.as_int().saturating_mul(r)),
                "/" => Value::Int(if r != 0 { lhs.as_int() / r } else { 0 }),
                "%" => Value::Int(if r != 0 { lhs.as_int() % r } else { 0 }),
                _ => lhs,
            };
        }
        lhs
    }

    fn parse_unary(&mut self, toks: &[ExprTok], pos: &mut usize) -> Value {
        if *pos < toks.len() {
            if toks[*pos] == ExprTok::Op("-".into()) {
                *pos += 1;
                let v = self.parse_unary(toks, pos);
                return Value::Int(-v.as_int());
            }
            if toks[*pos] == ExprTok::Op("!".into()) {
                *pos += 1;
                let v = self.parse_unary(toks, pos);
                return Value::Bool(!v.as_bool());
            }
        }
        self.parse_primary(toks, pos)
    }

    fn parse_primary(&mut self, toks: &[ExprTok], pos: &mut usize) -> Value {
        if *pos >= toks.len() {
            return Value::Int(0);
        }
        match toks[*pos].clone() {
            ExprTok::Num(n) => {
                *pos += 1;
                Value::Int(n)
            }
            ExprTok::Str(s) => {
                *pos += 1;
                Value::Str(s)
            }
            ExprTok::LParen => {
                *pos += 1;
                let v = self.parse_or(toks, pos);
                if *pos < toks.len() && toks[*pos] == ExprTok::RParen {
                    *pos += 1;
                }
                v
            }
            ExprTok::LBracket => {
                *pos += 1;
                let mut items = Vec::new();
                while *pos < toks.len() && toks[*pos] != ExprTok::RBracket {
                    items.push(self.parse_or(toks, pos));
                    if *pos < toks.len() && toks[*pos] == ExprTok::Comma {
                        *pos += 1;
                    } else {
                        break;
                    }
                }
                if *pos < toks.len() && toks[*pos] == ExprTok::RBracket {
                    *pos += 1;
                }
                Value::List(items)
            }
            ExprTok::Ident(name) => {
                *pos += 1;
                if name == "true" {
                    return Value::Bool(true);
                }
                if name == "false" {
                    return Value::Bool(false);
                }

                if *pos < toks.len() && toks[*pos] == ExprTok::LParen {
                    *pos += 1;
                    let mut args = Vec::new();
                    while *pos < toks.len() && toks[*pos] != ExprTok::RParen {
                        args.push(self.parse_or(toks, pos));
                        if *pos < toks.len() && toks[*pos] == ExprTok::Comma {
                            *pos += 1;
                        } else {
                            break;
                        }
                    }
                    if *pos < toks.len() && toks[*pos] == ExprTok::RParen {
                        *pos += 1;
                    }

                    if name == "sys.rand" && args.len() >= 2 {
                        return Value::Int(self.next_rand(args[0].as_int(), args[1].as_int()));
                    }
                    if name == "len" && !args.is_empty() {
                        return Value::Int(args[0].as_int());
                    }
                    if let Some((var_name, method)) = name.split_once('.') {
                        if method == "push" && !args.is_empty() {
                            if let Some(Value::List(list)) = self.vars.get_mut(var_name) {
                                list.push(args[0].clone());
                                return Value::Int(list.len() as i64);
                            }
                        } else if method == "clear" {
                            if let Some(Value::List(list)) = self.vars.get_mut(var_name) {
                                list.clear();
                                return Value::Int(0);
                            }
                        } else if method == "len" {
                            if let Some(v) = self.vars.get(var_name) {
                                return Value::Int(v.as_int());
                            }
                        }
                    }
                    return Value::Int(0);
                }

                self.vars.get(&name).cloned().unwrap_or(Value::Int(0))
            }
            _ => {
                *pos += 1;
                Value::Int(0)
            }
        }
    }

    pub fn layout(&mut self, bounds: Rect, show_guides: bool) -> Vec<RenderedWidget> {
        let mut out = Vec::new();
        let nodes = self.tree.clone();
        self.layout_sequence(&nodes, bounds, true, show_guides, &mut out);
        out
    }

    fn measure_node(&mut self, node: &UiNode, avail_w: i32) -> (i32, i32) {
        match node {
            UiNode::Col { pad, gap, children } => {
                let inner_w = (avail_w - pad * 2).max(20);
                let mut total_h = pad * 2;
                let mut count = 0;
                for ch in children {
                    let (_, h) = self.measure_node(ch, inner_w);
                    if h > 0 {
                        if count > 0 {
                            total_h += gap;
                        }
                        total_h += h;
                        count += 1;
                    }
                }
                (avail_w, total_h.max(24))
            }
            UiNode::Row { pad, children, .. } => {
                let mut max_h = 34;
                for ch in children {
                    let (_, h) = self.measure_node(ch, avail_w);
                    if h > max_h {
                        max_h = h;
                    }
                }
                (avail_w, max_h + pad * 2)
            }
            UiNode::Grid { cols, gap, children } => {
                let c = (*cols).max(1);
                let rows = ((children.len() as i32) + c - 1) / c;
                let h = rows * 36 + (rows - 1).max(0) * gap;
                (avail_w, h.max(36))
            }
            UiNode::Card { pad, children, .. } => {
                let inner_w = (avail_w - pad * 2).max(20);
                let mut total_h = pad * 2;
                for (i, ch) in children.iter().enumerate() {
                    let (_, h) = self.measure_node(ch, inner_w);
                    if i > 0 {
                        total_h += 6;
                    }
                    total_h += h;
                }
                (avail_w, total_h.max(32))
            }
            UiNode::Text { scale, .. } => (avail_w, 7 * scale + 6),
            UiNode::Button { .. } => (avail_w, 34),
            UiNode::Input { .. } => (avail_w, 32),
            UiNode::Slider { .. } => (avail_w, 30),
            UiNode::Toggle { .. } => (avail_w, 28),
            UiNode::Progress { .. } => (avail_w, 18),
            UiNode::Canvas { w, h, .. } => (*w, *h),
            UiNode::If { cond, then_nodes, else_nodes } => {
                let branch = if self.eval_expr(cond).as_bool() {
                    then_nodes
                } else {
                    else_nodes
                };
                let mut total_h = 0;
                for (i, ch) in branch.iter().enumerate() {
                    let (_, h) = self.measure_node(ch, avail_w);
                    if i > 0 {
                        total_h += 6;
                    }
                    total_h += h;
                }
                (avail_w, total_h)
            }
            UiNode::For { list_var, body, .. } => {
                let len = match self.vars.get(list_var) {
                    Some(Value::List(items)) => items.len() as i32,
                    _ => 0,
                };
                let mut item_h = 0;
                for ch in body {
                    item_h += self.measure_node(ch, avail_w).1 + 4;
                }
                (avail_w, len * item_h)
            }
        }
    }

    fn layout_sequence(
        &mut self,
        nodes: &[UiNode],
        bounds: Rect,
        vertical: bool,
        show_guides: bool,
        out: &mut Vec<RenderedWidget>,
    ) {
        if nodes.is_empty() || bounds.w <= 0 || bounds.h <= 0 {
            return;
        }

        if vertical {
            let mut cur_y = bounds.y;
            for node in nodes {
                let (_, h) = self.measure_node(node, bounds.w);
                if h <= 0 {
                    continue;
                }
                let slot = Rect::new(bounds.x, cur_y, bounds.w, h.min(bounds.y + bounds.h - cur_y));
                self.layout_single(node, slot, show_guides, out);
                cur_y += h + 6;
            }
        } else {
            let n = nodes.len() as i32;
            let gap = 8;
            let cell_w = ((bounds.w - gap * (n - 1)) / n).max(20);
            let mut cur_x = bounds.x;
            for node in nodes {
                let slot = Rect::new(cur_x, bounds.y, cell_w, bounds.h);
                self.layout_single(node, slot, show_guides, out);
                cur_x += cell_w + gap;
            }
        }
    }

    fn layout_single(
        &mut self,
        node: &UiNode,
        slot: Rect,
        show_guides: bool,
        out: &mut Vec<RenderedWidget>,
    ) {
        match node {
            UiNode::Col { pad, gap, children } => {
                if show_guides {
                    out.push(RenderedWidget {
                        rect: slot,
                        kind: RenderedWidgetKind::GuideBox,
                    });
                }
                let inner = Rect::new(
                    slot.x + pad,
                    slot.y + pad,
                    (slot.w - pad * 2).max(0),
                    (slot.h - pad * 2).max(0),
                );
                let mut cur_y = inner.y;
                for ch in children {
                    let (_, h) = self.measure_node(ch, inner.w);
                    if h <= 0 {
                        continue;
                    }
                    let cslot = Rect::new(inner.x, cur_y, inner.w, h);
                    self.layout_single(ch, cslot, show_guides, out);
                    cur_y += h + gap;
                }
            }
            UiNode::Row { pad, gap, children } => {
                if show_guides {
                    out.push(RenderedWidget {
                        rect: slot,
                        kind: RenderedWidgetKind::GuideBox,
                    });
                }
                if children.is_empty() {
                    return;
                }
                let inner = Rect::new(
                    slot.x + pad,
                    slot.y + pad,
                    (slot.w - pad * 2).max(0),
                    (slot.h - pad * 2).max(0),
                );
                let n = children.len() as i32;
                let cell_w = ((inner.w - gap * (n - 1)) / n).max(16);
                let mut cur_x = inner.x;
                for ch in children {
                    let (_, h) = self.measure_node(ch, cell_w);
                    let cslot = Rect::new(cur_x, inner.y, cell_w, h.min(inner.h).max(24));
                    self.layout_single(ch, cslot, show_guides, out);
                    cur_x += cell_w + gap;
                }
            }
            UiNode::Grid { cols, gap, children } => {
                if show_guides {
                    out.push(RenderedWidget {
                        rect: slot,
                        kind: RenderedWidgetKind::GuideBox,
                    });
                }
                let c = (*cols).max(1);
                let cell_w = ((slot.w - gap * (c - 1)) / c).max(16);
                let cell_h = 36;
                for (idx, ch) in children.iter().enumerate() {
                    let col = (idx as i32) % c;
                    let row = (idx as i32) / c;
                    let cx = slot.x + col * (cell_w + gap);
                    let cy = slot.y + row * (cell_h + gap);
                    self.layout_single(ch, Rect::new(cx, cy, cell_w, cell_h), show_guides, out);
                }
            }
            UiNode::Card { bg, border, pad, radius, children } => {
                out.push(RenderedWidget {
                    rect: slot,
                    kind: RenderedWidgetKind::Card {
                        bg: *bg,
                        border: *border,
                        radius: *radius,
                    },
                });
                let inner = Rect::new(
                    slot.x + pad,
                    slot.y + pad,
                    (slot.w - pad * 2).max(0),
                    (slot.h - pad * 2).max(0),
                );
                let mut cur_y = inner.y;
                for ch in children {
                    let (_, h) = self.measure_node(ch, inner.w);
                    self.layout_single(ch, Rect::new(inner.x, cur_y, inner.w, h), show_guides, out);
                    cur_y += h + 6;
                }
            }
            UiNode::Text { content, scale, align, color, at } => {
                let text = self.interpolate(content);
                let rect = if let Some((ax, ay)) = at {
                    Rect::new(*ax, *ay, (text.len() * CHAR_W) as i32, 7 * scale + 4)
                } else {
                    slot
                };
                out.push(RenderedWidget {
                    rect,
                    kind: RenderedWidgetKind::Text {
                        text,
                        scale: *scale,
                        align: *align,
                        color: *color,
                    },
                });
            }
            UiNode::Button { text, style, script, at } => {
                let rendered = self.interpolate(text);
                let rect = if let Some((ax, ay, aw, ah)) = at {
                    Rect::new(*ax, *ay, *aw, *ah)
                } else {
                    slot
                };
                out.push(RenderedWidget {
                    rect,
                    kind: RenderedWidgetKind::Button {
                        text: rendered,
                        style: *style,
                        script: script.clone(),
                    },
                });
            }
            UiNode::Input { bind, placeholder } => {
                let text = self
                    .vars
                    .get(bind)
                    .map(|v| v.as_display_string())
                    .unwrap_or_default();
                out.push(RenderedWidget {
                    rect: slot,
                    kind: RenderedWidgetKind::Input {
                        bind: bind.clone(),
                        text,
                        placeholder: placeholder.clone(),
                    },
                });
            }
            UiNode::Slider { bind, min, max } => {
                let current = self.vars.get(bind).map(|v| v.as_int()).unwrap_or(*min);
                out.push(RenderedWidget {
                    rect: slot,
                    kind: RenderedWidgetKind::Slider {
                        bind: bind.clone(),
                        min: *min,
                        max: *max,
                        current,
                    },
                });
            }
            UiNode::Toggle { bind, label } => {
                let checked = self.vars.get(bind).map(|v| v.as_bool()).unwrap_or(false);
                out.push(RenderedWidget {
                    rect: slot,
                    kind: RenderedWidgetKind::Toggle {
                        bind: bind.clone(),
                        label: label.clone(),
                        checked,
                    },
                });
            }
            UiNode::Progress { val_expr, max_expr, color } => {
                let v = self.eval_expr(val_expr).as_int();
                let m = self.eval_expr(max_expr).as_int().max(1);
                let pct = ((v * 100) / m).clamp(0, 100) as i32;
                out.push(RenderedWidget {
                    rect: slot,
                    kind: RenderedWidgetKind::Progress {
                        ratio_pct: pct,
                        color: *color,
                    },
                });
            }
            UiNode::Canvas { w, h, bg, cmds } => {
                let mut resolved_cmds = Vec::with_capacity(cmds.len());
                for c in cmds {
                    match c {
                        CanvasDrawCmd::Clear(col) => resolved_cmds.push((0, 0, 0, 0, 0, *col)),
                        CanvasDrawCmd::Rect { x, y, w, h, color } => {
                            resolved_cmds.push((
                                1,
                                self.eval_expr(x).as_int() as i32,
                                self.eval_expr(y).as_int() as i32,
                                self.eval_expr(w).as_int() as i32,
                                self.eval_expr(h).as_int() as i32,
                                *color,
                            ));
                        }
                        CanvasDrawCmd::Circle { x, y, r, color } => {
                            resolved_cmds.push((
                                2,
                                self.eval_expr(x).as_int() as i32,
                                self.eval_expr(y).as_int() as i32,
                                self.eval_expr(r).as_int() as i32,
                                0,
                                *color,
                            ));
                        }
                        CanvasDrawCmd::Line { x1, y1, x2, y2, color } => {
                            resolved_cmds.push((
                                3,
                                self.eval_expr(x1).as_int() as i32,
                                self.eval_expr(y1).as_int() as i32,
                                self.eval_expr(x2).as_int() as i32,
                                self.eval_expr(y2).as_int() as i32,
                                *color,
                            ));
                        }
                    }
                }
                out.push(RenderedWidget {
                    rect: Rect::new(slot.x, slot.y, *w, *h),
                    kind: RenderedWidgetKind::CanvasBox {
                        bg: *bg,
                        cmds: resolved_cmds,
                    },
                });
            }
            UiNode::If { cond, then_nodes, else_nodes } => {
                let branch = if self.eval_expr(cond).as_bool() {
                    then_nodes
                } else {
                    else_nodes
                };
                self.layout_sequence(branch, slot, true, show_guides, out);
            }
            UiNode::For { item_var, list_var, body } => {
                let items = match self.vars.get(list_var) {
                    Some(Value::List(l)) => l.clone(),
                    _ => Vec::new(),
                };
                let mut cur_y = slot.y;
                for item in items {
                    self.vars.insert(item_var.clone(), item);
                    for ch in body {
                        let (_, h) = self.measure_node(ch, slot.w);
                        self.layout_single(ch, Rect::new(slot.x, cur_y, slot.w, h), show_guides, out);
                        cur_y += h + 4;
                    }
                }
            }
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

    pub fn pack_declarative_source(width: u16, height: u16, title: &str, wbml_source: &str) -> Vec<u8> {
        let mut bin = Self::pack_system(WboSubsystem::DeclarativeApp, width, height, title);
        let src_bytes = wbml_source.as_bytes();
        let slen = src_bytes.len().min(u32::MAX as usize) as u32;
        bin.extend_from_slice(&slen.to_le_bytes());
        bin.extend_from_slice(&src_bytes[..slen as usize]);
        bin
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 10 || &bytes[0..4] != WBO_MAGIC {
            return Err("Invalid .wbo binary header (expected magic WBO\\x02)");
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
                return Err("Truncated .wbo payload length");
            }
            let slen = u32::from_le_bytes([
                bytes[cur],
                bytes[cur + 1],
                bytes[cur + 2],
                bytes[cur + 3],
            ]) as usize;
            if cur + 4 + slen > bytes.len() {
                return Err("Truncated .wbo payload");
            }
            let src = core::str::from_utf8(&bytes[cur + 4..cur + 4 + slen])
                .map_err(|_| "Invalid UTF-8 in .wbo payload")?;
            let (_, _, _, rt) = parse_wbml_program(src).map_err(|_| "Corrupt WBML bytecode")?;
            runtime = Some(rt);
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

pub fn parse_color(spec: &str) -> u32 {
    let s = spec.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(28);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(37);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(52);
            return rgba(r, g, b);
        }
    }
    match s {
        "dark" => rgba(13, 17, 23),
        "card" => rgba(22, 27, 34),
        "blue" => rgba(31, 111, 235),
        "green" => rgba(46, 160, 67),
        "amber" => rgba(210, 153, 34),
        "cyan" => rgba(57, 197, 207),
        "red" => rgba(218, 54, 51),
        "white" => rgba(240, 246, 252),
        "muted" => rgba(139, 148, 158),
        _ => rgba(28, 37, 52),
    }
}

pub fn compile_wbml_to_wbo(source: &str) -> Result<(String, Vec<u8>), String> {
    let (title, width, height, _) = parse_wbml_program(source)?;
    let bin = WboExecutable::pack_declarative_source(width, height, &title, source);
    Ok((title, bin))
}

pub fn parse_wbml_program(source: &str) -> Result<(String, u16, u16, DeclarativeRuntime), String> {
    let mut title = String::from("Untitled App");
    let mut width: u16 = 380;
    let mut height: u16 = 260;
    let mut vars: BTreeMap<String, Value> = BTreeMap::new();
    let mut init_script = String::new();
    let mut tick_interval_ms: u64 = 0;
    let mut tick_script = String::new();
    let mut key_var = String::new();
    let mut key_script = String::new();

    let trimmed = source.trim();
    let top_body = if let Some(rest) = trimmed.strip_prefix("app ") {
        let (t, after_q) = extract_quoted(rest)
            .ok_or_else(|| String::from("Expected quoted title after 'app'"))?;
        title = t.to_string();
        if let Some(brace_pos) = after_q.find('{') {
            let dims: Vec<&str> = after_q[..brace_pos].split_whitespace().collect();
            if dims.len() >= 2 {
                width = dims[0].parse::<u16>().unwrap_or(380).max(220);
                height = dims[1].parse::<u16>().unwrap_or(260).max(140);
            }
            extract_braced_body(&after_q[brace_pos..])
                .ok_or_else(|| String::from("Unclosed '{' on app block"))?
        } else {
            let first_nl = trimmed.find('\n').unwrap_or(trimmed.len());
            let dims: Vec<&str> = after_q.split_whitespace().collect();
            if dims.len() >= 2 {
                width = dims[0].parse::<u16>().unwrap_or(380).max(220);
                height = dims[1].parse::<u16>().unwrap_or(260).max(140);
            }
            trimmed[first_nl..].to_string()
        }
    } else {
        trimmed.to_string()
    };

    let mut tree = Vec::new();
    parse_ui_block(
        &top_body,
        &mut vars,
        &mut init_script,
        &mut tick_interval_ms,
        &mut tick_script,
        &mut key_var,
        &mut key_script,
        &mut tree,
    )?;

    let runtime = DeclarativeRuntime {
        vars,
        tree,
        init_script,
        tick_interval_ms,
        tick_script,
        last_tick_ms: 0,
        key_var,
        key_script,
        focused_input: None,
        rng_state: 0x9E3779B97F4A7C15,
        pending_effects: Vec::new(),
        initialized: false,
    };

    Ok((title, width, height, runtime))
}

#[allow(clippy::too_many_arguments)]
fn parse_ui_block(
    block: &str,
    vars: &mut BTreeMap<String, Value>,
    init_script: &mut String,
    tick_interval_ms: &mut u64,
    tick_script: &mut String,
    key_var: &mut String,
    key_script: &mut String,
    out_nodes: &mut Vec<UiNode>,
) -> Result<(), String> {
    let items = split_wbml_items(block)?;
    for item in items {
        let line = item.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }

        if let Some(rest) = line.strip_prefix("state") {
            let body = extract_braced_body(rest.trim())
                .ok_or_else(|| String::from("Expected '{ ... }' after state"))?;
            let mut tmp_rt = DeclarativeRuntime {
                vars: vars.clone(),
                tree: Vec::new(),
                init_script: String::new(),
                tick_interval_ms: 0,
                tick_script: String::new(),
                last_tick_ms: 0,
                key_var: String::new(),
                key_script: String::new(),
                focused_input: None,
                rng_state: 1,
                pending_effects: Vec::new(),
                initialized: true,
            };
            tmp_rt.run_script(&body);
            *vars = tmp_rt.vars;
        } else if let Some(rest) = line.strip_prefix("var ") {
            if let Some((lhs, rhs)) = rest.split_once('=') {
                let mut tmp_rt = DeclarativeRuntime {
                    vars: vars.clone(),
                    tree: Vec::new(),
                    init_script: String::new(),
                    tick_interval_ms: 0,
                    tick_script: String::new(),
                    last_tick_ms: 0,
                    key_var: String::new(),
                    key_script: String::new(),
                    focused_input: None,
                    rng_state: 1,
                    pending_effects: Vec::new(),
                    initialized: true,
                };
                let val = tmp_rt.eval_expr(rhs.trim());
                vars.insert(lhs.trim().to_string(), val);
            }
        } else if let Some(rest) = line.strip_prefix("on_init") {
            let body = extract_braced_body(rest.trim())
                .ok_or_else(|| String::from("Expected '{ ... }' after on_init"))?;
            *init_script = body;
        } else if let Some(rest) = line.strip_prefix("on_tick") {
            let r = rest.trim();
            let (ms_str, after_paren) = extract_paren_arg(r)
                .ok_or_else(|| String::from("Usage: on_tick(ms) { ... }"))?;
            *tick_interval_ms = ms_str.trim().parse::<u64>().unwrap_or(100).max(16);
            *tick_script = extract_braced_body(after_paren.trim())
                .ok_or_else(|| String::from("Expected '{ ... }' after on_tick(...)"))?;
        } else if let Some(rest) = line.strip_prefix("on_key") {
            let r = rest.trim();
            let (kvar, after_paren) = extract_paren_arg(r)
                .ok_or_else(|| String::from("Usage: on_key(k) { ... }"))?;
            *key_var = kvar.trim().to_string();
            *key_script = extract_braced_body(after_paren.trim())
                .ok_or_else(|| String::from("Expected '{ ... }' after on_key(...)"))?;
        } else if let Some(rest) = line.strip_prefix("col") {
            let (header, body) = split_header_and_body(rest)?;
            let attrs = parse_attrs(&header);
            let pad = attrs.get("pad").and_then(|s| s.parse().ok()).unwrap_or(0);
            let gap = attrs.get("gap").and_then(|s| s.parse().ok()).unwrap_or(8);
            let mut children = Vec::new();
            parse_ui_block(&body, vars, init_script, tick_interval_ms, tick_script, key_var, key_script, &mut children)?;
            out_nodes.push(UiNode::Col { pad, gap, children });
        } else if let Some(rest) = line.strip_prefix("row") {
            let (header, body) = split_header_and_body(rest)?;
            let attrs = parse_attrs(&header);
            let pad = attrs.get("pad").and_then(|s| s.parse().ok()).unwrap_or(0);
            let gap = attrs.get("gap").and_then(|s| s.parse().ok()).unwrap_or(8);
            let mut children = Vec::new();
            parse_ui_block(&body, vars, init_script, tick_interval_ms, tick_script, key_var, key_script, &mut children)?;
            out_nodes.push(UiNode::Row { pad, gap, children });
        } else if let Some(rest) = line.strip_prefix("grid") {
            let (header, body) = split_header_and_body(rest)?;
            let attrs = parse_attrs(&header);
            let cols = attrs.get("cols").and_then(|s| s.parse().ok()).unwrap_or(2);
            let gap = attrs.get("gap").and_then(|s| s.parse().ok()).unwrap_or(8);
            let mut children = Vec::new();
            parse_ui_block(&body, vars, init_script, tick_interval_ms, tick_script, key_var, key_script, &mut children)?;
            out_nodes.push(UiNode::Grid { cols, gap, children });
        } else if let Some(rest) = line.strip_prefix("card") {
            let (header, body) = split_header_and_body(rest)?;
            let attrs = parse_attrs(&header);
            let bg = attrs.get("bg").map(|s| parse_color(s)).unwrap_or_else(|| rgba(13, 17, 23));
            let border = attrs.get("border").map(|s| parse_color(s)).unwrap_or_else(|| rgba(48, 54, 61));
            let pad = attrs.get("pad").and_then(|s| s.parse().ok()).unwrap_or(10);
            let radius = attrs.get("radius").and_then(|s| s.parse().ok()).unwrap_or(6);
            let mut children = Vec::new();
            parse_ui_block(&body, vars, init_script, tick_interval_ms, tick_script, key_var, key_script, &mut children)?;
            out_nodes.push(UiNode::Card { bg, border, pad, radius, children });
        } else if let Some(rest) = line.strip_prefix("text ") {
            let (content, after_q) = extract_quoted(rest)
                .ok_or_else(|| String::from("Expected quoted string after 'text'"))?;
            let attrs = parse_attrs(after_q);
            let scale = match attrs.get("size").map(|s| s.as_str()) {
                Some("lg") => 3,
                Some("sm") => 1,
                _ => 2,
            };
            let align = match attrs.get("align").map(|s| s.as_str()) {
                Some("center") => TextAlign::Center,
                Some("right") => TextAlign::Right,
                _ => TextAlign::Left,
            };
            let color = attrs
                .get("color")
                .map(|s| parse_color(s))
                .unwrap_or_else(|| rgba(230, 237, 243));
            out_nodes.push(UiNode::Text {
                content: content.to_string(),
                scale,
                align,
                color,
                at: None,
            });
        } else if let Some(rest) = line.strip_prefix("label ") {
            let (content, _) = extract_quoted(rest)
                .ok_or_else(|| String::from("Expected quoted text in label"))?;
            let prefix_end = rest.find('"').unwrap_or(0);
            let toks: Vec<&str> = rest[..prefix_end].split_whitespace().collect();
            let x = toks.get(1).and_then(|s| s.parse().ok()).unwrap_or(16);
            let y = toks.get(2).and_then(|s| s.parse().ok()).unwrap_or(16);
            out_nodes.push(UiNode::Text {
                content: content.to_string(),
                scale: 2,
                align: TextAlign::Left,
                color: rgba(230, 237, 243),
                at: Some((x, y)),
            });
        } else if let Some(rest) = line.strip_prefix("btn ") {
            let (text, after_q) = extract_quoted(rest)
                .ok_or_else(|| String::from("Expected quoted text after 'btn'"))?;
            let (attr_part, script) = if let Some(b0) = after_q.find('{') {
                let body = extract_braced_body(&after_q[b0..])
                    .ok_or_else(|| String::from("Unclosed '{' in btn"))?;
                (&after_q[..b0], body)
            } else {
                (after_q, String::new())
            };
            let attrs = parse_attrs(attr_part);
            let style = match attrs.get("style").map(|s| s.as_str()) {
                Some("primary") => ButtonStyle::Primary,
                Some("accent") => ButtonStyle::Accent,
                Some("danger") => ButtonStyle::Danger,
                _ => ButtonStyle::Default,
            };
            out_nodes.push(UiNode::Button {
                text: text.to_string(),
                style,
                script,
                at: None,
            });
        } else if let Some(rest) = line.strip_prefix("button ") {
            let (text, after_q) = extract_quoted(rest)
                .ok_or_else(|| String::from("Expected quoted text in button"))?;
            let prefix_end = rest.find('"').unwrap_or(0);
            let toks: Vec<&str> = rest[..prefix_end].split_whitespace().collect();
            let x = toks.get(1).and_then(|s| s.parse().ok()).unwrap_or(16);
            let y = toks.get(2).and_then(|s| s.parse().ok()).unwrap_or(16);
            let w = toks.get(3).and_then(|s| s.parse().ok()).unwrap_or(90);
            let h = toks.get(4).and_then(|s| s.parse().ok()).unwrap_or(30);
            let script = if let Some(b0) = after_q.find('{') {
                extract_braced_body(&after_q[b0..]).unwrap_or_default()
            } else {
                String::new()
            };
            out_nodes.push(UiNode::Button {
                text: text.to_string(),
                style: ButtonStyle::Default,
                script,
                at: Some((x, y, w, h)),
            });
        } else if let Some(rest) = line.strip_prefix("input") {
            let attrs = parse_attrs(rest);
            let bind = attrs.get("bind").cloned().unwrap_or_else(|| String::from("text"));
            let placeholder = attrs
                .get("placeholder")
                .cloned()
                .unwrap_or_else(|| String::from("Type here..."));
            out_nodes.push(UiNode::Input { bind, placeholder });
        } else if let Some(rest) = line.strip_prefix("slider") {
            let attrs = parse_attrs(rest);
            let bind = attrs.get("bind").cloned().unwrap_or_else(|| String::from("val"));
            let min = attrs.get("min").and_then(|s| s.parse().ok()).unwrap_or(0);
            let max = attrs.get("max").and_then(|s| s.parse().ok()).unwrap_or(100);
            out_nodes.push(UiNode::Slider { bind, min, max });
        } else if let Some(rest) = line.strip_prefix("toggle") {
            let attrs = parse_attrs(rest);
            let bind = attrs.get("bind").cloned().unwrap_or_else(|| String::from("flag"));
            let label = attrs.get("label").cloned().unwrap_or_else(|| bind.clone());
            out_nodes.push(UiNode::Toggle { bind, label });
        } else if let Some(rest) = line.strip_prefix("progress") {
            let attrs = parse_attrs(rest);
            let val_expr = attrs.get("val").cloned().unwrap_or_else(|| String::from("50"));
            let max_expr = attrs.get("max").cloned().unwrap_or_else(|| String::from("100"));
            let color = attrs
                .get("color")
                .map(|s| parse_color(s))
                .unwrap_or_else(|| rgba(88, 242, 132));
            out_nodes.push(UiNode::Progress {
                val_expr,
                max_expr,
                color,
            });
        } else if let Some(rest) = line.strip_prefix("canvas") {
            let (header, body) = split_header_and_body(rest)?;
            let attrs = parse_attrs(&header);
            let w = attrs.get("w").and_then(|s| s.parse().ok()).unwrap_or(240);
            let h = attrs.get("h").and_then(|s| s.parse().ok()).unwrap_or(160);
            let bg = attrs.get("bg").map(|s| parse_color(s)).unwrap_or_else(|| rgba(10, 14, 20));
            let mut cmds = Vec::new();
            for raw_cmd in body.lines() {
                let c = raw_cmd.trim();
                if c.is_empty() || c.starts_with('#') {
                    continue;
                }
                let p: Vec<&str> = c.split_whitespace().collect();
                if p.is_empty() {
                    continue;
                }
                match p[0] {
                    "clear" if p.len() >= 2 => cmds.push(CanvasDrawCmd::Clear(parse_color(p[1]))),
                    "rect" if p.len() >= 6 => cmds.push(CanvasDrawCmd::Rect {
                        x: p[1].into(),
                        y: p[2].into(),
                        w: p[3].into(),
                        h: p[4].into(),
                        color: parse_color(p[5]),
                    }),
                    "circle" if p.len() >= 5 => cmds.push(CanvasDrawCmd::Circle {
                        x: p[1].into(),
                        y: p[2].into(),
                        r: p[3].into(),
                        color: parse_color(p[4]),
                    }),
                    "line" if p.len() >= 6 => cmds.push(CanvasDrawCmd::Line {
                        x1: p[1].into(),
                        y1: p[2].into(),
                        x2: p[3].into(),
                        y2: p[4].into(),
                        color: parse_color(p[5]),
                    }),
                    _ => {}
                }
            }
            out_nodes.push(UiNode::Canvas { w, h, bg, cmds });
        } else if let Some(rest) = line.strip_prefix("if ") {
            let (cond, then_body, else_body) = extract_if_blocks(rest)
                .ok_or_else(|| String::from("Invalid 'if' block in UI"))?;
            let mut then_nodes = Vec::new();
            parse_ui_block(&then_body, vars, init_script, tick_interval_ms, tick_script, key_var, key_script, &mut then_nodes)?;
            let mut else_nodes = Vec::new();
            if let Some(eb) = else_body {
                parse_ui_block(&eb, vars, init_script, tick_interval_ms, tick_script, key_var, key_script, &mut else_nodes)?;
            }
            out_nodes.push(UiNode::If {
                cond,
                then_nodes,
                else_nodes,
            });
        } else if let Some(rest) = line.strip_prefix("for ") {
            let b0 = rest.find('{').ok_or_else(|| String::from("Expected '{' in for loop"))?;
            let hdr = rest[..b0].trim();
            let (item_var, list_var) = hdr
                .split_once(" in ")
                .ok_or_else(|| String::from("Usage: for item in items { ... }"))?;
            let body_str = extract_braced_body(&rest[b0..])
                .ok_or_else(|| String::from("Unclosed '{' in for loop"))?;
            let mut body = Vec::new();
            parse_ui_block(&body_str, vars, init_script, tick_interval_ms, tick_script, key_var, key_script, &mut body)?;
            out_nodes.push(UiNode::For {
                item_var: item_var.trim().to_string(),
                list_var: list_var.trim().to_string(),
                body,
            });
        } else if let Some(rest) = line.strip_prefix("rect ") {
            let toks: Vec<&str> = rest.split_whitespace().collect();
            if toks.len() >= 4 {
                let x = toks[0].parse().unwrap_or(10);
                let y = toks[1].parse().unwrap_or(10);
                let col = parse_color(toks.get(4).copied().unwrap_or("dark"));
                out_nodes.push(UiNode::Text {
                    content: String::new(),
                    scale: 1,
                    align: TextAlign::Left,
                    color: col,
                    at: Some((x, y)),
                });
            }
        } else {
            return Err(format!("Unknown WBML directive: '{}'", line));
        }
    }
    Ok(())
}

fn extract_quoted(s: &str) -> Option<(&str, &str)> {
    let start = s.find('"')?;
    let rest = &s[start + 1..];
    let end = rest.find('"')?;
    Some((&rest[..end], rest[end + 1..].trim()))
}

fn extract_paren_arg(s: &str) -> Option<(&str, &str)> {
    let start = s.find('(')?;
    let rest = &s[start + 1..];
    let end = rest.find(')')?;
    Some((&rest[..end], &rest[end + 1..]))
}

fn extract_braced_body(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    for (i, ch) in s[start..].char_indices() {
        if ch == '"' {
            in_str = !in_str;
        } else if !in_str {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start + 1..start + i].to_string());
                }
            }
        }
    }
    None
}

fn split_header_and_body(s: &str) -> Result<(String, String), String> {
    let b0 = s.find('{').ok_or_else(|| String::from("Expected '{' block"))?;
    let body = extract_braced_body(&s[b0..]).ok_or_else(|| String::from("Unclosed '{' block"))?;
    Ok((s[..b0].trim().to_string(), body))
}

fn extract_if_blocks(rest: &str) -> Option<(String, String, Option<String>)> {
    let b0 = rest.find('{')?;
    let cond = rest[..b0].trim().to_string();
    let then_body = extract_braced_body(&rest[b0..])?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut end_idx = 0;
    for (i, ch) in rest[b0..].char_indices() {
        if ch == '"' {
            in_str = !in_str;
        } else if !in_str {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    end_idx = b0 + i + 1;
                    break;
                }
            }
        }
    }
    let after = rest[end_idx..].trim();
    if let Some(after_else) = after.strip_prefix("else") {
        let else_body = extract_braced_body(after_else.trim())?;
        Some((cond, then_body, Some(else_body)))
    } else {
        Some((cond, then_body, None))
    }
}

fn parse_attrs(s: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut key = String::new();
        while let Some(&kc) = chars.peek() {
            if kc == '=' || kc.is_whitespace() {
                break;
            }
            key.push(kc);
            chars.next();
        }
        if chars.peek() == Some(&'=') {
            chars.next();
            let mut val = String::new();
            if chars.peek() == Some(&'"') {
                chars.next();
                while let Some(vc) = chars.next() {
                    if vc == '"' {
                        break;
                    }
                    val.push(vc);
                }
            } else {
                while let Some(&vc) = chars.peek() {
                    if vc.is_whitespace() {
                        break;
                    }
                    val.push(vc);
                    chars.next();
                }
            }
            map.insert(key, val);
        }
    }
    map
}

fn split_wbml_items(src: &str) -> Result<Vec<String>, String> {
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str = false;

    for ch in src.chars() {
        if ch == '"' {
            in_str = !in_str;
            cur.push(ch);
        } else if !in_str {
            if ch == '{' {
                depth += 1;
                cur.push(ch);
            } else if ch == '}' {
                depth -= 1;
                cur.push(ch);
                if depth < 0 {
                    return Err(String::from("Unexpected '}'"));
                }
            } else if (ch == '\n' || ch == ';') && depth == 0 {
                let t = cur.trim();
                if !t.is_empty() {
                    items.push(t.to_string());
                }
                cur.clear();
            } else {
                cur.push(ch);
            }
        } else {
            cur.push(ch);
        }
    }
    if depth != 0 {
        return Err(String::from("Unclosed '{' block"));
    }
    let t = cur.trim();
    if !t.is_empty() {
        items.push(t.to_string());
    }
    Ok(items)
}

fn split_top_level_statements(script: &str) -> Vec<String> {
    split_wbml_items(script).unwrap_or_default()
}

fn split_top_level_comma(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    let mut in_str = false;
    for (i, ch) in s.char_indices() {
        if ch == '"' {
            in_str = !in_str;
        } else if !in_str {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => return Some((&s[..i], &s[i + 1..])),
                _ => {}
            }
        }
    }
    None
}

fn split_assignment(stmt: &str) -> Option<(&str, &str)> {
    let mut in_str = false;
    let bytes = stmt.as_bytes();
    for (i, ch) in stmt.char_indices() {
        if ch == '"' {
            in_str = !in_str;
        } else if !in_str && ch == '=' {
            let prev = if i > 0 { bytes[i - 1] } else { 0 };
            let next = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
            if !matches!(prev, b'=' | b'!' | b'<' | b'>') && next != b'=' {
                return Some((&stmt[..i], &stmt[i + 1..]));
            }
        }
    }
    None
}

#[derive(Clone, PartialEq)]
enum ExprTok {
    Num(i64),
    Str(String),
    Ident(String),
    Op(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
}

fn tokenize_expr(expr: &str) -> Vec<ExprTok> {
    let mut toks = Vec::new();
    let mut chars = expr.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c.is_ascii_digit() {
            let mut num = String::new();
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    num.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            toks.push(ExprTok::Num(num.parse().unwrap_or(0)));
        } else if c == '"' {
            chars.next();
            let mut s = String::new();
            while let Some(sc) = chars.next() {
                if sc == '"' {
                    break;
                }
                s.push(sc);
            }
            toks.push(ExprTok::Str(s));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let mut id = String::new();
            while let Some(&ic) = chars.peek() {
                if ic.is_ascii_alphanumeric() || ic == '_' || ic == '.' {
                    id.push(ic);
                    chars.next();
                } else {
                    break;
                }
            }
            toks.push(ExprTok::Ident(id));
        } else if c == '(' {
            chars.next();
            toks.push(ExprTok::LParen);
        } else if c == ')' {
            chars.next();
            toks.push(ExprTok::RParen);
        } else if c == '[' {
            chars.next();
            toks.push(ExprTok::LBracket);
        } else if c == ']' {
            chars.next();
            toks.push(ExprTok::RBracket);
        } else if c == ',' {
            chars.next();
            toks.push(ExprTok::Comma);
        } else {
            let first = chars.next().unwrap();
            let mut op = String::new();
            op.push(first);
            if let Some(&next) = chars.peek() {
                if (matches!(first, '=' | '!' | '<' | '>') && next == '=')
                    || (first == '&' && next == '&')
                    || (first == '|' && next == '|')
                {
                    op.push(chars.next().unwrap());
                }
            }
            toks.push(ExprTok::Op(op));
        }
    }
    toks
}

pub fn install_system_binaries(fs: &mut FileSystem) {
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/Terminal.wbo",
        WboExecutable::pack_system(WboSubsystem::Terminal, 680, 420, "Terminal"),
    );
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/Files.wbo",
        WboExecutable::pack_system(WboSubsystem::Files, 480, 360, "File Explorer"),
    );
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/SysMon.wbo",
        WboExecutable::pack_system(WboSubsystem::SysMon, 440, 240, "System Monitor"),
    );
    let _ = fs.write_bytes(
        "C:/WebOS/bin/Apps/Editor.wbo",
        WboExecutable::pack_system(WboSubsystem::Editor, 960, 520, "Code Editor"),
    );

    for (src, out) in [
        ("C:/usr/counter.wbml", "C:/WebOS/bin/Apps/Counter.wbo"),
        ("C:/usr/calculator.wbml", "C:/WebOS/bin/Apps/Calculator.wbo"),
        ("C:/usr/snake.wbml", "C:/WebOS/bin/Apps/Arcade_Runner.wbo"),
    ] {
        if let Some(code) = fs.read_str(src).map(|s| s.to_string()) {
            if let Ok((_, bin)) = compile_wbml_to_wbo(&code) {
                let _ = fs.write_bytes(out, bin);
            }
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

    let mut out = String::new();
    let mut i = 0;
    let bytes = line.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let rem = &line[i..];
            if rem.starts_with("$CWD") {
                out.push_str(cwd);
                i += 4;
                continue;
            } else if rem.starts_with("$rest") {
                out.push_str(&rest);
                i += 5;
                continue;
            } else if rem.starts_with("$1") {
                out.push_str(a1);
                i += 2;
                continue;
            } else if rem.starts_with("$2") {
                out.push_str(a2);
                i += 2;
                continue;
            }
        }
        if let Some(ch) = line[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

pub fn format_wbo_inspection(bytes: &[u8]) -> String {
    match WboExecutable::parse(bytes) {
        Ok(exe) => {
            let vars_count = exe.runtime.as_ref().map(|r| r.vars.len()).unwrap_or(0);
            format!(
                "[WBOv2] \"{}\" | Subsys: {} | {}x{} | State Vars: {}",
                exe.title, exe.subsystem as u8, exe.width, exe.height, vars_count
            )
        }
        Err(e) => format!("[Invalid WBO]: {}", e),
    }
}