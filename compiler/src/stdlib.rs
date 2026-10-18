/// P11 — NXL Standard Library
///
/// # Implementation status
///
/// | Module        | Status        |
/// |---------------|---------------|
/// | collections   | IMPLEMENTED   |
/// | strings       | IMPLEMENTED   |
/// | math          | IMPLEMENTED   |
/// | errors        | IMPLEMENTED   |
/// | env           | IMPLEMENTED   |
/// | process       | IMPLEMENTED   |
/// | logging       | IMPLEMENTED   |
/// | json          | IMPLEMENTED   |
/// | random        | IMPLEMENTED   |
/// | time          | IMPLEMENTED   |
/// | testing       | IMPLEMENTED   |
/// | fs            | IMPLEMENTED   |
/// | http          | NOT IMPLEMENTED (P15+) |
/// | crypto        | IMPLEMENTED   |
/// | networking    | NOT IMPLEMENTED (P15+) |
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::diagnostic::Diagnostic;
use crate::source::Span;

// ── StdlibError ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct StdlibError {
    pub code: &'static str,
    pub message: String,
}

impl StdlibError {
    fn new(code: &'static str, msg: impl Into<String>) -> Self {
        Self {
            code,
            message: msg.into(),
        }
    }
}

impl fmt::Display for StdlibError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error[{}]: {}", self.code, self.message)
    }
}

impl From<StdlibError> for Diagnostic {
    fn from(e: StdlibError) -> Self {
        Diagnostic::error(e.code, e.message, Span::default())
    }
}

pub type StdResult<T> = Result<T, StdlibError>;

// ── NxlValue — universal value type for stdlib APIs ──────────────────────────

/// A dynamically-typed value that stdlib functions accept and return.
#[derive(Clone, Debug, PartialEq)]
pub enum NxlValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<NxlValue>),
    Map(Vec<(String, NxlValue)>),
}

impl fmt::Display for NxlValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NxlValue::Null => write!(f, "null"),
            NxlValue::Bool(b) => write!(f, "{b}"),
            NxlValue::Int(n) => write!(f, "{n}"),
            NxlValue::Float(n) => write!(f, "{n}"),
            NxlValue::Str(s) => write!(f, "{s}"),
            NxlValue::List(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            NxlValue::Map(pairs) => {
                write!(f, "{{")?;
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

// ── collections ───────────────────────────────────────────────────────────────

/// NXL List — ordered, growable sequence.
#[derive(Clone, Debug, Default)]
pub struct NxlList {
    items: Vec<NxlValue>,
}

impl NxlList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, v: NxlValue) {
        self.items.push(v);
    }

    pub fn pop(&mut self) -> Option<NxlValue> {
        self.items.pop()
    }

    pub fn get(&self, index: usize) -> StdResult<&NxlValue> {
        self.items.get(index).ok_or_else(|| {
            StdlibError::new(
                "NXL11001",
                format!("index {index} out of bounds (len={})", self.items.len()),
            )
        })
    }

    pub fn set(&mut self, index: usize, v: NxlValue) -> StdResult<()> {
        let len = self.items.len();
        self.items
            .get_mut(index)
            .map(|slot| *slot = v)
            .ok_or_else(|| {
                StdlibError::new(
                    "NXL11001",
                    format!("index {index} out of bounds (len={len})"),
                )
            })
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn contains(&self, v: &NxlValue) -> bool {
        self.items.contains(v)
    }

    pub fn remove(&mut self, index: usize) -> StdResult<NxlValue> {
        let len = self.items.len();
        if index >= len {
            return Err(StdlibError::new(
                "NXL11001",
                format!("index {index} out of bounds (len={len})"),
            ));
        }
        Ok(self.items.remove(index))
    }

    pub fn iter(&self) -> std::slice::Iter<'_, NxlValue> {
        self.items.iter()
    }

    pub fn to_value(&self) -> NxlValue {
        NxlValue::List(self.items.clone())
    }
}

/// NXL Map — ordered key-value store (insertion order preserved).
#[derive(Clone, Debug, Default)]
pub struct NxlMap {
    pairs: Vec<(String, NxlValue)>,
}

impl NxlMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, value: NxlValue) {
        let key = key.into();
        if let Some(pair) = self.pairs.iter_mut().find(|(k, _)| k == &key) {
            pair.1 = value;
        } else {
            self.pairs.push((key, value));
        }
    }

    pub fn get(&self, key: &str) -> Option<&NxlValue> {
        self.pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn remove(&mut self, key: &str) -> Option<NxlValue> {
        if let Some(pos) = self.pairs.iter().position(|(k, _)| k == key) {
            Some(self.pairs.remove(pos).1)
        } else {
            None
        }
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.pairs.iter().any(|(k, _)| k == key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.pairs.iter().map(|(k, _)| k.as_str())
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    pub fn to_value(&self) -> NxlValue {
        NxlValue::Map(self.pairs.clone())
    }
}

// ── strings ───────────────────────────────────────────────────────────────────

pub mod strings {
    use super::{StdResult, StdlibError};

    pub fn to_upper(s: &str) -> String {
        s.to_uppercase()
    }
    pub fn to_lower(s: &str) -> String {
        s.to_lowercase()
    }
    pub fn trim(s: &str) -> &str {
        s.trim()
    }
    pub fn trim_start(s: &str) -> &str {
        s.trim_start()
    }
    pub fn trim_end(s: &str) -> &str {
        s.trim_end()
    }
    pub fn len(s: &str) -> usize {
        s.chars().count()
    }
    pub fn is_empty(s: &str) -> bool {
        s.is_empty()
    }
    pub fn contains(s: &str, pat: &str) -> bool {
        s.contains(pat)
    }
    pub fn starts_with(s: &str, pat: &str) -> bool {
        s.starts_with(pat)
    }
    pub fn ends_with(s: &str, pat: &str) -> bool {
        s.ends_with(pat)
    }
    pub fn replace(s: &str, from: &str, to: &str) -> String {
        s.replace(from, to)
    }
    pub fn split(s: &str, pat: &str) -> Vec<String> {
        s.split(pat).map(|p| p.to_owned()).collect()
    }
    pub fn join(parts: &[String], sep: &str) -> String {
        parts.join(sep)
    }
    pub fn repeat(s: &str, n: usize) -> String {
        s.repeat(n)
    }
    pub fn reverse(s: &str) -> String {
        s.chars().rev().collect()
    }

    pub fn char_at(s: &str, index: usize) -> StdResult<char> {
        s.chars().nth(index).ok_or_else(|| {
            StdlibError::new("NXL11002", format!("char index {index} out of bounds"))
        })
    }

    pub fn substring(s: &str, start: usize, end: usize) -> StdResult<String> {
        let chars: Vec<char> = s.chars().collect();
        if start > end || end > chars.len() {
            return Err(StdlibError::new(
                "NXL11002",
                format!(
                    "substring [{start}..{end}] out of bounds (len={})",
                    chars.len()
                ),
            ));
        }
        Ok(chars[start..end].iter().collect())
    }

    pub fn parse_int(s: &str) -> StdResult<i64> {
        s.trim()
            .parse::<i64>()
            .map_err(|_| StdlibError::new("NXL11003", format!("cannot parse '{s}' as Int")))
    }

    pub fn parse_float(s: &str) -> StdResult<f64> {
        s.trim()
            .parse::<f64>()
            .map_err(|_| StdlibError::new("NXL11003", format!("cannot parse '{s}' as Float")))
    }

    pub fn pad_left(s: &str, width: usize, ch: char) -> String {
        let n = s.chars().count();
        if n >= width {
            return s.to_owned();
        }
        let pad: String = std::iter::repeat_n(ch, width - n).collect();
        format!("{pad}{s}")
    }

    pub fn pad_right(s: &str, width: usize, ch: char) -> String {
        let n = s.chars().count();
        if n >= width {
            return s.to_owned();
        }
        let pad: String = std::iter::repeat_n(ch, width - n).collect();
        format!("{s}{pad}")
    }
}

// ── math ──────────────────────────────────────────────────────────────────────

pub mod math {
    pub const PI: f64 = std::f64::consts::PI;
    pub const E: f64 = std::f64::consts::E;
    pub const TAU: f64 = std::f64::consts::TAU;
    pub const INFINITY: f64 = f64::INFINITY;
    pub const NAN: f64 = f64::NAN;

    pub fn abs_int(n: i64) -> i64 {
        n.abs()
    }
    pub fn abs_float(n: f64) -> f64 {
        n.abs()
    }
    pub fn sqrt(n: f64) -> f64 {
        n.sqrt()
    }
    pub fn cbrt(n: f64) -> f64 {
        n.cbrt()
    }
    pub fn pow_int(base: i64, exp: u32) -> i64 {
        base.pow(exp)
    }
    pub fn pow_float(base: f64, exp: f64) -> f64 {
        base.powf(exp)
    }
    pub fn exp(n: f64) -> f64 {
        n.exp()
    }
    pub fn ln(n: f64) -> f64 {
        n.ln()
    }
    pub fn log2(n: f64) -> f64 {
        n.log2()
    }
    pub fn log10(n: f64) -> f64 {
        n.log10()
    }
    pub fn floor(n: f64) -> f64 {
        n.floor()
    }
    pub fn ceil(n: f64) -> f64 {
        n.ceil()
    }
    pub fn round(n: f64) -> f64 {
        n.round()
    }
    pub fn trunc(n: f64) -> f64 {
        n.trunc()
    }
    pub fn sin(n: f64) -> f64 {
        n.sin()
    }
    pub fn cos(n: f64) -> f64 {
        n.cos()
    }
    pub fn tan(n: f64) -> f64 {
        n.tan()
    }
    pub fn asin(n: f64) -> f64 {
        n.asin()
    }
    pub fn acos(n: f64) -> f64 {
        n.acos()
    }
    pub fn atan(n: f64) -> f64 {
        n.atan()
    }
    pub fn atan2(y: f64, x: f64) -> f64 {
        y.atan2(x)
    }
    pub fn min_int(a: i64, b: i64) -> i64 {
        a.min(b)
    }
    pub fn max_int(a: i64, b: i64) -> i64 {
        a.max(b)
    }
    pub fn min_float(a: f64, b: f64) -> f64 {
        a.min(b)
    }
    pub fn max_float(a: f64, b: f64) -> f64 {
        a.max(b)
    }
    pub fn clamp_int(v: i64, lo: i64, hi: i64) -> i64 {
        v.clamp(lo, hi)
    }
    pub fn clamp_float(v: f64, lo: f64, hi: f64) -> f64 {
        v.clamp(lo, hi)
    }
    pub fn is_nan(n: f64) -> bool {
        n.is_nan()
    }
    pub fn is_infinite(n: f64) -> bool {
        n.is_infinite()
    }
    pub fn is_finite(n: f64) -> bool {
        n.is_finite()
    }
    pub fn to_radians(deg: f64) -> f64 {
        deg.to_radians()
    }
    pub fn to_degrees(rad: f64) -> f64 {
        rad.to_degrees()
    }
    pub fn gcd(mut a: u64, mut b: u64) -> u64 {
        while b != 0 {
            let t = b;
            b = a % b;
            a = t;
        }
        a
    }
    pub fn lcm(a: u64, b: u64) -> u64 {
        if a == 0 || b == 0 {
            0
        } else {
            a / gcd(a, b) * b
        }
    }
}

// ── env ───────────────────────────────────────────────────────────────────────

pub mod env {
    use super::{StdResult, StdlibError};

    pub fn get_var(key: &str) -> StdResult<String> {
        std::env::var(key).map_err(|_| {
            StdlibError::new("NXL11004", format!("environment variable '{key}' not set"))
        })
    }

    pub fn get_var_or(key: &str, default: &str) -> String {
        std::env::var(key).unwrap_or_else(|_| default.to_owned())
    }

    pub fn set_var(key: &str, value: &str) {
        // Safety: single-threaded stdlib context; documented limitation.
        unsafe {
            std::env::set_var(key, value);
        }
    }

    pub fn remove_var(key: &str) {
        unsafe {
            std::env::remove_var(key);
        }
    }

    pub fn vars() -> Vec<(String, String)> {
        std::env::vars().collect()
    }

    pub fn current_dir() -> StdResult<String> {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| StdlibError::new("NXL11004", e.to_string()))
    }

    pub fn args() -> Vec<String> {
        std::env::args().collect()
    }
}

// ── process ───────────────────────────────────────────────────────────────────

pub mod process {
    use super::{StdResult, StdlibError};

    #[derive(Debug, Clone)]
    pub struct ProcessOutput {
        pub stdout: String,
        pub stderr: String,
        pub exit_code: i32,
    }

    impl ProcessOutput {
        pub fn success(&self) -> bool {
            self.exit_code == 0
        }
    }

    pub fn run(program: &str, args: &[&str]) -> StdResult<ProcessOutput> {
        let output = std::process::Command::new(program)
            .args(args)
            .output()
            .map_err(|e| StdlibError::new("NXL11005", format!("failed to run '{program}': {e}")))?;
        Ok(ProcessOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    pub fn exit(code: i32) -> ! {
        std::process::exit(code)
    }
}

// ── logging ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

pub struct Logger {
    pub name: String,
    pub min_level: LogLevel,
}

impl Logger {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            min_level: LogLevel::Info,
        }
    }

    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.min_level = level;
        self
    }

    fn log(&self, level: LogLevel, msg: &str) {
        if level >= self.min_level {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs();
            eprintln!("[{ts}] {level} [{}] {msg}", self.name);
        }
    }

    pub fn debug(&self, msg: &str) {
        self.log(LogLevel::Debug, msg);
    }
    pub fn info(&self, msg: &str) {
        self.log(LogLevel::Info, msg);
    }
    pub fn warn(&self, msg: &str) {
        self.log(LogLevel::Warn, msg);
    }
    pub fn error(&self, msg: &str) {
        self.log(LogLevel::Error, msg);
    }
}

// ── json ──────────────────────────────────────────────────────────────────────

pub mod json {
    use super::{NxlValue, StdResult, StdlibError};

    /// Serialize an `NxlValue` to a JSON string.
    pub fn stringify(value: &NxlValue) -> String {
        match value {
            NxlValue::Null => "null".to_owned(),
            NxlValue::Bool(b) => b.to_string(),
            NxlValue::Int(n) => n.to_string(),
            NxlValue::Float(n) => {
                if n.is_nan() || n.is_infinite() {
                    "null".to_owned()
                } else {
                    format!("{n}")
                }
            }
            NxlValue::Str(s) => format!("\"{}\"", escape_str(s)),
            NxlValue::List(items) => {
                let inner: Vec<String> = items.iter().map(stringify).collect();
                format!("[{}]", inner.join(","))
            }
            NxlValue::Map(pairs) => {
                let inner: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("\"{}\":{}", escape_str(k), stringify(v)))
                    .collect();
                format!("{{{}}}", inner.join(","))
            }
        }
    }

    fn escape_str(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for ch in s.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c => out.push(c),
            }
        }
        out
    }

    /// Parse a JSON string into an `NxlValue`.
    pub fn parse(input: &str) -> StdResult<NxlValue> {
        let input = input.trim();
        parse_value(input, &mut 0)
            .ok_or_else(|| StdlibError::new("NXL11006", format!("invalid JSON: {input}")))
    }

    fn parse_value(s: &str, pos: &mut usize) -> Option<NxlValue> {
        skip_ws(s, pos);
        let ch = s.as_bytes().get(*pos)?;
        match ch {
            b'n' => {
                *pos += 4;
                Some(NxlValue::Null)
            }
            b't' => {
                *pos += 4;
                Some(NxlValue::Bool(true))
            }
            b'f' => {
                *pos += 5;
                Some(NxlValue::Bool(false))
            }
            b'"' => parse_string(s, pos).map(NxlValue::Str),
            b'[' => parse_array(s, pos),
            b'{' => parse_object(s, pos),
            b'-' | b'0'..=b'9' => parse_number(s, pos),
            _ => None,
        }
    }

    fn skip_ws(s: &str, pos: &mut usize) {
        while *pos < s.len() && matches!(s.as_bytes()[*pos], b' ' | b'\t' | b'\n' | b'\r') {
            *pos += 1;
        }
    }

    fn parse_string(s: &str, pos: &mut usize) -> Option<String> {
        if s.as_bytes().get(*pos)? != &b'"' {
            return None;
        }
        *pos += 1;
        let mut out = String::new();
        while *pos < s.len() {
            let ch = s.as_bytes()[*pos];
            if ch == b'"' {
                *pos += 1;
                return Some(out);
            }
            if ch == b'\\' {
                *pos += 1;
                match s.as_bytes().get(*pos)? {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    _ => return None,
                }
            } else {
                out.push(s[*pos..].chars().next()?);
            }
            *pos += 1;
        }
        None
    }

    fn parse_number(s: &str, pos: &mut usize) -> Option<NxlValue> {
        let start = *pos;
        if s.as_bytes().get(*pos) == Some(&b'-') {
            *pos += 1;
        }
        while *pos < s.len() && s.as_bytes()[*pos].is_ascii_digit() {
            *pos += 1;
        }
        let is_float = *pos < s.len()
            && (s.as_bytes()[*pos] == b'.'
                || s.as_bytes()[*pos] == b'e'
                || s.as_bytes()[*pos] == b'E');
        if is_float {
            if *pos < s.len() && s.as_bytes()[*pos] == b'.' {
                *pos += 1;
                while *pos < s.len() && s.as_bytes()[*pos].is_ascii_digit() {
                    *pos += 1;
                }
            }
            if *pos < s.len() && (s.as_bytes()[*pos] == b'e' || s.as_bytes()[*pos] == b'E') {
                *pos += 1;
                if *pos < s.len() && (s.as_bytes()[*pos] == b'+' || s.as_bytes()[*pos] == b'-') {
                    *pos += 1;
                }
                while *pos < s.len() && s.as_bytes()[*pos].is_ascii_digit() {
                    *pos += 1;
                }
            }
            s[start..*pos].parse::<f64>().ok().map(NxlValue::Float)
        } else {
            s[start..*pos].parse::<i64>().ok().map(NxlValue::Int)
        }
    }

    fn parse_array(s: &str, pos: &mut usize) -> Option<NxlValue> {
        *pos += 1; // '['
        let mut items = Vec::new();
        skip_ws(s, pos);
        if s.as_bytes().get(*pos) == Some(&b']') {
            *pos += 1;
            return Some(NxlValue::List(items));
        }
        loop {
            items.push(parse_value(s, pos)?);
            skip_ws(s, pos);
            match s.as_bytes().get(*pos)? {
                b',' => {
                    *pos += 1;
                }
                b']' => {
                    *pos += 1;
                    return Some(NxlValue::List(items));
                }
                _ => return None,
            }
        }
    }

    fn parse_object(s: &str, pos: &mut usize) -> Option<NxlValue> {
        *pos += 1; // '{'
        let mut pairs = Vec::new();
        skip_ws(s, pos);
        if s.as_bytes().get(*pos) == Some(&b'}') {
            *pos += 1;
            return Some(NxlValue::Map(pairs));
        }
        loop {
            skip_ws(s, pos);
            let key = parse_string(s, pos)?;
            skip_ws(s, pos);
            if s.as_bytes().get(*pos)? != &b':' {
                return None;
            }
            *pos += 1;
            let val = parse_value(s, pos)?;
            pairs.push((key, val));
            skip_ws(s, pos);
            match s.as_bytes().get(*pos)? {
                b',' => {
                    *pos += 1;
                }
                b'}' => {
                    *pos += 1;
                    return Some(NxlValue::Map(pairs));
                }
                _ => return None,
            }
        }
    }
}

// ── random ────────────────────────────────────────────────────────────────────

/// A simple LCG pseudo-random number generator.
///
/// Not cryptographically secure. For crypto-quality randomness use P16 crypto module.
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9e3779b97f4a7c15,
        }
    }

    pub fn from_entropy() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .subsec_nanos() as u64;
        Self::new(seed)
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    pub fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    pub fn next_int(&mut self, lo: i64, hi: i64) -> i64 {
        assert!(lo <= hi);
        if lo == hi {
            return lo;
        }
        let range = (hi - lo + 1) as u64;
        lo + (self.next_u64() % range) as i64
    }

    pub fn next_float(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        let n = slice.len();
        for i in (1..n).rev() {
            let j = (self.next_u64() as usize) % (i + 1);
            slice.swap(i, j);
        }
    }

    pub fn choose<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T> {
        if slice.is_empty() {
            return None;
        }
        Some(&slice[(self.next_u64() as usize) % slice.len()])
    }
}

// ── time ──────────────────────────────────────────────────────────────────────

pub mod time {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    /// Unix timestamp in seconds.
    pub fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs()
    }

    /// Unix timestamp in milliseconds.
    pub fn now_millis() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis()
    }

    /// Unix timestamp in microseconds.
    pub fn now_micros() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_micros()
    }

    /// Sleep for the given number of milliseconds.
    pub fn sleep_ms(ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }

    /// A monotonic stopwatch.
    pub struct Stopwatch {
        start: Instant,
    }

    impl Stopwatch {
        pub fn start() -> Self {
            Self {
                start: Instant::now(),
            }
        }
        pub fn elapsed_ms(&self) -> u128 {
            self.start.elapsed().as_millis()
        }
        pub fn elapsed_us(&self) -> u128 {
            self.start.elapsed().as_micros()
        }
    }
}

// ── fs ────────────────────────────────────────────────────────────────────────

pub mod fs {
    use super::{StdResult, StdlibError};
    use std::path::Path;

    pub fn read_to_string(path: &str) -> StdResult<String> {
        std::fs::read_to_string(path)
            .map_err(|e| StdlibError::new("NXL11007", format!("read '{path}': {e}")))
    }

    pub fn write_string(path: &str, content: &str) -> StdResult<()> {
        std::fs::write(path, content)
            .map_err(|e| StdlibError::new("NXL11007", format!("write '{path}': {e}")))
    }

    pub fn append_string(path: &str, content: &str) -> StdResult<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .map_err(|e| StdlibError::new("NXL11007", format!("open '{path}': {e}")))?;
        file.write_all(content.as_bytes())
            .map_err(|e| StdlibError::new("NXL11007", format!("append '{path}': {e}")))
    }

    pub fn exists(path: &str) -> bool {
        Path::new(path).exists()
    }
    pub fn is_file(path: &str) -> bool {
        Path::new(path).is_file()
    }
    pub fn is_dir(path: &str) -> bool {
        Path::new(path).is_dir()
    }

    pub fn create_dir(path: &str) -> StdResult<()> {
        std::fs::create_dir_all(path)
            .map_err(|e| StdlibError::new("NXL11007", format!("mkdir '{path}': {e}")))
    }

    pub fn remove_file(path: &str) -> StdResult<()> {
        std::fs::remove_file(path)
            .map_err(|e| StdlibError::new("NXL11007", format!("remove '{path}': {e}")))
    }

    pub fn list_dir(path: &str) -> StdResult<Vec<String>> {
        std::fs::read_dir(path)
            .map_err(|e| StdlibError::new("NXL11007", format!("readdir '{path}': {e}")))?
            .map(|entry| {
                entry
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .map_err(|e| StdlibError::new("NXL11007", e.to_string()))
            })
            .collect()
    }

    pub fn file_size(path: &str) -> StdResult<u64> {
        std::fs::metadata(path)
            .map(|m| m.len())
            .map_err(|e| StdlibError::new("NXL11007", format!("metadata '{path}': {e}")))
    }
}

// ── testing ───────────────────────────────────────────────────────────────────

/// Lightweight test assertion helpers for NXL test code.
pub mod testing {
    use super::{NxlValue, StdResult, StdlibError};

    pub fn assert_eq(left: &NxlValue, right: &NxlValue) -> StdResult<()> {
        if left == right {
            Ok(())
        } else {
            Err(StdlibError::new(
                "NXL11008",
                format!("assertion failed: {left} != {right}"),
            ))
        }
    }

    pub fn assert_ne(left: &NxlValue, right: &NxlValue) -> StdResult<()> {
        if left != right {
            Ok(())
        } else {
            Err(StdlibError::new(
                "NXL11008",
                format!("assertion failed: expected values to differ, both are {left}"),
            ))
        }
    }

    pub fn assert_true(value: bool, msg: &str) -> StdResult<()> {
        if value {
            Ok(())
        } else {
            Err(StdlibError::new(
                "NXL11008",
                format!("assertion failed: {msg}"),
            ))
        }
    }

    pub fn assert_false(value: bool, msg: &str) -> StdResult<()> {
        assert_true(!value, msg)
    }

    pub fn assert_ok<T>(result: &StdResult<T>) -> StdResult<()> {
        if result.is_ok() {
            Ok(())
        } else {
            Err(StdlibError::new(
                "NXL11008",
                "expected Ok, got Err".to_owned(),
            ))
        }
    }

    pub fn assert_err<T>(result: &StdResult<T>) -> StdResult<()> {
        if result.is_err() {
            Ok(())
        } else {
            Err(StdlibError::new(
                "NXL11008",
                "expected Err, got Ok".to_owned(),
            ))
        }
    }

    /// Run a named test closure, returning a pass/fail report.
    pub fn run_test(name: &str, f: impl FnOnce() -> StdResult<()>) -> TestResult {
        match f() {
            Ok(()) => TestResult {
                name: name.to_owned(),
                passed: true,
                message: None,
            },
            Err(e) => TestResult {
                name: name.to_owned(),
                passed: false,
                message: Some(e.message),
            },
        }
    }

    #[derive(Debug)]
    pub struct TestResult {
        pub name: String,
        pub passed: bool,
        pub message: Option<String>,
    }

    impl TestResult {
        pub fn is_pass(&self) -> bool {
            self.passed
        }
    }
}

// ── crypto ────────────────────────────────────────────────────────────────────

/// Cryptographic hashing and byte-level encodings.
///
/// SHA-256 uses the vetted `sha2` crate rather than a hand-written
/// implementation. Hex and base64 are plain encodings, not cryptographic
/// primitives, so they are implemented directly. Every function operates on
/// the UTF-8 bytes of its input and is mirrored byte-for-byte by the
/// JavaScript backend so the VM and Node agree.
pub mod crypto {
    use super::{StdResult, StdlibError};
    use sha2::{Digest, Sha256};

    const B64_ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// Lowercase hex encoding of the UTF-8 bytes of `s`.
    pub fn hex_encode(s: &str) -> String {
        let mut out = String::with_capacity(s.len() * 2);
        for byte in s.as_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    /// Decode a hex string back into the UTF-8 text it encodes. Errors on an
    /// odd length, a non-hex character, or bytes that are not valid UTF-8.
    pub fn hex_decode(s: &str) -> StdResult<String> {
        if s.len() % 2 != 0 {
            return Err(StdlibError::new(
                "NXL11009",
                "hex string has an odd length".to_owned(),
            ));
        }
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(s.len() / 2);
        for pair in bytes.chunks(2) {
            let hi = hex_val(pair[0]).ok_or_else(|| {
                StdlibError::new(
                    "NXL11009",
                    format!("invalid hex character '{}'", pair[0] as char),
                )
            })?;
            let lo = hex_val(pair[1]).ok_or_else(|| {
                StdlibError::new(
                    "NXL11009",
                    format!("invalid hex character '{}'", pair[1] as char),
                )
            })?;
            out.push((hi << 4) | lo);
        }
        String::from_utf8(out).map_err(|_| {
            StdlibError::new(
                "NXL11009",
                "hex string does not decode to valid UTF-8".to_owned(),
            )
        })
    }

    fn hex_val(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    /// Standard base64 (with padding) of the UTF-8 bytes of `s`.
    pub fn base64_encode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let triple = (b0 << 16) | (b1 << 8) | b2;
            out.push(B64_ALPHABET[(triple >> 18 & 0x3F) as usize] as char);
            out.push(B64_ALPHABET[(triple >> 12 & 0x3F) as usize] as char);
            out.push(if chunk.len() > 1 {
                B64_ALPHABET[(triple >> 6 & 0x3F) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                B64_ALPHABET[(triple & 0x3F) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    /// Decode standard base64 back into the UTF-8 text it encodes. Errors on
    /// invalid base64 or bytes that are not valid UTF-8.
    pub fn base64_decode(s: &str) -> StdResult<String> {
        if s.len() % 4 != 0 {
            return Err(StdlibError::new(
                "NXL11009",
                "base64 string length must be a multiple of 4".to_owned(),
            ));
        }
        let mut out = Vec::with_capacity(s.len() / 4 * 3);
        for chunk in s.as_bytes().chunks(4) {
            // '=' may only appear as one or two trailing characters, never in
            // the first two positions of a block.
            let first_pad = chunk.iter().position(|&b| b == b'=');
            let pad = first_pad.map_or(0, |i| 4 - i);
            if pad > 2 {
                return Err(StdlibError::new(
                    "NXL11009",
                    "invalid base64 padding".to_owned(),
                ));
            }
            if let Some(i) = first_pad {
                if chunk[i..].iter().any(|&b| b != b'=') {
                    return Err(StdlibError::new(
                        "NXL11009",
                        "invalid base64 padding".to_owned(),
                    ));
                }
            }
            let mut acc: u32 = 0;
            for &b in chunk.iter() {
                let val = if b == b'=' {
                    0
                } else {
                    B64_ALPHABET.iter().position(|&c| c == b).ok_or_else(|| {
                        StdlibError::new(
                            "NXL11009",
                            format!("invalid base64 character '{}'", b as char),
                        )
                    })? as u32
                };
                acc = (acc << 6) | val;
            }
            out.push((acc >> 16) as u8);
            if pad < 2 {
                out.push((acc >> 8) as u8);
            }
            if pad < 1 {
                out.push(acc as u8);
            }
        }
        String::from_utf8(out).map_err(|_| {
            StdlibError::new(
                "NXL11009",
                "base64 string does not decode to valid UTF-8".to_owned(),
            )
        })
    }

    /// Lowercase hex SHA-256 digest of the UTF-8 bytes of `s`.
    pub fn sha256_hex(s: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(s.as_bytes());
        let digest = hasher.finalize();
        let mut out = String::with_capacity(64);
        for byte in digest {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }
}

// ── StdlibModule registry ─────────────────────────────────────────────────────

/// Registry of all available stdlib modules.
///
/// Used by the VM and runtime to resolve `import std::*` calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdlibModule {
    Collections,
    Strings,
    Math,
    Errors,
    Env,
    Process,
    Logging,
    Json,
    Random,
    Time,
    Fs,
    Testing,
    /// Planned — not yet implemented.
    Http,
    /// Cryptographic hashing and byte encodings (`sha256_hex`, `hex_*`,
    /// `base64_*`).
    Crypto,
    /// Planned — not yet implemented.
    Networking,
}

impl StdlibModule {
    /// Resolve a module name string to a `StdlibModule`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "std::collections" | "collections" => Some(Self::Collections),
            "std::strings" | "strings" => Some(Self::Strings),
            "std::math" | "math" => Some(Self::Math),
            "std::errors" | "errors" => Some(Self::Errors),
            "std::env" | "env" => Some(Self::Env),
            "std::process" | "process" => Some(Self::Process),
            "std::logging" | "logging" => Some(Self::Logging),
            "std::json" | "json" => Some(Self::Json),
            "std::random" | "random" => Some(Self::Random),
            "std::time" | "time" => Some(Self::Time),
            "std::fs" | "fs" => Some(Self::Fs),
            "std::testing" | "testing" => Some(Self::Testing),
            "std::http" | "http" => Some(Self::Http),
            "std::crypto" | "crypto" => Some(Self::Crypto),
            "std::networking" | "networking" => Some(Self::Networking),
            _ => None,
        }
    }

    /// Returns true if this module is fully implemented in this phase.
    pub fn is_implemented(&self) -> bool {
        !matches!(self, Self::Http | Self::Networking)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Collections => "std::collections",
            Self::Strings => "std::strings",
            Self::Math => "std::math",
            Self::Errors => "std::errors",
            Self::Env => "std::env",
            Self::Process => "std::process",
            Self::Logging => "std::logging",
            Self::Json => "std::json",
            Self::Random => "std::random",
            Self::Time => "std::time",
            Self::Fs => "std::fs",
            Self::Testing => "std::testing",
            Self::Http => "std::http",
            Self::Crypto => "std::crypto",
            Self::Networking => "std::networking",
        }
    }
}

/// All stdlib modules available in this release.
pub fn all_modules() -> Vec<StdlibModule> {
    vec![
        StdlibModule::Collections,
        StdlibModule::Strings,
        StdlibModule::Math,
        StdlibModule::Errors,
        StdlibModule::Env,
        StdlibModule::Process,
        StdlibModule::Logging,
        StdlibModule::Json,
        StdlibModule::Random,
        StdlibModule::Time,
        StdlibModule::Fs,
        StdlibModule::Testing,
        StdlibModule::Crypto,
    ]
}

// ── Builtins exposed to NXL programs ──────────────────────────────────────────

/// The static type of a stdlib builtin parameter or result.
///
/// The NXL type system has no dedicated `Char` or list-of-string type yet, so
/// only functions whose signature is built from these four types are exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinType {
    Int,
    Float,
    Str,
    Bool,
}

impl BuiltinType {
    /// Whether an argument of type `arg` is acceptable where `self` is
    /// declared. Integers promote to Float; nothing else is implicit.
    pub fn accepts(self, arg: BuiltinType) -> bool {
        self == arg || (self == BuiltinType::Float && arg == BuiltinType::Int)
    }
}

impl NxlValue {
    /// The `BuiltinType` of a scalar value, or `None` for null/list/map.
    pub fn builtin_type(&self) -> Option<BuiltinType> {
        match self {
            NxlValue::Bool(_) => Some(BuiltinType::Bool),
            NxlValue::Int(_) => Some(BuiltinType::Int),
            NxlValue::Float(_) => Some(BuiltinType::Float),
            NxlValue::Str(_) => Some(BuiltinType::Str),
            _ => None,
        }
    }
}

/// A stdlib function reachable from NXL, keyed by its fully qualified name
/// (`std::math::sqrt`). Kept in step with the VM dispatch and the JavaScript
/// backend so all three runtimes agree.
pub struct BuiltinSpec {
    pub name: &'static str,
    pub params: &'static [BuiltinType],
    pub ret: BuiltinType,
}

impl BuiltinSpec {
    pub fn arity(&self) -> usize {
        self.params.len()
    }
}

use BuiltinType::{Bool as B, Float as F, Int as I, Str as S};

static STDLIB_BUILTINS: &[BuiltinSpec] = &[
    // std::strings — single-signature functions over String/Int/Bool.
    BuiltinSpec {
        name: "std::strings::to_upper",
        params: &[S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::strings::to_lower",
        params: &[S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::strings::trim",
        params: &[S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::strings::trim_start",
        params: &[S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::strings::trim_end",
        params: &[S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::strings::len",
        params: &[S],
        ret: I,
    },
    BuiltinSpec {
        name: "std::strings::is_empty",
        params: &[S],
        ret: B,
    },
    BuiltinSpec {
        name: "std::strings::contains",
        params: &[S, S],
        ret: B,
    },
    BuiltinSpec {
        name: "std::strings::starts_with",
        params: &[S, S],
        ret: B,
    },
    BuiltinSpec {
        name: "std::strings::ends_with",
        params: &[S, S],
        ret: B,
    },
    BuiltinSpec {
        name: "std::strings::replace",
        params: &[S, S, S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::strings::repeat",
        params: &[S, I],
        ret: S,
    },
    BuiltinSpec {
        name: "std::strings::reverse",
        params: &[S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::strings::substring",
        params: &[S, I, I],
        ret: S,
    },
    BuiltinSpec {
        name: "std::strings::parse_int",
        params: &[S],
        ret: I,
    },
    BuiltinSpec {
        name: "std::strings::parse_float",
        params: &[S],
        ret: F,
    },
    // std::math — the Float-parameter subset (Int promotes to Float).
    BuiltinSpec {
        name: "std::math::sqrt",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::cbrt",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::exp",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::ln",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::log2",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::log10",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::floor",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::ceil",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::round",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::trunc",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::sin",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::cos",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::tan",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::asin",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::acos",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::atan",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::atan2",
        params: &[F, F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::pow_float",
        params: &[F, F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::to_radians",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::to_degrees",
        params: &[F],
        ret: F,
    },
    BuiltinSpec {
        name: "std::math::is_nan",
        params: &[F],
        ret: B,
    },
    BuiltinSpec {
        name: "std::math::is_infinite",
        params: &[F],
        ret: B,
    },
    BuiltinSpec {
        name: "std::math::is_finite",
        params: &[F],
        ret: B,
    },
    // std::crypto — UTF-8 byte hashing and encodings, all Str -> Str.
    BuiltinSpec {
        name: "std::crypto::sha256_hex",
        params: &[S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::crypto::hex_encode",
        params: &[S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::crypto::hex_decode",
        params: &[S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::crypto::base64_encode",
        params: &[S],
        ret: S,
    },
    BuiltinSpec {
        name: "std::crypto::base64_decode",
        params: &[S],
        ret: S,
    },
];

/// Look up a stdlib builtin by its fully qualified name.
pub fn builtin(name: &str) -> Option<&'static BuiltinSpec> {
    STDLIB_BUILTINS.iter().find(|spec| spec.name == name)
}

/// The canonical `std::<module>` prefix for a module name, accepting both the
/// qualified (`std::math`) and short (`math`) spellings. `None` for modules
/// that are not implemented yet (http/networking) or unknown.
pub fn canonical_module(name: &str) -> Option<&'static str> {
    StdlibModule::from_name(name)
        .filter(|module| module.is_implemented())
        .map(|module| module.name())
}

fn as_float(v: &NxlValue) -> StdResult<f64> {
    match v {
        NxlValue::Float(n) => Ok(*n),
        NxlValue::Int(n) => Ok(*n as f64),
        _ => Err(StdlibError::new(
            "NXL11011",
            "expected a Float argument".to_owned(),
        )),
    }
}

fn as_int(v: &NxlValue) -> StdResult<i64> {
    match v {
        NxlValue::Int(n) => Ok(*n),
        _ => Err(StdlibError::new(
            "NXL11011",
            "expected an Int argument".to_owned(),
        )),
    }
}

fn as_str(v: &NxlValue) -> StdResult<&str> {
    match v {
        NxlValue::Str(s) => Ok(s),
        _ => Err(StdlibError::new(
            "NXL11011",
            "expected a String argument".to_owned(),
        )),
    }
}

/// Invoke a stdlib builtin. Validates arity and argument types (Int promotes
/// to Float) and returns the result as an `NxlValue`. Failures are ordinary
/// `StdlibError`s, which the runtimes surface as catchable runtime errors.
pub fn call_builtin(name: &str, args: &[NxlValue]) -> StdResult<NxlValue> {
    let spec = builtin(name).ok_or_else(|| {
        StdlibError::new(
            "NXL11010",
            format!("unknown standard-library function '{name}'"),
        )
    })?;
    if args.len() != spec.params.len() {
        return Err(StdlibError::new(
            "NXL11010",
            format!(
                "'{name}' expects {} argument(s), got {}",
                spec.params.len(),
                args.len()
            ),
        ));
    }
    for (index, (param, arg)) in spec.params.iter().zip(args).enumerate() {
        match arg.builtin_type() {
            Some(actual) if param.accepts(actual) => {}
            _ => {
                return Err(StdlibError::new(
                    "NXL11011",
                    format!("'{name}' argument {} has the wrong type", index + 1),
                ))
            }
        }
    }

    match name {
        "std::strings::to_upper" => Ok(NxlValue::Str(strings::to_upper(as_str(&args[0])?))),
        "std::strings::to_lower" => Ok(NxlValue::Str(strings::to_lower(as_str(&args[0])?))),
        "std::strings::trim" => Ok(NxlValue::Str(strings::trim(as_str(&args[0])?).to_owned())),
        "std::strings::trim_start" => Ok(NxlValue::Str(
            strings::trim_start(as_str(&args[0])?).to_owned(),
        )),
        "std::strings::trim_end" => Ok(NxlValue::Str(
            strings::trim_end(as_str(&args[0])?).to_owned(),
        )),
        "std::strings::len" => Ok(NxlValue::Int(strings::len(as_str(&args[0])?) as i64)),
        "std::strings::is_empty" => Ok(NxlValue::Bool(strings::is_empty(as_str(&args[0])?))),
        "std::strings::contains" => Ok(NxlValue::Bool(strings::contains(
            as_str(&args[0])?,
            as_str(&args[1])?,
        ))),
        "std::strings::starts_with" => Ok(NxlValue::Bool(strings::starts_with(
            as_str(&args[0])?,
            as_str(&args[1])?,
        ))),
        "std::strings::ends_with" => Ok(NxlValue::Bool(strings::ends_with(
            as_str(&args[0])?,
            as_str(&args[1])?,
        ))),
        "std::strings::replace" => Ok(NxlValue::Str(strings::replace(
            as_str(&args[0])?,
            as_str(&args[1])?,
            as_str(&args[2])?,
        ))),
        "std::strings::repeat" => {
            let text = as_str(&args[0])?;
            let count = as_int(&args[1])?;
            let count = usize::try_from(count).map_err(|_| {
                StdlibError::new("NXL11011", "repeat count must not be negative".to_owned())
            })?;
            // Guard the capacity computation so an absurd count errors instead
            // of aborting the process with an allocation panic.
            if (text.len() as u64).checked_mul(count as u64).is_none() {
                return Err(StdlibError::new(
                    "NXL11011",
                    "repeat result is too large".to_owned(),
                ));
            }
            Ok(NxlValue::Str(strings::repeat(text, count)))
        }
        "std::strings::reverse" => Ok(NxlValue::Str(strings::reverse(as_str(&args[0])?))),
        "std::strings::substring" => {
            let start = usize::try_from(as_int(&args[1])?).map_err(|_| {
                StdlibError::new(
                    "NXL11002",
                    "substring start must not be negative".to_owned(),
                )
            })?;
            let end = usize::try_from(as_int(&args[2])?).map_err(|_| {
                StdlibError::new("NXL11002", "substring end must not be negative".to_owned())
            })?;
            strings::substring(as_str(&args[0])?, start, end).map(NxlValue::Str)
        }
        "std::strings::parse_int" => strings::parse_int(as_str(&args[0])?).map(NxlValue::Int),
        "std::strings::parse_float" => strings::parse_float(as_str(&args[0])?).map(NxlValue::Float),

        "std::math::sqrt" => Ok(NxlValue::Float(math::sqrt(as_float(&args[0])?))),
        "std::math::cbrt" => Ok(NxlValue::Float(math::cbrt(as_float(&args[0])?))),
        "std::math::exp" => Ok(NxlValue::Float(math::exp(as_float(&args[0])?))),
        "std::math::ln" => Ok(NxlValue::Float(math::ln(as_float(&args[0])?))),
        "std::math::log2" => Ok(NxlValue::Float(math::log2(as_float(&args[0])?))),
        "std::math::log10" => Ok(NxlValue::Float(math::log10(as_float(&args[0])?))),
        "std::math::floor" => Ok(NxlValue::Float(math::floor(as_float(&args[0])?))),
        "std::math::ceil" => Ok(NxlValue::Float(math::ceil(as_float(&args[0])?))),
        "std::math::round" => Ok(NxlValue::Float(math::round(as_float(&args[0])?))),
        "std::math::trunc" => Ok(NxlValue::Float(math::trunc(as_float(&args[0])?))),
        "std::math::sin" => Ok(NxlValue::Float(math::sin(as_float(&args[0])?))),
        "std::math::cos" => Ok(NxlValue::Float(math::cos(as_float(&args[0])?))),
        "std::math::tan" => Ok(NxlValue::Float(math::tan(as_float(&args[0])?))),
        "std::math::asin" => Ok(NxlValue::Float(math::asin(as_float(&args[0])?))),
        "std::math::acos" => Ok(NxlValue::Float(math::acos(as_float(&args[0])?))),
        "std::math::atan" => Ok(NxlValue::Float(math::atan(as_float(&args[0])?))),
        "std::math::atan2" => Ok(NxlValue::Float(math::atan2(
            as_float(&args[0])?,
            as_float(&args[1])?,
        ))),
        "std::math::pow_float" => Ok(NxlValue::Float(math::pow_float(
            as_float(&args[0])?,
            as_float(&args[1])?,
        ))),
        "std::math::to_radians" => Ok(NxlValue::Float(math::to_radians(as_float(&args[0])?))),
        "std::math::to_degrees" => Ok(NxlValue::Float(math::to_degrees(as_float(&args[0])?))),
        "std::math::is_nan" => Ok(NxlValue::Bool(math::is_nan(as_float(&args[0])?))),
        "std::math::is_infinite" => Ok(NxlValue::Bool(math::is_infinite(as_float(&args[0])?))),
        "std::math::is_finite" => Ok(NxlValue::Bool(math::is_finite(as_float(&args[0])?))),

        "std::crypto::sha256_hex" => Ok(NxlValue::Str(crypto::sha256_hex(as_str(&args[0])?))),
        "std::crypto::hex_encode" => Ok(NxlValue::Str(crypto::hex_encode(as_str(&args[0])?))),
        "std::crypto::hex_decode" => crypto::hex_decode(as_str(&args[0])?).map(NxlValue::Str),
        "std::crypto::base64_encode" => Ok(NxlValue::Str(crypto::base64_encode(as_str(&args[0])?))),
        "std::crypto::base64_decode" => crypto::base64_decode(as_str(&args[0])?).map(NxlValue::Str),

        _ => Err(StdlibError::new(
            "NXL11010",
            format!("unknown standard-library function '{name}'"),
        )),
    }
}
