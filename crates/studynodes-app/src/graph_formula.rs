use std::f64::consts::{E, PI};

use rune::{Color, El, LineCap, Shape, TextEdit, TextStyle, canvas, col, el, row, text};
use studynodes_graph::model::NodeMeta;

use crate::theme;

pub const DEFAULT_CENTER_X: f64 = 0.0;
pub const DEFAULT_CENTER_Y: f64 = 0.0;
pub const DEFAULT_SPAN_X: f64 = 12.0;
pub const MIN_SPAN_X: f64 = 1e-4;
pub const MAX_SPAN_X: f64 = 1e5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphViewport {
    pub center_x: f64,
    pub center_y: f64,
    pub span_x: f64,
}

impl Default for GraphViewport {
    fn default() -> Self {
        Self {
            center_x: DEFAULT_CENTER_X,
            center_y: DEFAULT_CENTER_Y,
            span_x: DEFAULT_SPAN_X,
        }
    }
}

impl GraphViewport {
    pub fn from_meta(meta: &NodeMeta) -> Self {
        let cx = meta
            .graph_center_x
            .filter(|v| v.is_finite())
            .unwrap_or(DEFAULT_CENTER_X);
        let cy = meta
            .graph_center_y
            .filter(|v| v.is_finite())
            .unwrap_or(DEFAULT_CENTER_Y);
        let sx = meta
            .graph_span_x
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(DEFAULT_SPAN_X)
            .clamp(MIN_SPAN_X, MAX_SPAN_X);
        Self {
            center_x: cx,
            center_y: cy,
            span_x: sx,
        }
    }

    pub fn write_to_meta(&self, meta: &mut NodeMeta) {
        meta.graph_center_x = Some(self.center_x);
        meta.graph_center_y = Some(self.center_y);
        meta.graph_span_x = Some(self.span_x.clamp(MIN_SPAN_X, MAX_SPAN_X));
    }

    #[inline]
    pub fn span_y(&self, w: f32, h: f32) -> f64 {
        let wf = (w as f64).max(1.0);
        let hf = (h as f64).max(1.0);
        self.span_x * (hf / wf)
    }

    #[inline]
    pub fn x_bounds(&self) -> (f64, f64) {
        let half = self.span_x * 0.5;
        (self.center_x - half, self.center_x + half)
    }

    #[inline]
    pub fn y_bounds(&self, w: f32, h: f32) -> (f64, f64) {
        let half = self.span_y(w, h) * 0.5;
        (self.center_y - half, self.center_y + half)
    }

    #[inline]
    pub fn math_to_px(&self, x: f64, y: f64, w: f32, h: f32) -> [f32; 2] {
        let wf = (w as f64).max(1.0);
        let hf = (h as f64).max(1.0);
        let sy = self.span_y(w, h);
        let px = ((x - (self.center_x - self.span_x * 0.5)) / self.span_x) * wf;
        let py = (((self.center_y + sy * 0.5) - y) / sy) * hf;
        [px as f32, py as f32]
    }

    #[inline]
    pub fn px_to_math(&self, px: f32, py: f32, w: f32, h: f32) -> (f64, f64) {
        let wf = (w as f64).max(1.0);
        let hf = (h as f64).max(1.0);
        let sy = self.span_y(w, h);
        let x = (self.center_x - self.span_x * 0.5) + (px as f64 / wf) * self.span_x;
        let y = (self.center_y + sy * 0.5) - (py as f64 / hf) * sy;
        (x, y)
    }

    /// Zoom around normalized plot coordinates `(norm_x, norm_y)` in `[0, 1]`.
    /// `factor < 1.0` zooms in (smaller span), `factor > 1.0` zooms out (larger span).
    pub fn zoom_at(&mut self, norm_x: f32, norm_y: f32, factor: f64, w: f32, h: f32) {
        let nx = (norm_x as f64).clamp(0.0, 1.0);
        let ny = (norm_y as f64).clamp(0.0, 1.0);
        let f = factor.clamp(0.05, 20.0);
        let (ax, ay) = self.px_to_math((nx * w as f64) as f32, (ny * h as f64) as f32, w, h);
        let new_span_x = (self.span_x * f).clamp(MIN_SPAN_X, MAX_SPAN_X);
        let new_span_y = new_span_x * ((h as f64).max(1.0) / (w as f64).max(1.0));
        self.span_x = new_span_x;
        self.center_x = ax - (nx - 0.5) * new_span_x;
        self.center_y = ay + (ny - 0.5) * new_span_y;
    }

    pub fn pan_by_px(&mut self, start_cx: f64, start_cy: f64, dx: f32, dy: f32, w: f32, h: f32) {
        let wf = (w as f64).max(1.0);
        let hf = (h as f64).max(1.0);
        let sy = self.span_y(w, h);
        self.center_x = start_cx - (dx as f64 / wf) * self.span_x;
        self.center_y = start_cy + (dy as f64 / hf) * sy;
    }

    pub fn zoom_ratio(&self) -> f64 {
        DEFAULT_SPAN_X / self.span_x.max(MIN_SPAN_X)
    }
}

pub fn format_zoom_badge(vp: &GraphViewport) -> String {
    let r = vp.zoom_ratio();
    if r >= 100.0 {
        format!("{:.0}×", r)
    } else if r >= 10.0 {
        format!("{:.1}×", r)
    } else if r >= 0.1 {
        format!("{:.1}×", r)
    } else {
        format!("{:.2}×", r)
    }
}

// ---------------------------------------------------------------------------
// Math Tokenizer, AST, Parser, Simplifier, and Evaluator
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelOp {
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    VarX,
    VarY,
    Placeholder,
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Sqrt(Box<Expr>),
    Root(Box<Expr>, Box<Expr>),
    Abs(Box<Expr>),
    Factorial(Box<Expr>),
    DegToRad(Box<Expr>),
    Call(String, Vec<Expr>),
    Deriv(Box<Expr>),
    IndefIntegral(Box<Expr>),
    DefIntegral(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlotStatement {
    ExplicitY {
        raw: String,
        expr: Expr,
    },
    ExplicitX {
        raw: String,
        expr: Expr,
    },
    EquationX {
        raw: String,
        lhs: Expr,
        rhs: Expr,
    },
    ImplicitEq {
        raw: String,
        lhs: Expr,
        rhs: Expr,
    },
    Inequality {
        raw: String,
        lhs: Expr,
        rhs: Expr,
        op: RelOp,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    PlaceholderIncomplete,
    Message(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Empty => write!(f, "Enter a formula or equation"),
            ParseError::PlaceholderIncomplete => write!(f, "Fill in □ placeholder"),
            ParseError::Message(m) => write!(f, "{m}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Placeholder,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Bang,
    Deg,
    Sqrt,
    NthRootPrefix(f64),
    Integral,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Pipe,
    Comma,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
}

fn superscript_char_to_ascii(ch: char) -> Option<char> {
    match ch {
        '⁰' => Some('0'),
        '¹' => Some('1'),
        '²' => Some('2'),
        '³' => Some('3'),
        '⁴' => Some('4'),
        '⁵' => Some('5'),
        '⁶' => Some('6'),
        '⁷' => Some('7'),
        '⁸' => Some('8'),
        '⁹' => Some('9'),
        '⁻' => Some('-'),
        '⁺' => Some('+'),
        'ⁿ' => Some('n'),
        _ => None,
    }
}

fn subscript_char_to_ascii(ch: char) -> Option<char> {
    match ch {
        '₀' => Some('0'),
        '₁' => Some('1'),
        '₂' => Some('2'),
        '₃' => Some('3'),
        '₄' => Some('4'),
        '₅' => Some('5'),
        '₆' => Some('6'),
        '₇' => Some('7'),
        '₈' => Some('8'),
        '₉' => Some('9'),
        _ => None,
    }
}

fn preprocess_tex_and_unicode(input: &str) -> String {
    let mut s = input
        .replace("\\frac", "frac")
        .replace("\\sqrt", "sqrt")
        .replace("\\sin", "sin")
        .replace("\\cos", "cos")
        .replace("\\tan", "tan")
        .replace("\\cot", "cot")
        .replace("\\sec", "sec")
        .replace("\\csc", "csc")
        .replace("\\arcsin", "arcsin")
        .replace("\\arccos", "arccos")
        .replace("\\arctan", "arctan")
        .replace("\\ln", "ln")
        .replace("\\log", "log")
        .replace("\\exp", "exp")
        .replace("\\abs", "abs")
        .replace("\\pi", "pi")
        .replace("\\infty", "inf")
        .replace("\\cdot", "*")
        .replace("\\times", "*")
        .replace("\\div", "/")
        .replace("\\leq", "<=")
        .replace("\\le", "<=")
        .replace("\\geq", ">=")
        .replace("\\ge", ">=")
        .replace("\\left|", "|")
        .replace("\\right|", "|")
        .replace("\\left(", "(")
        .replace("\\right)", ")");

    // Convert simple `frac{A}{B}` -> `(A)/(B)` and `sqrt[N]{X}` -> `root(N, X)` and `sqrt{X}` -> `sqrt(X)`
    s = rewrite_tex_braced_frac(&s);
    s = rewrite_tex_braced_sqrt(&s);
    s = s.replace('{', "(").replace('}', ")");

    // Replace Unicode math symbols
    let mut out = String::with_capacity(s.len() + 8);
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '×' | '·' => out.push('*'),
            '÷' => out.push('/'),
            '−' | '–' | '—' => out.push('-'),
            'π' => {
                if out
                    .chars()
                    .last()
                    .is_some_and(|p| p.is_ascii_alphanumeric() || p == ')')
                {
                    out.push('*');
                }
                out.push_str("pi");
            }
            '∞' => out.push_str("inf"),
            '≤' => out.push_str("<="),
            '≥' => out.push_str(">="),
            '∛' => out.push_str("³√"),
            '∜' => out.push_str("⁴√"),
            _ if subscript_char_to_ascii(c).is_some() => {
                out.push(subscript_char_to_ascii(c).unwrap_or('0'));
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

fn find_matching_brace(chars: &[char], open_idx: usize) -> Option<usize> {
    if chars.get(open_idx) != Some(&'{') {
        return None;
    }
    let mut depth = 0usize;
    for (idx, &ch) in chars.iter().enumerate().skip(open_idx) {
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn rewrite_tex_braced_frac(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("frac{") {
            let open1 = i + 4;
            if let Some(close1) = find_matching_brace(&chars, open1) {
                if chars.get(close1 + 1) == Some(&'{') {
                    if let Some(close2) = find_matching_brace(&chars, close1 + 1) {
                        let a: String = chars[open1 + 1..close1].iter().collect();
                        let b: String = chars[close1 + 2..close2].iter().collect();
                        out.push_str(&format!(
                            "(({})/({}))",
                            rewrite_tex_braced_frac(&a),
                            rewrite_tex_braced_frac(&b)
                        ));
                        i = close2 + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn rewrite_tex_braced_sqrt(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("sqrt[") {
            if let Some(rel_rb) = rest[5..].find(']') {
                let rb = i + 5 + rel_rb;
                if chars.get(rb + 1) == Some(&'{') {
                    if let Some(close) = find_matching_brace(&chars, rb + 1) {
                        let idx_str: String = chars[i + 5..rb].iter().collect();
                        let rad: String = chars[rb + 2..close].iter().collect();
                        out.push_str(&format!(
                            "root({}, {})",
                            idx_str,
                            rewrite_tex_braced_sqrt(&rad)
                        ));
                        i = close + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn tokenize(raw: &str) -> Result<Vec<Tok>, ParseError> {
    let s = preprocess_tex_and_unicode(raw);
    let chars: Vec<char> = s.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // Check superscript cluster: either before √ (nth root index) or after atom (exponent)
        if superscript_char_to_ascii(c).is_some() {
            let mut sup = String::new();
            let mut j = i;
            while j < chars.len() {
                if let Some(mapped) = superscript_char_to_ascii(chars[j]) {
                    sup.push(mapped);
                    j += 1;
                } else {
                    break;
                }
            }
            // Skip optional spaces before checking if √ follows
            let mut k = j;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            if chars.get(k) == Some(&'√') {
                let n = sup.parse::<f64>().unwrap_or(f64::NAN);
                if n.is_finite() {
                    toks.push(Tok::NthRootPrefix(n));
                    i = k + 1;
                    continue;
                }
            }
            // Otherwise emit ^ (exponent)
            toks.push(Tok::Caret);
            if sup == "n" {
                toks.push(Tok::Ident("n".into()));
            } else if let Ok(v) = sup.parse::<f64>() {
                if v < 0.0 {
                    toks.push(Tok::LParen);
                    toks.push(Tok::Minus);
                    toks.push(Tok::Num(-v));
                    toks.push(Tok::RParen);
                } else {
                    toks.push(Tok::Num(v));
                }
            } else {
                return Err(ParseError::Message(format!("Invalid superscript '{sup}'")));
            }
            i = j;
            continue;
        }

        match c {
            '□' => {
                toks.push(Tok::Placeholder);
                i += 1;
            }
            '+' => {
                toks.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                toks.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                toks.push(Tok::Star);
                i += 1;
            }
            '/' => {
                toks.push(Tok::Slash);
                i += 1;
            }
            '^' => {
                toks.push(Tok::Caret);
                i += 1;
            }
            '!' => {
                toks.push(Tok::Bang);
                i += 1;
            }
            '°' => {
                toks.push(Tok::Deg);
                i += 1;
            }
            '√' => {
                toks.push(Tok::Sqrt);
                i += 1;
            }
            '∫' => {
                toks.push(Tok::Integral);
                i += 1;
            }
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            '[' => {
                toks.push(Tok::LBracket);
                i += 1;
            }
            ']' => {
                toks.push(Tok::RBracket);
                i += 1;
            }
            '|' => {
                toks.push(Tok::Pipe);
                i += 1;
            }
            ',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            '=' => {
                toks.push(Tok::Eq);
                i += 1;
            }
            '<' => {
                if chars.get(i + 1) == Some(&'=') {
                    toks.push(Tok::Le);
                    i += 2;
                } else {
                    toks.push(Tok::Lt);
                    i += 1;
                }
            }
            '>' => {
                if chars.get(i + 1) == Some(&'=') {
                    toks.push(Tok::Ge);
                    i += 2;
                } else {
                    toks.push(Tok::Gt);
                    i += 1;
                }
            }
            _ if c.is_ascii_digit() || c == '.' => {
                let start = i;
                let mut seen_dot = c == '.';
                i += 1;
                while i < chars.len() {
                    let ch = chars[i];
                    if ch.is_ascii_digit() {
                        i += 1;
                    } else if ch == '.' && !seen_dot {
                        seen_dot = true;
                        i += 1;
                    } else {
                        break;
                    }
                }
                // Scientific notation like 1e-3 or 2.5e4
                if (chars.get(i) == Some(&'e') || chars.get(i) == Some(&'E'))
                    && (chars.get(i + 1).is_some_and(|ch| ch.is_ascii_digit())
                        || ((chars.get(i + 1) == Some(&'+') || chars.get(i + 1) == Some(&'-'))
                            && chars.get(i + 2).is_some_and(|ch| ch.is_ascii_digit())))
                {
                    i += 1;
                    if chars.get(i) == Some(&'+') || chars.get(i) == Some(&'-') {
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let num_str: String = chars[start..i].iter().collect();
                let val = num_str
                    .parse::<f64>()
                    .map_err(|_| ParseError::Message(format!("Invalid number '{num_str}'")))?;
                toks.push(Tok::Num(val));
            }
            _ if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let ident: String = chars[start..i].iter().collect();
                let lower = ident.to_ascii_lowercase();
                if is_known_ident_or_func(&lower) {
                    toks.push(Tok::Ident(lower));
                } else {
                    // Split implicit multi-letter variable runs like "xy" -> "x", "y" or "ax" -> "a", "x"
                    let mut remaining = lower.as_str();
                    while !remaining.is_empty() {
                        if let Some(prefix) = KNOWN_FUNCS
                            .iter()
                            .copied()
                            .find(|fn_name| remaining.starts_with(fn_name))
                        {
                            toks.push(Tok::Ident(prefix.to_string()));
                            remaining = &remaining[prefix.len()..];
                        } else if remaining.starts_with("pi") {
                            toks.push(Tok::Ident("pi".into()));
                            remaining = &remaining[2..];
                        } else if remaining.starts_with("dx") && remaining.len() == 2 {
                            toks.push(Tok::Ident("dx".into()));
                            remaining = "";
                        } else {
                            let ch = remaining.chars().next().unwrap();
                            toks.push(Tok::Ident(ch.to_string()));
                            remaining = &remaining[ch.len_utf8()..];
                        }
                    }
                }
            }
            _ => {
                return Err(ParseError::Message(format!("Unexpected character '{c}'")));
            }
        }
    }

    Ok(toks)
}

const KNOWN_FUNCS: &[&str] = &[
    "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "sqrt", "cbrt", "nthroot", "root",
    "log10", "log2", "sign", "sgn", "asin", "acos", "atan", "floor", "ceil", "round", "sin",
    "cos", "tan", "cot", "sec", "csc", "exp", "abs", "log", "ln", "min", "max", "int",
];

fn is_known_ident_or_func(s: &str) -> bool {
    matches!(s, "x" | "y" | "e" | "pi" | "inf" | "dx" | "d" | "n")
        || KNOWN_FUNCS.contains(&s)
}

fn is_func_name(s: &str) -> bool {
    KNOWN_FUNCS.contains(&s)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    pipe_depth: usize,
}

impl Parser {
    fn new(toks: Vec<Tok>) -> Self {
        Self {
            toks,
            pos: 0,
            pipe_depth: 0,
        }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn peek_n(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.pos + n)
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn starts_implicit_mul(&self) -> bool {
        match self.peek() {
            Some(
                Tok::Num(_)
                | Tok::Ident(_)
                | Tok::LParen
                | Tok::LBracket
                | Tok::Sqrt
                | Tok::NthRootPrefix(_)
                | Tok::Integral
                | Tok::Placeholder,
            ) => true,
            Some(Tok::Pipe) if self.pipe_depth == 0 => true,
            _ => false,
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_add()
    }

    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_mul()?;
        loop {
            match self.peek() {
                Some(Tok::Plus) => {
                    self.bump();
                    let rhs = self.parse_mul()?;
                    lhs = Expr::Add(Box::new(lhs), Box::new(rhs));
                }
                Some(Tok::Minus) => {
                    self.bump();
                    let rhs = self.parse_mul()?;
                    lhs = Expr::Sub(Box::new(lhs), Box::new(rhs));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Tok::Star) => {
                    self.bump();
                    let rhs = self.parse_unary()?;
                    lhs = Expr::Mul(Box::new(lhs), Box::new(rhs));
                }
                Some(Tok::Slash) => {
                    self.bump();
                    let rhs = self.parse_unary()?;
                    lhs = Expr::Div(Box::new(lhs), Box::new(rhs));
                }
                _ if self.starts_implicit_mul() => {
                    // Don't consume trailing `dx` inside an integral body as implicit multiplication
                    if matches!(self.peek(), Some(Tok::Ident(s)) if s == "dx") {
                        break;
                    }
                    let rhs = self.parse_unary()?;
                    lhs = Expr::Mul(Box::new(lhs), Box::new(rhs));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(Tok::Plus) => {
                self.bump();
                self.parse_unary()
            }
            Some(Tok::Minus) => {
                self.bump();
                let inner = self.parse_unary()?;
                Ok(Expr::Neg(Box::new(inner)))
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Result<Expr, ParseError> {
        let base = self.parse_postfix()?;
        if matches!(self.peek(), Some(Tok::Caret)) {
            self.bump();
            let exp = self.parse_unary()?;
            Ok(Expr::Pow(Box::new(base), Box::new(exp)))
        } else {
            Ok(base)
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Tok::Bang) => {
                    self.bump();
                    expr = Expr::Factorial(Box::new(expr));
                }
                Some(Tok::Deg) => {
                    self.bump();
                    expr = Expr::DegToRad(Box::new(expr));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        // Check `d / dx (...)` or `d / dx expr`
        if matches!(self.peek(), Some(Tok::Ident(s)) if s == "d")
            && matches!(self.peek_n(1), Some(Tok::Slash))
            && matches!(self.peek_n(2), Some(Tok::Ident(s)) if s == "dx")
        {
            self.pos += 3;
            let body = if matches!(self.peek(), Some(Tok::LParen)) {
                self.bump();
                let inner = self.parse_expr()?;
                if !matches!(self.bump(), Some(Tok::RParen)) {
                    return Err(ParseError::Message("Expected ')' after d/dx(...)".into()));
                }
                inner
            } else {
                self.parse_unary()?
            };
            return Ok(Expr::Deriv(Box::new(body)));
        }

        match self.bump() {
            Some(Tok::Num(v)) => Ok(Expr::Num(v)),
            Some(Tok::Placeholder) => Ok(Expr::Placeholder),
            Some(Tok::Sqrt) => {
                let rad = if matches!(self.peek(), Some(Tok::LParen)) {
                    self.bump();
                    let inner = self.parse_expr()?;
                    if !matches!(self.bump(), Some(Tok::RParen)) {
                        return Err(ParseError::Message("Expected ')' after √(".into()));
                    }
                    inner
                } else {
                    self.parse_power()?
                };
                Ok(Expr::Sqrt(Box::new(rad)))
            }
            Some(Tok::NthRootPrefix(n)) => {
                let rad = if matches!(self.peek(), Some(Tok::LParen)) {
                    self.bump();
                    let inner = self.parse_expr()?;
                    if !matches!(self.bump(), Some(Tok::RParen)) {
                        return Err(ParseError::Message("Expected ')' after root(".into()));
                    }
                    inner
                } else {
                    self.parse_power()?
                };
                Ok(Expr::Root(Box::new(Expr::Num(n)), Box::new(rad)))
            }
            Some(Tok::Integral) => self.parse_integral(),
            Some(Tok::LParen) => {
                let inner = self.parse_expr()?;
                if !matches!(self.bump(), Some(Tok::RParen)) {
                    return Err(ParseError::Message("Unclosed '('".into()));
                }
                Ok(inner)
            }
            Some(Tok::LBracket) => {
                let inner = self.parse_expr()?;
                if !matches!(self.bump(), Some(Tok::RBracket)) {
                    return Err(ParseError::Message("Unclosed '['".into()));
                }
                Ok(inner)
            }
            Some(Tok::Pipe) => {
                self.pipe_depth += 1;
                let inner = self.parse_expr()?;
                self.pipe_depth = self.pipe_depth.saturating_sub(1);
                if !matches!(self.bump(), Some(Tok::Pipe)) {
                    return Err(ParseError::Message("Unclosed '|...|'".into()));
                }
                Ok(Expr::Abs(Box::new(inner)))
            }
            Some(Tok::Ident(name)) => {
                match name.as_str() {
                    "x" => Ok(Expr::VarX),
                    "y" => Ok(Expr::VarY),
                    "pi" => Ok(Expr::Num(PI)),
                    "e" => Ok(Expr::Num(E)),
                    "inf" => Ok(Expr::Num(f64::INFINITY)),
                    _ if is_func_name(&name) => {
                        if matches!(self.peek(), Some(Tok::LParen)) {
                            self.bump();
                            let mut args = Vec::new();
                            if !matches!(self.peek(), Some(Tok::RParen)) {
                                args.push(self.parse_expr()?);
                                while matches!(self.peek(), Some(Tok::Comma)) {
                                    self.bump();
                                    args.push(self.parse_expr()?);
                                }
                            }
                            if !matches!(self.bump(), Some(Tok::RParen)) {
                                return Err(ParseError::Message(format!(
                                    "Expected ')' after {name}(...)"
                                )));
                            }
                            if name == "root" || name == "nthroot" {
                                if args.len() == 2 {
                                    let idx = args.remove(0);
                                    let rad = args.remove(0);
                                    return Ok(Expr::Root(Box::new(idx), Box::new(rad)));
                                } else {
                                    return Err(ParseError::Message(
                                        "root(n, expr) expects 2 arguments".into(),
                                    ));
                                }
                            }
                            if name == "int" && args.len() == 3 {
                                let a = args.remove(0);
                                let b = args.remove(0);
                                let f = args.remove(0);
                                return Ok(Expr::DefIntegral(
                                    Box::new(a),
                                    Box::new(b),
                                    Box::new(f),
                                ));
                            }
                            if args.is_empty() {
                                return Err(ParseError::Message(format!(
                                    "{name}() requires an argument"
                                )));
                            }
                            Ok(Expr::Call(name, args))
                        } else {
                            // Allow `sin x` without parentheses
                            let arg = self.parse_power()?;
                            Ok(Expr::Call(name, vec![arg]))
                        }
                    }
                    _ => Err(ParseError::Message(format!("Unknown identifier '{name}'"))),
                }
            }
            Some(other) => Err(ParseError::Message(format!(
                "Unexpected token '{other:?}'"
            ))),
            None => Err(ParseError::Message("Unexpected end of expression".into())),
        }
    }

    fn parse_integral(&mut self) -> Result<Expr, ParseError> {
        let mut args = Vec::new();
        if matches!(self.peek(), Some(Tok::LParen)) {
            self.bump();
            args.push(self.parse_expr()?);
            while matches!(self.peek(), Some(Tok::Comma)) {
                self.bump();
                args.push(self.parse_expr()?);
            }
            if !matches!(self.bump(), Some(Tok::RParen)) {
                return Err(ParseError::Message("Expected ')' in integral".into()));
            }
        } else {
            args.push(self.parse_expr()?);
        }
        // Optional trailing `dx` (or `d * x`)
        if matches!(self.peek(), Some(Tok::Ident(s)) if s == "dx") {
            self.bump();
        } else if matches!(self.peek(), Some(Tok::Ident(s)) if s == "d")
            && matches!(self.peek_n(1), Some(Tok::Ident(s)) if s == "x")
        {
            self.pos += 2;
        }
        match args.len() {
            1 => Ok(Expr::IndefIntegral(Box::new(args.remove(0)))),
            3 => {
                let a = args.remove(0);
                let b = args.remove(0);
                let body = args.remove(0);
                Ok(Expr::DefIntegral(Box::new(a), Box::new(b), Box::new(body)))
            }
            _ => Err(ParseError::Message(
                "Integral expects ∫(expr)dx or ∫(a, b, expr)dx".into(),
            )),
        }
    }
}

fn contains_placeholder(expr: &Expr) -> bool {
    match expr {
        Expr::Placeholder => true,
        Expr::Num(_) | Expr::VarX | Expr::VarY => false,
        Expr::Neg(a)
        | Expr::Sqrt(a)
        | Expr::Abs(a)
        | Expr::Factorial(a)
        | Expr::DegToRad(a)
        | Expr::Deriv(a)
        | Expr::IndefIntegral(a) => contains_placeholder(a),
        Expr::Add(a, b)
        | Expr::Sub(a, b)
        | Expr::Mul(a, b)
        | Expr::Div(a, b)
        | Expr::Pow(a, b)
        | Expr::Root(a, b) => contains_placeholder(a) || contains_placeholder(b),
        Expr::DefIntegral(a, b, c) => {
            contains_placeholder(a) || contains_placeholder(b) || contains_placeholder(c)
        }
        Expr::Call(_, args) => args.iter().any(contains_placeholder),
    }
}

fn uses_var(expr: &Expr, var: char) -> bool {
    match expr {
        Expr::VarX => var == 'x',
        Expr::VarY => var == 'y',
        Expr::Num(_) | Expr::Placeholder => false,
        Expr::Neg(a)
        | Expr::Sqrt(a)
        | Expr::Abs(a)
        | Expr::Factorial(a)
        | Expr::DegToRad(a) => uses_var(a, var),
        Expr::Deriv(a) | Expr::IndefIntegral(a) => var == 'x' || uses_var(a, var),
        Expr::Add(a, b)
        | Expr::Sub(a, b)
        | Expr::Mul(a, b)
        | Expr::Div(a, b)
        | Expr::Pow(a, b)
        | Expr::Root(a, b) => uses_var(a, var) || uses_var(b, var),
        Expr::DefIntegral(a, b, c) => uses_var(a, var) || uses_var(b, var) || uses_var(c, var),
        Expr::Call(_, args) => args.iter().any(|a| uses_var(a, var)),
    }
}

pub fn parse_single_statement(line: &str) -> Result<PlotStatement, ParseError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    // Strip trailing `= □` or trailing `=` so partial equations like `2x + ³√(27) = □` plot cleanly!
    let cleaned = trimmed
        .strip_suffix("= □")
        .or_else(|| trimmed.strip_suffix("=□"))
        .or_else(|| trimmed.strip_suffix('='))
        .map(str::trim)
        .unwrap_or(trimmed);
    if cleaned.is_empty() {
        return Err(ParseError::Empty);
    }

    let toks = tokenize(cleaned)?;
    if toks.is_empty() {
        return Err(ParseError::Empty);
    }

    // Detect `f(x) = ...` or `g(x) = ...` prefix cleanly before parsing
    if toks.len() >= 5
        && matches!(&toks[0], Tok::Ident(_))
        && matches!(toks[1], Tok::LParen)
        && matches!(&toks[2], Tok::Ident(v) if v == "x")
        && matches!(toks[3], Tok::RParen)
    {
        if let Some(op_tok) = toks.get(4) {
            if matches!(
                op_tok,
                Tok::Eq | Tok::Lt | Tok::Gt | Tok::Le | Tok::Ge
            ) {
                let mut rhs_p = Parser::new(toks[5..].to_vec());
                let rhs = rhs_p.parse_expr()?;
                if rhs_p.pos < rhs_p.toks.len() {
                    return Err(ParseError::Message("Unexpected trailing tokens".into()));
                }
                if contains_placeholder(&rhs) {
                    return Err(ParseError::PlaceholderIncomplete);
                }
                return Ok(match op_tok {
                    Tok::Eq => PlotStatement::ExplicitY {
                        raw: cleaned.to_string(),
                        expr: rhs,
                    },
                    Tok::Lt | Tok::Gt | Tok::Le | Tok::Ge => PlotStatement::Inequality {
                        raw: cleaned.to_string(),
                        lhs: Expr::VarY,
                        rhs,
                        op: match op_tok {
                            Tok::Lt => RelOp::Lt,
                            Tok::Gt => RelOp::Gt,
                            Tok::Le => RelOp::Le,
                            _ => RelOp::Ge,
                        },
                    },
                    _ => unreachable!(),
                });
            }
        }
    }

    // Split on top-level relation operator if present
    let mut rel_idx = None;
    let mut rel_op = None;
    for (idx, t) in toks.iter().enumerate() {
        let op = match t {
            Tok::Eq => Some(RelOp::Eq),
            Tok::Lt => Some(RelOp::Lt),
            Tok::Gt => Some(RelOp::Gt),
            Tok::Le => Some(RelOp::Le),
            Tok::Ge => Some(RelOp::Ge),
            _ => None,
        };
        if let Some(op) = op {
            if rel_idx.is_some() {
                return Err(ParseError::Message(
                    "Only one relation (=, <, >, ≤, ≥) per line".into(),
                ));
            }
            rel_idx = Some(idx);
            rel_op = Some(op);
        }
    }

    if let (Some(idx), Some(op)) = (rel_idx, rel_op) {
        if idx == 0 || idx + 1 >= toks.len() {
            return Err(ParseError::Message("Incomplete equation".into()));
        }
        let mut lhs_p = Parser::new(toks[..idx].to_vec());
        let lhs = lhs_p.parse_expr()?;
        if lhs_p.pos < lhs_p.toks.len() {
            return Err(ParseError::Message("Unexpected token before relation".into()));
        }
        let mut rhs_p = Parser::new(toks[idx + 1..].to_vec());
        let rhs = rhs_p.parse_expr()?;
        if rhs_p.pos < rhs_p.toks.len() {
            return Err(ParseError::Message("Unexpected token after relation".into()));
        }
        if contains_placeholder(&lhs) || contains_placeholder(&rhs) {
            return Err(ParseError::PlaceholderIncomplete);
        }

        if op == RelOp::Eq {
            if matches!(lhs, Expr::VarY) && !uses_var(&rhs, 'y') {
                return Ok(PlotStatement::ExplicitY {
                    raw: cleaned.to_string(),
                    expr: rhs,
                });
            }
            if matches!(rhs, Expr::VarY) && !uses_var(&lhs, 'y') {
                return Ok(PlotStatement::ExplicitY {
                    raw: cleaned.to_string(),
                    expr: lhs,
                });
            }
            if matches!(lhs, Expr::VarX) && !uses_var(&rhs, 'x') {
                return Ok(PlotStatement::ExplicitX {
                    raw: cleaned.to_string(),
                    expr: rhs,
                });
            }
            if matches!(rhs, Expr::VarX) && !uses_var(&lhs, 'x') {
                return Ok(PlotStatement::ExplicitX {
                    raw: cleaned.to_string(),
                    expr: lhs,
                });
            }
            if !uses_var(&lhs, 'y') && !uses_var(&rhs, 'y') && (uses_var(&lhs, 'x') || uses_var(&rhs, 'x')) {
                return Ok(PlotStatement::EquationX {
                    raw: cleaned.to_string(),
                    lhs,
                    rhs,
                });
            }
            return Ok(PlotStatement::ImplicitEq {
                raw: cleaned.to_string(),
                lhs,
                rhs,
            });
        } else {
            return Ok(PlotStatement::Inequality {
                raw: cleaned.to_string(),
                lhs,
                rhs,
                op,
            });
        }
    }

    let mut p = Parser::new(toks);
    let expr = p.parse_expr()?;
    if p.pos < p.toks.len() {
        return Err(ParseError::Message("Unexpected trailing tokens".into()));
    }
    if contains_placeholder(&expr) {
        return Err(ParseError::PlaceholderIncomplete);
    }
    if uses_var(&expr, 'y') && !uses_var(&expr, 'x') {
        Ok(PlotStatement::ImplicitEq {
            raw: cleaned.to_string(),
            lhs: expr,
            rhs: Expr::Num(0.0),
        })
    } else {
        Ok(PlotStatement::ExplicitY {
            raw: cleaned.to_string(),
            expr,
        })
    }
}

pub fn parse_statements(content: &str) -> (Vec<PlotStatement>, Option<String>) {
    let mut out = Vec::new();
    let mut first_err: Option<String> = None;
    for raw_line in content.split(['\n', ';']) {
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        match parse_single_statement(line) {
            Ok(stmt) => out.push(stmt),
            Err(ParseError::Empty) => {}
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e.to_string());
                }
            }
        }
    }
    (out, first_err)
}

/// Real gamma function via Lanczos approximation for smooth factorial evaluation `x! = Γ(x + 1)`.
fn gamma(z: f64) -> f64 {
    const COEFFS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if z < 0.5 {
        let s = (PI * z).sin();
        if s.abs() < 1e-12 {
            return f64::NAN;
        }
        PI / (s * gamma(1.0 - z))
    } else {
        let zm1 = z - 1.0;
        let mut x = COEFFS[0];
        for (i, &c) in COEFFS.iter().enumerate().skip(1) {
            x += c / (zm1 + i as f64);
        }
        let t = zm1 + 7.5;
        (2.0 * PI).sqrt() * t.powf(zm1 + 0.5) * (-t).exp() * x
    }
}

fn eval_factorial(v: f64) -> f64 {
    if !v.is_finite() || v > 170.0 {
        return f64::NAN;
    }
    if v >= 0.0 && (v - v.round()).abs() < 1e-9 && v <= 20.0 {
        let n = v.round() as u64;
        let mut acc = 1.0f64;
        for k in 2..=n {
            acc *= k as f64;
        }
        return acc;
    }
    gamma(v + 1.0)
}

fn eval_root(n: f64, rad: f64) -> f64 {
    if !n.is_finite() || !rad.is_finite() || n == 0.0 {
        return f64::NAN;
    }
    if rad >= 0.0 {
        rad.powf(1.0 / n)
    } else if (n - n.round()).abs() < 1e-9 && (n.round() as i64).rem_euclid(2) == 1 {
        -(-rad).powf(1.0 / n)
    } else {
        f64::NAN
    }
}

fn eval_simpson(body: &Expr, a: f64, b: f64, y: f64, depth: u8) -> f64 {
    if !a.is_finite() || !b.is_finite() {
        return f64::NAN;
    }
    if (b - a).abs() < 1e-12 {
        return 0.0;
    }
    let steps: usize = 32;
    let h = (b - a) / steps as f64;
    let mut sum = eval_expr_inner(body, a, y, depth + 1) + eval_expr_inner(body, b, y, depth + 1);
    if !sum.is_finite() {
        return f64::NAN;
    }
    for i in 1..steps {
        let t = a + i as f64 * h;
        let ft = eval_expr_inner(body, t, y, depth + 1);
        if !ft.is_finite() {
            return f64::NAN;
        }
        sum += if i % 2 == 0 { 2.0 * ft } else { 4.0 * ft };
    }
    sum * h / 3.0
}

pub fn eval_expr(expr: &Expr, x: f64, y: f64) -> f64 {
    eval_expr_inner(expr, x, y, 0)
}

fn eval_expr_inner(expr: &Expr, x: f64, y: f64, depth: u8) -> f64 {
    if depth > 48 {
        return f64::NAN;
    }
    match expr {
        Expr::Num(v) => *v,
        Expr::VarX => x,
        Expr::VarY => y,
        Expr::Placeholder => f64::NAN,
        Expr::Neg(a) => -eval_expr_inner(a, x, y, depth + 1),
        Expr::Add(a, b) => eval_expr_inner(a, x, y, depth + 1) + eval_expr_inner(b, x, y, depth + 1),
        Expr::Sub(a, b) => eval_expr_inner(a, x, y, depth + 1) - eval_expr_inner(b, x, y, depth + 1),
        Expr::Mul(a, b) => eval_expr_inner(a, x, y, depth + 1) * eval_expr_inner(b, x, y, depth + 1),
        Expr::Div(a, b) => {
            let num = eval_expr_inner(a, x, y, depth + 1);
            let den = eval_expr_inner(b, x, y, depth + 1);
            if den.abs() < 1e-12 {
                f64::NAN
            } else {
                num / den
            }
        }
        Expr::Pow(a, b) => {
            let base = eval_expr_inner(a, x, y, depth + 1);
            let exp = eval_expr_inner(b, x, y, depth + 1);
            if base < 0.0 && (exp - exp.round()).abs() > 1e-9 {
                f64::NAN
            } else {
                base.powf(exp)
            }
        }
        Expr::Sqrt(a) => {
            let v = eval_expr_inner(a, x, y, depth + 1);
            if v < 0.0 {
                f64::NAN
            } else {
                v.sqrt()
            }
        }
        Expr::Root(n_expr, rad_expr) => {
            let n = eval_expr_inner(n_expr, x, y, depth + 1);
            let rad = eval_expr_inner(rad_expr, x, y, depth + 1);
            eval_root(n, rad)
        }
        Expr::Abs(a) => eval_expr_inner(a, x, y, depth + 1).abs(),
        Expr::Factorial(a) => eval_factorial(eval_expr_inner(a, x, y, depth + 1)),
        Expr::DegToRad(a) => eval_expr_inner(a, x, y, depth + 1) * (PI / 180.0),
        Expr::Deriv(body) => {
            let h = 1e-5 * (1.0 + x.abs());
            let f1 = eval_expr_inner(body, x + h, y, depth + 1);
            let f0 = eval_expr_inner(body, x - h, y, depth + 1);
            (f1 - f0) / (2.0 * h)
        }
        Expr::IndefIntegral(body) => eval_simpson(body, 0.0, x, y, depth + 1),
        Expr::DefIntegral(a, b, body) => {
            let va = eval_expr_inner(a, x, y, depth + 1);
            let vb = eval_expr_inner(b, x, y, depth + 1);
            eval_simpson(body, va, vb, y, depth + 1)
        }
        Expr::Call(name, args) => {
            let v = eval_expr_inner(&args[0], x, y, depth + 1);
            match name.as_str() {
                "sin" => v.sin(),
                "cos" => v.cos(),
                "tan" => {
                    let c = v.cos();
                    if c.abs() < 1e-9 {
                        f64::NAN
                    } else {
                        v.sin() / c
                    }
                }
                "cot" => {
                    let s = v.sin();
                    if s.abs() < 1e-9 {
                        f64::NAN
                    } else {
                        v.cos() / s
                    }
                }
                "sec" => {
                    let c = v.cos();
                    if c.abs() < 1e-9 {
                        f64::NAN
                    } else {
                        1.0 / c
                    }
                }
                "csc" => {
                    let s = v.sin();
                    if s.abs() < 1e-9 {
                        f64::NAN
                    } else {
                        1.0 / s
                    }
                }
                "asin" | "arcsin" => v.asin(),
                "acos" | "arccos" => v.acos(),
                "atan" | "arctan" => v.atan(),
                "sinh" => v.sinh(),
                "cosh" => v.cosh(),
                "tanh" => v.tanh(),
                "ln" => {
                    if v <= 0.0 {
                        f64::NAN
                    } else {
                        v.ln()
                    }
                }
                "log" | "log10" => {
                    if v <= 0.0 {
                        f64::NAN
                    } else {
                        v.log10()
                    }
                }
                "log2" => {
                    if v <= 0.0 {
                        f64::NAN
                    } else {
                        v.log2()
                    }
                }
                "exp" => v.exp(),
                "abs" => v.abs(),
                "sign" | "sgn" => {
                    if v > 0.0 {
                        1.0
                    } else if v < 0.0 {
                        -1.0
                    } else if v == 0.0 {
                        0.0
                    } else {
                        f64::NAN
                    }
                }
                "sqrt" => {
                    if v < 0.0 {
                        f64::NAN
                    } else {
                        v.sqrt()
                    }
                }
                "cbrt" => v.cbrt(),
                "floor" => v.floor(),
                "ceil" => v.ceil(),
                "round" => v.round(),
                "min" if args.len() >= 2 => v.min(eval_expr_inner(&args[1], x, y, depth + 1)),
                "max" if args.len() >= 2 => v.max(eval_expr_inner(&args[1], x, y, depth + 1)),
                _ => f64::NAN,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Constant Folding & Human-Readable Simplification Readout
// ---------------------------------------------------------------------------

fn format_compact_num(v: f64) -> String {
    if !v.is_finite() {
        return if v.is_nan() {
            "NaN".into()
        } else if v.is_sign_positive() {
            "∞".into()
        } else {
            "-∞".into()
        };
    }
    if v.abs() < 1e-11 {
        return "0".into();
    }
    if (v - v.round()).abs() < 1e-9 && v.abs() < 1e12 {
        return format!("{}", v.round() as i64);
    }
    let s = format!("{:.4}", v);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

pub fn simplify_constants(expr: &Expr) -> Expr {
    if !uses_var(expr, 'x') && !uses_var(expr, 'y') && !contains_placeholder(expr) {
        let v = eval_expr(expr, 0.0, 0.0);
        if v.is_finite() {
            return Expr::Num(v);
        }
    }
    match expr {
        Expr::Neg(a) => {
            let sa = simplify_constants(a);
            if let Expr::Num(v) = sa {
                Expr::Num(-v)
            } else {
                Expr::Neg(Box::new(sa))
            }
        }
        Expr::Add(a, b) => {
            let sa = simplify_constants(a);
            let sb = simplify_constants(b);
            match (&sa, &sb) {
                (Expr::Num(0.0), _) => sb,
                (_, Expr::Num(0.0)) => sa,
                (Expr::Num(u), Expr::Num(v)) => Expr::Num(u + v),
                _ => Expr::Add(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Sub(a, b) => {
            let sa = simplify_constants(a);
            let sb = simplify_constants(b);
            match (&sa, &sb) {
                (_, Expr::Num(0.0)) => sa,
                (Expr::Num(u), Expr::Num(v)) => Expr::Num(u - v),
                _ => Expr::Sub(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Mul(a, b) => {
            let sa = simplify_constants(a);
            let sb = simplify_constants(b);
            match (&sa, &sb) {
                (Expr::Num(0.0), _) | (_, Expr::Num(0.0)) => Expr::Num(0.0),
                (Expr::Num(1.0), _) => sb,
                (_, Expr::Num(1.0)) => sa,
                (Expr::Num(u), Expr::Num(v)) => Expr::Num(u * v),
                _ => Expr::Mul(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Div(a, b) => {
            let sa = simplify_constants(a);
            let sb = simplify_constants(b);
            match (&sa, &sb) {
                (Expr::Num(0.0), _) => Expr::Num(0.0),
                (_, Expr::Num(1.0)) => sa,
                _ => Expr::Div(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Pow(a, b) => {
            let sa = simplify_constants(a);
            let sb = simplify_constants(b);
            match (&sa, &sb) {
                (_, Expr::Num(0.0)) => Expr::Num(1.0),
                (_, Expr::Num(1.0)) => sa,
                _ => Expr::Pow(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Sqrt(a) => Expr::Sqrt(Box::new(simplify_constants(a))),
        Expr::Root(n, r) => Expr::Root(
            Box::new(simplify_constants(n)),
            Box::new(simplify_constants(r)),
        ),
        Expr::Abs(a) => Expr::Abs(Box::new(simplify_constants(a))),
        Expr::Factorial(a) => Expr::Factorial(Box::new(simplify_constants(a))),
        Expr::DegToRad(a) => Expr::DegToRad(Box::new(simplify_constants(a))),
        Expr::Deriv(a) => Expr::Deriv(Box::new(simplify_constants(a))),
        Expr::IndefIntegral(a) => Expr::IndefIntegral(Box::new(simplify_constants(a))),
        Expr::DefIntegral(a, b, c) => Expr::DefIntegral(
            Box::new(simplify_constants(a)),
            Box::new(simplify_constants(b)),
            Box::new(simplify_constants(c)),
        ),
        Expr::Call(name, args) => Expr::Call(
            name.clone(),
            args.iter().map(simplify_constants).collect(),
        ),
        other => other.clone(),
    }
}

fn format_expr_compact(expr: &Expr) -> String {
    match expr {
        Expr::Num(v) => format_compact_num(*v),
        Expr::VarX => "x".into(),
        Expr::VarY => "y".into(),
        Expr::Placeholder => "□".into(),
        Expr::Neg(a) => format!("-{}", format_expr_compact(a)),
        Expr::Add(a, b) => format!("{} + {}", format_expr_compact(a), format_expr_compact(b)),
        Expr::Sub(a, b) => format!("{} - {}", format_expr_compact(a), format_expr_compact(b)),
        Expr::Mul(a, b) => {
            let sa = format_expr_compact(a);
            let sb = format_expr_compact(b);
            if matches!(**a, Expr::Num(_)) && matches!(**b, Expr::VarX | Expr::VarY) {
                format!("{sa}{sb}")
            } else {
                format!("{sa}·{sb}")
            }
        }
        Expr::Div(a, b) => format!("{}/{}", format_expr_compact(a), format_expr_compact(b)),
        Expr::Pow(a, b) => {
            let sa = format_expr_compact(a);
            if matches!(**b, Expr::Num(v) if (v - 2.0).abs() < 1e-9) {
                format!("{sa}²")
            } else if matches!(**b, Expr::Num(v) if (v - 3.0).abs() < 1e-9) {
                format!("{sa}³")
            } else {
                format!("{sa}^{}", format_expr_compact(b))
            }
        }
        Expr::Sqrt(a) => format!("√({})", format_expr_compact(a)),
        Expr::Root(n, r) => format!("{}√({})", format_expr_compact(n), format_expr_compact(r)),
        Expr::Abs(a) => format!("|{}|", format_expr_compact(a)),
        Expr::Factorial(a) => format!("{}!", format_expr_compact(a)),
        Expr::DegToRad(a) => format!("{}°", format_expr_compact(a)),
        Expr::Deriv(a) => format!("d/dx({})", format_expr_compact(a)),
        Expr::IndefIntegral(a) => format!("∫({})dx", format_expr_compact(a)),
        Expr::DefIntegral(a, b, c) => format!(
            "∫({}, {}, {})dx",
            format_expr_compact(a),
            format_expr_compact(b),
            format_expr_compact(c)
        ),
        Expr::Call(name, args) => {
            let joined = args
                .iter()
                .map(format_expr_compact)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name}({joined})")
        }
    }
}

fn find_roots_in_range(lhs: &Expr, rhs: &Expr, x_min: f64, x_max: f64) -> Vec<f64> {
    let steps = 240usize;
    let dx = (x_max - x_min) / steps as f64;
    let f = |x: f64| eval_expr(lhs, x, 0.0) - eval_expr(rhs, x, 0.0);
    let mut roots = Vec::new();
    let mut prev_x = x_min;
    let mut prev_v = f(prev_x);
    for i in 1..=steps {
        let cur_x = x_min + i as f64 * dx;
        let cur_v = f(cur_x);
        if prev_v.is_finite() && cur_v.is_finite() {
            if prev_v.abs() < 1e-10 {
                if roots.last().is_none_or(|&r: &f64| (r - prev_x).abs() > dx * 0.5) {
                    roots.push(prev_x);
                }
            } else if prev_v * cur_v < 0.0 && (cur_v - prev_v).abs() < 1e5 {
                let mut lo = prev_x;
                let mut hi = cur_x;
                let mut flo = prev_v;
                for _ in 0..32 {
                    let mid = 0.5 * (lo + hi);
                    let fmid = f(mid);
                    if !fmid.is_finite() {
                        break;
                    }
                    if flo * fmid <= 0.0 {
                        hi = mid;
                    } else {
                        lo = mid;
                        flo = fmid;
                    }
                }
                let root = 0.5 * (lo + hi);
                if roots.last().is_none_or(|&r: &f64| (r - root).abs() > dx * 0.5) {
                    roots.push(root);
                }
            }
        }
        prev_x = cur_x;
        prev_v = cur_v;
    }
    roots
}

pub fn evaluate_readout(stmts: &[PlotStatement], vp: &GraphViewport) -> Option<String> {
    let first = stmts.first()?;
    match first {
        PlotStatement::ExplicitY { expr, .. } => {
            let simp = simplify_constants(expr);
            if !uses_var(&simp, 'x') && !uses_var(&simp, 'y') {
                let v = eval_expr(&simp, 0.0, 0.0);
                if v.is_finite() {
                    return Some(format!("= {}", format_compact_num(v)));
                }
            }
            Some(format_expr_compact(&simp))
        }
        PlotStatement::ExplicitX { expr, .. } => {
            let simp = simplify_constants(expr);
            Some(format!("x = {}", format_expr_compact(&simp)))
        }
        PlotStatement::EquationX { lhs, rhs, .. } => {
            let (xmin, xmax) = vp.x_bounds();
            let roots = find_roots_in_range(lhs, rhs, xmin, xmax);
            if roots.is_empty() {
                let sl = format_expr_compact(&simplify_constants(lhs));
                let sr = format_expr_compact(&simplify_constants(rhs));
                Some(format!("{sl} = {sr}"))
            } else {
                let formatted = roots
                    .iter()
                    .take(3)
                    .map(|r| format!("x = {}", format_compact_num(*r)))
                    .collect::<Vec<_>>()
                    .join(", ");
                Some(formatted)
            }
        }
        PlotStatement::ImplicitEq { lhs, rhs, .. } => {
            let sl = format_expr_compact(&simplify_constants(lhs));
            let sr = format_expr_compact(&simplify_constants(rhs));
            Some(format!("{sl} = {sr}"))
        }
        PlotStatement::Inequality { lhs, rhs, op, .. } => {
            let sl = format_expr_compact(&simplify_constants(lhs));
            let sr = format_expr_compact(&simplify_constants(rhs));
            let sym = match op {
                RelOp::Eq => "=",
                RelOp::Lt => "<",
                RelOp::Gt => ">",
                RelOp::Le => "≤",
                RelOp::Ge => "≥",
            };
            Some(format!("{sl} {sym} {sr}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Accurate Vector Plot Builder (Grid, Axes, Clipping, Explicit/Implicit/Ineq)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct TickLabel {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub align_right: bool,
}

#[derive(Clone, Debug)]
pub struct GraphPlotOutput {
    pub shape: Shape,
    pub tick_labels: Vec<TickLabel>,
    pub curve_legend: Vec<(String, Color)>,
    pub readout: Option<String>,
    pub error: Option<String>,
}

fn curve_palette_color(idx: usize) -> Color {
    if theme::active_theme().is_dark() {
        const DARK_PALETTE: [&str; 6] = [
            "#4DB6AC", "#E06C75", "#60A5FA", "#D9A74A", "#A78BFA", "#6AB07A",
        ];
        theme::hex(DARK_PALETTE[idx % DARK_PALETTE.len()])
    } else {
        const LIGHT_PALETTE: [&str; 6] = [
            "#0F766E", "#DC2626", "#2563EB", "#B45309", "#7C3AED", "#15803D",
        ];
        theme::hex(LIGHT_PALETTE[idx % LIGHT_PALETTE.len()])
    }
}

fn nice_step(span: f64, px_len: f32, target_px: f64) -> f64 {
    let target_ticks = ((px_len as f64) / target_px).max(2.0);
    let raw = span / target_ticks;
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let pow10 = 10.0f64.powf(raw.log10().floor());
    let norm = raw / pow10;
    let mult = if norm < 1.5 {
        1.0
    } else if norm < 3.5 {
        2.0
    } else if norm < 7.5 {
        5.0
    } else {
        10.0
    };
    (mult * pow10).max(1e-6)
}

fn format_tick_value(val: f64, step: f64) -> String {
    if val.abs() < step * 1e-6 {
        return "0".into();
    }
    if step >= 1.0 && (val - val.round()).abs() < 1e-6 {
        return format!("{}", val.round() as i64);
    }
    if step.abs() < 1e-3 || val.abs() >= 1e5 {
        return format!("{:.1e}", val);
    }
    let decimals = (-step.log10().floor()).max(0.0).min(4.0) as usize;
    let s = format!("{:.*}", decimals, val);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// Cohen-Sutherland 2D line segment clipping against [xmin, xmax] x [ymin, ymax]
fn cohen_sutherland_clip(
    mut x0: f32,
    mut y0: f32,
    mut x1: f32,
    mut y1: f32,
    xmin: f32,
    ymin: f32,
    xmax: f32,
    ymax: f32,
) -> Option<([f32; 2], [f32; 2])> {
    const INSIDE: u8 = 0;
    const LEFT: u8 = 1;
    const RIGHT: u8 = 2;
    const BOTTOM: u8 = 4;
    const TOP: u8 = 8;

    let code = |x: f32, y: f32| -> u8 {
        let mut c = INSIDE;
        if x < xmin {
            c |= LEFT;
        } else if x > xmax {
            c |= RIGHT;
        }
        if y < ymin {
            c |= BOTTOM;
        } else if y > ymax {
            c |= TOP;
        }
        c
    };

    let mut c0 = code(x0, y0);
    let mut c1 = code(x1, y1);
    for _ in 0..8 {
        if (c0 | c1) == 0 {
            return Some(([x0, y0], [x1, y1]));
        }
        if (c0 & c1) != 0 {
            return None;
        }
        let out = if c0 != 0 { c0 } else { c1 };
        let dx = x1 - x0;
        let dy = y1 - y0;
        let (nx, ny) = if (out & TOP) != 0 {
            let x = if dy.abs() > 1e-9 {
                x0 + dx * (ymax - y0) / dy
            } else {
                x0
            };
            (x, ymax)
        } else if (out & BOTTOM) != 0 {
            let x = if dy.abs() > 1e-9 {
                x0 + dx * (ymin - y0) / dy
            } else {
                x0
            };
            (x, ymin)
        } else if (out & RIGHT) != 0 {
            let y = if dx.abs() > 1e-9 {
                y0 + dy * (xmax - x0) / dx
            } else {
                y0
            };
            (xmax, y)
        } else {
            let y = if dx.abs() > 1e-9 {
                y0 + dy * (xmin - x0) / dx
            } else {
                y0
            };
            (xmin, y)
        };

        if out == c0 {
            x0 = nx;
            y0 = ny;
            c0 = code(x0, y0);
        } else {
            x1 = nx;
            y1 = ny;
            c1 = code(x1, y1);
        }
    }
    None
}

fn push_clipped_polyline(
    mut shape: Shape,
    pts: &[[f32; 2]],
    w: f32,
    h: f32,
    stroke_w: f32,
    color: Color,
    dashed: bool,
) -> Shape {
    let mut run: Vec<[f32; 2]> = Vec::new();
    let pad = 0.5;
    let flush = |shape: Shape, run: &mut Vec<[f32; 2]>| -> Shape {
        if run.len() >= 2 {
            if dashed {
                draw_dashed_polyline(shape, run, stroke_w, color, 6.0, 4.0)
            } else {
                shape.stroke(std::mem::take(run), stroke_w, color)
            }
        } else {
            run.clear();
            shape
        }
    };

    for seg in pts.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        if !a[0].is_finite() || !a[1].is_finite() || !b[0].is_finite() || !b[1].is_finite() {
            shape = flush(shape, &mut run);
            continue;
        }
        if let Some((ca, cb)) = cohen_sutherland_clip(a[0], a[1], b[0], b[1], pad, pad, w - pad, h - pad)
        {
            if let Some(last) = run.last() {
                if (last[0] - ca[0]).hypot(last[1] - ca[1]) > 0.75 {
                    shape = flush(shape, &mut run);
                    run.push(ca);
                }
            } else {
                run.push(ca);
            }
            run.push(cb);
        } else {
            shape = flush(shape, &mut run);
        }
    }
    flush(shape, &mut run)
}

fn draw_dashed_polyline(
    mut shape: Shape,
    pts: &[[f32; 2]],
    stroke_w: f32,
    color: Color,
    dash_len: f32,
    gap_len: f32,
) -> Shape {
    let cycle = dash_len + gap_len;
    let mut dist_acc = 0.0f32;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = dx.hypot(dy);
        if len < 0.05 {
            continue;
        }
        let mut s = 0.0f32;
        while s < len {
            let phase = (dist_acc + s) % cycle;
            let rem_in_phase = if phase < dash_len {
                dash_len - phase
            } else {
                cycle - phase
            };
            let step = rem_in_phase.min(len - s);
            if phase < dash_len && step > 0.25 {
                let t0 = s / len;
                let t1 = (s + step) / len;
                shape = shape.line(
                    [a[0] + dx * t0, a[1] + dy * t0],
                    [a[0] + dx * t1, a[1] + dy * t1],
                    stroke_w,
                    LineCap::Round,
                    color,
                );
            }
            s += step;
        }
        dist_acc += len;
    }
    shape
}

fn draw_arrow_head(
    shape: Shape,
    tip: [f32; 2],
    dir: [f32; 2],
    len: f32,
    half_w: f32,
    color: Color,
) -> Shape {
    let px = -dir[1];
    let py = dir[0];
    let bx = tip[0] - dir[0] * len;
    let by = tip[1] - dir[1] * len;
    shape.fill(
        vec![
            tip,
            [bx + px * half_w, by + py * half_w],
            [bx - px * half_w, by - py * half_w],
        ],
        color,
    )
}

fn plot_explicit_y(
    mut shape: Shape,
    expr: &Expr,
    vp: &GraphViewport,
    w: f32,
    h: f32,
    color: Color,
    stroke_w: f32,
    dashed: bool,
    shade_mode: Option<RelOp>,
) -> Shape {
    let (xmin, xmax) = vp.x_bounds();
    let (ymin, ymax) = vp.y_bounds(w, h);
    let sy = (ymax - ymin).max(1e-9);
    let n = ((w as usize) * 2).clamp(180, 560);
    let dx = (xmax - xmin) / n as f64;

    let mut samples: Vec<(f64, f64)> = Vec::with_capacity(n + 64);
    for i in 0..=n {
        let x = xmin + i as f64 * dx;
        let y = eval_expr(expr, x, 0.0);
        samples.push((x, y));
    }

    // Adaptive midpoint insertion for steep continuous slopes
    let mut refined: Vec<(f64, f64)> = Vec::with_capacity(samples.len() + 64);
    for w_pair in samples.windows(2) {
        let (x0, y0) = w_pair[0];
        let (x1, y1) = w_pair[1];
        refined.push((x0, y0));
        if y0.is_finite() && y1.is_finite() && (y1 - y0).abs() > sy * 0.12 && (y1 - y0).abs() < sy * 1.8
        {
            let xm = 0.5 * (x0 + x1);
            let ym = eval_expr(expr, xm, 0.0);
            refined.push((xm, ym));
        }
    }
    if let Some(&last) = samples.last() {
        refined.push(last);
    }

    // Inequality shading under or over explicit y = f(x)
    if let Some(op) = shade_mode {
        let shade_col = color.alpha(0.16);
        let shade_above = matches!(op, RelOp::Gt | RelOp::Ge);
        let edge_y_px = if shade_above { 0.0 } else { h };
        for pair in refined.windows(2) {
            let (x0, y0) = pair[0];
            let (x1, y1) = pair[1];
            if !y0.is_finite() || !y1.is_finite() || (y1 - y0).abs() > sy * 1.4 {
                continue;
            }
            let p0 = vp.math_to_px(x0, y0, w, h);
            let p1 = vp.math_to_px(x1, y1, w, h);
            let cx0 = p0[0].clamp(0.0, w);
            let cx1 = p1[0].clamp(0.0, w);
            let cy0 = p0[1].clamp(0.0, h);
            let cy1 = p1[1].clamp(0.0, h);
            if (cx1 - cx0).abs() < 0.05 {
                continue;
            }
            if (cy0 - edge_y_px).abs() < 0.5 && (cy1 - edge_y_px).abs() < 0.5 {
                continue;
            }
            shape = shape.fill(
                vec![[cx0, edge_y_px], [cx1, edge_y_px], [cx1, cy1], [cx0, cy0]],
                shade_col,
            );
        }
    }

    let mut run: Vec<[f32; 2]> = Vec::new();
    for pair in refined.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        if !y0.is_finite() || !y1.is_finite() {
            shape = push_clipped_polyline(shape, &run, w, h, stroke_w, color, dashed);
            run.clear();
            continue;
        }
        // Split across vertical asymptotes (huge jump or sign flip across outside-of-view pole)
        let jump = (y1 - y0).abs();
        let pole_jump = jump > sy * 1.35
            && ((y0 > ymax && y1 < ymin) || (y0 < ymin && y1 > ymax) || jump > sy * 3.0);
        if pole_jump {
            shape = push_clipped_polyline(shape, &run, w, h, stroke_w, color, dashed);
            run.clear();
            continue;
        }
        let p0 = vp.math_to_px(x0, y0, w, h);
        let p1 = vp.math_to_px(x1, y1, w, h);
        if run.is_empty() {
            run.push(p0);
        }
        run.push(p1);
    }
    push_clipped_polyline(shape, &run, w, h, stroke_w, color, dashed)
}

fn plot_explicit_x(
    mut shape: Shape,
    expr: &Expr,
    vp: &GraphViewport,
    w: f32,
    h: f32,
    color: Color,
    stroke_w: f32,
    dashed: bool,
) -> Shape {
    let (xmin, xmax) = vp.x_bounds();
    let (ymin, ymax) = vp.y_bounds(w, h);
    let sx = (xmax - xmin).max(1e-9);
    let n = ((h as usize) * 2).clamp(160, 480);
    let dy = (ymax - ymin) / n as f64;

    let mut run: Vec<[f32; 2]> = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    for i in 0..=n {
        let y = ymin + i as f64 * dy;
        let x = eval_expr(expr, 0.0, y);
        if !x.is_finite() {
            shape = push_clipped_polyline(shape, &run, w, h, stroke_w, color, dashed);
            run.clear();
            prev = None;
            continue;
        }
        if let Some((px_val, _)) = prev {
            if (x - px_val).abs() > sx * 1.5 {
                shape = push_clipped_polyline(shape, &run, w, h, stroke_w, color, dashed);
                run.clear();
            }
        }
        run.push(vp.math_to_px(x, y, w, h));
        prev = Some((x, y));
    }
    push_clipped_polyline(shape, &run, w, h, stroke_w, color, dashed)
}

fn plot_implicit_marching_squares(
    mut shape: Shape,
    lhs: &Expr,
    rhs: &Expr,
    vp: &GraphViewport,
    w: f32,
    h: f32,
    color: Color,
    stroke_w: f32,
    dashed: bool,
    shade_op: Option<RelOp>,
) -> Shape {
    let cols = ((w / 3.5).round() as usize).clamp(52, 124);
    let rows = ((h / 3.5).round() as usize).clamp(44, 108);
    let (xmin, xmax) = vp.x_bounds();
    let (ymin, ymax) = vp.y_bounds(w, h);
    let dx = (xmax - xmin) / cols as f64;
    let dy = (ymax - ymin) / rows as f64;

    let mut grid = vec![f64::NAN; (cols + 1) * (rows + 1)];
    for r in 0..=rows {
        let y = ymax - r as f64 * dy;
        for c in 0..=cols {
            let x = xmin + c as f64 * dx;
            let v = eval_expr(lhs, x, y) - eval_expr(rhs, x, y);
            grid[r * (cols + 1) + c] = if v.is_finite() && v.abs() < 1e6 {
                v
            } else {
                f64::NAN
            };
        }
    }

    // Optional inequality shading via horizontally merged satisfied cells
    if let Some(op) = shade_op {
        let shade_col = color.alpha(0.16);
        let cell_w = w / cols as f32;
        let cell_h = h / rows as f32;
        for r in 0..rows {
            let y_mid = ymax - (r as f64 + 0.5) * dy;
            let mut run_start: Option<usize> = None;
            for c in 0..=cols {
                let inside = if c < cols {
                    let x_mid = xmin + (c as f64 + 0.5) * dx;
                    let v = eval_expr(lhs, x_mid, y_mid) - eval_expr(rhs, x_mid, y_mid);
                    if v.is_finite() {
                        match op {
                            RelOp::Lt => v < 0.0,
                            RelOp::Le => v <= 0.0,
                            RelOp::Gt => v > 0.0,
                            RelOp::Ge => v >= 0.0,
                            RelOp::Eq => false,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                match (run_start, inside) {
                    (None, true) => run_start = Some(c),
                    (Some(c0), false) => {
                        let x0 = (c0 as f32 * cell_w).clamp(0.0, w);
                        let x1 = (c as f32 * cell_w).clamp(0.0, w);
                        let y0 = (r as f32 * cell_h).clamp(0.0, h);
                        let y1 = ((r + 1) as f32 * cell_h).clamp(0.0, h);
                        if x1 - x0 > 0.1 && y1 - y0 > 0.1 {
                            shape = shape.fill(
                                vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
                                shade_col,
                            );
                        }
                        run_start = None;
                    }
                    _ => {}
                }
            }
        }
    }

    let cell_w = w / cols as f32;
    let cell_h = h / rows as f32;
    let interp = |v0: f64, v1: f64| -> f32 {
        let denom = v1 - v0;
        if denom.abs() < 1e-12 {
            0.5
        } else {
            (-v0 / denom).clamp(0.0, 1.0) as f32
        }
    };

    for r in 0..rows {
        let y0 = r as f32 * cell_h;
        let y1 = (r + 1) as f32 * cell_h;
        for c in 0..cols {
            let x0 = c as f32 * cell_w;
            let x1 = (c + 1) as f32 * cell_w;

            // Corners: tl (0), tr (1), br (2), bl (3)
            let tl = grid[r * (cols + 1) + c];
            let tr = grid[r * (cols + 1) + (c + 1)];
            let br = grid[(r + 1) * (cols + 1) + (c + 1)];
            let bl = grid[(r + 1) * (cols + 1) + c];
            if !tl.is_finite() || !tr.is_finite() || !br.is_finite() || !bl.is_finite() {
                continue;
            }
            let max_jump = (tl - tr)
                .abs()
                .max((tr - br).abs())
                .max((br - bl).abs())
                .max((bl - tl).abs());
            if max_jump > 1e4 {
                continue;
            }

            let mut mask = 0u8;
            if tl > 0.0 {
                mask |= 8;
            }
            if tr > 0.0 {
                mask |= 4;
            }
            if br > 0.0 {
                mask |= 2;
            }
            if bl > 0.0 {
                mask |= 1;
            }
            if mask == 0 || mask == 15 {
                continue;
            }

            // Edge midpoints: top(0), right(1), bottom(2), left(3)
            let top = [x0 + interp(tl, tr) * cell_w, y0];
            let right = [x1, y0 + interp(tr, br) * cell_h];
            let bottom = [x0 + interp(bl, br) * cell_w, y1];
            let left = [x0, y0 + interp(tl, bl) * cell_h];

            let emit_seg = |s: Shape, a: [f32; 2], b: [f32; 2]| -> Shape {
                if let Some((ca, cb)) =
                    cohen_sutherland_clip(a[0], a[1], b[0], b[1], 0.5, 0.5, w - 0.5, h - 0.5)
                {
                    if dashed {
                        draw_dashed_polyline(s, &[ca, cb], stroke_w, color, 5.0, 4.0)
                    } else {
                        s.line(ca, cb, stroke_w, LineCap::Round, color)
                    }
                } else {
                    s
                }
            };

            shape = match mask {
                1 | 14 => emit_seg(shape, left, bottom),
                2 | 13 => emit_seg(shape, bottom, right),
                3 | 12 => emit_seg(shape, left, right),
                4 | 11 => emit_seg(shape, top, right),
                6 | 9 => emit_seg(shape, top, bottom),
                7 | 8 => emit_seg(shape, left, top),
                5 => {
                    let xm = xmin + (c as f64 + 0.5) * dx;
                    let ym = ymax - (r as f64 + 0.5) * dy;
                    let center = eval_expr(lhs, xm, ym) - eval_expr(rhs, xm, ym);
                    if center > 0.0 {
                        let s = emit_seg(shape, left, top);
                        emit_seg(s, bottom, right)
                    } else {
                        let s = emit_seg(shape, left, bottom);
                        emit_seg(s, top, right)
                    }
                }
                10 => {
                    let xm = xmin + (c as f64 + 0.5) * dx;
                    let ym = ymax - (r as f64 + 0.5) * dy;
                    let center = eval_expr(lhs, xm, ym) - eval_expr(rhs, xm, ym);
                    if center > 0.0 {
                        let s = emit_seg(shape, top, right);
                        emit_seg(s, left, bottom)
                    } else {
                        let s = emit_seg(shape, left, top);
                        emit_seg(s, bottom, right)
                    }
                }
                _ => shape,
            };
        }
    }
    shape
}

pub fn build_graph_plot(content: &str, vp: &GraphViewport, w: f32, h: f32, scale: f32) -> GraphPlotOutput {
    let w = w.max(64.0);
    let h = h.max(48.0);
    let (xmin, xmax) = vp.x_bounds();
    let (ymin, ymax) = vp.y_bounds(w, h);
    let sy = (ymax - ymin).max(1e-9);

    let mut shape = Shape::new();
    let mut tick_labels = Vec::new();

    let minor_col = theme::grid_minor();
    let major_col = theme::grid_major();
    let axis_col = theme::text_muted().alpha(0.75);

    let step_x = nice_step(vp.span_x, w, (58.0 * scale.clamp(0.7, 1.5)) as f64);
    let step_y = nice_step(sy, h, (48.0 * scale.clamp(0.7, 1.5)) as f64);

    // Minor vertical lines (4 subdivisions)
    let sub_x = step_x * 0.25;
    let first_mx = (xmin / sub_x).floor() as i64;
    let last_mx = (xmax / sub_x).ceil() as i64;
    for ix in first_mx..=last_mx {
        if ix.rem_euclid(4) == 0 {
            continue;
        }
        let mx = ix as f64 * sub_x;
        let [px, _] = vp.math_to_px(mx, 0.0, w, h);
        if px > 1.0 && px < w - 1.0 {
            shape = shape.line([px, 0.0], [px, h], 0.5, LineCap::Butt, minor_col);
        }
    }

    // Minor horizontal lines (4 subdivisions)
    let sub_y = step_y * 0.25;
    let first_my = (ymin / sub_y).floor() as i64;
    let last_my = (ymax / sub_y).ceil() as i64;
    for iy in first_my..=last_my {
        if iy.rem_euclid(4) == 0 {
            continue;
        }
        let my = iy as f64 * sub_y;
        let [_, py] = vp.math_to_px(0.0, my, w, h);
        if py > 1.0 && py < h - 1.0 {
            shape = shape.line([0.0, py], [w, py], 0.5, LineCap::Butt, minor_col);
        }
    }

    // Major vertical grid lines
    let k0_x = (xmin / step_x).floor() as i64;
    let k1_x = (xmax / step_x).ceil() as i64;
    let [axis_x_px, axis_y_px] = vp.math_to_px(0.0, 0.0, w, h);
    let x_label_y = if axis_y_px >= 14.0 && axis_y_px <= h - 16.0 {
        axis_y_px + 3.0
    } else if axis_y_px < 14.0 {
        4.0
    } else {
        h - 14.0
    };
    let y_label_x = if axis_x_px >= 28.0 && axis_x_px <= w - 18.0 {
        axis_x_px - 5.0
    } else if axis_x_px < 28.0 {
        4.0
    } else {
        w - 6.0
    };
    let y_align_right = axis_x_px >= 28.0 && axis_x_px <= w - 18.0 || axis_x_px > w - 18.0;

    for ix in k0_x..=k1_x {
        let mx = ix as f64 * step_x;
        let [px, _] = vp.math_to_px(mx, 0.0, w, h);
        if px >= 0.5 && px <= w - 0.5 {
            shape = shape.line([px, 0.0], [px, h], 0.85, LineCap::Butt, major_col);
            if mx.abs() > step_x * 1e-6 && px >= 16.0 && px <= w - 22.0 {
                tick_labels.push(TickLabel {
                    text: format_tick_value(mx, step_x),
                    x: (px + 2.0).clamp(2.0, (w - 28.0).max(2.0)),
                    y: x_label_y.clamp(2.0, (h - 13.0).max(2.0)),
                    align_right: false,
                });
            }
        }
    }

    // Major horizontal grid lines
    let k0_y = (ymin / step_y).floor() as i64;
    let k1_y = (ymax / step_y).ceil() as i64;
    for iy in k0_y..=k1_y {
        let my = iy as f64 * step_y;
        let [_, py] = vp.math_to_px(0.0, my, w, h);
        if py >= 0.5 && py <= h - 0.5 {
            shape = shape.line([0.0, py], [w, py], 0.85, LineCap::Butt, major_col);
            if my.abs() > step_y * 1e-6 && py >= 12.0 && py <= h - 14.0 {
                tick_labels.push(TickLabel {
                    text: format_tick_value(my, step_y),
                    x: y_label_x.clamp(2.0, (w - 4.0).max(2.0)),
                    y: (py - 6.0).clamp(2.0, (h - 13.0).max(2.0)),
                    align_right: y_align_right,
                });
            }
        }
    }

    // Axes lines & arrowheads
    if axis_y_px >= 0.5 && axis_y_px <= h - 0.5 {
        shape = shape.line([0.0, axis_y_px], [w - 4.0, axis_y_px], 1.25, LineCap::Butt, axis_col);
        shape = draw_arrow_head(shape, [w - 1.5, axis_y_px], [1.0, 0.0], 6.0, 3.0, axis_col);
    }
    if axis_x_px >= 0.5 && axis_x_px <= w - 0.5 {
        shape = shape.line([axis_x_px, h], [axis_x_px, 4.0], 1.25, LineCap::Butt, axis_col);
        shape = draw_arrow_head(shape, [axis_x_px, 1.5], [0.0, -1.0], 6.0, 3.0, axis_col);
    }
    if axis_x_px >= 10.0 && axis_x_px <= w - 14.0 && axis_y_px >= 10.0 && axis_y_px <= h - 14.0 {
        tick_labels.push(TickLabel {
            text: "0".into(),
            x: (axis_x_px - 10.0).clamp(2.0, (w - 14.0).max(2.0)),
            y: (axis_y_px + 2.0).clamp(2.0, (h - 13.0).max(2.0)),
            align_right: false,
        });
    }

    let (stmts, error) = parse_statements(content);
    let readout = evaluate_readout(&stmts, vp);
    let mut curve_legend = Vec::new();
    let stroke_w = (2.1 * scale.clamp(0.7, 1.35)).clamp(1.4, 3.0);

    for (idx, stmt) in stmts.iter().enumerate() {
        let color = curve_palette_color(idx);
        match stmt {
            PlotStatement::ExplicitY { raw, expr } => {
                curve_legend.push((raw.clone(), color));
                shape = plot_explicit_y(shape, expr, vp, w, h, color, stroke_w, false, None);
            }
            PlotStatement::ExplicitX { raw, expr } => {
                curve_legend.push((raw.clone(), color));
                shape = plot_explicit_x(shape, expr, vp, w, h, color, stroke_w, false);
            }
            PlotStatement::EquationX { raw, lhs, rhs } => {
                curve_legend.push((raw.clone(), color));
                let diff = Expr::Sub(Box::new(lhs.clone()), Box::new(rhs.clone()));
                shape = plot_explicit_y(
                    shape,
                    &diff,
                    vp,
                    w,
                    h,
                    color.alpha(0.45),
                    stroke_w * 0.85,
                    true,
                    None,
                );
                for root in find_roots_in_range(lhs, rhs, xmin, xmax) {
                    let [rx, ry] = vp.math_to_px(root, 0.0, w, h);
                    if rx >= 0.5 && rx <= w - 0.5 {
                        shape = shape.line([rx, 0.5], [rx, h - 0.5], stroke_w, LineCap::Butt, color);
                        if ry >= 2.0 && ry <= h - 2.0 {
                            shape = shape.fill(
                                vec![
                                    [rx - 3.5, ry],
                                    [rx, ry - 3.5],
                                    [rx + 3.5, ry],
                                    [rx, ry + 3.5],
                                ],
                                color,
                            );
                        }
                    }
                }
            }
            PlotStatement::ImplicitEq { raw, lhs, rhs } => {
                curve_legend.push((raw.clone(), color));
                shape = plot_implicit_marching_squares(
                    shape, lhs, rhs, vp, w, h, color, stroke_w, false, None,
                );
            }
            PlotStatement::Inequality { raw, lhs, rhs, op } => {
                curve_legend.push((raw.clone(), color));
                let dashed = matches!(op, RelOp::Lt | RelOp::Gt);
                if matches!(lhs, Expr::VarY) && !uses_var(rhs, 'y') {
                    shape = plot_explicit_y(
                        shape,
                        rhs,
                        vp,
                        w,
                        h,
                        color,
                        stroke_w,
                        dashed,
                        Some(*op),
                    );
                } else {
                    shape = plot_implicit_marching_squares(
                        shape,
                        lhs,
                        rhs,
                        vp,
                        w,
                        h,
                        color,
                        stroke_w,
                        dashed,
                        Some(*op),
                    );
                }
            }
        }
    }

    GraphPlotOutput {
        shape,
        tick_labels,
        curve_legend,
        readout,
        error,
    }
}

// ---------------------------------------------------------------------------
// 2D Readable Math Typesetter (Fractions, Nth-Roots with Vinculum, Exponents)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum DisplayNode {
    Row(Vec<DisplayNode>),
    Text(String),
    Op(String),
    Placeholder,
    ActivePlaceholder,
    Caret,
    Frac(Box<DisplayNode>, Box<DisplayNode>),
    Sqrt {
        index: Option<Box<DisplayNode>>,
        radicand: Box<DisplayNode>,
    },
    Sup(Box<DisplayNode>, Box<DisplayNode>),
    Sub(Box<DisplayNode>, Box<DisplayNode>),
    Abs(Box<DisplayNode>),
    Deriv(Box<DisplayNode>),
    Integral {
        lower: Option<Box<DisplayNode>>,
        upper: Option<Box<DisplayNode>>,
        body: Box<DisplayNode>,
    },
    BigOp {
        sym: &'static str,
        lower: Option<Box<DisplayNode>>,
        upper: Option<Box<DisplayNode>>,
        body: Option<Box<DisplayNode>>,
    },
}

const ACTIVE_PLACEHOLDER_SENTINEL: char = '\u{E000}';
const CARET_SENTINEL: char = '\u{E001}';

fn display_superscript_char_to_ascii(ch: char) -> Option<char> {
    match ch {
        'ⁱ' => Some('i'),
        'ˣ' => Some('x'),
        _ => superscript_char_to_ascii(ch),
    }
}

fn display_subscript_char_to_ascii(ch: char) -> Option<char> {
    match ch {
        'ᵢ' => Some('i'),
        'ⱼ' => Some('j'),
        'ₖ' => Some('k'),
        'ₙ' => Some('n'),
        'ₓ' => Some('x'),
        'ₐ' => Some('a'),
        'ₑ' => Some('e'),
        'ₒ' => Some('o'),
        '₊' => Some('+'),
        '₋' => Some('-'),
        '₌' => Some('='),
        '₍' => Some('('),
        '₎' => Some(')'),
        _ => subscript_char_to_ascii(ch),
    }
}

fn inject_display_cursor(content: &str, selection: Option<(usize, usize)>) -> String {
    let Some((raw_a, raw_b)) = selection else {
        return content.to_string();
    };
    let len = content.len();
    let a = raw_a.min(len);
    let b = raw_b.min(len);
    if a > b || !content.is_char_boundary(a) || !content.is_char_boundary(b) {
        return content.to_string();
    }
    if &content[a..b] == "□" {
        let mut out = String::with_capacity(content.len() + 4);
        out.push_str(&content[..a]);
        out.push(ACTIVE_PLACEHOLDER_SENTINEL);
        out.push_str(&content[b..]);
        return out;
    }
    if a == b && !content.trim().is_empty() {
        let mut out = String::with_capacity(content.len() + 4);
        out.push_str(&content[..a]);
        out.push(CARET_SENTINEL);
        out.push_str(&content[a..]);
        return out;
    }
    content.to_string()
}

fn find_matching_delim(chars: &[char], open_idx: usize, open: char, close: char) -> Option<usize> {
    if chars.get(open_idx) != Some(&open) {
        return None;
    }
    let mut depth = 0usize;
    for (idx, &ch) in chars.iter().enumerate().skip(open_idx) {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn split_top_level_relation(line: &str) -> Option<(String, &'static str, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut depth = 0i32;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            '⇔' if depth == 0 => {
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 1..].iter().collect();
                return Some((l, "⇔", r));
            }
            '→' if depth == 0 => {
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 1..].iter().collect();
                return Some((l, "→", r));
            }
            '≈' if depth == 0 => {
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 1..].iter().collect();
                return Some((l, "≈", r));
            }
            '≠' if depth == 0 => {
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 1..].iter().collect();
                return Some((l, "≠", r));
            }
            '∝' if depth == 0 => {
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 1..].iter().collect();
                return Some((l, "∝", r));
            }
            '!' if depth == 0 && chars.get(i + 1) == Some(&'=') => {
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 2..].iter().collect();
                return Some((l, "≠", r));
            }
            '=' if depth == 0 => {
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 1..].iter().collect();
                return Some((l, "=", r));
            }
            '≤' if depth == 0 => {
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 1..].iter().collect();
                return Some((l, "≤", r));
            }
            '≥' if depth == 0 => {
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 1..].iter().collect();
                return Some((l, "≥", r));
            }
            '-' if depth == 0 && chars.get(i + 1) == Some(&'>') => {
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 2..].iter().collect();
                return Some((l, "→", r));
            }
            '<' if depth == 0 => {
                if chars.get(i + 1) == Some(&'=') && chars.get(i + 2) == Some(&'>') {
                    let l: String = chars[..i].iter().collect();
                    let r: String = chars[i + 3..].iter().collect();
                    return Some((l, "⇔", r));
                }
                if chars.get(i + 1) == Some(&'=') {
                    let l: String = chars[..i].iter().collect();
                    let r: String = chars[i + 2..].iter().collect();
                    return Some((l, "≤", r));
                }
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 1..].iter().collect();
                return Some((l, "<", r));
            }
            '>' if depth == 0 => {
                if chars.get(i + 1) == Some(&'=') {
                    let l: String = chars[..i].iter().collect();
                    let r: String = chars[i + 2..].iter().collect();
                    return Some((l, "≥", r));
                }
                let l: String = chars[..i].iter().collect();
                let r: String = chars[i + 1..].iter().collect();
                return Some((l, ">", r));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in s.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            ',' if depth == 0 => {
                parts.push(s[start..idx].trim().to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(s[start..].trim().to_string());
    parts
}

fn parse_display_node(raw: &str) -> DisplayNode {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DisplayNode::Text(String::new());
    }
    if let Some((l, op, r)) = split_top_level_relation(trimmed) {
        return DisplayNode::Row(vec![
            parse_display_add(&l),
            DisplayNode::Op(format!(" {op} ")),
            parse_display_add(&r),
        ]);
    }
    parse_display_add(trimmed)
}

fn parse_display_add(s: &str) -> DisplayNode {
    let chars: Vec<char> = s.trim().chars().collect();
    let mut depth = 0i32;
    let mut items = Vec::new();
    let mut seg_start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        match chars[i] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            '+' | '-' | '−' | '±' if depth == 0 && i > seg_start => {
                // Don't split after '^', '_', binary operators, or opening delimiters
                let prev = chars[..i]
                    .iter()
                    .rev()
                    .find(|c| !c.is_whitespace())
                    .copied()
                    .unwrap_or(' ');
                if !matches!(
                    prev,
                    '^' | '_' | '*' | '/' | '·' | '×' | '÷' | '(' | '{' | '[' | ','
                ) {
                    let seg: String = chars[seg_start..i].iter().collect();
                    items.push(parse_display_mul(&seg));
                    let op_ch = if chars[i] == '-' { '−' } else { chars[i] };
                    items.push(DisplayNode::Op(format!(" {op_ch} ")));
                    seg_start = i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let tail: String = chars[seg_start..].iter().collect();
    if items.is_empty() {
        parse_display_mul(&tail)
    } else {
        items.push(parse_display_mul(&tail));
        DisplayNode::Row(items)
    }
}

fn strip_outer_parens(s: &str) -> &str {
    let t = s.trim();
    if t.starts_with('(') && t.ends_with(')') {
        let chars: Vec<char> = t.chars().collect();
        if find_matching_delim(&chars, 0, '(', ')') == Some(chars.len() - 1) {
            return &t[1..t.len() - 1];
        }
    }
    t
}

fn parse_display_mul(s: &str) -> DisplayNode {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return DisplayNode::Text(String::new());
    }
    // Check top-level `/` (not `d/dx`)
    let chars: Vec<char> = trimmed.chars().collect();
    let mut depth = 0i32;
    let mut slash_pos = None;
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            '/' | '÷' if depth == 0 => {
                let before: String = chars[..i].iter().collect();
                let after: String = chars[i + 1..].iter().collect();
                if before.trim() == "d" && after.trim_start().starts_with("dx") {
                    continue;
                }
                slash_pos = Some(i);
                break;
            }
            _ => {}
        }
    }
    if let Some(sp) = slash_pos {
        let num_s: String = chars[..sp].iter().collect();
        let den_s: String = chars[sp + 1..].iter().collect();
        let num_clean = strip_outer_parens(&num_s);
        let den_clean = strip_outer_parens(&den_s);
        return DisplayNode::Frac(
            Box::new(parse_display_add(num_clean)),
            Box::new(parse_display_add(den_clean)),
        );
    }

    parse_display_tokens(trimmed)
}

fn consume_script_arg(chars: &[char], start_idx: usize) -> (DisplayNode, usize) {
    let mut i = start_idx;
    if i >= chars.len() {
        return (DisplayNode::Placeholder, i);
    }
    if chars[i] == '(' {
        if let Some(close) = find_matching_delim(chars, i, '(', ')') {
            let inner: String = chars[i + 1..close].iter().collect();
            return (parse_display_add(&inner), close + 1);
        }
        return (DisplayNode::Placeholder, i + 1);
    }
    if chars[i] == '{' {
        if let Some(close) = find_matching_delim(chars, i, '{', '}') {
            let inner: String = chars[i + 1..close].iter().collect();
            return (parse_display_add(&inner), close + 1);
        }
        return (DisplayNode::Placeholder, i + 1);
    }
    if chars[i] == ACTIVE_PLACEHOLDER_SENTINEL {
        return (DisplayNode::ActivePlaceholder, i + 1);
    }
    if chars[i] == '□' {
        return (DisplayNode::Placeholder, i + 1);
    }
    if chars[i] == CARET_SENTINEL {
        return (DisplayNode::Caret, i + 1);
    }
    let start = i;
    while i < chars.len()
        && (chars[i].is_alphanumeric() || chars[i] == '.' || chars[i] == '-' || chars[i] == '−')
    {
        i += 1;
    }
    if i > start {
        let s: String = chars[start..i].iter().collect();
        (DisplayNode::Text(s), i)
    } else {
        (DisplayNode::Placeholder, i)
    }
}

fn attach_postfix(items: &mut Vec<DisplayNode>, wrap: impl FnOnce(DisplayNode) -> DisplayNode) {
    match items.pop() {
        Some(DisplayNode::Caret) => {
            if let Some(real_base) = items.pop() {
                items.push(wrap(DisplayNode::Row(vec![real_base, DisplayNode::Caret])));
            } else {
                items.push(wrap(DisplayNode::Caret));
            }
        }
        Some(prev) => items.push(wrap(prev)),
        None => items.push(wrap(DisplayNode::Placeholder)),
    }
}

fn parse_display_tokens(s: &str) -> DisplayNode {
    let chars: Vec<char> = s.trim().chars().collect();
    let mut items: Vec<DisplayNode> = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c == ACTIVE_PLACEHOLDER_SENTINEL {
            items.push(DisplayNode::ActivePlaceholder);
            i += 1;
            continue;
        }

        if c == CARET_SENTINEL {
            items.push(DisplayNode::Caret);
            i += 1;
            continue;
        }

        // `d/dx` derivative
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("d/dx") {
            let after_i = i + 4;
            let mut k = after_i;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            let body = if chars.get(k) == Some(&'(') {
                if let Some(close) = find_matching_delim(&chars, k, '(', ')') {
                    let inner: String = chars[k + 1..close].iter().collect();
                    i = close + 1;
                    parse_display_add(&inner)
                } else {
                    i = k + 1;
                    DisplayNode::Placeholder
                }
            } else {
                let rem: String = chars[k..].iter().collect();
                i = chars.len();
                if rem.is_empty() {
                    DisplayNode::Placeholder
                } else {
                    parse_display_tokens(&rem)
                }
            };
            items.push(DisplayNode::Deriv(Box::new(body)));
            continue;
        }

        // LaTeX `\frac{num}{den}`
        if rest.starts_with("\\frac")
            && !rest[5..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic())
        {
            let mut k = i + 5;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            if chars.get(k) == Some(&'{') {
                if let Some(c1) = find_matching_delim(&chars, k, '{', '}') {
                    let num_str: String = chars[k + 1..c1].iter().collect();
                    let mut m = c1 + 1;
                    while m < chars.len() && chars[m].is_whitespace() {
                        m += 1;
                    }
                    if chars.get(m) == Some(&'{') {
                        if let Some(c2) = find_matching_delim(&chars, m, '{', '}') {
                            let den_str: String = chars[m + 1..c2].iter().collect();
                            items.push(DisplayNode::Frac(
                                Box::new(parse_display_add(&num_str)),
                                Box::new(parse_display_add(&den_str)),
                            ));
                            i = c2 + 1;
                            continue;
                        }
                    }
                }
            }
        }

        // LaTeX `\sqrt[n]{expr}` or `\sqrt{expr}`
        if rest.starts_with("\\sqrt")
            && !rest[5..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic())
        {
            let mut k = i + 5;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            let mut idx_node = None;
            if chars.get(k) == Some(&'[') {
                if let Some(close_b) = find_matching_delim(&chars, k, '[', ']') {
                    let inner: String = chars[k + 1..close_b].iter().collect();
                    idx_node = Some(Box::new(parse_display_add(&inner)));
                    k = close_b + 1;
                    while k < chars.len() && chars[k].is_whitespace() {
                        k += 1;
                    }
                }
            }
            let (rad, next_i) = consume_radical_arg(&chars, k);
            items.push(DisplayNode::Sqrt {
                index: idx_node,
                radicand: Box::new(rad),
            });
            i = next_i;
            continue;
        }

        // Big operators `∑` and `∏`
        if c == '∑' || c == '∏' {
            let sym = if c == '∑' { "∑" } else { "∏" };
            let mut k = i + 1;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            if chars.get(k) == Some(&'(') {
                if let Some(close) = find_matching_delim(&chars, k, '(', ')') {
                    let inner: String = chars[k + 1..close].iter().collect();
                    let args = split_top_level_commas(&inner);
                    i = close + 1;
                    if args.len() == 3 {
                        items.push(DisplayNode::BigOp {
                            sym,
                            lower: Some(Box::new(parse_display_add(&args[0]))),
                            upper: Some(Box::new(parse_display_add(&args[1]))),
                            body: Some(Box::new(parse_display_add(&args[2]))),
                        });
                    } else if args.len() == 2 {
                        items.push(DisplayNode::BigOp {
                            sym,
                            lower: Some(Box::new(parse_display_add(&args[0]))),
                            upper: None,
                            body: Some(Box::new(parse_display_add(&args[1]))),
                        });
                    } else {
                        items.push(DisplayNode::BigOp {
                            sym,
                            lower: None,
                            upper: None,
                            body: Some(Box::new(parse_display_add(&inner))),
                        });
                    }
                    continue;
                }
            }

            let mut lower: Option<DisplayNode> = None;
            let mut upper: Option<DisplayNode> = None;
            let mut has_bounds = false;

            loop {
                while k < chars.len() && chars[k].is_whitespace() {
                    k += 1;
                }
                if k >= chars.len() {
                    break;
                }
                if chars[k] == '_' && lower.is_none() {
                    let (node, next_k) = consume_script_arg(&chars, k + 1);
                    lower = Some(node);
                    has_bounds = true;
                    k = next_k;
                } else if chars[k] == '^' && upper.is_none() {
                    let (node, next_k) = consume_script_arg(&chars, k + 1);
                    upper = Some(node);
                    has_bounds = true;
                    k = next_k;
                } else if lower.is_none() && display_subscript_char_to_ascii(chars[k]).is_some() {
                    let mut sub = String::new();
                    while k < chars.len() {
                        if let Some(ch) = display_subscript_char_to_ascii(chars[k]) {
                            sub.push(ch);
                            k += 1;
                        } else {
                            break;
                        }
                    }
                    lower = Some(DisplayNode::Text(sub));
                    has_bounds = true;
                } else if upper.is_none() && display_superscript_char_to_ascii(chars[k]).is_some() {
                    let mut sup = String::new();
                    while k < chars.len() {
                        if let Some(ch) = display_superscript_char_to_ascii(chars[k]) {
                            sup.push(ch);
                            k += 1;
                        } else {
                            break;
                        }
                    }
                    upper = Some(DisplayNode::Text(sup));
                    has_bounds = true;
                } else {
                    break;
                }
            }

            if has_bounds {
                let rem: String = chars[k..].iter().collect();
                let body = if rem.trim().is_empty() {
                    None
                } else {
                    Some(Box::new(parse_display_tokens(&rem)))
                };
                i = chars.len();
                items.push(DisplayNode::BigOp {
                    sym,
                    lower: lower.map(Box::new),
                    upper: upper.map(Box::new),
                    body,
                });
                continue;
            }

            items.push(DisplayNode::BigOp {
                sym,
                lower: None,
                upper: None,
                body: None,
            });
            i += 1;
            continue;
        }

        // `∫(...)dx` or `∫(a, b, ...)dx`
        if c == '∫' {
            let mut k = i + 1;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            if chars.get(k) == Some(&'(') {
                if let Some(close) = find_matching_delim(&chars, k, '(', ')') {
                    let inner: String = chars[k + 1..close].iter().collect();
                    let args = split_top_level_commas(&inner);
                    let mut next_i = close + 1;
                    while next_i < chars.len() && chars[next_i].is_whitespace() {
                        next_i += 1;
                    }
                    if next_i + 1 < chars.len() && chars[next_i] == 'd' && chars[next_i + 1] == 'x' {
                        next_i += 2;
                    }
                    i = next_i;
                    if args.len() == 3 {
                        items.push(DisplayNode::Integral {
                            lower: Some(Box::new(parse_display_add(&args[0]))),
                            upper: Some(Box::new(parse_display_add(&args[1]))),
                            body: Box::new(parse_display_add(&args[2])),
                        });
                    } else {
                        items.push(DisplayNode::Integral {
                            lower: None,
                            upper: None,
                            body: Box::new(parse_display_add(&inner)),
                        });
                    }
                    continue;
                }
            }
            items.push(DisplayNode::Text("∫".into()));
            i += 1;
            continue;
        }

        // Superscripts before √ -> nth root, otherwise superscript postfix
        if display_superscript_char_to_ascii(c).is_some() {
            let mut sup = String::new();
            let mut j = i;
            while j < chars.len() {
                if let Some(m) = display_superscript_char_to_ascii(chars[j]) {
                    sup.push(m);
                    j += 1;
                } else {
                    break;
                }
            }
            let mut k = j;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            if chars.get(k) == Some(&'√') {
                let (rad, next_i) = consume_radical_arg(&chars, k + 1);
                i = next_i;
                let idx_node = if sup == "□" {
                    DisplayNode::Placeholder
                } else {
                    DisplayNode::Text(sup)
                };
                items.push(DisplayNode::Sqrt {
                    index: Some(Box::new(idx_node)),
                    radicand: Box::new(rad),
                });
                continue;
            }
            let sup_node = DisplayNode::Text(sup);
            if items.is_empty() {
                items.push(sup_node);
            } else {
                attach_postfix(&mut items, |base| DisplayNode::Sup(Box::new(base), Box::new(sup_node)));
            }
            i = j;
            continue;
        }

        // Unicode subscripts -> subscript postfix
        if display_subscript_char_to_ascii(c).is_some() {
            let mut sub = String::new();
            let mut j = i;
            while j < chars.len() {
                if let Some(m) = display_subscript_char_to_ascii(chars[j]) {
                    sub.push(m);
                    j += 1;
                } else {
                    break;
                }
            }
            let sub_node = DisplayNode::Text(sub);
            if items.is_empty() {
                items.push(sub_node);
            } else {
                attach_postfix(&mut items, |base| DisplayNode::Sub(Box::new(base), Box::new(sub_node)));
            }
            i = j;
            continue;
        }

        if c == '√' {
            let (rad, next_i) = consume_radical_arg(&chars, i + 1);
            i = next_i;
            items.push(DisplayNode::Sqrt {
                index: None,
                radicand: Box::new(rad),
            });
            continue;
        }

        if c == '□' {
            items.push(DisplayNode::Placeholder);
            i += 1;
            continue;
        }

        if c == '|' {
            if let Some(close) = chars[i + 1..].iter().position(|&ch| ch == '|') {
                let end = i + 1 + close;
                let inner: String = chars[i + 1..end].iter().collect();
                items.push(DisplayNode::Abs(Box::new(parse_display_add(&inner))));
                i = end + 1;
                continue;
            }
        }

        if c == '(' {
            if let Some(close) = find_matching_delim(&chars, i, '(', ')') {
                let inner: String = chars[i + 1..close].iter().collect();
                items.push(DisplayNode::Row(vec![
                    DisplayNode::Text("(".into()),
                    parse_display_add(&inner),
                    DisplayNode::Text(")".into()),
                ]));
                i = close + 1;
                continue;
            }
        }

        if c == '^' {
            let (exp, next_i) = consume_script_arg(&chars, i + 1);
            i = next_i;
            if items.is_empty() {
                items.push(exp);
            } else {
                attach_postfix(&mut items, |base| DisplayNode::Sup(Box::new(base), Box::new(exp)));
            }
            continue;
        }

        if c == '_' {
            let (sub, next_i) = consume_script_arg(&chars, i + 1);
            i = next_i;
            if items.is_empty() {
                items.push(sub);
            } else {
                attach_postfix(&mut items, |base| DisplayNode::Sub(Box::new(base), Box::new(sub)));
            }
            continue;
        }

        if c == '*' || c == '·' {
            items.push(DisplayNode::Op(" · ".into()));
            i += 1;
            continue;
        }

        if c == '×' {
            items.push(DisplayNode::Op(" × ".into()));
            i += 1;
            continue;
        }

        // `root(n, expr)` or `nthroot(n, expr)` -> nice radical
        if rest.starts_with("root(") || rest.starts_with("nthroot(") {
            let paren_i = i + if rest.starts_with("nthroot(") { 7 } else { 4 };
            if let Some(close) = find_matching_delim(&chars, paren_i, '(', ')') {
                let inner: String = chars[paren_i + 1..close].iter().collect();
                let args = split_top_level_commas(&inner);
                if args.len() == 2 {
                    i = close + 1;
                    items.push(DisplayNode::Sqrt {
                        index: Some(Box::new(parse_display_add(&args[0]))),
                        radicand: Box::new(parse_display_add(&args[1])),
                    });
                    continue;
                }
            }
        }

        // `log10` or `log₁₀` or `log2`
        if rest.starts_with("log10") || rest.starts_with("log₁₀") {
            let adv = if rest.starts_with("log10") { 5 } else { "log₁₀".chars().count() };
            items.push(DisplayNode::Sub(
                Box::new(DisplayNode::Text("log".into())),
                Box::new(DisplayNode::Text("10".into())),
            ));
            i += adv;
            continue;
        }
        if rest.starts_with("log2") || rest.starts_with("log₂") {
            let adv = if rest.starts_with("log2") { 4 } else { "log₂".chars().count() };
            items.push(DisplayNode::Sub(
                Box::new(DisplayNode::Text("log".into())),
                Box::new(DisplayNode::Text("2".into())),
            ));
            i += adv;
            continue;
        }

        // Accumulate normal run of characters
        let start = i;
        while i < chars.len() {
            let ch = chars[i];
            if ch.is_whitespace()
                || matches!(
                    ch,
                    '√' | '∫'
                        | '∑'
                        | '∏'
                        | '□'
                        | '|'
                        | '('
                        | '^'
                        | '_'
                        | '*'
                        | '·'
                        | '×'
                        | '\\'
                )
                || ch == ACTIVE_PLACEHOLDER_SENTINEL
                || ch == CARET_SENTINEL
                || display_superscript_char_to_ascii(ch).is_some()
                || display_subscript_char_to_ascii(ch).is_some()
            {
                break;
            }
            let r_sub: String = chars[i..].iter().collect();
            if r_sub.starts_with("d/dx")
                || r_sub.starts_with("root(")
                || r_sub.starts_with("nthroot(")
                || r_sub.starts_with("log10")
                || r_sub.starts_with("log₁₀")
                || r_sub.starts_with("log2")
                || r_sub.starts_with("log₂")
            {
                break;
            }
            i += 1;
        }
        if i > start {
            let chunk: String = chars[start..i].iter().collect();
            items.push(DisplayNode::Text(chunk));
        } else {
            items.push(DisplayNode::Text(c.to_string()));
            i += 1;
        }
    }

    if items.len() == 1 {
        items.pop().unwrap()
    } else {
        DisplayNode::Row(items)
    }
}

fn consume_radical_arg(chars: &[char], start_idx: usize) -> (DisplayNode, usize) {
    let mut k = start_idx;
    while k < chars.len() && chars[k].is_whitespace() {
        k += 1;
    }
    if chars.get(k) == Some(&'(') {
        if let Some(close) = find_matching_delim(chars, k, '(', ')') {
            let inner: String = chars[k + 1..close].iter().collect();
            return (parse_display_add(&inner), close + 1);
        }
    }
    if chars.get(k) == Some(&'{') {
        if let Some(close) = find_matching_delim(chars, k, '{', '}') {
            let inner: String = chars[k + 1..close].iter().collect();
            return (parse_display_add(&inner), close + 1);
        }
    }
    if chars.get(k) == Some(&ACTIVE_PLACEHOLDER_SENTINEL) {
        return (DisplayNode::ActivePlaceholder, k + 1);
    }
    if chars.get(k) == Some(&'□') {
        return (DisplayNode::Placeholder, k + 1);
    }
    if chars.get(k) == Some(&CARET_SENTINEL) {
        return (DisplayNode::Caret, k + 1);
    }
    let mut j = k;
    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '.') {
        j += 1;
    }
    if j > k {
        let s: String = chars[k..j].iter().collect();
        (DisplayNode::Text(s), j)
    } else {
        (DisplayNode::Placeholder, k)
    }
}

fn render_display_node(node: &DisplayNode, font_size: f32) -> El {
    let fs = font_size.clamp(8.0, 30.0);
    let fg = theme::text_primary();
    let muted = theme::text_secondary();
    match node {
        DisplayNode::Row(children) => {
            let mut r = row().center_y().gap(1.0);
            for ch in children {
                r = r.child(render_display_node(ch, fs));
            }
            r
        }
        DisplayNode::Text(s) => text(s)
            .font(TextStyle::new(fs, 500.0).lh(fs * 1.25))
            .color(fg),
        DisplayNode::Op(s) => text(s)
            .font(TextStyle::new(fs, 450.0).lh(fs * 1.25))
            .color(fg.alpha(0.86)),
        DisplayNode::Placeholder => el()
            .w((fs * 0.68).clamp(7.5, 17.0))
            .h((fs * 0.90).clamp(10.0, 21.0))
            .mx(1.5)
            .radius(2.5)
            .fill(theme::bg_control().alpha(0.65))
            .border(1.0, theme::text_muted().alpha(0.60)),
        DisplayNode::ActivePlaceholder => {
            let w = (fs * 0.72).clamp(8.5, 19.0);
            let h = (fs * 0.94).clamp(11.0, 22.0);
            col()
                .items_center()
                .gap(1.5)
                .mx(1.5)
                .child(
                    el()
                        .w(w)
                        .h(h)
                        .radius(2.5)
                        .fill(theme::accent_claude().alpha(0.16))
                        .border(1.35, theme::accent_claude()),
                )
                .child(el().w(w).h(2.0).radius(1.0).fill(theme::accent_claude()))
        }
        DisplayNode::Caret => el()
            .w(1.5)
            .h((fs * 1.12).clamp(11.0, 25.0))
            .mx(0.5)
            .radius(0.75)
            .fill(theme::accent_claude()),
        DisplayNode::Frac(num, den) => {
            let sub_fs = (fs * 0.78).max(7.5);
            col()
                .items_center()
                .gap(1.5)
                .px(2.0)
                .child(render_display_node(num, sub_fs))
                .child(el().w_full().min_w((fs * 0.8).max(10.0)).h(1.25).fill(fg))
                .child(render_display_node(den, sub_fs))
        }
        DisplayNode::Sqrt { index, radicand } => {
            let idx_fs = (fs * 0.60).max(7.0);
            let rad_h = fs * 1.3;
            let hook_w = (fs * 0.58).max(7.0);
            let radical_shape = Shape::new().stroke(
                vec![
                    [1.0, rad_h * 0.56],
                    [hook_w * 0.35, rad_h * 0.56],
                    [hook_w * 0.62, rad_h - 2.0],
                    [hook_w, 1.5],
                ],
                1.35,
                fg,
            );
            let mut hook_col = row().items_end();
            if let Some(idx_node) = index {
                hook_col = hook_col.child(
                    el().pb(rad_h * 0.42)
                        .mr(-hook_w * 0.15)
                        .child(render_display_node(idx_node, idx_fs)),
                );
            }
            hook_col = hook_col.child(
                el().w(hook_w)
                    .h(rad_h)
                    .child(canvas(radical_shape).absolute(0.0, 0.0).w_full().h_full()),
            );

            row()
                .items_end()
                .child(hook_col)
                .child(
                    el().pt(1.5)
                        .px(3.0)
                        .border_sides([1.25, 0.0, 0.0, 0.0], fg)
                        .child(render_display_node(radicand, fs)),
                )
        }
        DisplayNode::Sup(base, exp) => {
            let sup_fs = (fs * 0.66).max(7.5);
            row()
                .items_start()
                .child(render_display_node(base, fs))
                .child(el().pb(fs * 0.35).child(render_display_node(exp, sup_fs)))
        }
        DisplayNode::Sub(base, sub) => {
            let sub_fs = (fs * 0.66).max(7.5);
            row()
                .items_end()
                .child(render_display_node(base, fs))
                .child(el().pt(fs * 0.25).child(render_display_node(sub, sub_fs)))
        }
        DisplayNode::Abs(inner) => row()
            .center_y()
            .gap(1.5)
            .child(el().w(1.25).h(fs * 1.18).fill(fg))
            .child(render_display_node(inner, fs))
            .child(el().w(1.25).h(fs * 1.18).fill(fg)),
        DisplayNode::Deriv(inner) => {
            let sub_fs = (fs * 0.70).max(7.5);
            row()
                .center_y()
                .gap(3.0)
                .child(
                    col()
                        .items_center()
                        .gap(1.0)
                        .child(text("d").font(TextStyle::new(sub_fs, 500.0)).color(fg))
                        .child(el().w_full().min_w(10.0).h(1.2).fill(fg))
                        .child(text("dx").font(TextStyle::new(sub_fs, 500.0)).color(fg)),
                )
                .child(render_display_node(inner, fs))
        }
        DisplayNode::Integral { lower, upper, body } => {
            let lim_fs = (fs * 0.60).max(7.0);
            let mut r = row().center_y().gap(2.0).child(
                text("∫")
                    .font(TextStyle::new(fs * 1.25, 450.0))
                    .color(fg),
            );
            if lower.is_some() || upper.is_some() {
                let mut lim_col = col().gap(1.0);
                if let Some(u) = upper {
                    lim_col = lim_col.child(render_display_node(u, lim_fs));
                }
                if let Some(l) = lower {
                    lim_col = lim_col.child(render_display_node(l, lim_fs));
                }
                r = r.child(lim_col);
            }
            r.child(render_display_node(body, fs))
                .child(text(" dx").font(TextStyle::new(fs * 0.85, 450.0)).color(muted))
        }
        DisplayNode::BigOp {
            sym,
            lower,
            upper,
            body,
        } => {
            let lim_fs = (fs * 0.58).max(7.0);
            let mut op_col = col().items_center().gap(0.5);
            if let Some(u) = upper {
                op_col = op_col.child(render_display_node(u, lim_fs));
            }
            op_col = op_col.child(
                text(*sym)
                    .font(TextStyle::new(fs * 1.32, 450.0).lh(fs * 1.2))
                    .color(fg),
            );
            if let Some(l) = lower {
                op_col = op_col.child(render_display_node(l, lim_fs));
            }
            if let Some(b) = body {
                row()
                    .center_y()
                    .gap(4.0)
                    .child(op_col)
                    .child(render_display_node(b, fs))
            } else {
                op_col
            }
        }
    }
}

fn render_pretty_math_impl(
    content: &str,
    font_size: f32,
    selection: Option<(usize, usize)>,
    show_curve_dots: bool,
    split_semicolons: bool,
    placeholder: &str,
) -> El {
    let fs = font_size.clamp(9.0, 28.0);
    if content.trim().is_empty() {
        if selection.is_some() && !show_curve_dots {
            return row()
                .center_y()
                .gap(4.0)
                .child(
                    el().w(1.5)
                        .h((fs * 1.15).clamp(13.0, 26.0))
                        .radius(0.75)
                        .fill(theme::accent_claude()),
                )
                .child(
                    el().pb(2.0)
                        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_default())
                        .child(
                            text(placeholder)
                                .font(TextStyle::new(fs * 0.80, 400.0))
                                .color(theme::text_muted()),
                        ),
                );
        }
        return text(placeholder)
            .font(TextStyle::new(fs * 0.85, 400.0))
            .color(theme::text_muted());
    }

    let decorated = inject_display_cursor(content, selection);
    let raw_lines: Vec<&str> = if split_semicolons {
        decorated.split(['\n', ';']).collect()
    } else {
        decorated.split('\n').collect()
    };
    let lines: Vec<&str> = raw_lines
        .into_iter()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return text(placeholder)
            .font(TextStyle::new(fs * 0.85, 400.0))
            .color(theme::text_muted());
    }
    let mut c = col().gap(5.0);
    for (idx, line) in lines.iter().enumerate() {
        let ast = parse_display_node(line);
        let mut r = row().center_y().gap(6.0);
        if show_curve_dots && lines.len() > 1 {
            r = r.child(
                el().size(6.0, 6.0)
                    .radius(3.0)
                    .fill(curve_palette_color(idx)),
            );
        }
        r = r.child(render_display_node(&ast, fs));
        c = c.child(r);
    }
    c
}

pub fn render_pretty_math(content: &str, font_size: f32) -> El {
    render_pretty_math_impl(content, font_size, None, true, true, "y = sin(x)")
}

pub fn render_pretty_math_formula(
    content: &str,
    font_size: f32,
    selection: Option<(usize, usize)>,
    placeholder: &str,
) -> El {
    render_pretty_math_impl(content, font_size, selection, false, false, placeholder)
}

pub fn render_keycap_math(expr: &str, font_size: f32) -> El {
    let ast = parse_display_node(expr);
    render_display_node(&ast, font_size.clamp(8.0, 14.0))
}

/// Evaluate or simplify a standalone math expression/equation for `NodeKind::Formula`.
/// Returns `None` cleanly for empty, multi-statement symbolic notes, or trivial echoes.
pub fn evaluate_formula_readout(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (stmts, err) = parse_statements(trimmed);
    if err.is_some() || stmts.is_empty() {
        return None;
    }
    let mut vp = GraphViewport {
        center_x: 0.0,
        center_y: 0.0,
        span_x: 40.0,
    };
    let mut ro = evaluate_readout(&stmts, &vp)?;
    if matches!(stmts.first(), Some(PlotStatement::EquationX { .. })) && !ro.starts_with("x = ") {
        vp.span_x = 200.0;
        if let Some(wider) = evaluate_readout(&stmts, &vp) {
            ro = wider;
        }
    }
    let norm = |s: &str| {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .replace("·", "")
            .replace("−", "-")
    };
    let n_ro = norm(&ro);
    let first_line = trimmed.lines().next().unwrap_or(trimmed).trim();
    if n_ro == norm(first_line) {
        return None;
    }
    if let Some((_, rhs)) = first_line.split_once('=') {
        if n_ro == norm(rhs) {
            return None;
        }
    }
    Some(ro)
}

// ---------------------------------------------------------------------------
// Photomath-Style Math Keypad & Templates
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeypadMode {
    #[default]
    Sci,
    Basic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeypadAction {
    InsertTemplate(&'static str),
    Newline,
    MoveLeft,
    MoveRight,
    Backspace,
    Clear,
    ToggleMode,
}

#[derive(Clone, Copy, Debug)]
pub struct KeypadButton {
    pub id: &'static str,
    pub label: &'static str,
    pub action: KeypadAction,
}

pub const SCI_KEYPAD_ROWS: &[&[KeypadButton]] = &[
    &[
        KeypadButton { id: "lt", label: "<", action: KeypadAction::InsertTemplate("<") },
        KeypadButton { id: "gt", label: ">", action: KeypadAction::InsertTemplate(">") },
        KeypadButton { id: "pi", label: "π", action: KeypadAction::InsertTemplate("π") },
        KeypadButton { id: "inf", label: "∞", action: KeypadAction::InsertTemplate("∞") },
        KeypadButton { id: "deg", label: "°", action: KeypadAction::InsertTemplate("°") },
        KeypadButton { id: "sin", label: "sin", action: KeypadAction::InsertTemplate("sin(□)") },
    ],
    &[
        KeypadButton { id: "int", label: "∫□dx", action: KeypadAction::InsertTemplate("∫(□)dx") },
        KeypadButton { id: "fact", label: "!", action: KeypadAction::InsertTemplate("!") },
        KeypadButton { id: "e", label: "e", action: KeypadAction::InsertTemplate("e") },
        KeypadButton { id: "ln", label: "ln", action: KeypadAction::InsertTemplate("ln(□)") },
        KeypadButton { id: "log10", label: "log₁₀", action: KeypadAction::InsertTemplate("log₁₀(□)") },
        KeypadButton { id: "cos", label: "cos", action: KeypadAction::InsertTemplate("cos(□)") },
    ],
    &[
        KeypadButton { id: "deriv", label: "d/dx □", action: KeypadAction::InsertTemplate("d/dx(□)") },
        KeypadButton { id: "abs", label: "|□|", action: KeypadAction::InsertTemplate("|□|") },
        KeypadButton { id: "exp", label: "exp", action: KeypadAction::InsertTemplate("exp(□)") },
        KeypadButton { id: "sign", label: "sign", action: KeypadAction::InsertTemplate("sign(□)") },
        KeypadButton { id: "sqrt", label: "√□", action: KeypadAction::InsertTemplate("√(□)") },
        KeypadButton { id: "tan", label: "tan", action: KeypadAction::InsertTemplate("tan(□)") },
    ],
    &[
        KeypadButton { id: "frac", label: "a/b", action: KeypadAction::InsertTemplate("(□)/(□)") },
        KeypadButton { id: "pow", label: "x^□", action: KeypadAction::InsertTemplate("^□") },
        KeypadButton { id: "nroot", label: "ⁿ√□", action: KeypadAction::InsertTemplate("³√(□)") },
        KeypadButton { id: "le", label: "≤", action: KeypadAction::InsertTemplate("≤") },
        KeypadButton { id: "ge", label: "≥", action: KeypadAction::InsertTemplate("≥") },
        KeypadButton { id: "cot", label: "cot", action: KeypadAction::InsertTemplate("cot(□)") },
    ],
];

pub const BASIC_KEYPAD_ROWS: &[&[KeypadButton]] = &[
    &[
        KeypadButton { id: "x", label: "x", action: KeypadAction::InsertTemplate("x") },
        KeypadButton { id: "y", label: "y", action: KeypadAction::InsertTemplate("y") },
        KeypadButton { id: "par", label: "( )", action: KeypadAction::InsertTemplate("(□)") },
        KeypadButton { id: "frac", label: "a/b", action: KeypadAction::InsertTemplate("(□)/(□)") },
        KeypadButton { id: "sq", label: "x²", action: KeypadAction::InsertTemplate("²") },
        KeypadButton { id: "pow", label: "x^□", action: KeypadAction::InsertTemplate("^□") },
    ],
    &[
        KeypadButton { id: "7", label: "7", action: KeypadAction::InsertTemplate("7") },
        KeypadButton { id: "8", label: "8", action: KeypadAction::InsertTemplate("8") },
        KeypadButton { id: "9", label: "9", action: KeypadAction::InsertTemplate("9") },
        KeypadButton { id: "div", label: "÷", action: KeypadAction::InsertTemplate(" / ") },
        KeypadButton { id: "sqrt", label: "√□", action: KeypadAction::InsertTemplate("√(□)") },
        KeypadButton { id: "cbrt", label: "³√□", action: KeypadAction::InsertTemplate("³√(□)") },
    ],
    &[
        KeypadButton { id: "4", label: "4", action: KeypadAction::InsertTemplate("4") },
        KeypadButton { id: "5", label: "5", action: KeypadAction::InsertTemplate("5") },
        KeypadButton { id: "6", label: "6", action: KeypadAction::InsertTemplate("6") },
        KeypadButton { id: "mul", label: "×", action: KeypadAction::InsertTemplate("·") },
        KeypadButton { id: "abs", label: "|□|", action: KeypadAction::InsertTemplate("|□|") },
        KeypadButton { id: "eq", label: "=", action: KeypadAction::InsertTemplate(" = ") },
    ],
    &[
        KeypadButton { id: "1", label: "1", action: KeypadAction::InsertTemplate("1") },
        KeypadButton { id: "2", label: "2", action: KeypadAction::InsertTemplate("2") },
        KeypadButton { id: "3", label: "3", action: KeypadAction::InsertTemplate("3") },
        KeypadButton { id: "0", label: "0", action: KeypadAction::InsertTemplate("0") },
        KeypadButton { id: "sub", label: "−", action: KeypadAction::InsertTemplate(" - ") },
        KeypadButton { id: "add", label: "+", action: KeypadAction::InsertTemplate(" + ") },
    ],
];

pub const GRAPH_PRESETS: &[(&str, &str)] = &[
    ("sin(x)", "y = sin(x)"),
    ("Parabola", "y = x² - 2x - 3"),
    ("Circle", "x² + y² = 9"),
    ("2x + ³√27", "2x + ³√(27)"),
    ("1 / x", "y = 1 / x"),
    ("y > sin(x)", "y > sin(x)"),
    ("Dual Trig", "y = sin(x)\ny = cos(x)"),
    ("d/dx(x³-3x)", "y = d/dx(x³ - 3x)"),
];

pub fn apply_keypad_to_edit(edit: &mut TextEdit, action: KeypadAction) {
    match action {
        KeypadAction::ToggleMode => {}
        KeypadAction::Clear => {
            edit.clear();
        }
        KeypadAction::Newline => {
            edit.insert("\n");
        }
        KeypadAction::MoveLeft => {
            let (a, _b) = edit.range();
            if let Some(prev_box) = edit.text[..a].rfind('□') {
                edit.set_selection(prev_box, prev_box + '□'.len_utf8());
            } else if a > 0 {
                let prev = edit.text[..a]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                edit.set_caret(prev);
            }
        }
        KeypadAction::MoveRight => {
            let (_a, b) = edit.range();
            if let Some(rel_box) = edit.text[b..].find('□') {
                let start = b + rel_box;
                edit.set_selection(start, start + '□'.len_utf8());
            } else if b < edit.text.len() {
                let next = edit.text[b..]
                    .chars()
                    .next()
                    .map(|ch| b + ch.len_utf8())
                    .unwrap_or(edit.text.len());
                edit.set_caret(next);
            }
        }
        KeypadAction::Backspace => {
            let (a, b) = edit.range();
            if a != b {
                edit.insert("");
            } else if a > 0 {
                let prev = edit.text[..a]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                edit.set_selection(prev, a);
                edit.insert("");
            }
        }
        KeypadAction::InsertTemplate(tpl) => {
            // If caret sits right before or after a `□` without selection, replace that `□`
            let (a, b) = edit.range();
            if a == b {
                let box_len = '□'.len_utf8();
                if edit.text[b..].starts_with('□') {
                    edit.set_selection(b, b + box_len);
                } else if edit.text[..a].ends_with('□') {
                    edit.set_selection(a - box_len, a);
                }
            }
            let (ins_start, _) = edit.range();
            edit.insert(tpl);
            if let Some(rel_box) = tpl.find('□') {
                let abs_start = ins_start + rel_box;
                let abs_end = abs_start + '□'.len_utf8();
                if abs_end <= edit.text.len() {
                    edit.set_selection(abs_start, abs_end);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_str(s: &str, x: f64, y: f64) -> f64 {
        match parse_single_statement(s).expect("should parse") {
            PlotStatement::ExplicitY { expr, .. } => eval_expr(&expr, x, y),
            PlotStatement::ImplicitEq { lhs, rhs, .. } => {
                eval_expr(&lhs, x, y) - eval_expr(&rhs, x, y)
            }
            _ => panic!("unexpected statement variant"),
        }
    }

    #[test]
    fn test_math_parsing_and_evaluation() {
        assert!((eval_str("2x + ³√(27)", 1.0, 0.0) - 5.0).abs() < 1e-9);
        assert!((eval_str("2x + ³√(27) = □", 2.0, 0.0) - 7.0).abs() < 1e-9);
        assert!((eval_str("y = 3sin(90°)", 0.0, 0.0) - 3.0).abs() < 1e-9);
        assert!((eval_str("y = log₁₀(100) + ln(e)", 0.0, 0.0) - 3.0).abs() < 1e-9);
        assert!((eval_str("y = |x| + 5!", -4.0, 0.0) - 124.0).abs() < 1e-9);
        assert!((eval_str("y = x² + x⁻¹", 2.0, 0.0) - 4.5).abs() < 1e-9);
        assert!((eval_str("y = d/dx(x³)", 2.0, 0.0) - 12.0).abs() < 1e-4);
        assert!((eval_str("y = ∫(cos(x))dx", PI * 0.5, 0.0) - 1.0).abs() < 1e-5);
        assert!((eval_str("y = cot(π/4)", 0.0, 0.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_simplification_readout_matches_photomath_example() {
        let (stmts, err) = parse_statements("2x + ³√(27) = □");
        assert!(err.is_none());
        let vp = GraphViewport::default();
        assert_eq!(evaluate_readout(&stmts, &vp).as_deref(), Some("2x + 3"));
    }

    #[test]
    fn test_plot_strict_clipping_and_asymptote_split() {
        let vp = GraphViewport::default();
        let w = 320.0;
        let h = 220.0;

        for expr in ["y = 1 / x", "y = tan(x)", "x² + y² = 9", "y > sin(x)"] {
            let plot = build_graph_plot(expr, &vp, w, h, 1.0);
            assert!(plot.error.is_none(), "unexpected error for {expr}: {:?}", plot.error);
            assert!(!plot.shape.is_empty(), "expected geometry for {expr}");
            for op in &plot.shape.ops {
                match op {
                    rune::shape::Op::Stroke { points, .. } => {
                        for p in points {
                            assert!(
                                p[0] >= -0.01 && p[0] <= w + 0.01 && p[1] >= -0.01 && p[1] <= h + 0.01,
                                "stroke point out of bounds for {expr}: {p:?}"
                            );
                        }
                    }
                    rune::shape::Op::Fill { points, .. } => {
                        for p in points {
                            assert!(
                                p[0] >= -0.01 && p[0] <= w + 0.01 && p[1] >= -0.01 && p[1] <= h + 0.01,
                                "fill point out of bounds for {expr}: {p:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_viewport_zoom_and_pan_preserves_anchor() {
        let mut vp = GraphViewport::default();
        let (w, h) = (400.0, 300.0);
        let (ax0, ay0) = vp.px_to_math(100.0, 75.0, w, h);
        vp.zoom_at(0.25, 0.25, 0.5, w, h);
        let (ax1, ay1) = vp.px_to_math(100.0, 75.0, w, h);
        assert!((ax0 - ax1).abs() < 1e-6);
        assert!((ay0 - ay1).abs() < 1e-6);
        assert!((vp.span_x - 6.0).abs() < 1e-6);

        let cx0 = vp.center_x;
        let cy0 = vp.center_y;
        vp.pan_by_px(cx0, cy0, 40.0, -30.0, w, h);
        assert!(vp.center_x < cx0);
        assert!(vp.center_y < cy0);
    }

    #[test]
    fn test_keypad_template_insertion_and_placeholder_navigation() {
        let mut edit = TextEdit::new("2x + ");
        apply_keypad_to_edit(&mut edit, KeypadAction::InsertTemplate("³√(□)"));
        assert_eq!(edit.text, "2x + ³√(□)");
        assert_eq!(edit.selected(), "□");

        apply_keypad_to_edit(&mut edit, KeypadAction::InsertTemplate("27"));
        assert_eq!(edit.text, "2x + ³√(27)");

        apply_keypad_to_edit(&mut edit, KeypadAction::Backspace);
        assert_eq!(edit.text, "2x + ³√(2)");
    }

    #[test]
    fn test_formula_2d_display_ast_and_readout() {
        assert_eq!(
            evaluate_formula_readout("2x + root(3, 27) = □").as_deref(),
            Some("2x + 3")
        );
        assert_eq!(
            evaluate_formula_readout("2x + root(3, 27) = ln(e)").as_deref(),
            Some("x = -1")
        );
        assert_eq!(evaluate_formula_readout("3 + 4").as_deref(), Some("= 7"));
        assert_eq!(evaluate_formula_readout("x"), None);
        assert_eq!(evaluate_formula_readout("Δw_ij = η · x_i · y_j"), None);

        assert!(matches!(
            parse_display_node("\\frac{a + b}{c}"),
            DisplayNode::Frac(_, _)
        ));
        assert!(matches!(
            parse_display_node("root(3, 27)"),
            DisplayNode::Sqrt { index: Some(_), .. }
        ));
        assert!(matches!(
            parse_display_node("∑(i=1, n, x_i)"),
            DisplayNode::BigOp { sym: "∑", lower: Some(_), upper: Some(_), body: Some(_) }
        ));
        assert!(matches!(
            parse_display_node("∑ᵢ₌₁ⁿ xᵢ"),
            DisplayNode::BigOp { sym: "∑", lower: Some(_), upper: Some(_), body: Some(_) }
        ));

        let decorated = inject_display_cursor("2x + root(□, □)", Some((10, 13)));
        assert!(matches!(
            parse_display_node(&decorated),
            DisplayNode::Row(ref v) if v.iter().any(|n| matches!(n, DisplayNode::Sqrt { index: Some(b), .. } if matches!(**b, DisplayNode::ActivePlaceholder)))
        ));
    }
}
