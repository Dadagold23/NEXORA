/// P09 — NXL Bytecode Virtual Machine
///
/// # Implementation status
///
/// | Subsystem           | Status                    |
/// |---------------------|---------------------------|
/// | Bytecode format     | IMPLEMENTED               |
/// | Instruction set     | IMPLEMENTED               |
/// | Constant pool       | IMPLEMENTED               |
/// | Stack/frame model   | IMPLEMENTED               |
/// | Function calls      | IMPLEMENTED               |
/// | Closures            | STUB (not yet wired)      |
/// | Exceptions/errors   | IMPLEMENTED               |
/// | Memory management   | IMPLEMENTED (host GC)     |
/// | Debug information   | IMPLEMENTED               |
/// | Serialization       | IMPLEMENTED               |
/// | VM execution        | IMPLEMENTED               |
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Condvar, Mutex,
};

use crate::diagnostic::Diagnostic;
use crate::nir::{self, BasicBlockId, Constant, Module, Terminator, Type};
use crate::source::{SourceFile, Span};

// ── Bytecode format version ───────────────────────────────────────────────────

pub const BYTECODE_MAGIC: u32 = 0x4E584C42; // "NXLB"
pub const BYTECODE_VERSION: u16 = 1;

// ── Constant pool ─────────────────────────────────────────────────────────────

/// An entry in the constant pool.
#[derive(Clone, Debug, PartialEq)]
pub enum PoolEntry {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    FunctionRef(String),
}

/// The constant pool for a compiled chunk.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConstantPool {
    entries: Vec<PoolEntry>,
}

impl ConstantPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a constant and return its pool index.
    pub fn intern(&mut self, entry: PoolEntry) -> u32 {
        if let Some(idx) = self.entries.iter().position(|e| *e == entry) {
            return idx as u32;
        }
        let idx = self.entries.len() as u32;
        self.entries.push(entry);
        idx
    }

    pub fn get(&self, idx: u32) -> Option<&PoolEntry> {
        self.entries.get(idx as usize)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── Bytecode instruction set ──────────────────────────────────────────────────

/// A single bytecode instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum Opcode {
    /// Push a constant from the pool onto the stack.
    LoadConst(u32),
    /// Push the value of a local slot.
    LoadLocal(u32),
    /// Pop the top of stack and store it in a local slot.
    StoreLocal(u32),
    /// Binary arithmetic/comparison/logical operation.
    BinaryOp(BinaryOp),
    /// Unary operation.
    UnaryOp(UnaryOp),
    /// Unconditional jump to instruction offset.
    Jump(u32),
    /// Pop condition; jump if truthy.
    JumpIfTrue(u32),
    /// Pop condition; jump if falsy.
    JumpIfFalse(u32),
    /// Call a function by name with `argc` arguments popped from the stack.
    Call { name: String, argc: u32 },
    /// Return the top-of-stack value (or unit if stack is empty).
    Return,
    /// Pop and discard the top of stack.
    Pop,
    /// Duplicate the top of stack.
    Dup,
    /// Push the unit/void value.
    PushUnit,
    /// Halt execution normally.
    Halt,
    /// Raise a runtime error with a message from the top of stack.
    Throw,
    /// Register an exception handler: records the current frame/stack depth
    /// and resumes at the given offset when unwinding.
    PushHandler(u32),
    /// Remove the most recently registered exception handler.
    PopHandler,
    /// Pop a value and raise it as an exception (unwinds to a handler).
    ThrowValue,
    /// Pop a value and push its NXL type name ("Int", "String", ...).
    TypeName,
    /// Pop a value and push its string representation.
    ToString,
    /// Pop `count` values and push them as an array (in order).
    BuildArray(u32),
    /// Pop an index then an object; push the element.
    IndexGet,
    /// Pop a value, an index, then an array; push the updated array.
    IndexSet,
    /// Push an empty object with the given type name.
    NewObject(String),
    /// Pop a value then an object; push the object with the field set.
    SetField(String),
    /// Pop an object; push the named field.
    GetField(String),
    /// Pop `argc` values and push an enum variant value.
    BuildEnum {
        type_name: String,
        variant: String,
        argc: u32,
    },
    /// Pop a value; push its variant name ("" for non-enums).
    VariantName,
    /// Advance a for-in iterator: pop the index then the iterable, push the
    /// item then a boolean saying whether an item was produced.
    IterNext,
    /// Spawn a thread running the named function with `argc` popped
    /// arguments; push the task handle.
    SpawnCall { callee: String, argc: u32 },
    /// Spawn a thread running a method: pop `argc` arguments then the
    /// receiver, dispatch on its runtime type, push the task handle.
    SpawnMethod { method: String, argc: u32 },
    /// Pop a task and block until it completes, pushing its value
    /// (re-raising failures); non-tasks pass through untouched.
    AwaitTask,
    /// Pop a value and push an already-completed task holding it.
    CompleteTask,
    /// Push a fresh channel.
    ChannelNew,
    /// Dynamic method call: pop `argc` arguments then the receiver, resolve
    /// `<runtime-type>#<method>`, call it with the receiver first, and push
    /// the return value then the receiver as it stood on return.
    CallMethod { method: String, argc: u32 },
    /// Like IndexGet, but pushes Unit instead of throwing on failure.
    PatternIndex,
    /// Like GetField, but pushes Unit instead of throwing on failure.
    PatternField(String),
    /// Debug annotation — carries source span, no runtime effect.
    DebugSpan(Span),
}

/// An in-flight VM failure: either a catchable exception value or a fatal
/// internal error that bypasses handlers.
#[derive(Clone, Debug, PartialEq)]
enum VmFault {
    /// A raised value looking for a handler.
    Throw(Value),
    /// Corrupt bytecode or a violated VM invariant; never caught.
    Fatal(String),
}

impl From<String> for VmFault {
    fn from(message: String) -> Self {
        VmFault::Fatal(message)
    }
}

/// A registered exception handler.
#[derive(Clone, Copy, Debug)]
struct Handler {
    /// Number of frames live when the handler was registered.
    frames_len: usize,
    /// Operand-stack depth to restore when unwinding.
    stack_len: usize,
    /// Instruction offset to resume at (exception value pushed first).
    target: u32,
}

/// Binary operation codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

/// Unary operation codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

// ── Compiled chunk ────────────────────────────────────────────────────────────

/// A compiled function body — the unit of bytecode.
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    /// Function name (for diagnostics and call dispatch).
    pub name: String,
    /// Number of parameters.
    pub arity: u32,
    /// Number of local slots (parameters + locals).
    pub local_count: u32,
    /// Bytecode instructions.
    pub code: Vec<Opcode>,
    /// Constant pool shared by this chunk.
    pub pool: ConstantPool,
    /// Source name for debug info.
    pub source_name: String,
}

impl Chunk {
    pub fn new(name: String, arity: u32, source_name: String) -> Self {
        Self {
            name,
            arity,
            local_count: arity,
            code: Vec::new(),
            pool: ConstantPool::new(),
            source_name,
        }
    }

    pub fn emit(&mut self, op: Opcode) -> u32 {
        let idx = self.code.len() as u32;
        self.code.push(op);
        idx
    }

    pub fn patch_jump(&mut self, idx: u32, target: u32) {
        match &mut self.code[idx as usize] {
            Opcode::Jump(t)
            | Opcode::JumpIfTrue(t)
            | Opcode::JumpIfFalse(t)
            | Opcode::PushHandler(t) => *t = target,
            _ => {}
        }
    }
}

/// A compiled program — all chunks plus the entry point name.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledProgram {
    pub chunks: Vec<Chunk>,
    pub entry_point: String,
    pub format_version: u16,
    pub source_name: String,
}

impl CompiledProgram {
    fn chunk_by_name(&self, name: &str) -> Option<&Chunk> {
        self.chunks.iter().find(|c| c.name == name)
    }
}

// ── Serialization ─────────────────────────────────────────────────────────────

impl CompiledProgram {
    /// Serialize to a compact binary format.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Magic + version
        buf.extend_from_slice(&BYTECODE_MAGIC.to_le_bytes());
        buf.extend_from_slice(&self.format_version.to_le_bytes());
        write_str(&mut buf, &self.source_name);
        write_str(&mut buf, &self.entry_point);
        buf.extend_from_slice(&(self.chunks.len() as u32).to_le_bytes());
        for chunk in &self.chunks {
            serialize_chunk(&mut buf, chunk);
        }
        buf
    }

    /// Deserialize from binary format.
    pub fn deserialize(data: &[u8]) -> Result<Self, String> {
        let mut cursor = 0usize;
        let magic = read_u32(data, &mut cursor)?;
        if magic != BYTECODE_MAGIC {
            return Err(format!("invalid bytecode magic: 0x{magic:08X}"));
        }
        let version = read_u16(data, &mut cursor)?;
        if version != BYTECODE_VERSION {
            return Err(format!(
                "unsupported bytecode version {version}, expected {BYTECODE_VERSION}"
            ));
        }
        let source_name = read_str(data, &mut cursor)?;
        let entry_point = read_str(data, &mut cursor)?;
        let chunk_count = read_u32(data, &mut cursor)? as usize;
        let mut chunks = Vec::with_capacity(chunk_count);
        for _ in 0..chunk_count {
            chunks.push(deserialize_chunk(data, &mut cursor)?);
        }
        Ok(Self {
            chunks,
            entry_point,
            format_version: version,
            source_name,
        })
    }
}

fn write_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
}

fn read_u32(data: &[u8], cursor: &mut usize) -> Result<u32, String> {
    if *cursor + 4 > data.len() {
        return Err("unexpected end of bytecode".to_owned());
    }
    let v = u32::from_le_bytes(data[*cursor..*cursor + 4].try_into().unwrap());
    *cursor += 4;
    Ok(v)
}

fn read_u16(data: &[u8], cursor: &mut usize) -> Result<u16, String> {
    if *cursor + 2 > data.len() {
        return Err("unexpected end of bytecode".to_owned());
    }
    let v = u16::from_le_bytes(data[*cursor..*cursor + 2].try_into().unwrap());
    *cursor += 2;
    Ok(v)
}

fn read_str(data: &[u8], cursor: &mut usize) -> Result<String, String> {
    let len = read_u32(data, cursor)? as usize;
    if *cursor + len > data.len() {
        return Err("unexpected end of bytecode string".to_owned());
    }
    let s = std::str::from_utf8(&data[*cursor..*cursor + len])
        .map_err(|e| e.to_string())?
        .to_owned();
    *cursor += len;
    Ok(s)
}

fn serialize_chunk(buf: &mut Vec<u8>, chunk: &Chunk) {
    write_str(buf, &chunk.name);
    buf.extend_from_slice(&chunk.arity.to_le_bytes());
    buf.extend_from_slice(&chunk.local_count.to_le_bytes());
    write_str(buf, &chunk.source_name);
    // Pool
    buf.extend_from_slice(&(chunk.pool.len() as u32).to_le_bytes());
    for entry in &chunk.pool.entries {
        serialize_pool_entry(buf, entry);
    }
    // Code
    buf.extend_from_slice(&(chunk.code.len() as u32).to_le_bytes());
    for op in &chunk.code {
        serialize_opcode(buf, op);
    }
}

fn deserialize_chunk(data: &[u8], cursor: &mut usize) -> Result<Chunk, String> {
    let name = read_str(data, cursor)?;
    let arity = read_u32(data, cursor)?;
    let local_count = read_u32(data, cursor)?;
    let source_name = read_str(data, cursor)?;
    let pool_len = read_u32(data, cursor)? as usize;
    let mut pool = ConstantPool::new();
    for _ in 0..pool_len {
        pool.entries.push(deserialize_pool_entry(data, cursor)?);
    }
    let code_len = read_u32(data, cursor)? as usize;
    let mut code = Vec::with_capacity(code_len);
    for _ in 0..code_len {
        code.push(deserialize_opcode(data, cursor)?);
    }
    Ok(Chunk {
        name,
        arity,
        local_count,
        code,
        pool,
        source_name,
    })
}

fn serialize_pool_entry(buf: &mut Vec<u8>, entry: &PoolEntry) {
    match entry {
        PoolEntry::Integer(n) => {
            buf.push(0);
            buf.extend_from_slice(&n.to_le_bytes());
        }
        PoolEntry::Float(f) => {
            buf.push(1);
            buf.extend_from_slice(&f.to_le_bytes());
        }
        PoolEntry::String(s) => {
            buf.push(2);
            write_str(buf, s);
        }
        PoolEntry::Boolean(b) => {
            buf.push(3);
            buf.push(*b as u8);
        }
        PoolEntry::FunctionRef(s) => {
            buf.push(4);
            write_str(buf, s);
        }
    }
}

fn deserialize_pool_entry(data: &[u8], cursor: &mut usize) -> Result<PoolEntry, String> {
    if *cursor >= data.len() {
        return Err("unexpected end of pool".to_owned());
    }
    let tag = data[*cursor];
    *cursor += 1;
    match tag {
        0 => {
            if *cursor + 8 > data.len() {
                return Err("truncated integer".to_owned());
            }
            let v = i64::from_le_bytes(data[*cursor..*cursor + 8].try_into().unwrap());
            *cursor += 8;
            Ok(PoolEntry::Integer(v))
        }
        1 => {
            if *cursor + 8 > data.len() {
                return Err("truncated float".to_owned());
            }
            let v = f64::from_le_bytes(data[*cursor..*cursor + 8].try_into().unwrap());
            *cursor += 8;
            Ok(PoolEntry::Float(v))
        }
        2 => Ok(PoolEntry::String(read_str(data, cursor)?)),
        3 => {
            if *cursor >= data.len() {
                return Err("truncated bool".to_owned());
            }
            let v = data[*cursor] != 0;
            *cursor += 1;
            Ok(PoolEntry::Boolean(v))
        }
        4 => Ok(PoolEntry::FunctionRef(read_str(data, cursor)?)),
        _ => Err(format!("unknown pool entry tag {tag}")),
    }
}

fn serialize_opcode(buf: &mut Vec<u8>, op: &Opcode) {
    match op {
        Opcode::LoadConst(i) => {
            buf.push(0x01);
            buf.extend_from_slice(&i.to_le_bytes());
        }
        Opcode::LoadLocal(i) => {
            buf.push(0x02);
            buf.extend_from_slice(&i.to_le_bytes());
        }
        Opcode::StoreLocal(i) => {
            buf.push(0x03);
            buf.extend_from_slice(&i.to_le_bytes());
        }
        Opcode::BinaryOp(op) => {
            buf.push(0x04);
            buf.push(*op as u8);
        }
        Opcode::UnaryOp(op) => {
            buf.push(0x05);
            buf.push(*op as u8);
        }
        Opcode::Jump(t) => {
            buf.push(0x06);
            buf.extend_from_slice(&t.to_le_bytes());
        }
        Opcode::JumpIfTrue(t) => {
            buf.push(0x07);
            buf.extend_from_slice(&t.to_le_bytes());
        }
        Opcode::JumpIfFalse(t) => {
            buf.push(0x08);
            buf.extend_from_slice(&t.to_le_bytes());
        }
        Opcode::Call { name, argc } => {
            buf.push(0x09);
            write_str(buf, name);
            buf.extend_from_slice(&argc.to_le_bytes());
        }
        Opcode::Return => buf.push(0x0A),
        Opcode::Pop => buf.push(0x0B),
        Opcode::Dup => buf.push(0x0C),
        Opcode::PushUnit => buf.push(0x0D),
        Opcode::Halt => buf.push(0x0E),
        Opcode::Throw => buf.push(0x0F),
        Opcode::PushHandler(t) => {
            buf.push(0x11);
            buf.extend_from_slice(&t.to_le_bytes());
        }
        Opcode::PopHandler => buf.push(0x12),
        Opcode::ThrowValue => buf.push(0x13),
        Opcode::TypeName => buf.push(0x14),
        Opcode::ToString => buf.push(0x15),
        Opcode::BuildArray(count) => {
            buf.push(0x16);
            buf.extend_from_slice(&count.to_le_bytes());
        }
        Opcode::IndexGet => buf.push(0x17),
        Opcode::IndexSet => buf.push(0x18),
        Opcode::NewObject(type_name) => {
            buf.push(0x19);
            write_str(buf, type_name);
        }
        Opcode::SetField(name) => {
            buf.push(0x1A);
            write_str(buf, name);
        }
        Opcode::GetField(name) => {
            buf.push(0x1B);
            write_str(buf, name);
        }
        Opcode::BuildEnum {
            type_name,
            variant,
            argc,
        } => {
            buf.push(0x1C);
            write_str(buf, type_name);
            write_str(buf, variant);
            buf.extend_from_slice(&argc.to_le_bytes());
        }
        Opcode::VariantName => buf.push(0x1D),
        Opcode::IterNext => buf.push(0x1E),
        Opcode::SpawnCall { callee, argc } => {
            buf.push(0x22);
            write_str(buf, callee);
            buf.extend_from_slice(&argc.to_le_bytes());
        }
        Opcode::SpawnMethod { method, argc } => {
            buf.push(0x23);
            write_str(buf, method);
            buf.extend_from_slice(&argc.to_le_bytes());
        }
        Opcode::AwaitTask => buf.push(0x24),
        Opcode::CompleteTask => buf.push(0x25),
        Opcode::ChannelNew => buf.push(0x26),
        Opcode::CallMethod { method, argc } => {
            buf.push(0x21);
            write_str(buf, method);
            buf.extend_from_slice(&argc.to_le_bytes());
        }
        Opcode::PatternIndex => buf.push(0x1F),
        Opcode::PatternField(name) => {
            buf.push(0x20);
            write_str(buf, name);
        }
        Opcode::DebugSpan(s) => {
            buf.push(0x10);
            buf.extend_from_slice(&(s.start as u32).to_le_bytes());
            buf.extend_from_slice(&(s.end as u32).to_le_bytes());
        }
    }
}

fn deserialize_opcode(data: &[u8], cursor: &mut usize) -> Result<Opcode, String> {
    if *cursor >= data.len() {
        return Err("unexpected end of code".to_owned());
    }
    let tag = data[*cursor];
    *cursor += 1;
    match tag {
        0x01 => Ok(Opcode::LoadConst(read_u32(data, cursor)?)),
        0x02 => Ok(Opcode::LoadLocal(read_u32(data, cursor)?)),
        0x03 => Ok(Opcode::StoreLocal(read_u32(data, cursor)?)),
        0x04 => {
            if *cursor >= data.len() {
                return Err("truncated BinaryOp".to_owned());
            }
            let op = decode_binary_op(data[*cursor])?;
            *cursor += 1;
            Ok(Opcode::BinaryOp(op))
        }
        0x05 => {
            if *cursor >= data.len() {
                return Err("truncated UnaryOp".to_owned());
            }
            let op = decode_unary_op(data[*cursor])?;
            *cursor += 1;
            Ok(Opcode::UnaryOp(op))
        }
        0x06 => Ok(Opcode::Jump(read_u32(data, cursor)?)),
        0x07 => Ok(Opcode::JumpIfTrue(read_u32(data, cursor)?)),
        0x08 => Ok(Opcode::JumpIfFalse(read_u32(data, cursor)?)),
        0x09 => {
            let name = read_str(data, cursor)?;
            let argc = read_u32(data, cursor)?;
            Ok(Opcode::Call { name, argc })
        }
        0x0A => Ok(Opcode::Return),
        0x0B => Ok(Opcode::Pop),
        0x0C => Ok(Opcode::Dup),
        0x0D => Ok(Opcode::PushUnit),
        0x0E => Ok(Opcode::Halt),
        0x0F => Ok(Opcode::Throw),
        0x11 => Ok(Opcode::PushHandler(read_u32(data, cursor)?)),
        0x12 => Ok(Opcode::PopHandler),
        0x13 => Ok(Opcode::ThrowValue),
        0x14 => Ok(Opcode::TypeName),
        0x15 => Ok(Opcode::ToString),
        0x16 => Ok(Opcode::BuildArray(read_u32(data, cursor)?)),
        0x17 => Ok(Opcode::IndexGet),
        0x18 => Ok(Opcode::IndexSet),
        0x19 => Ok(Opcode::NewObject(read_str(data, cursor)?)),
        0x1A => Ok(Opcode::SetField(read_str(data, cursor)?)),
        0x1B => Ok(Opcode::GetField(read_str(data, cursor)?)),
        0x1C => {
            let type_name = read_str(data, cursor)?;
            let variant = read_str(data, cursor)?;
            let argc = read_u32(data, cursor)?;
            Ok(Opcode::BuildEnum {
                type_name,
                variant,
                argc,
            })
        }
        0x1D => Ok(Opcode::VariantName),
        0x1E => Ok(Opcode::IterNext),
        0x22 => {
            let callee = read_str(data, cursor)?;
            let argc = read_u32(data, cursor)?;
            Ok(Opcode::SpawnCall { callee, argc })
        }
        0x23 => {
            let method = read_str(data, cursor)?;
            let argc = read_u32(data, cursor)?;
            Ok(Opcode::SpawnMethod { method, argc })
        }
        0x24 => Ok(Opcode::AwaitTask),
        0x25 => Ok(Opcode::CompleteTask),
        0x26 => Ok(Opcode::ChannelNew),
        0x21 => {
            let method = read_str(data, cursor)?;
            let argc = read_u32(data, cursor)?;
            Ok(Opcode::CallMethod { method, argc })
        }
        0x1F => Ok(Opcode::PatternIndex),
        0x20 => Ok(Opcode::PatternField(read_str(data, cursor)?)),
        0x10 => {
            let start = read_u32(data, cursor)? as usize;
            let end = read_u32(data, cursor)? as usize;
            Ok(Opcode::DebugSpan(Span { start, end }))
        }
        _ => Err(format!("unknown opcode 0x{tag:02X}")),
    }
}

fn decode_binary_op(b: u8) -> Result<BinaryOp, String> {
    match b {
        0 => Ok(BinaryOp::Add),
        1 => Ok(BinaryOp::Sub),
        2 => Ok(BinaryOp::Mul),
        3 => Ok(BinaryOp::Div),
        4 => Ok(BinaryOp::Rem),
        5 => Ok(BinaryOp::Eq),
        6 => Ok(BinaryOp::Ne),
        7 => Ok(BinaryOp::Lt),
        8 => Ok(BinaryOp::Le),
        9 => Ok(BinaryOp::Gt),
        10 => Ok(BinaryOp::Ge),
        11 => Ok(BinaryOp::And),
        12 => Ok(BinaryOp::Or),
        13 => Ok(BinaryOp::BitAnd),
        14 => Ok(BinaryOp::BitOr),
        15 => Ok(BinaryOp::BitXor),
        16 => Ok(BinaryOp::Shl),
        17 => Ok(BinaryOp::Shr),
        _ => Err(format!("unknown BinaryOp {b}")),
    }
}

fn decode_unary_op(b: u8) -> Result<UnaryOp, String> {
    match b {
        0 => Ok(UnaryOp::Neg),
        1 => Ok(UnaryOp::Not),
        _ => Err(format!("unknown UnaryOp {b}")),
    }
}

// ── NIR → Bytecode compiler ───────────────────────────────────────────────────

/// Compile a NIR `Module` to a `CompiledProgram`.
pub fn compile(module: &Module) -> Result<CompiledProgram, Diagnostic> {
    let mut chunks = Vec::new();
    for function in &module.functions {
        chunks.push(compile_function(function, &module.functions)?);
    }
    let entry_point = module
        .entry_point
        .clone()
        .unwrap_or_else(|| "main".to_owned());
    Ok(CompiledProgram {
        chunks,
        entry_point,
        format_version: BYTECODE_VERSION,
        source_name: module.metadata.source_name.clone(),
    })
}

fn compile_function(
    function: &nir::Function,
    all_functions: &[nir::Function],
) -> Result<Chunk, Diagnostic> {
    let mut chunk = Chunk::new(
        function.name.clone(),
        function.parameters.len() as u32,
        function.debug.source_name.clone(),
    );

    // Assign a local slot to every ValueId defined in this function.
    let mut value_slots: HashMap<nir::ValueId, u32> = HashMap::new();
    // Parameters occupy slots 0..arity.
    for block in &function.basic_blocks {
        for instr in &block.instructions {
            if let nir::Instruction::Parameter { result, index, .. } = instr {
                value_slots.insert(*result, *index as u32);
            }
        }
    }
    // Remaining values get slots starting after parameters.
    let mut next_slot = function.parameters.len() as u32;
    for block in &function.basic_blocks {
        for instr in &block.instructions {
            for result in nir_results(instr) {
                if let std::collections::hash_map::Entry::Vacant(e) = value_slots.entry(result) {
                    e.insert(next_slot);
                    next_slot += 1;
                }
            }
        }
    }
    chunk.local_count = next_slot;

    // Also track NXL name → slot for Store/Load/Assign.
    let mut name_slots: HashMap<String, u32> = HashMap::new();
    // Pre-populate parameter names.
    for (i, (name, _)) in function.parameters.iter().enumerate() {
        name_slots.insert(name.clone(), i as u32);
    }

    // Map BasicBlockId → instruction offset (filled in two passes).
    let _block_ids: Vec<BasicBlockId> = function.basic_blocks.iter().map(|b| b.id).collect();
    // First pass: record where each block starts.
    let mut block_offsets: HashMap<BasicBlockId, u32> = HashMap::new();
    // We'll do a single-pass compile and patch jumps afterward.
    // Collect (patch_site, target_block_id) for back-patching.
    let mut patches: Vec<(u32, BasicBlockId)> = Vec::new();

    for (block_idx, block) in function.basic_blocks.iter().enumerate() {
        block_offsets.insert(block.id, chunk.code.len() as u32);

        for instr in &block.instructions {
            chunk.emit(Opcode::DebugSpan(instr.span()));
            let mut loop_stack = Vec::new();
            compile_instruction(
                instr,
                &mut chunk,
                &mut value_slots,
                &mut name_slots,
                &mut next_slot,
                &mut patches,
                all_functions,
                &mut loop_stack,
            )?;
        }

        // Terminator
        match &block.terminator {
            Terminator::Return { value, .. } => {
                if let Some(v) = value {
                    let slot = *value_slots.get(v).unwrap_or(&0);
                    chunk.emit(Opcode::LoadLocal(slot));
                } else {
                    chunk.emit(Opcode::PushUnit);
                }
                chunk.emit(Opcode::Return);
            }
            Terminator::Jump { target, .. } => {
                let patch_site = chunk.emit(Opcode::Jump(0));
                patches.push((patch_site, *target));
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
                ..
            } => {
                let cond_slot = *value_slots.get(condition).unwrap_or(&0);
                chunk.emit(Opcode::LoadLocal(cond_slot));
                let false_site = chunk.emit(Opcode::JumpIfFalse(0));
                patches.push((false_site, *else_block));
                let true_site = chunk.emit(Opcode::Jump(0));
                patches.push((true_site, *then_block));
            }
            Terminator::Unreachable { .. } => {
                let idx = chunk
                    .pool
                    .intern(PoolEntry::String("unreachable".to_owned()));
                chunk.emit(Opcode::LoadConst(idx));
                chunk.emit(Opcode::Throw);
            }
        }

        let _ = block_idx;
    }

    // Back-patch all jump targets.
    for (site, target_id) in patches {
        if let Some(&offset) = block_offsets.get(&target_id) {
            chunk.patch_jump(site, offset);
        }
    }
    // Nested regions may have allocated extra slots after the pre-pass.
    chunk.local_count = chunk.local_count.max(next_slot);

    Ok(chunk)
}

/// An in-progress inline loop for nested structured emission.
struct InlineLoop {
    /// Offset of the loop header (condition check).
    header: u32,
    /// Jump sites that must target the loop exit.
    exits: Vec<u32>,
}

#[allow(clippy::too_many_arguments)]
fn compile_instruction(
    instr: &nir::Instruction,
    chunk: &mut Chunk,
    value_slots: &mut HashMap<nir::ValueId, u32>,
    name_slots: &mut HashMap<String, u32>,
    next_slot: &mut u32,
    patches: &mut Vec<(u32, BasicBlockId)>,
    _all_functions: &[nir::Function],
    loop_stack: &mut Vec<InlineLoop>,
) -> Result<(), Diagnostic> {
    match instr {
        nir::Instruction::Parameter { .. } => {
            // Already handled — parameters are pre-assigned to slots.
        }

        nir::Instruction::Constant { result, value, .. } => {
            let idx = match value {
                Constant::Integer(n) => chunk.pool.intern(PoolEntry::Integer(*n)),
                Constant::Float(f) => chunk.pool.intern(PoolEntry::Float(*f)),
                Constant::String(s) => chunk.pool.intern(PoolEntry::String(s.clone())),
                Constant::Boolean(b) => chunk.pool.intern(PoolEntry::Boolean(*b)),
            };
            chunk.emit(Opcode::LoadConst(idx));
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::Load { result, name, .. } => {
            let src_slot = name_slots
                .get(name)
                .copied()
                .unwrap_or_else(|| ensure_slot(result, value_slots, next_slot));
            chunk.emit(Opcode::LoadLocal(src_slot));
            let dst_slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(dst_slot));
        }

        nir::Instruction::Store { name, value, .. } => {
            let src_slot = *value_slots.get(value).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(src_slot));
            let dst_slot = *name_slots.entry(name.clone()).or_insert_with(|| {
                let s = *next_slot;
                *next_slot += 1;
                s
            });
            chunk.emit(Opcode::StoreLocal(dst_slot));
        }

        nir::Instruction::Assign { name, value, .. } => {
            let src_slot = *value_slots.get(value).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(src_slot));
            let dst_slot = *name_slots.entry(name.clone()).or_insert_with(|| {
                let s = *next_slot;
                *next_slot += 1;
                s
            });
            chunk.emit(Opcode::StoreLocal(dst_slot));
            // Also update value_slots so subsequent loads see the new value.
            value_slots.insert(*value, dst_slot);
        }

        nir::Instruction::Unary {
            result,
            operator,
            operand,
            ..
        } => {
            let src = *value_slots.get(operand).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(src));
            let uop = match operator {
                crate::ast::UnaryOperator::Negate => UnaryOp::Neg,
                crate::ast::UnaryOperator::Not => UnaryOp::Not,
                _ => UnaryOp::Neg,
            };
            chunk.emit(Opcode::UnaryOp(uop));
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::Binary {
            result,
            operator,
            left,
            right,
            ..
        } => {
            let ls = *value_slots.get(left).unwrap_or(&0);
            let rs = *value_slots.get(right).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(ls));
            chunk.emit(Opcode::LoadLocal(rs));
            let bop = nir_binary_to_vm(*operator);
            chunk.emit(Opcode::BinaryOp(bop));
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::Call {
            result,
            callee,
            arguments,
            return_type,
            ..
        } => {
            for arg in arguments {
                let s = *value_slots.get(arg).unwrap_or(&0);
                chunk.emit(Opcode::LoadLocal(s));
            }
            chunk.emit(Opcode::Call {
                name: callee.clone(),
                argc: arguments.len() as u32,
            });
            if !matches!(return_type, Type::Void) {
                let slot = ensure_slot(result, value_slots, next_slot);
                chunk.emit(Opcode::StoreLocal(slot));
            } else {
                chunk.emit(Opcode::Pop);
            }
        }

        nir::Instruction::ToString { result, value, .. } => {
            let src = *value_slots.get(value).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(src));
            chunk.emit(Opcode::ToString);
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::TypeName { result, value, .. } => {
            let src = *value_slots.get(value).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(src));
            chunk.emit(Opcode::TypeName);
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::Catch { result, .. } => {
            // The unwinder pushed the exception value; spill it to a slot.
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::Throw { value, .. } => {
            let src = *value_slots.get(value).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(src));
            chunk.emit(Opcode::ThrowValue);
        }

        nir::Instruction::PushHandler { catch_block, .. } => {
            let site = chunk.emit(Opcode::PushHandler(0));
            patches.push((site, *catch_block));
        }

        nir::Instruction::PopHandler { .. } => {
            chunk.emit(Opcode::PopHandler);
        }

        nir::Instruction::Try { .. } => {
            compile_try(instr, chunk, value_slots, name_slots, next_slot, loop_stack)?;
        }

        nir::Instruction::IteratorNext {
            has_next,
            item,
            iterable,
            index,
            ..
        } => {
            let iter_slot = *value_slots.get(iterable).unwrap_or(&0);
            let index_slot = *value_slots.get(index).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(iter_slot));
            chunk.emit(Opcode::LoadLocal(index_slot));
            chunk.emit(Opcode::IterNext);
            // IterNext pushes the item, then has_next, then the next index.
            let index_out = ensure_slot(index, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(index_out));
            let has_slot = ensure_slot(has_next, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(has_slot));
            let item_slot = ensure_slot(item, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(item_slot));
        }

        nir::Instruction::ArrayNew {
            result, elements, ..
        } => {
            for element in elements {
                let slot = *value_slots.get(element).unwrap_or(&0);
                chunk.emit(Opcode::LoadLocal(slot));
            }
            chunk.emit(Opcode::BuildArray(elements.len() as u32));
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::IndexGet {
            result,
            object,
            index,
            ..
        } => {
            let obj_slot = *value_slots.get(object).unwrap_or(&0);
            let idx_slot = *value_slots.get(index).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(obj_slot));
            chunk.emit(Opcode::LoadLocal(idx_slot));
            chunk.emit(Opcode::IndexGet);
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::IndexSet {
            result,
            object,
            index,
            value,
            ..
        } => {
            let obj_slot = *value_slots.get(object).unwrap_or(&0);
            let idx_slot = *value_slots.get(index).unwrap_or(&0);
            let val_slot = *value_slots.get(value).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(obj_slot));
            chunk.emit(Opcode::LoadLocal(idx_slot));
            chunk.emit(Opcode::LoadLocal(val_slot));
            chunk.emit(Opcode::IndexSet);
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::StructNew {
            result,
            type_name,
            fields,
            ..
        } => {
            chunk.emit(Opcode::NewObject(type_name.clone()));
            let obj_slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(obj_slot));
            for (name, value) in fields {
                let val_slot = *value_slots.get(value).unwrap_or(&0);
                chunk.emit(Opcode::LoadLocal(obj_slot));
                chunk.emit(Opcode::LoadLocal(val_slot));
                chunk.emit(Opcode::SetField(name.clone()));
                chunk.emit(Opcode::StoreLocal(obj_slot));
            }
        }

        nir::Instruction::FieldGet {
            result,
            object,
            field,
            ..
        } => {
            let obj_slot = *value_slots.get(object).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(obj_slot));
            chunk.emit(Opcode::GetField(field.clone()));
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::FieldSet {
            result,
            object,
            field,
            value,
            ..
        } => {
            let obj_slot = *value_slots.get(object).unwrap_or(&0);
            let val_slot = *value_slots.get(value).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(obj_slot));
            chunk.emit(Opcode::LoadLocal(val_slot));
            chunk.emit(Opcode::SetField(field.clone()));
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::EnumNew {
            result,
            type_name,
            variant,
            arguments,
            ..
        } => {
            for argument in arguments {
                let slot = *value_slots.get(argument).unwrap_or(&0);
                chunk.emit(Opcode::LoadLocal(slot));
            }
            chunk.emit(Opcode::BuildEnum {
                type_name: type_name.clone(),
                variant: variant.clone(),
                argc: arguments.len() as u32,
            });
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::VariantName { result, value, .. } => {
            let src = *value_slots.get(value).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(src));
            chunk.emit(Opcode::VariantName);
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::SpawnCall {
            result,
            callee,
            arguments,
            ..
        } => {
            for argument in arguments {
                let slot = *value_slots.get(argument).unwrap_or(&0);
                chunk.emit(Opcode::LoadLocal(slot));
            }
            chunk.emit(Opcode::SpawnCall {
                callee: callee.clone(),
                argc: arguments.len() as u32,
            });
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::SpawnMethod {
            result,
            receiver,
            method,
            arguments,
            ..
        } => {
            let recv_slot = *value_slots.get(receiver).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(recv_slot));
            for argument in arguments {
                let slot = *value_slots.get(argument).unwrap_or(&0);
                chunk.emit(Opcode::LoadLocal(slot));
            }
            chunk.emit(Opcode::SpawnMethod {
                method: method.clone(),
                argc: arguments.len() as u32,
            });
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::CompleteTask { result, value, .. } => {
            let src = *value_slots.get(value).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(src));
            chunk.emit(Opcode::CompleteTask);
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::AwaitTask { result, task, .. } => {
            let src = *value_slots.get(task).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(src));
            chunk.emit(Opcode::AwaitTask);
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::ChannelNew { result, .. } => {
            chunk.emit(Opcode::ChannelNew);
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::MethodCall {
            result,
            receiver_out,
            receiver,
            method,
            arguments,
            ..
        } => {
            let recv_slot = *value_slots.get(receiver).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(recv_slot));
            for argument in arguments {
                let slot = *value_slots.get(argument).unwrap_or(&0);
                chunk.emit(Opcode::LoadLocal(slot));
            }
            chunk.emit(Opcode::CallMethod {
                method: method.clone(),
                argc: arguments.len() as u32,
            });
            // CallMethod pushes the return value, then the updated receiver.
            let out_slot = ensure_slot(receiver_out, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(out_slot));
            let ret_slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(ret_slot));
        }

        nir::Instruction::PatternIndex {
            result,
            object,
            index,
            ..
        } => {
            let obj_slot = *value_slots.get(object).unwrap_or(&0);
            let idx_slot = *value_slots.get(index).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(obj_slot));
            chunk.emit(Opcode::LoadLocal(idx_slot));
            chunk.emit(Opcode::PatternIndex);
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        nir::Instruction::PatternField {
            result,
            object,
            field,
            ..
        } => {
            let obj_slot = *value_slots.get(object).unwrap_or(&0);
            chunk.emit(Opcode::LoadLocal(obj_slot));
            chunk.emit(Opcode::PatternField(field.clone()));
            let slot = ensure_slot(result, value_slots, next_slot);
            chunk.emit(Opcode::StoreLocal(slot));
        }

        // Structural instructions are handled by the CFG; skip here.
        // (They are still reachable through nested regions, which use
        // `compile_nested` instead of this function.)
        nir::Instruction::If { .. }
        | nir::Instruction::Loop { .. }
        | nir::Instruction::Break { .. }
        | nir::Instruction::Continue { .. }
        | nir::Instruction::Return { .. } => {}

        // Remaining instructions: emit a no-op slot allocation so ValueIds
        // are still defined (avoids slot lookup panics).
        _ => {
            for result in nir_results(instr) {
                ensure_slot(&result, value_slots, next_slot);
            }
        }
    }
    Ok(())
}

/// Emit a nested `try` statement inline: handler registration around the body
/// with a dispatch chain for the catch clauses and shared `finally` tail.
#[allow(clippy::too_many_arguments)]
fn compile_try(
    instr: &nir::Instruction,
    chunk: &mut Chunk,
    value_slots: &mut HashMap<nir::ValueId, u32>,
    name_slots: &mut HashMap<String, u32>,
    next_slot: &mut u32,
    loop_stack: &mut Vec<InlineLoop>,
) -> Result<(), Diagnostic> {
    let nir::Instruction::Try {
        body,
        catches,
        finally,
        tmp,
        ..
    } = instr
    else {
        return Err(Diagnostic::error(
            "NXL3001",
            "compile_try called on a non-try instruction",
            Span::default(),
        ));
    };
    let tmp_slot = ensure_slot(tmp, value_slots, next_slot);

    let site_push = chunk.emit(Opcode::PushHandler(0));
    compile_nested(body, chunk, value_slots, name_slots, next_slot, loop_stack)?;
    chunk.emit(Opcode::PopHandler);
    // Normal + matched-catch paths converge on the finally tail.
    let mut fin_sites = vec![chunk.emit(Opcode::Jump(0))];

    // Exceptional entry: the unwinder pushed the value; spill it.
    let dispatch_ip = chunk.code.len() as u32;
    chunk.patch_jump(site_push, dispatch_ip);
    chunk.emit(Opcode::StoreLocal(tmp_slot));

    let mut saw_bare = false;
    for catch in catches {
        if saw_bare {
            // Unreachable (the lowerer rejects this); skip emission.
            continue;
        }
        // Bind the catch name for the duration of its body.
        let bind_slot = *next_slot;
        *next_slot += 1;
        let old_binding = name_slots.insert(catch.name.clone(), bind_slot);
        if let Some(annotation) = &catch.annotation {
            chunk.emit(Opcode::LoadLocal(tmp_slot));
            chunk.emit(Opcode::TypeName);
            let ann_idx = chunk.pool.intern(PoolEntry::String(annotation.clone()));
            chunk.emit(Opcode::LoadConst(ann_idx));
            chunk.emit(Opcode::BinaryOp(BinaryOp::Eq));
            let site_next = chunk.emit(Opcode::JumpIfFalse(0));
            chunk.emit(Opcode::LoadLocal(tmp_slot));
            chunk.emit(Opcode::StoreLocal(bind_slot));
            compile_nested(
                &catch.body,
                chunk,
                value_slots,
                name_slots,
                next_slot,
                loop_stack,
            )?;
            fin_sites.push(chunk.emit(Opcode::Jump(0)));
            let next_ip = chunk.code.len() as u32;
            chunk.patch_jump(site_next, next_ip);
        } else {
            chunk.emit(Opcode::LoadLocal(tmp_slot));
            chunk.emit(Opcode::StoreLocal(bind_slot));
            compile_nested(
                &catch.body,
                chunk,
                value_slots,
                name_slots,
                next_slot,
                loop_stack,
            )?;
            fin_sites.push(chunk.emit(Opcode::Jump(0)));
            saw_bare = true;
        }
        match old_binding {
            Some(slot) => {
                name_slots.insert(catch.name.clone(), slot);
            }
            None => {
                name_slots.remove(&catch.name);
            }
        }
    }

    if !saw_bare {
        // No catch claimed the value: run finally (if any), then rethrow.
        if let Some(fin) = finally {
            compile_nested(fin, chunk, value_slots, name_slots, next_slot, loop_stack)?;
        }
        chunk.emit(Opcode::LoadLocal(tmp_slot));
        chunk.emit(Opcode::ThrowValue);
    }

    // Shared finally tail for the normal and matched-catch paths.
    let fin_ip = chunk.code.len() as u32;
    for site in fin_sites {
        chunk.patch_jump(site, fin_ip);
    }
    if let Some(fin) = finally {
        compile_nested(fin, chunk, value_slots, name_slots, next_slot, loop_stack)?;
    }
    // Keep the frame's local count covering slots allocated above.
    // (StoreLocal resizes defensively at runtime regardless.)
    Ok(())
}

/// Emit nested structured instructions inline within the current chunk:
/// `if`/`loop` become jumps, `break`/`continue` target the innermost inline
/// loop, and nested `try` recurses. Anything else delegates to
/// `compile_instruction`.
#[allow(clippy::too_many_arguments)]
fn compile_nested(
    instrs: &[nir::Instruction],
    chunk: &mut Chunk,
    value_slots: &mut HashMap<nir::ValueId, u32>,
    name_slots: &mut HashMap<String, u32>,
    next_slot: &mut u32,
    loop_stack: &mut Vec<InlineLoop>,
) -> Result<(), Diagnostic> {
    for instr in instrs {
        match instr {
            nir::Instruction::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                chunk.emit(Opcode::DebugSpan(instr.span()));
                let cond_slot = *value_slots.get(condition).unwrap_or(&0);
                chunk.emit(Opcode::LoadLocal(cond_slot));
                let site_else = chunk.emit(Opcode::JumpIfFalse(0));
                compile_nested(
                    then_branch,
                    chunk,
                    value_slots,
                    name_slots,
                    next_slot,
                    loop_stack,
                )?;
                match else_branch {
                    Some(els) => {
                        let site_end = chunk.emit(Opcode::Jump(0));
                        let else_ip = chunk.code.len() as u32;
                        chunk.patch_jump(site_else, else_ip);
                        compile_nested(els, chunk, value_slots, name_slots, next_slot, loop_stack)?;
                        let end_ip = chunk.code.len() as u32;
                        chunk.patch_jump(site_end, end_ip);
                    }
                    None => {
                        let else_ip = chunk.code.len() as u32;
                        chunk.patch_jump(site_else, else_ip);
                    }
                }
            }
            nir::Instruction::Loop {
                condition,
                condition_instructions,
                body,
                span,
            } => {
                chunk.emit(Opcode::DebugSpan(*span));
                let header = chunk.code.len() as u32;
                compile_nested(
                    condition_instructions,
                    chunk,
                    value_slots,
                    name_slots,
                    next_slot,
                    loop_stack,
                )?;
                // Reserve the loop frame before the body so break/continue
                // inside it resolve to this loop.
                let frame_index = loop_stack.len();
                loop_stack.push(InlineLoop {
                    header,
                    exits: Vec::new(),
                });
                if let Some(cond) = condition {
                    let cond_slot = *value_slots.get(cond).unwrap_or(&0);
                    chunk.emit(Opcode::LoadLocal(cond_slot));
                    let site_exit = chunk.emit(Opcode::JumpIfFalse(0));
                    loop_stack[frame_index].exits.push(site_exit);
                }
                compile_nested(body, chunk, value_slots, name_slots, next_slot, loop_stack)?;
                chunk.emit(Opcode::Jump(header));
                let frame = loop_stack.pop().unwrap();
                let exit_ip = chunk.code.len() as u32;
                for site in frame.exits {
                    chunk.patch_jump(site, exit_ip);
                }
            }
            nir::Instruction::Break { span } => {
                chunk.emit(Opcode::DebugSpan(*span));
                match loop_stack.last_mut() {
                    Some(frame) => {
                        frame.exits.push(chunk.emit(Opcode::Jump(0)));
                    }
                    None => {
                        return Err(Diagnostic::error(
                            "NXL3001",
                            "break outside a loop reached codegen",
                            *span,
                        ));
                    }
                }
            }
            nir::Instruction::Continue { span } => {
                chunk.emit(Opcode::DebugSpan(*span));
                match loop_stack.last() {
                    Some(frame) => {
                        chunk.emit(Opcode::Jump(frame.header));
                    }
                    None => {
                        return Err(Diagnostic::error(
                            "NXL3001",
                            "continue outside a loop reached codegen",
                            *span,
                        ));
                    }
                }
            }
            nir::Instruction::Return { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "return across a try boundary is not supported yet",
                    *span,
                ));
            }
            _ => {
                chunk.emit(Opcode::DebugSpan(instr.span()));
                compile_instruction(
                    instr,
                    chunk,
                    value_slots,
                    name_slots,
                    next_slot,
                    &mut Vec::new(),
                    &[],
                    loop_stack,
                )?;
            }
        }
    }
    Ok(())
}

fn ensure_slot(
    result: &nir::ValueId,
    value_slots: &mut HashMap<nir::ValueId, u32>,
    next_slot: &mut u32,
) -> u32 {
    *value_slots.entry(*result).or_insert_with(|| {
        let s = *next_slot;
        *next_slot += 1;
        s
    })
}

fn nir_results(instr: &nir::Instruction) -> Vec<nir::ValueId> {
    match instr {
        nir::Instruction::Parameter { result, .. }
        | nir::Instruction::Constant { result, .. }
        | nir::Instruction::Load { result, .. }
        | nir::Instruction::Unary { result, .. }
        | nir::Instruction::Binary { result, .. }
        | nir::Instruction::Call { result, .. }
        | nir::Instruction::Allocate { result, .. }
        | nir::Instruction::LoadMemory { result, .. }
        | nir::Instruction::Borrow { result, .. }
        | nir::Instruction::Spawn { result, .. }
        | nir::Instruction::Await { result, .. }
        | nir::Instruction::ChannelReceive { result, .. }
        | nir::Instruction::ForeignCall { result, .. }
        | nir::Instruction::ToString { result, .. }
        | nir::Instruction::TypeName { result, .. }
        | nir::Instruction::Catch { result, .. } => vec![*result],
        nir::Instruction::IteratorNext { has_next, item, .. } => vec![*has_next, *item],
        nir::Instruction::ArrayNew { result, .. }
        | nir::Instruction::IndexGet { result, .. }
        | nir::Instruction::IndexSet { result, .. }
        | nir::Instruction::StructNew { result, .. }
        | nir::Instruction::FieldGet { result, .. }
        | nir::Instruction::FieldSet { result, .. }
        | nir::Instruction::EnumNew { result, .. }
        | nir::Instruction::VariantName { result, .. } => vec![*result],
        nir::Instruction::PatternIndex { result, .. } => vec![*result],
        nir::Instruction::PatternField { result, .. } => vec![*result],
        nir::Instruction::SpawnCall { result, .. }
        | nir::Instruction::SpawnMethod { result, .. }
        | nir::Instruction::CompleteTask { result, .. }
        | nir::Instruction::AwaitTask { result, .. }
        | nir::Instruction::ChannelNew { result, .. } => vec![*result],
        nir::Instruction::MethodCall {
            result,
            receiver_out,
            ..
        } => vec![*result, *receiver_out],
        nir::Instruction::Try {
            body,
            catches,
            finally,
            tmp,
            ..
        } => {
            let mut out = vec![*tmp];
            out.extend(nir_results_nested(body));
            for catch in catches {
                out.extend(nir_results_nested(&catch.body));
            }
            if let Some(fin) = finally {
                out.extend(nir_results_nested(fin));
            }
            out
        }
        _ => Vec::new(),
    }
}

/// Result ids defined in a nested instruction region, descending through
/// structured instructions (`if`/`loop`/`try`) as well as flat ones.
fn nir_results_nested(instrs: &[nir::Instruction]) -> Vec<nir::ValueId> {
    let mut out = Vec::new();
    for instr in instrs {
        match instr {
            nir::Instruction::If {
                then_branch,
                else_branch,
                ..
            } => {
                out.extend(nir_results_nested(then_branch));
                if let Some(els) = else_branch {
                    out.extend(nir_results_nested(els));
                }
            }
            nir::Instruction::Loop {
                condition_instructions,
                body,
                ..
            } => {
                out.extend(nir_results_nested(condition_instructions));
                out.extend(nir_results_nested(body));
            }
            _ => out.extend(nir_results(instr)),
        }
    }
    out
}

fn nir_binary_to_vm(op: crate::ast::BinaryOperator) -> BinaryOp {
    use crate::ast::BinaryOperator::*;
    match op {
        Add => BinaryOp::Add,
        Subtract => BinaryOp::Sub,
        Multiply => BinaryOp::Mul,
        Divide => BinaryOp::Div,
        Remainder => BinaryOp::Rem,
        Equal => BinaryOp::Eq,
        NotEqual => BinaryOp::Ne,
        Less => BinaryOp::Lt,
        LessEqual => BinaryOp::Le,
        Greater => BinaryOp::Gt,
        GreaterEqual => BinaryOp::Ge,
        And => BinaryOp::And,
        Or => BinaryOp::Or,
        BitAnd => BinaryOp::BitAnd,
        BitOr => BinaryOp::BitOr,
        BitXor => BinaryOp::BitXor,
        ShiftLeft => BinaryOp::Shl,
        ShiftRight => BinaryOp::Shr,
        _ => BinaryOp::Add, // Range/In/Is not yet supported in VM
    }
}

// ── Runtime values ────────────────────────────────────────────────────────────

/// A value on the VM stack or in a local slot.
#[derive(Clone, Debug)]
pub enum Value {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Array(Vec<Value>),
    Object {
        type_name: String,
        fields: Vec<(String, Value)>,
    },
    Enum {
        type_name: String,
        variant: String,
        args: Vec<Value>,
    },
    Task(TaskShared),
    Channel(ChannelShared),
    Unit,
}

/// Structural equality; tasks and channels compare by identity.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (
                Value::Object {
                    type_name: a_type,
                    fields: a_fields,
                },
                Value::Object {
                    type_name: b_type,
                    fields: b_fields,
                },
            ) => a_type == b_type && a_fields == b_fields,
            (
                Value::Enum {
                    type_name: a_type,
                    variant: a_variant,
                    args: a_args,
                },
                Value::Enum {
                    type_name: b_type,
                    variant: b_variant,
                    args: b_args,
                },
            ) => a_type == b_type && a_variant == b_variant && a_args == b_args,
            (Value::Task(a), Value::Task(b)) => Arc::ptr_eq(&a.0, &b.0),
            (Value::Channel(a), Value::Channel(b)) => Arc::ptr_eq(&a.0, &b.0),
            (Value::Unit, Value::Unit) => true,
            _ => false,
        }
    }
}

/// Shared handle to a spawned task's outcome.
#[derive(Clone, Debug)]
pub struct TaskShared(pub Arc<TaskState>);

/// Shared handle to a channel's queue.
#[derive(Clone, Debug)]
pub struct ChannelShared(pub Arc<ChannelState>);

/// Outcome slot for one spawned task.
#[derive(Debug)]
pub struct TaskState {
    pub id: u64,
    state: Mutex<TaskOutcome>,
    notify: Condvar,
}

#[derive(Clone, Debug)]
enum TaskOutcome {
    Running,
    Done(Result<Value, String>),
}

/// A rendezvous queue shared across threads. Senders never block;
/// receivers block until a value arrives or the channel closes.
#[derive(Debug)]
pub struct ChannelState {
    pub id: u64,
    queue: Mutex<VecDeque<Value>>,
    closed: AtomicBool,
    notify: Condvar,
}

static NEXT_HANDLE_ID: AtomicU64 = AtomicU64::new(1);

fn next_handle_id() -> u64 {
    NEXT_HANDLE_ID.fetch_add(1, Ordering::SeqCst)
}

impl TaskShared {
    fn new() -> Self {
        Self(Arc::new(TaskState {
            id: next_handle_id(),
            state: Mutex::new(TaskOutcome::Running),
            notify: Condvar::new(),
        }))
    }

    fn completed(value: Value) -> Self {
        Self(Arc::new(TaskState {
            id: next_handle_id(),
            state: Mutex::new(TaskOutcome::Done(Ok(value))),
            notify: Condvar::new(),
        }))
    }

    fn complete(&self, outcome: Result<Value, String>) {
        if let Ok(mut state) = self.0.state.lock() {
            *state = TaskOutcome::Done(outcome);
        }
        self.0.notify.notify_all();
    }

    /// Block until the task finishes; completed tasks return immediately.
    fn join(&self) -> Result<Value, String> {
        let mut state = self
            .0
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            match &*state {
                TaskOutcome::Done(outcome) => return outcome.clone(),
                TaskOutcome::Running => {
                    state = self
                        .0
                        .notify
                        .wait(state)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
            }
        }
    }

    fn is_done(&self) -> bool {
        matches!(
            *self.0.state.lock().unwrap_or_else(|p| p.into_inner()),
            TaskOutcome::Done(_)
        )
    }
}

impl ChannelShared {
    fn new() -> Self {
        Self(Arc::new(ChannelState {
            id: next_handle_id(),
            queue: Mutex::new(VecDeque::new()),
            closed: AtomicBool::new(false),
            notify: Condvar::new(),
        }))
    }

    fn send(&self, value: Value) -> Result<(), String> {
        if self.0.closed.load(Ordering::SeqCst) {
            return Err("send on a closed channel".to_owned());
        }
        let mut queue = self.0.queue.lock().unwrap_or_else(|p| p.into_inner());
        if self.0.closed.load(Ordering::SeqCst) {
            return Err("send on a closed channel".to_owned());
        }
        queue.push_back(value);
        self.0.notify.notify_one();
        Ok(())
    }

    /// Block for the next value; fails (catchably) when closed and drained.
    fn recv(&self) -> Result<Value, String> {
        let mut queue = self.0.queue.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if let Some(value) = queue.pop_front() {
                return Ok(value);
            }
            if self.0.closed.load(Ordering::SeqCst) {
                return Err("receive on a closed, empty channel".to_owned());
            }
            queue = self
                .0
                .notify
                .wait(queue)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn close(&self) {
        self.0.closed.store(true, Ordering::SeqCst);
        self.0.notify.notify_all();
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(n) => write!(f, "{n}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::String(s) => write!(f, "{s}"),
            Value::Boolean(b) => write!(f, "{b}"),
            Value::Array(elements) => {
                write!(f, "[")?;
                for (index, element) in elements.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{element}")?;
                }
                write!(f, "]")
            }
            Value::Object { type_name, fields } => {
                write!(f, "{} {{ ", short_type_name(type_name))?;
                for (index, (name, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {value}")?;
                }
                write!(f, " }}")
            }
            Value::Enum {
                type_name,
                variant,
                args,
            } => {
                write!(f, "{}.{}", short_type_name(type_name), variant)?;
                if !args.is_empty() {
                    write!(f, "(")?;
                    for (index, argument) in args.iter().enumerate() {
                        if index > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{argument}")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Value::Task(shared) => write!(f, "task#{}", shared.0.id),
            Value::Channel(shared) => write!(f, "channel#{}", shared.0.id),
            Value::Unit => write!(f, "()"),
        }
    }
}

// ── Call frame ────────────────────────────────────────────────────────────────

struct Frame {
    /// Name of the function being executed (for diagnostics).
    _name: String,
    /// Instruction pointer within the chunk's code.
    ip: usize,
    /// Local variable slots.
    locals: Vec<Value>,
}

impl Frame {
    fn new(name: String, local_count: u32, args: Vec<Value>) -> Self {
        let mut locals = vec![Value::Unit; local_count as usize];
        for (i, arg) in args.into_iter().enumerate() {
            if i < locals.len() {
                locals[i] = arg;
            }
        }
        Self {
            _name: name.to_owned(),
            ip: 0,
            locals,
        }
    }
}

// ── VM execution result ───────────────────────────────────────────────────────

/// The result of executing a compiled program in the VM.
#[derive(Clone, Debug, PartialEq)]
pub struct VmResult {
    /// Lines written by `print()` calls, in order.
    pub output: Vec<String>,
    /// Debug spans of executed instructions, in execution order (P21 debug trace).
    pub spans: Vec<Span>,
    /// The final return value of the entry-point function.
    pub return_value: Value,
}

impl VmResult {
    pub fn with_spans(output: Vec<String>, return_value: Value, spans: Vec<Span>) -> Self {
        Self {
            output,
            return_value,
            spans,
        }
    }
}

// ── VM ────────────────────────────────────────────────────────────────────────

/// The NXL stack-machine virtual machine.
pub struct Vm {
    program: Arc<CompiledProgram>,
    /// Operand stack.
    stack: Vec<Value>,
    /// Call stack.
    frames: Vec<Frame>,
    /// Registered exception handlers (innermost last).
    handlers: Vec<Handler>,
    /// Captured output (print() calls), shared with spawned workers.
    output: Arc<Mutex<Vec<String>>>,
    /// Debug spans observed during execution (P21 trace).
    debug_spans: Vec<Span>,
    /// Tasks spawned by this VM (plus descendants it reaps), joined when
    /// the entry point returns so no work is silently lost.
    spawned: Vec<TaskShared>,
}

impl Vm {
    fn with_output(program: Arc<CompiledProgram>, output: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            program,
            stack: Vec::new(),
            frames: Vec::new(),
            handlers: Vec::new(),
            output,
            debug_spans: Vec::new(),
            spawned: Vec::new(),
        }
    }

    fn run(&mut self) -> Result<Value, VmFault> {
        let entry = self.program.entry_point.clone();
        let result = self.call_function(&entry, vec![]).map(|(ret, _)| ret);
        if result.is_ok() {
            // Structured shutdown: reap every spawned task (looping, since
            // reaped tasks may have spawned their own). A failed background
            // task fails the run loudly instead of vanishing.
            loop {
                let pending: Vec<TaskShared> = self
                    .spawned
                    .iter()
                    .filter(|task| !task.is_done())
                    .cloned()
                    .collect();
                if pending.is_empty() {
                    break;
                }
                for task in &pending {
                    if let Err(message) = task.join() {
                        return Err(VmFault::Throw(Value::String(format!(
                            "background task failed: {message}"
                        ))));
                    }
                }
            }
        }
        result
    }

    fn emit_output(&mut self, line: String) {
        if let Ok(mut output) = self.output.lock() {
            output.push(line);
        }
    }

    /// Call a function chunk. Returns the return value plus the final value
    /// of the callee's first local (its `self` for methods).
    fn call_function(&mut self, name: &str, args: Vec<Value>) -> Result<(Value, Value), VmFault> {
        // Built-in: print
        if name == "print" {
            let line = args
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            self.emit_output(line);
            return Ok((Value::Unit, Value::Unit));
        }

        // Built-in: escape_html. Any value is rendered the way the runtimes
        // render it and then HTML-escaped, so building markup in NXL has a
        // primitive that cannot emit a live tag by accident.
        if name == "escape_html" {
            let Some(argument) = args.first() else {
                return Err(VmFault::Throw(Value::String(
                    "escape_html expects 1 argument".to_owned(),
                )));
            };
            return Ok((
                Value::String(crate::html::escape_html(&argument.to_string())),
                Value::Unit,
            ));
        }

        // Built-in: strip_tags, the text-only counterpart.
        if name == "strip_tags" {
            let Some(argument) = args.first() else {
                return Err(VmFault::Throw(Value::String(
                    "strip_tags expects 1 argument".to_owned(),
                )));
            };
            return Ok((
                Value::String(crate::html::strip_tags(&argument.to_string())),
                Value::Unit,
            ));
        }

        let chunk = self
            .program
            .chunk_by_name(name)
            .ok_or_else(|| VmFault::Throw(Value::String(format!("undefined function '{name}'"))))?
            .clone();

        if args.len() != chunk.arity as usize {
            return Err(VmFault::Throw(Value::String(format!(
                "function '{name}' expects {} args, got {}",
                chunk.arity,
                args.len()
            ))));
        }

        let frame = Frame::new(name.to_owned(), chunk.local_count, args);
        self.frames.push(frame);

        let result = self.execute_chunk(&chunk);

        // Handler unwinding never truncates frames itself: a fault either
        // resumes in place or propagates with every frame still live, so the
        // top frame here is always ours.
        match result {
            Ok(ret) => {
                let this = self
                    .frames
                    .last()
                    .and_then(|frame| frame.locals.first().cloned())
                    .unwrap_or(Value::Unit);
                self.frames.pop();
                Ok((ret, this))
            }
            Err(fault) => {
                self.frames.pop();
                Err(fault)
            }
        }
    }

    /// Offer a raised value to the nearest handler owned by the frame at
    /// `depth` (1-based frame count). Returns `Ok(())` when execution resumed
    /// at the handler, or `Err(value)` when the fault must propagate outward.
    fn handle_throw(&mut self, depth: usize, value: Value) -> Result<(), Value> {
        // Drop handlers that outlived their frame without running PopHandler.
        // (Codegen always balances them; this is defence in depth.)
        while self
            .handlers
            .last()
            .is_some_and(|handler| handler.frames_len > depth)
        {
            self.handlers.pop();
        }
        match self.handlers.last() {
            Some(handler) if handler.frames_len == depth => {
                let handler = self.handlers.pop().unwrap();
                self.stack.truncate(handler.stack_len);
                self.stack.push(value);
                if depth == 0 || depth > self.frames.len() {
                    // Resumption frame is gone; propagate instead.
                    let value = self.stack.pop().unwrap_or(Value::Unit);
                    Err(value)
                } else {
                    self.frames[depth - 1].ip = handler.target as usize;
                    Ok(())
                }
            }
            _ => Err(value),
        }
    }

    /// Resolve `receiver.method` to a chunk name, or a catchable error.
    fn method_target(&self, receiver: &Value, method: &str) -> Result<String, String> {
        let type_name = match receiver {
            Value::Object { type_name, .. } | Value::Enum { type_name, .. } => type_name.clone(),
            other => {
                return Err(format!(
                    "type '{}' has no method '{method}'",
                    short_type_name(type_name_of(other))
                ));
            }
        };
        let target = format!("{type_name}#{method}");
        if self.program.chunk_by_name(&target).is_none() {
            return Err(format!(
                "unknown method '{method}' on type '{}'",
                short_type_name(&type_name)
            ));
        }
        Ok(target)
    }

    /// Spawn a thread running `target` with `args`; the task is registered
    /// for the structured end-of-run join.
    fn spawn_call(&mut self, target: String, args: Vec<Value>) -> TaskShared {
        let shared = TaskShared::new();
        self.spawned.push(shared.clone());
        let program = Arc::clone(&self.program);
        let output = Arc::clone(&self.output);
        let worker = shared.clone();
        let started = std::thread::Builder::new()
            .name(target.clone())
            .spawn(move || {
                let mut child = Vm::with_output(program, output);
                let outcome = child
                    .call_function(&target, args)
                    .map(|(ret, _)| ret)
                    .map_err(|fault| match fault {
                        VmFault::Throw(value) => format!("task failed: {value}"),
                        VmFault::Fatal(message) => message,
                    });
                worker.complete(outcome);
            });
        if started.is_err() {
            shared.complete(Err("could not spawn task thread".to_owned()));
        }
        shared
    }

    /// Raise a catchable error, offering it to the local handler first.
    /// Returns `Ok(())` when a handler resumed, else the fault to propagate.
    fn raise(&mut self, depth: usize, message: String) -> Result<(), VmFault> {
        let value = Value::String(message);
        match self.handle_throw(depth, value) {
            Ok(()) => Ok(()),
            Err(value) => Err(VmFault::Throw(value)),
        }
    }

    fn execute_chunk(&mut self, chunk: &Chunk) -> Result<Value, VmFault> {
        let frame_idx = self.frames.len() - 1;
        let depth = self.frames.len();
        loop {
            let ip = self.frames[frame_idx].ip;
            if ip >= chunk.code.len() {
                return Ok(Value::Unit);
            }
            self.frames[frame_idx].ip += 1;
            let op = chunk.code[ip].clone();

            match op {
                Opcode::LoadConst(idx) => {
                    let entry = chunk
                        .pool
                        .get(idx)
                        .ok_or_else(|| VmFault::Fatal("invalid pool index".to_owned()))?;
                    self.stack.push(pool_to_value(entry));
                }

                Opcode::LoadLocal(slot) => {
                    let v = self.frames[frame_idx]
                        .locals
                        .get(slot as usize)
                        .cloned()
                        .unwrap_or(Value::Unit);
                    self.stack.push(v);
                }

                Opcode::StoreLocal(slot) => {
                    let v = self.stack.pop().unwrap_or(Value::Unit);
                    let locals = &mut self.frames[frame_idx].locals;
                    if slot as usize >= locals.len() {
                        locals.resize(slot as usize + 1, Value::Unit);
                    }
                    locals[slot as usize] = v;
                }

                Opcode::BinaryOp(bop) => {
                    let rhs = self.stack.pop().unwrap_or(Value::Unit);
                    let lhs = self.stack.pop().unwrap_or(Value::Unit);
                    match eval_binary(bop, lhs, rhs) {
                        Ok(result) => self.stack.push(result),
                        Err(message) => {
                            // Runtime failures are catchable values, like thrown ones.
                            let value = Value::String(message);
                            match self.handle_throw(depth, value) {
                                Ok(()) => {}
                                Err(value) => return Err(VmFault::Throw(value)),
                            }
                        }
                    }
                }

                Opcode::UnaryOp(uop) => {
                    let operand = self.stack.pop().unwrap_or(Value::Unit);
                    match eval_unary(uop, operand) {
                        Ok(result) => self.stack.push(result),
                        Err(message) => {
                            let value = Value::String(message);
                            match self.handle_throw(depth, value) {
                                Ok(()) => {}
                                Err(value) => return Err(VmFault::Throw(value)),
                            }
                        }
                    }
                }

                Opcode::Jump(target) => {
                    self.frames[frame_idx].ip = target as usize;
                }

                Opcode::JumpIfTrue(target) => {
                    let cond = self.stack.pop().unwrap_or(Value::Unit);
                    if is_truthy(&cond) {
                        self.frames[frame_idx].ip = target as usize;
                    }
                }

                Opcode::JumpIfFalse(target) => {
                    let cond = self.stack.pop().unwrap_or(Value::Unit);
                    if !is_truthy(&cond) {
                        self.frames[frame_idx].ip = target as usize;
                    }
                }

                Opcode::Call { name, argc } => {
                    let mut args = vec![Value::Unit; argc as usize];
                    for i in (0..argc as usize).rev() {
                        args[i] = self.stack.pop().unwrap_or(Value::Unit);
                    }
                    match self.call_function(&name, args) {
                        Ok((ret, _)) => self.stack.push(ret),
                        Err(VmFault::Throw(value)) => match self.handle_throw(depth, value) {
                            Ok(()) => {}
                            Err(value) => return Err(VmFault::Throw(value)),
                        },
                        Err(fatal) => return Err(fatal),
                    }
                }

                Opcode::CallMethod { method, argc } => {
                    let mut args = vec![Value::Unit; argc as usize];
                    for i in (0..argc as usize).rev() {
                        args[i] = self.stack.pop().unwrap_or(Value::Unit);
                    }
                    let receiver = self.stack.pop().unwrap_or(Value::Unit);
                    // Channels expose builtin methods through call syntax.
                    if let Value::Channel(shared) = &receiver {
                        match channel_method(shared, &method, &args) {
                            Ok(returned) => {
                                self.stack.push(returned);
                                self.stack.push(receiver);
                                continue;
                            }
                            Err(message) => {
                                self.raise(depth, message)?;
                                continue;
                            }
                        }
                    }
                    let target = match self.method_target(&receiver, &method) {
                        Ok(target) => target,
                        Err(message) => {
                            self.raise(depth, message)?;
                            continue;
                        }
                    };
                    // Arity: receiver plus explicit arguments.
                    let mut call_args = vec![receiver];
                    call_args.extend(args);
                    match self.call_function(&target, call_args) {
                        Ok((ret, updated)) => {
                            // The callee's final self slot flows back so the
                            // caller observes receiver mutations.
                            self.stack.push(ret);
                            self.stack.push(updated);
                        }
                        Err(VmFault::Throw(value)) => match self.handle_throw(depth, value) {
                            Ok(()) => {}
                            Err(value) => return Err(VmFault::Throw(value)),
                        },
                        Err(fatal) => return Err(fatal),
                    }
                }

                Opcode::SpawnCall { callee, argc } => {
                    let mut args = vec![Value::Unit; argc as usize];
                    for i in (0..argc as usize).rev() {
                        args[i] = self.stack.pop().unwrap_or(Value::Unit);
                    }
                    if self.program.chunk_by_name(&callee).is_none() {
                        self.raise(depth, format!("cannot spawn unknown function '{callee}'"))?;
                        continue;
                    }
                    let task = self.spawn_call(callee.clone(), args);
                    self.stack.push(Value::Task(task));
                }

                Opcode::SpawnMethod { method, argc } => {
                    let mut args = vec![Value::Unit; argc as usize];
                    for i in (0..argc as usize).rev() {
                        args[i] = self.stack.pop().unwrap_or(Value::Unit);
                    }
                    let receiver = self.stack.pop().unwrap_or(Value::Unit);
                    if matches!(receiver, Value::Channel(_)) {
                        self.raise(depth, "cannot spawn a channel operation".to_owned())?;
                        continue;
                    }
                    let target = match self.method_target(&receiver, &method) {
                        Ok(target) => target,
                        Err(message) => {
                            self.raise(depth, message)?;
                            continue;
                        }
                    };
                    let mut call_args = vec![receiver];
                    call_args.extend(args);
                    let task = self.spawn_call(target, call_args);
                    self.stack.push(Value::Task(task));
                }

                Opcode::AwaitTask => {
                    let task = self.stack.pop().unwrap_or(Value::Unit);
                    match task {
                        Value::Task(shared) => match shared.join() {
                            Ok(value) => self.stack.push(value),
                            Err(message) => {
                                self.raise(depth, message)?;
                            }
                        },
                        // Awaiting a plain value is the value itself.
                        other => self.stack.push(other),
                    }
                }

                Opcode::CompleteTask => {
                    let value = self.stack.pop().unwrap_or(Value::Unit);
                    self.stack.push(Value::Task(TaskShared::completed(value)));
                }

                Opcode::ChannelNew => {
                    self.stack.push(Value::Channel(ChannelShared::new()));
                }

                Opcode::Return => {
                    return Ok(self.stack.pop().unwrap_or(Value::Unit));
                }

                Opcode::Pop => {
                    self.stack.pop();
                }

                Opcode::Dup => {
                    let v = self.stack.last().cloned().unwrap_or(Value::Unit);
                    self.stack.push(v);
                }

                Opcode::PushUnit => {
                    self.stack.push(Value::Unit);
                }

                Opcode::Halt => {
                    return Ok(self.stack.pop().unwrap_or(Value::Unit));
                }

                Opcode::Throw => {
                    let msg = self.stack.pop().unwrap_or(Value::Unit).to_string();
                    let value = Value::String(msg);
                    match self.handle_throw(depth, value) {
                        Ok(()) => {}
                        Err(value) => return Err(VmFault::Throw(value)),
                    }
                }

                Opcode::PushHandler(target) => {
                    self.handlers.push(Handler {
                        frames_len: self.frames.len(),
                        stack_len: self.stack.len(),
                        target,
                    });
                }

                Opcode::PopHandler => {
                    self.handlers.pop();
                }

                Opcode::ThrowValue => {
                    let value = self.stack.pop().unwrap_or(Value::Unit);
                    match self.handle_throw(depth, value) {
                        Ok(()) => {}
                        Err(value) => return Err(VmFault::Throw(value)),
                    }
                }

                Opcode::TypeName => {
                    let value = self.stack.pop().unwrap_or(Value::Unit);
                    self.stack
                        .push(Value::String(type_name_of(&value).to_owned()));
                }

                Opcode::ToString => {
                    let value = self.stack.pop().unwrap_or(Value::Unit);
                    self.stack.push(Value::String(value.to_string()));
                }

                Opcode::BuildArray(count) => {
                    let count = count as usize;
                    let mut elements = vec![Value::Unit; self.stack.len().min(count)];
                    for slot in elements.iter_mut().rev() {
                        *slot = self.stack.pop().unwrap_or(Value::Unit);
                    }
                    self.stack.push(Value::Array(elements));
                }

                Opcode::IndexGet => {
                    let index = self.stack.pop().unwrap_or(Value::Unit);
                    let object = self.stack.pop().unwrap_or(Value::Unit);
                    match (object, index) {
                        (Value::Array(elements), Value::Integer(i)) => {
                            match elements.get(i as usize).cloned() {
                                Some(element) => self.stack.push(element),
                                None => {
                                    let value = Value::String(format!(
                                        "index {i} out of bounds for length {}",
                                        elements.len()
                                    ));
                                    match self.handle_throw(depth, value) {
                                        Ok(()) => {}
                                        Err(value) => return Err(VmFault::Throw(value)),
                                    }
                                }
                            }
                        }
                        // Enums expose payload arguments positionally, which
                        // is also how match patterns destructure them.
                        (Value::Enum { args, .. }, Value::Integer(i)) => {
                            match args.get(i as usize).cloned() {
                                Some(element) => self.stack.push(element),
                                None => {
                                    let value =
                                        Value::String(format!("payload index {i} out of bounds"));
                                    match self.handle_throw(depth, value) {
                                        Ok(()) => {}
                                        Err(value) => return Err(VmFault::Throw(value)),
                                    }
                                }
                            }
                        }
                        (object, index) => {
                            let value =
                                Value::String(format!("cannot index {object} with {index}"));
                            match self.handle_throw(depth, value) {
                                Ok(()) => {}
                                Err(value) => return Err(VmFault::Throw(value)),
                            }
                        }
                    }
                }

                Opcode::IndexSet => {
                    let value = self.stack.pop().unwrap_or(Value::Unit);
                    let index = self.stack.pop().unwrap_or(Value::Unit);
                    let object = self.stack.pop().unwrap_or(Value::Unit);
                    match (object, index) {
                        (Value::Array(mut elements), Value::Integer(i)) => {
                            let i = i as usize;
                            if i >= elements.len() {
                                let value = Value::String(format!(
                                    "index {i} out of bounds for length {}",
                                    elements.len()
                                ));
                                match self.handle_throw(depth, value) {
                                    Ok(()) => {}
                                    Err(value) => return Err(VmFault::Throw(value)),
                                }
                            } else {
                                elements[i] = value;
                                self.stack.push(Value::Array(elements));
                            }
                        }
                        (object, index) => {
                            let value =
                                Value::String(format!("cannot index {object} with {index}"));
                            match self.handle_throw(depth, value) {
                                Ok(()) => {}
                                Err(value) => return Err(VmFault::Throw(value)),
                            }
                        }
                    }
                }

                Opcode::NewObject(type_name) => {
                    self.stack.push(Value::Object {
                        type_name,
                        fields: Vec::new(),
                    });
                }

                Opcode::SetField(name) => {
                    let value = self.stack.pop().unwrap_or(Value::Unit);
                    let object = self.stack.pop().unwrap_or(Value::Unit);
                    match object {
                        Value::Object {
                            type_name,
                            mut fields,
                        } => {
                            match fields.iter_mut().find(|(field, _)| field == &name) {
                                Some(slot) => slot.1 = value,
                                None => fields.push((name, value)),
                            }
                            self.stack.push(Value::Object { type_name, fields });
                        }
                        object => {
                            let value =
                                Value::String(format!("cannot set field '{name}' on {object}"));
                            match self.handle_throw(depth, value) {
                                Ok(()) => {}
                                Err(value) => return Err(VmFault::Throw(value)),
                            }
                        }
                    }
                }

                Opcode::GetField(name) => {
                    let object = self.stack.pop().unwrap_or(Value::Unit);
                    match object {
                        Value::Object { fields, .. } => {
                            match fields.iter().find(|(field, _)| field == &name) {
                                Some((_, value)) => self.stack.push(value.clone()),
                                None => {
                                    let value = Value::String(format!("unknown field '{name}'"));
                                    match self.handle_throw(depth, value) {
                                        Ok(()) => {}
                                        Err(value) => return Err(VmFault::Throw(value)),
                                    }
                                }
                            }
                        }
                        object => {
                            let value =
                                Value::String(format!("cannot read field '{name}' on {object}"));
                            match self.handle_throw(depth, value) {
                                Ok(()) => {}
                                Err(value) => return Err(VmFault::Throw(value)),
                            }
                        }
                    }
                }

                Opcode::BuildEnum {
                    type_name,
                    variant,
                    argc,
                } => {
                    let argc = argc as usize;
                    let mut args = vec![Value::Unit; self.stack.len().min(argc)];
                    for slot in args.iter_mut().rev() {
                        *slot = self.stack.pop().unwrap_or(Value::Unit);
                    }
                    self.stack.push(Value::Enum {
                        type_name,
                        variant,
                        args,
                    });
                }

                Opcode::VariantName => {
                    let value = self.stack.pop().unwrap_or(Value::Unit);
                    match value {
                        Value::Enum { variant, .. } => self.stack.push(Value::String(variant)),
                        _ => self.stack.push(Value::String(String::new())),
                    }
                }

                Opcode::PatternIndex => {
                    let index = self.stack.pop().unwrap_or(Value::Unit);
                    let object = self.stack.pop().unwrap_or(Value::Unit);
                    let element = match (object, index) {
                        (Value::Array(elements), Value::Integer(i)) => {
                            elements.get(i as usize).cloned().unwrap_or(Value::Unit)
                        }
                        (Value::Enum { args, .. }, Value::Integer(i)) => {
                            args.get(i as usize).cloned().unwrap_or(Value::Unit)
                        }
                        _ => Value::Unit,
                    };
                    self.stack.push(element);
                }

                Opcode::PatternField(name) => {
                    let object = self.stack.pop().unwrap_or(Value::Unit);
                    match object {
                        Value::Object { fields, .. } => {
                            let element = fields
                                .iter()
                                .find(|(field, _)| field == &name)
                                .map(|(_, value)| value.clone())
                                .unwrap_or(Value::Unit);
                            self.stack.push(element);
                        }
                        _ => self.stack.push(Value::Unit),
                    }
                }

                Opcode::IterNext => {
                    let index = self.stack.pop().unwrap_or(Value::Unit);
                    let iterable = self.stack.pop().unwrap_or(Value::Unit);
                    let position = match index {
                        Value::Integer(n) if n >= 0 => n as usize,
                        _ => 0,
                    };
                    // Pushes the item, then has_next, then the next index.
                    // A non-positive range simply yields nothing.
                    match iterable {
                        Value::Channel(shared) => {
                            // Streaming receive: a closed, drained channel
                            // ends the loop instead of throwing.
                            let shared = shared.clone();
                            drop(index);
                            match shared.recv() {
                                Ok(element) => {
                                    self.stack.push(element);
                                    self.stack.push(Value::Boolean(true));
                                    self.stack.push(Value::Unit);
                                }
                                Err(_) => {
                                    self.stack.push(Value::Unit);
                                    self.stack.push(Value::Boolean(false));
                                    self.stack.push(Value::Unit);
                                }
                            }
                        }
                        Value::Integer(limit) => {
                            if limit > 0 && position < limit as usize {
                                self.stack.push(Value::Integer(position as i64));
                                self.stack.push(Value::Boolean(true));
                            } else {
                                self.stack.push(Value::Unit);
                                self.stack.push(Value::Boolean(false));
                            }
                            self.stack.push(Value::Integer(position as i64 + 1));
                        }
                        Value::Array(elements) => {
                            match elements.get(position).cloned() {
                                Some(element) => {
                                    self.stack.push(element);
                                    self.stack.push(Value::Boolean(true));
                                }
                                None => {
                                    self.stack.push(Value::Unit);
                                    self.stack.push(Value::Boolean(false));
                                }
                            }
                            self.stack.push(Value::Integer(position as i64 + 1));
                        }
                        iterable => {
                            let value = Value::String(format!("{iterable} is not iterable"));
                            match self.handle_throw(depth, value) {
                                Ok(()) => {}
                                Err(value) => return Err(VmFault::Throw(value)),
                            }
                        }
                    }
                }

                Opcode::DebugSpan(s) => {
                    self.debug_spans.push(s);
                }
            }
        }
    }
}

// ── Pure helpers ──────────────────────────────────────────────────────────────

/// Short display name for a possibly qualified type
/// (`app::models#User` -> `User`).
fn short_type_name(qualified: &str) -> &str {
    qualified
        .rsplit('#')
        .next()
        .and_then(|segment| segment.rsplit("::").next())
        .unwrap_or(qualified)
}

/// A channel builtin invoked through method-call syntax
/// (`c.send(v)`, `c.recv()`, `c.close()`).
fn channel_method(shared: &ChannelShared, method: &str, args: &[Value]) -> Result<Value, String> {
    match (method, args) {
        ("send", [value]) => shared.send(value.clone()).map(|()| Value::Unit),
        ("recv", []) => shared.recv(),
        ("close", []) => {
            shared.close();
            Ok(Value::Unit)
        }
        ("send", _) => Err("channel.send takes exactly one argument".to_owned()),
        _ => Err(format!("unknown channel method '{method}'")),
    }
}

/// The NXL type name of a runtime value (mirrors `nir::type_from_name`).
/// Composite values report their nominal type name when they have one.
fn type_name_of(value: &Value) -> &str {
    match value {
        Value::Integer(_) => "Int",
        Value::Float(_) => "Float",
        Value::Boolean(_) => "Bool",
        Value::String(_) => "String",
        Value::Array(_) => "Array",
        Value::Object { type_name, .. } => type_name,
        Value::Enum { type_name, .. } => type_name,
        Value::Task(_) => "Task",
        Value::Channel(_) => "Channel",
        Value::Unit => "Void",
    }
}

fn pool_to_value(entry: &PoolEntry) -> Value {
    match entry {
        PoolEntry::Integer(n) => Value::Integer(*n),
        PoolEntry::Float(f) => Value::Float(*f),
        PoolEntry::String(s) => Value::String(s.clone()),
        PoolEntry::Boolean(b) => Value::Boolean(*b),
        PoolEntry::FunctionRef(name) => Value::String(name.clone()),
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Boolean(b) => *b,
        Value::Integer(n) => *n != 0,
        Value::Unit => false,
        _ => true,
    }
}

fn eval_unary(op: UnaryOp, v: Value) -> Result<Value, String> {
    match (op, v) {
        (UnaryOp::Neg, Value::Integer(n)) => Ok(Value::Integer(-n)),
        (UnaryOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        (UnaryOp::Not, Value::Boolean(b)) => Ok(Value::Boolean(!b)),
        (op, v) => Err(format!("type error: {op:?} cannot be applied to {v:?}")),
    }
}

fn eval_binary(op: BinaryOp, lhs: Value, rhs: Value) -> Result<Value, String> {
    match (op, lhs, rhs) {
        // Integer arithmetic
        (BinaryOp::Add, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
        (BinaryOp::Sub, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
        (BinaryOp::Mul, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
        (BinaryOp::Div, Value::Integer(a), Value::Integer(b)) => {
            if b == 0 {
                Err("division by zero".to_owned())
            } else {
                Ok(Value::Integer(a / b))
            }
        }
        (BinaryOp::Rem, Value::Integer(a), Value::Integer(b)) => {
            if b == 0 {
                Err("remainder by zero".to_owned())
            } else {
                Ok(Value::Integer(a % b))
            }
        }
        // Float arithmetic
        (BinaryOp::Add, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (BinaryOp::Sub, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (BinaryOp::Mul, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (BinaryOp::Div, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        // String concatenation
        (BinaryOp::Add, Value::String(a), Value::String(b)) => Ok(Value::String(a + &b)),
        // Equality is total and strict (like `===`): values of different
        // runtime tags are never equal instead of raising a type error.
        // This keeps match-pattern fallthrough sound on dynamic values.
        (BinaryOp::Eq, Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a == b)),
        (BinaryOp::Ne, Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a != b)),
        (BinaryOp::Lt, Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a < b)),
        (BinaryOp::Le, Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a <= b)),
        (BinaryOp::Gt, Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a > b)),
        (BinaryOp::Ge, Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a >= b)),
        // Comparisons — float
        (BinaryOp::Eq, Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a == b)),
        (BinaryOp::Ne, Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a != b)),
        (BinaryOp::Lt, Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a < b)),
        (BinaryOp::Le, Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a <= b)),
        (BinaryOp::Gt, Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a > b)),
        (BinaryOp::Ge, Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a >= b)),
        // Comparisons — string
        (BinaryOp::Eq, Value::String(a), Value::String(b)) => Ok(Value::Boolean(a == b)),
        (BinaryOp::Ne, Value::String(a), Value::String(b)) => Ok(Value::Boolean(a != b)),
        // Comparisons — composite values compare structurally.
        (BinaryOp::Eq, Value::Array(a), Value::Array(b)) => Ok(Value::Boolean(a == b)),
        (BinaryOp::Ne, Value::Array(a), Value::Array(b)) => Ok(Value::Boolean(a != b)),
        (
            BinaryOp::Eq,
            Value::Object {
                type_name: a_type,
                fields: a_fields,
            },
            Value::Object {
                type_name: b_type,
                fields: b_fields,
            },
        ) => Ok(Value::Boolean(a_type == b_type && a_fields == b_fields)),
        (
            BinaryOp::Ne,
            Value::Object {
                type_name: a_type,
                fields: a_fields,
            },
            Value::Object {
                type_name: b_type,
                fields: b_fields,
            },
        ) => Ok(Value::Boolean(a_type != b_type || a_fields != b_fields)),
        (
            BinaryOp::Eq,
            Value::Enum {
                type_name: a_type,
                variant: a_variant,
                args: a_args,
            },
            Value::Enum {
                type_name: b_type,
                variant: b_variant,
                args: b_args,
            },
        ) => Ok(Value::Boolean(
            a_type == b_type && a_variant == b_variant && a_args == b_args,
        )),
        (
            BinaryOp::Ne,
            Value::Enum {
                type_name: a_type,
                variant: a_variant,
                args: a_args,
            },
            Value::Enum {
                type_name: b_type,
                variant: b_variant,
                args: b_args,
            },
        ) => Ok(Value::Boolean(
            a_type != b_type || a_variant != b_variant || a_args != b_args,
        )),
        // Comparisons — bool
        (BinaryOp::Eq, Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(a == b)),
        (BinaryOp::Ne, Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(a != b)),
        // Logical
        (BinaryOp::And, Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(a && b)),
        (BinaryOp::Or, Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(a || b)),
        // Unit equals itself.
        (BinaryOp::Eq, Value::Unit, Value::Unit) => Ok(Value::Boolean(true)),
        (BinaryOp::Ne, Value::Unit, Value::Unit) => Ok(Value::Boolean(false)),
        // Tasks and channels compare by identity.
        (BinaryOp::Eq, Value::Task(a), Value::Task(b)) => {
            Ok(Value::Boolean(Arc::ptr_eq(&a.0, &b.0)))
        }
        (BinaryOp::Ne, Value::Task(a), Value::Task(b)) => {
            Ok(Value::Boolean(!Arc::ptr_eq(&a.0, &b.0)))
        }
        (BinaryOp::Eq, Value::Channel(a), Value::Channel(b)) => {
            Ok(Value::Boolean(Arc::ptr_eq(&a.0, &b.0)))
        }
        (BinaryOp::Ne, Value::Channel(a), Value::Channel(b)) => {
            Ok(Value::Boolean(!Arc::ptr_eq(&a.0, &b.0)))
        }
        // Cross-tag equality is false rather than an error (strict identity).
        (BinaryOp::Eq, _, _) => Ok(Value::Boolean(false)),
        (BinaryOp::Ne, _, _) => Ok(Value::Boolean(true)),
        // Bitwise integer
        (BinaryOp::BitAnd, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a & b)),
        (BinaryOp::BitOr, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a | b)),
        (BinaryOp::BitXor, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a ^ b)),
        (BinaryOp::Shl, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a << b)),
        (BinaryOp::Shr, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a >> b)),
        (op, lhs, rhs) => Err(format!(
            "type error: {op:?} cannot be applied to {lhs:?} and {rhs:?}"
        )),
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Compile a NIR module to bytecode and execute it in the VM.
pub fn execute(module: &Module) -> Result<VmResult, Diagnostic> {
    let program = compile(module)?;
    execute_program(&program)
}

/// Execute a pre-compiled program in the VM.
pub fn execute_program(program: &CompiledProgram) -> Result<VmResult, Diagnostic> {
    let output = Arc::new(Mutex::new(Vec::new()));
    let mut vm = Vm::with_output(Arc::new(program.clone()), Arc::clone(&output));
    let return_value = vm.run().map_err(|fault| match fault {
        // An uncaught exception keeps the historical message shape.
        VmFault::Throw(value) => Diagnostic::error(
            "NXL9001",
            format!("runtime error: {value}"),
            Span::default(),
        ),
        VmFault::Fatal(message) => Diagnostic::error("NXL9001", message, Span::default()),
    })?;
    // Joined tasks are done writing (leaked threads on error paths may
    // still hold the Arc, so clone under the lock instead of unwrapping).
    let output = output.lock().map(|guard| guard.clone()).unwrap_or_default();
    Ok(VmResult::with_spans(output, return_value, vm.debug_spans))
}

/// Compile a NXL source file and execute it in the VM.
pub fn run(source: &SourceFile) -> Result<VmResult, Diagnostic> {
    let module = crate::compile_to_nir(source)?;
    execute(&module)
}

// ── Display ───────────────────────────────────────────────────────────────────

impl fmt::Display for CompiledProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "; NXL bytecode v{} — {}",
            self.format_version, self.source_name
        )?;
        writeln!(f, "; entry: {}", self.entry_point)?;
        for chunk in &self.chunks {
            writeln!(
                f,
                "\nfn {} (arity={}, locals={}):",
                chunk.name, chunk.arity, chunk.local_count
            )?;
            for (i, op) in chunk.code.iter().enumerate() {
                writeln!(f, "  {:04}  {op:?}", i)?;
            }
        }
        Ok(())
    }
}
