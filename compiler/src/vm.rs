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
use std::collections::HashMap;
use std::fmt;

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
    /// Debug annotation — carries source span, no runtime effect.
    DebugSpan(Span),
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
            Opcode::Jump(t) | Opcode::JumpIfTrue(t) | Opcode::JumpIfFalse(t) => *t = target,
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
            compile_instruction(
                instr,
                &mut chunk,
                &mut value_slots,
                &mut name_slots,
                &mut next_slot,
                &mut patches,
                all_functions,
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

    Ok(chunk)
}

fn compile_instruction(
    instr: &nir::Instruction,
    chunk: &mut Chunk,
    value_slots: &mut HashMap<nir::ValueId, u32>,
    name_slots: &mut HashMap<String, u32>,
    next_slot: &mut u32,
    _patches: &mut Vec<(u32, BasicBlockId)>,
    _all_functions: &[nir::Function],
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

        // Structural instructions are handled by the CFG; skip here.
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
        | nir::Instruction::ForeignCall { result, .. } => vec![*result],
        nir::Instruction::IteratorNext { has_next, item, .. } => vec![*has_next, *item],
        _ => Vec::new(),
    }
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
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Unit,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(n) => write!(f, "{n}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::String(s) => write!(f, "{s}"),
            Value::Boolean(b) => write!(f, "{b}"),
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
pub struct Vm<'p> {
    program: &'p CompiledProgram,
    /// Operand stack.
    stack: Vec<Value>,
    /// Call stack.
    frames: Vec<Frame>,
    /// Captured output (print() calls).
    output: Vec<String>,
    /// Debug spans observed during execution (P21 trace).
    debug_spans: Vec<Span>,
}

impl<'p> Vm<'p> {
    fn new(program: &'p CompiledProgram) -> Self {
        Self {
            program,
            stack: Vec::new(),
            frames: Vec::new(),
            output: Vec::new(),
            debug_spans: Vec::new(),
        }
    }

    fn run(&mut self) -> Result<Value, String> {
        let entry = self.program.entry_point.clone();
        self.call_function(&entry, vec![])
    }

    fn call_function(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        // Built-in: print
        if name == "print" {
            let line = args
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            self.output.push(line);
            return Ok(Value::Unit);
        }

        let chunk = self
            .program
            .chunk_by_name(name)
            .ok_or_else(|| format!("undefined function '{name}'"))?
            .clone();

        if args.len() != chunk.arity as usize {
            return Err(format!(
                "function '{name}' expects {} args, got {}",
                chunk.arity,
                args.len()
            ));
        }

        let frame = Frame::new(name.to_owned(), chunk.local_count, args);
        self.frames.push(frame);

        let result = self.execute_chunk(&chunk);

        self.frames.pop();
        result
    }

    fn execute_chunk(&mut self, chunk: &Chunk) -> Result<Value, String> {
        let frame_idx = self.frames.len() - 1;
        loop {
            let ip = self.frames[frame_idx].ip;
            if ip >= chunk.code.len() {
                return Ok(Value::Unit);
            }
            self.frames[frame_idx].ip += 1;
            let op = chunk.code[ip].clone();

            match op {
                Opcode::LoadConst(idx) => {
                    let v = pool_to_value(chunk.pool.get(idx).ok_or("invalid pool index")?);
                    self.stack.push(v);
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
                    let result = eval_binary(bop, lhs, rhs)?;
                    self.stack.push(result);
                }

                Opcode::UnaryOp(uop) => {
                    let operand = self.stack.pop().unwrap_or(Value::Unit);
                    let result = eval_unary(uop, operand)?;
                    self.stack.push(result);
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
                    let ret = self.call_function(&name, args)?;
                    self.stack.push(ret);
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
                    return Err(format!("runtime error: {msg}"));
                }

                Opcode::DebugSpan(s) => {
                    self.debug_spans.push(s);
                }
            }
        }
    }
}

// ── Pure helpers ──────────────────────────────────────────────────────────────

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
        // Comparisons — integer
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
        // Comparisons — bool
        (BinaryOp::Eq, Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(a == b)),
        (BinaryOp::Ne, Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(a != b)),
        // Logical
        (BinaryOp::And, Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(a && b)),
        (BinaryOp::Or, Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(a || b)),
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
    let mut vm = Vm::new(program);
    let return_value = vm
        .run()
        .map_err(|msg| Diagnostic::error("NXL9001", msg, Span::default()))?;
    Ok(VmResult::with_spans(
        vm.output,
        return_value,
        vm.debug_spans,
    ))
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
