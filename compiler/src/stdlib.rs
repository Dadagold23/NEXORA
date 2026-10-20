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
/// | http          | IMPLEMENTED   |
/// | crypto        | IMPLEMENTED   |
/// | networking    | IMPLEMENTED   |
/// | concurrency   | IMPLEMENTED   |
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

/// Blocking HTTP/1.1 client.
///
/// A deliberately small hand-rolled client over `std::net::TcpStream`: no
/// extra dependencies and no TLS. Only `http://` URLs are accepted;
/// `https://` fails with a catchable error that names the limitation, so the
/// missing TLS support is never silent. Requests send `Connection: close`
/// and reading stops as soon as the message is complete (Content-Length,
/// chunked, or read-to-EOF bodies). Responses are held in memory and capped
/// at [`MAX_RESPONSE_BYTES`]. Redirects are returned as-is, never followed,
/// and bodies are decoded to text with UTF-8 replacement characters.
///
/// The JavaScript backend has no synchronous fetch, so these functions are
/// rejected at compile time there (NXL2007) and run only on the VM.
pub mod http {
    use std::io::{ErrorKind, Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;

    use super::{NxlValue, StdResult, StdlibError};

    /// Upper bound on a buffered response (headers included); larger
    /// responses fail with a catchable error instead of exhausting memory.
    pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
    /// Connect and per-read deadline for [`request`]; `get`/`post` use it.
    pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

    fn net_err(message: String) -> StdlibError {
        StdlibError::new("NXL11012", message)
    }

    /// A parsed `http://` URL: authority plus request target.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Url {
        pub host: String,
        pub port: u16,
        pub path: String,
    }

    /// Parse `http://host[:port][/path][?query]`. IPv6 hosts need brackets
    /// (`http://[::1]:8080/`); fragments are stripped (they never reach the
    /// server). `https://` is recognised specifically so the error can name
    /// TLS rather than report a generic scheme failure.
    pub fn parse_url(url: &str) -> StdResult<Url> {
        let rest = url.strip_prefix("http://").ok_or_else(|| {
            if url.starts_with("https://") {
                net_err(
                    "https (TLS) is not supported by std::http yet; use http:// or a local server"
                        .to_owned(),
                )
            } else {
                net_err(format!(
                    "invalid URL '{url}': expected http://host[:port][/path]"
                ))
            }
        })?;
        let rest = rest.split('#').next().unwrap_or("");
        let (authority, target) = match rest.find(['/', '?']) {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        if authority.is_empty() {
            return Err(net_err(format!("invalid URL '{url}': missing host")));
        }
        let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
            let close = inner.find(']').ok_or_else(|| {
                net_err(format!("invalid URL '{url}': unterminated IPv6 literal"))
            })?;
            let host = &inner[..close];
            if host.is_empty() {
                return Err(net_err(format!("invalid URL '{url}': missing host")));
            }
            let tail = &inner[close + 1..];
            let port = match tail {
                "" => 80,
                _ if tail.starts_with(':') && tail.len() > 1 => parse_port(&tail[1..], url)?,
                _ => {
                    return Err(net_err(format!(
                        "invalid URL '{url}': expected ':port' after the IPv6 literal"
                    )))
                }
            };
            (host, port)
        } else if authority.matches(':').count() > 1 {
            return Err(net_err(format!(
                "invalid URL '{url}': IPv6 hosts must be written as [{authority}]"
            )));
        } else if let Some((host, port_text)) = authority.split_once(':') {
            if host.is_empty() {
                return Err(net_err(format!("invalid URL '{url}': missing host")));
            }
            if port_text.is_empty() {
                return Err(net_err(format!("invalid URL '{url}': empty port")));
            }
            (host, parse_port(port_text, url)?)
        } else {
            (authority, 80)
        };
        let path = if target.is_empty() {
            "/".to_owned()
        } else if target.starts_with('?') {
            // A query with no path still needs the "/" separator.
            format!("/{target}")
        } else {
            target.to_owned()
        };
        Ok(Url {
            host: host.to_owned(),
            port,
            path,
        })
    }

    fn parse_port(text: &str, url: &str) -> StdResult<u16> {
        text.parse::<u16>().map_err(|_| {
            net_err(format!(
                "invalid URL '{url}': port '{text}' is not a number"
            ))
        })
    }

    /// A parsed response: status code, headers as received, decoded body.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Response {
        pub status: i64,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    impl Response {
        /// Case-insensitive lookup of one header value.
        pub fn header(&self, name: &str) -> Option<&str> {
            find_header(&self.headers, name)
        }
    }

    fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
        let want = name.to_ascii_lowercase();
        headers
            .iter()
            .find(|(key, _)| key.to_ascii_lowercase() == want)
            .map(|(_, value)| value.as_str())
    }

    /// Issue one request and parse the response. `timeout` bounds the
    /// connection attempt and each read; `get`/`post` use
    /// [`REQUEST_TIMEOUT`]. This is the low-level entry point tests use to
    /// exercise the timeout path without waiting ten seconds.
    pub fn request(
        url: &str,
        method: &str,
        body: Option<&str>,
        timeout: Duration,
    ) -> StdResult<Response> {
        let url = parse_url(url)?;
        let addresses = (url.host.as_str(), url.port)
            .to_socket_addrs()
            .map_err(|e| net_err(format!("cannot resolve '{}': {e}", url.host)))?
            .collect::<Vec<_>>();
        if addresses.is_empty() {
            return Err(net_err(format!("cannot resolve '{}'", url.host)));
        }
        let mut last_err = None;
        let mut stream = None;
        for address in &addresses {
            match TcpStream::connect_timeout(address, timeout) {
                Ok(connected) => {
                    stream = Some(connected);
                    break;
                }
                Err(e) => last_err = Some(e),
            }
        }
        let stream = stream.ok_or_else(|| {
            net_err(format!(
                "cannot connect to {}:{}: {}",
                url.host,
                url.port,
                last_err
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "no route to host".to_owned())
            ))
        })?;
        stream
            .set_read_timeout(Some(timeout))
            .and_then(|_| stream.set_write_timeout(Some(timeout)))
            .map_err(|e| net_err(format!("cannot set socket timeouts: {e}")))?;

        let host_header = if url.port == 80 {
            url.host.clone()
        } else {
            format!("{}:{}", url.host, url.port)
        };
        let mut request = format!(
            "{method} {} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: nxl/0.1\r\nAccept: */*\r\nConnection: close\r\n",
            url.path
        );
        if let Some(body) = body {
            request.push_str("Content-Type: application/json; charset=utf-8\r\n");
            request.push_str(&format!("Content-Length: {}\r\n", body.len()));
            request.push_str("\r\n");
            request.push_str(body);
        } else {
            request.push_str("\r\n");
        }

        stream
            .try_clone()
            .and_then(|mut writer| writer.write_all(request.as_bytes()).map(|_| writer))
            .map_err(|e| net_err(format!("cannot send the request: {e}")))?;

        read_response(stream)
    }

    /// `std::http::get` — GET `url`, returning `{status, body, headers}`.
    pub fn get(url: &str) -> StdResult<NxlValue> {
        request(url, "GET", None, REQUEST_TIMEOUT).map(response_value)
    }

    /// `std::http::post` — POST `body` to `url`, sent as
    /// `application/json; charset=utf-8`, returning `{status, body, headers}`.
    pub fn post(url: &str, body: &str) -> StdResult<NxlValue> {
        request(url, "POST", Some(body), REQUEST_TIMEOUT).map(response_value)
    }

    /// The Map handed to NXL programs for a response.
    fn response_value(response: Response) -> NxlValue {
        NxlValue::Map(vec![
            ("status".to_owned(), NxlValue::Int(response.status)),
            ("body".to_owned(), NxlValue::Str(response.body)),
            (
                "headers".to_owned(),
                NxlValue::Map(
                    response
                        .headers
                        .into_iter()
                        .map(|(key, value)| (key, NxlValue::Str(value)))
                        .collect(),
                ),
            ),
        ])
    }

    /// `std::http::header` — case-insensitive header lookup on a response
    /// from `std::http::get` or `std::http::post`. Header names are not
    /// valid field identifiers (`content-type`), which is why lookup goes
    /// through this function rather than field or index access.
    pub fn header(response: &NxlValue, name: &str) -> StdResult<String> {
        let headers = match response {
            NxlValue::Map(pairs) => pairs.iter().find(|(key, _)| key == "headers"),
            _ => None,
        }
        .and_then(|(_, value)| match value {
            NxlValue::Map(headers) => Some(headers),
            _ => None,
        })
        .ok_or_else(|| {
            net_err("expected an HTTP response from std::http::get or std::http::post".to_owned())
        })?;
        let want = name.to_ascii_lowercase();
        headers
            .iter()
            .find(|(key, _)| key.to_ascii_lowercase() == want)
            .and_then(|(_, value)| match value {
                NxlValue::Str(value) => Some(value.clone()),
                _ => None,
            })
            .ok_or_else(|| net_err(format!("response has no header '{name}'")))
    }

    /// Buffered reader over the socket that enforces the response cap.
    struct MessageReader {
        stream: TcpStream,
        buf: Vec<u8>,
        eof: bool,
    }

    impl MessageReader {
        fn new(stream: TcpStream) -> Self {
            Self {
                stream,
                buf: Vec::new(),
                eof: false,
            }
        }

        /// Read one chunk, or record EOF. Errors (including the read
        /// timeout, which Windows reports as WouldBlock or TimedOut) become
        /// catchable `NXL11012` failures.
        fn fill(&mut self) -> StdResult<()> {
            if self.eof {
                return Ok(());
            }
            let mut chunk = [0u8; 8192];
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    self.eof = true;
                    Ok(())
                }
                Ok(n) => {
                    if self.buf.len() + n > MAX_RESPONSE_BYTES {
                        return Err(net_err(format!(
                            "response exceeds {MAX_RESPONSE_BYTES} bytes"
                        )));
                    }
                    self.buf.extend_from_slice(&chunk[..n]);
                    Ok(())
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => Ok(()),
                Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
                    Err(net_err("timed out waiting for the response".to_owned()))
                }
                Err(e) => Err(net_err(format!("connection error: {e}"))),
            }
        }

        /// Buffer until at least `len` bytes are available, EOF permitting.
        fn ensure(&mut self, len: usize) -> StdResult<()> {
            while self.buf.len() < len && !self.eof {
                self.fill()?;
            }
            Ok(())
        }

        /// Offset just past the blank line that ends the header block.
        fn find_headers_end(&mut self) -> StdResult<usize> {
            loop {
                if let Some(pos) = find_from(&self.buf, b"\r\n\r\n", 0) {
                    return Ok(pos + 4);
                }
                if self.eof {
                    return Err(net_err(
                        "connection closed before the response headers completed".to_owned(),
                    ));
                }
                self.fill()?;
            }
        }

        fn read_to_eof(&mut self) -> StdResult<()> {
            while !self.eof {
                self.fill()?;
            }
            Ok(())
        }
    }

    /// First index of `needle` at or after `from`, if any.
    fn find_from(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
        if from >= haystack.len() {
            return None;
        }
        haystack[from..]
            .windows(needle.len())
            .position(|window| window == needle)
            .map(|i| i + from)
    }

    fn read_response(stream: TcpStream) -> StdResult<Response> {
        let mut reader = MessageReader::new(stream);
        let head_end = reader.find_headers_end()?;
        let head = String::from_utf8_lossy(&reader.buf[..head_end]).into_owned();
        let mut lines = head.split("\r\n");
        let status_line = lines.next().unwrap_or("").to_owned();
        let mut parts = status_line.split(' ');
        let version = parts.next().unwrap_or("");
        if !version.starts_with("HTTP/1.") {
            return Err(net_err(format!(
                "unrecognised response line '{status_line}'"
            )));
        }
        let status = parts
            .next()
            .unwrap_or("")
            .parse::<i64>()
            .map_err(|_| net_err(format!("unparsable status line '{status_line}'")))?;
        let headers = lines
            .filter(|line| !line.is_empty())
            .map(|line| match line.split_once(':') {
                Some((key, value)) => (key.to_owned(), value.trim().to_owned()),
                None => (line.to_owned(), String::new()),
            })
            .collect::<Vec<_>>();

        // Chunked wins over Content-Length when both appear (RFC 9112 §6.1).
        let chunked = headers.iter().any(|(key, value)| {
            key.eq_ignore_ascii_case("transfer-encoding")
                && value.to_ascii_lowercase().contains("chunked")
        });
        let body = if chunked {
            decode_body(&read_chunked(&mut reader, head_end)?)
        } else if let Some((_, value)) = headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        {
            let len = value.parse::<usize>().map_err(|_| {
                net_err(format!("invalid Content-Length '{value}' in the response"))
            })?;
            reader.ensure(head_end + len)?;
            if reader.buf.len() < head_end + len {
                return Err(net_err(
                    "connection closed before the body completed".to_owned(),
                ));
            }
            decode_body(&reader.buf[head_end..head_end + len])
        } else {
            reader.read_to_eof()?;
            decode_body(&reader.buf[head_end..])
        };
        Ok(Response {
            status,
            headers,
            body,
        })
    }

    /// Decode a chunked body starting at `cursor` (past the header block).
    /// The terminating zero chunk ends it; trailers are ignored.
    fn read_chunked(reader: &mut MessageReader, mut cursor: usize) -> StdResult<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let line_end = loop {
                if let Some(pos) = find_from(&reader.buf, b"\r\n", cursor) {
                    break pos;
                }
                if reader.eof {
                    return Err(net_err(
                        "connection closed inside a chunked body".to_owned(),
                    ));
                }
                reader.fill()?;
            };
            let line = std::str::from_utf8(&reader.buf[cursor..line_end])
                .map_err(|_| net_err("chunked body has an unparsable size line".to_owned()))?;
            let size_text = line.split(';').next().unwrap_or("").trim();
            let size = usize::from_str_radix(size_text, 16)
                .map_err(|_| net_err(format!("invalid chunk size '{size_text}'")))?;
            cursor = line_end + 2;
            if size == 0 {
                return Ok(out);
            }
            reader.ensure(cursor + size + 2)?;
            if reader.buf.len() < cursor + size + 2 {
                return Err(net_err(
                    "connection closed inside a chunked body".to_owned(),
                ));
            }
            out.extend_from_slice(&reader.buf[cursor..cursor + size]);
            if &reader.buf[cursor + size..cursor + size + 2] != b"\r\n" {
                return Err(net_err("chunked body is missing a terminator".to_owned()));
            }
            cursor += size + 2;
            if out.len() > MAX_RESPONSE_BYTES {
                return Err(net_err(format!(
                    "response exceeds {MAX_RESPONSE_BYTES} bytes"
                )));
            }
        }
    }

    /// Bodies are text in NXL (there is no binary type), so bytes are
    /// decoded leniently: invalid sequences become replacement characters
    /// rather than failing the whole response.
    fn decode_body(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// DNS resolution and TCP reachability checks.
///
/// Both functions are blocking and VM-only: the JavaScript backend has no
/// synchronous DNS or socket API, so it rejects them at compile time
/// (NXL2007) instead of emitting calls that could never succeed.
pub mod networking {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::{Duration, Instant};

    use super::{NxlValue, StdResult, StdlibError};

    fn net_err(message: String) -> StdlibError {
        StdlibError::new("NXL11012", message)
    }

    /// Addresses for `host` exactly as the platform resolver reports them
    /// (an empty list if the name resolves to nothing at all).
    pub fn resolve(host: &str) -> StdResult<NxlValue> {
        let addresses = (host, 0u16)
            .to_socket_addrs()
            .map_err(|e| net_err(format!("cannot resolve '{host}': {e}")))?
            .map(|address| NxlValue::Str(address.ip().to_string()))
            .collect::<Vec<_>>();
        Ok(NxlValue::List(addresses))
    }

    /// True when a TCP connection to `host:port` succeeds within
    /// `timeout_ms` (shared across all resolved addresses). An unusable
    /// port or timeout is an error, not `false`.
    pub fn tcp_probe(host: &str, port: i64, timeout_ms: i64) -> StdResult<bool> {
        if !(1..=65535).contains(&port) {
            return Err(net_err(format!(
                "port {port} is outside the valid range 1-65535"
            )));
        }
        if timeout_ms < 1 {
            return Err(net_err("timeout_ms must be at least 1".to_owned()));
        }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        let addresses = (host, port as u16)
            .to_socket_addrs()
            .map_err(|e| net_err(format!("cannot resolve '{host}': {e}")))?;
        for address in addresses {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            if TcpStream::connect_timeout(&address, remaining).is_ok() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

// ── concurrency ───────────────────────────────────────────────────────────────

/// Cooperative concurrency helpers.
///
/// `sleep` is the one *sanctioned* blocking operation in this module: it
/// parks the calling worker thread for the given milliseconds. Everything
/// else in the concurrency surface (task inspection, await timeouts, channel
/// receive timeouts, cancellation via channel close) lives on the task and
/// channel values themselves as methods, because those builtins take
/// task/channel values that the typed stdlib bridge does not accept.
pub mod concurrency {
    use super::{NxlValue, StdResult, StdlibError};

    /// The longest accepted sleep: one hour. A stray value must not park a
    /// worker for effectively forever.
    pub const MAX_SLEEP_MS: i64 = 60 * 60 * 1000;

    /// Block the calling worker thread for `ms` milliseconds.
    ///
    /// Negative values and values over [`MAX_SLEEP_MS`] are errors rather
    /// than silent no-ops.
    pub fn sleep(ms: i64) -> StdResult<NxlValue> {
        if ms < 0 {
            return Err(StdlibError::new(
                "NXL11011",
                "std.concurrency::sleep: milliseconds must not be negative".to_owned(),
            ));
        }
        if ms > MAX_SLEEP_MS {
            return Err(StdlibError::new(
                "NXL11011",
                format!(
                    "std.concurrency::sleep: {ms}ms exceeds the one-hour limit ({MAX_SLEEP_MS}ms)"
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
        Ok(NxlValue::Null)
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
    /// Blocking HTTP/1.1 client (`get`, `post`, `header`); VM-only because
    /// the JavaScript backend has no synchronous fetch.
    Http,
    /// Cryptographic hashing and byte encodings (`sha256_hex`, `hex_*`,
    /// `base64_*`).
    Crypto,
    /// DNS resolution and TCP reachability (`resolve`, `tcp_probe`);
    /// VM-only for the same reason as `Http`.
    Networking,
    /// Cooperative concurrency helpers (`sleep`); VM-only because a blocking
    /// sleep has no synchronous JavaScript equivalent.
    Concurrency,
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
            "std::concurrency" | "concurrency" => Some(Self::Concurrency),
            _ => None,
        }
    }

    /// Returns true if this module is fully implemented in this phase.
    ///
    /// Every module in the registry is implemented as of this release; the
    /// check stays so a future planned module cannot resolve silently.
    pub fn is_implemented(&self) -> bool {
        true
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
            Self::Concurrency => "std::concurrency",
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
        StdlibModule::Http,
        StdlibModule::Crypto,
        StdlibModule::Networking,
        StdlibModule::Concurrency,
    ]
}

// ── Builtins exposed to NXL programs ──────────────────────────────────────────

/// The static type of a stdlib builtin parameter or result.
///
/// The NXL type system has no dedicated `Char` or list-of-string type yet,
/// so most signatures are built from the four scalar types below. `Value`
/// covers the rest — null, lists, and maps (including whole HTTP responses)
/// — for the few functions that accept or return structured data; it lowers
/// to the NXL `Unknown` type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinType {
    Int,
    Float,
    Str,
    Bool,
    /// Any NXL value (only where a signature explicitly declares one).
    Value,
}

impl BuiltinType {
    /// Whether an argument of type `arg` is acceptable where `self` is
    /// declared. Integers promote to Float; a `Value` parameter accepts
    /// everything; nothing else is implicit.
    pub fn accepts(self, arg: BuiltinType) -> bool {
        self == BuiltinType::Value
            || self == arg
            || (self == BuiltinType::Float && arg == BuiltinType::Int)
    }
}

impl NxlValue {
    /// The `BuiltinType` of a value: scalars carry their own type and
    /// null/list/map are `Value`, which only `Value` parameters accept.
    pub fn builtin_type(&self) -> Option<BuiltinType> {
        match self {
            NxlValue::Bool(_) => Some(BuiltinType::Bool),
            NxlValue::Int(_) => Some(BuiltinType::Int),
            NxlValue::Float(_) => Some(BuiltinType::Float),
            NxlValue::Str(_) => Some(BuiltinType::Str),
            NxlValue::Null | NxlValue::List(_) | NxlValue::Map(_) => Some(BuiltinType::Value),
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

use BuiltinType::{Bool as B, Float as F, Int as I, Str as S, Value as V};

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
    // std::http — blocking HTTP/1.1; responses are Maps (VM-only).
    BuiltinSpec {
        name: "std::http::get",
        params: &[S],
        ret: V,
    },
    BuiltinSpec {
        name: "std::http::post",
        params: &[S, S],
        ret: V,
    },
    BuiltinSpec {
        name: "std::http::header",
        params: &[V, S],
        ret: S,
    },
    // std::networking — resolver and TCP reachability (VM-only).
    BuiltinSpec {
        name: "std::networking::resolve",
        params: &[S],
        ret: V,
    },
    BuiltinSpec {
        name: "std::networking::tcp_probe",
        params: &[S, I, I],
        ret: B,
    },
    // std::concurrency — cooperative helpers (VM-only: a blocking sleep has
    // no synchronous JavaScript equivalent).
    BuiltinSpec {
        name: "std::concurrency::sleep",
        params: &[I],
        ret: V,
    },
];

/// Look up a stdlib builtin by its fully qualified name.
pub fn builtin(name: &str) -> Option<&'static BuiltinSpec> {
    STDLIB_BUILTINS.iter().find(|spec| spec.name == name)
}

/// Builtins only the VM can run: blocking network I/O has no synchronous
/// JavaScript equivalent. The JavaScript backend rejects these at compile
/// time (NXL2007) rather than emitting a call that could never succeed.
pub fn is_vm_only_builtin(name: &str) -> bool {
    name.starts_with("std::http::")
        || name.starts_with("std::networking::")
        || name.starts_with("std::concurrency::")
}

/// The canonical `std::<module>` prefix for a module name, accepting both the
/// qualified (`std::math`) and short (`math`) spellings. `None` for unknown
/// module names.
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

        "std::http::get" => http::get(as_str(&args[0])?),
        "std::http::post" => http::post(as_str(&args[0])?, as_str(&args[1])?),
        "std::http::header" => http::header(&args[0], as_str(&args[1])?).map(NxlValue::Str),
        "std::networking::resolve" => networking::resolve(as_str(&args[0])?),
        "std::networking::tcp_probe" => {
            networking::tcp_probe(as_str(&args[0])?, as_int(&args[1])?, as_int(&args[2])?)
                .map(NxlValue::Bool)
        }
        "std::concurrency::sleep" => concurrency::sleep(as_int(&args[0])?),

        _ => Err(StdlibError::new(
            "NXL11010",
            format!("unknown standard-library function '{name}'"),
        )),
    }
}
