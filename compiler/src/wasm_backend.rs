//! NXL → WebAssembly (core binary format) backend.
//!
//! Scope (v1): scalar programs — `Int` becomes `i64`, `Float` becomes `f64`,
//! `Bool` becomes `i32` — plus user functions, flattened control flow,
//! `print`, and all 23 `std.math` builtins. Everything else (strings,
//! arrays, structs, enums, closures, channels, tasks, exceptions, FFI,
//! dynamic dispatch) is refused at compile time with a loud NXL2007
//! diagnostic rather than silently mis-emitted.
//!
//! Execution contract: the module exports `_start` (no parameters, no
//! results) and imports its host helpers from `env`. The reference host that
//! the parity tests use lives in `compiler/tests/wasm_backend_tests.rs`; the
//! full contract and documented divergences are in `docs/23-webassembly.md`.
//!
//! Documented divergences from the VM:
//! - integer division/remainder by zero traps (as does `i64::MIN / -1`)
//!   instead of raising a catchable VM error;
//! - integer overflow wraps (native WASM semantics) where the VM raises a
//!   catchable "integer overflow" error;
//! - float-to-string formatting follows the host's JavaScript formatting,
//!   exactly like the JavaScript backend already documents;
//! - transcendentals come from the host's math library and may differ from
//!   Rust's results at ULP level, again like the JavaScript backend.
//!
//! The binary encoder is hand-rolled (LEB128 plus the seven sections this
//! subset needs) instead of pulling in a WASM crate; see the deviation note
//! in docs/23-webassembly.md.

use std::collections::{HashMap, HashSet};

use crate::ast::{BinaryOperator, UnaryOperator};
use crate::backend::{analyze_loops, block_index, LoopInfo};
use crate::diagnostic::Diagnostic;
use crate::nir::{
    BasicBlockId, Constant, Function, Instruction, Module, Terminator, Type, ValueId,
};
use crate::source::{SourceFile, Span};

// ── Public artifact and entry points ──────────────────────────────────────────

/// A compiled WebAssembly module.
#[derive(Clone, Debug, PartialEq)]
pub struct WasmArtifact {
    /// Logical target identifier, always `"wasm"`.
    pub target: String,
    /// The raw WASM binary (magic header included), ready to write to disk.
    pub bytes: Vec<u8>,
}

/// Compile a NXL source file to a WebAssembly artifact.
pub fn compile_wasm(source: &SourceFile) -> Result<WasmArtifact, Diagnostic> {
    let module = crate::compile_to_nir(source)?;
    compile_wasm_module(&module)
}

/// Compile an already-lowered module (possibly multi-file) to WebAssembly.
pub fn compile_wasm_module(module: &Module) -> Result<WasmArtifact, Diagnostic> {
    // The entry wrapper takes no arguments; refuse an entry point with
    // parameters instead of dropping them silently.
    if let Some(entry) = &module.entry_point {
        if let Some(function) = module.functions.iter().find(|f| &f.name == entry) {
            if !function.parameters.is_empty() {
                return Err(Diagnostic::error(
                    "NXL2007",
                    format!(
                        "entry point '{entry}' cannot take parameters in the WebAssembly backend; \
                         run it with `nxl run` (VM)"
                    ),
                    function.debug.function_span,
                ));
            }
        }
    }

    // Pass 1 — validate every function and collect its local types. A
    // refusal anywhere fails the whole compile before any bytes are built.
    let mut analyses = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        analyses.push(analyze_function(module, function)?);
    }

    // Pass 2 — the import list: `print` helpers first (fixed order), then
    // the remaining host helpers alphabetically, so output is deterministic.
    let mut used_imports = HashSet::new();
    for analysis in &analyses {
        used_imports.extend(analysis.imports.iter().copied());
    }
    let import_order = ordered_imports(&used_imports);
    let import_indices: HashMap<HostImport, u32> = import_order
        .iter()
        .enumerate()
        .map(|(i, import)| (*import, i as u32))
        .collect();

    // Pass 3 — the function index space: imports first, then every module
    // function in declaration order, then `_start`.
    let func_indices: HashMap<String, u32> = module
        .functions
        .iter()
        .enumerate()
        .map(|(i, function)| (function.name.clone(), i as u32))
        .collect();
    let start_index = import_order.len() as u32 + module.functions.len() as u32;

    // Pass 4 — the type section, interned in first-use order.
    let mut types: Vec<WasmFuncType> = Vec::new();
    let import_type_ids: Vec<u32> = import_order
        .iter()
        .map(|import| intern_type(&mut types, import.functype()))
        .collect();
    let mut function_type_ids = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        function_type_ids.push(intern_type(&mut types, function_type(function)?));
    }
    let start_type_id = intern_type(&mut types, WasmFuncType::default());

    // Pass 5 — function bodies.
    let mut bodies = Vec::with_capacity(module.functions.len());
    for (function, analysis) in module.functions.iter().zip(&analyses) {
        bodies.push(emit_function(
            function,
            analysis,
            &import_indices,
            &func_indices,
        )?);
    }

    // The `_start` wrapper: call the entry point (or nothing) and drop its
    // result when it has one. A module without an entry still exports an
    // empty `_start` so hosts always have a uniform hook.
    let mut start_body = Vec::new();
    // Every code entry begins with the local-declaration vector; `_start`
    // has no locals, but the zero group-count byte is still mandatory.
    push_uleb(&mut start_body, 0);
    if let Some(entry) = &module.entry_point {
        if let Some((index, function)) = module
            .functions
            .iter()
            .enumerate()
            .find(|(_, function)| &function.name == entry)
        {
            let call_index = import_order.len() as u32 + index as u32;
            push_op(&mut start_body, Op::CALL);
            push_uleb(&mut start_body, call_index as u64);
            if return_type(function).is_some() {
                push_op(&mut start_body, Op::DROP);
            }
        }
    }
    push_op(&mut start_body, Op::END);

    Ok(assemble(
        &types,
        &import_order,
        &import_type_ids,
        &function_type_ids,
        start_type_id,
        start_index,
        &bodies,
        &start_body,
    ))
}

// ── Value and function types ──────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WasmVal {
    I32,
    I64,
    F64,
}

impl WasmVal {
    fn byte(self) -> u8 {
        match self {
            WasmVal::I32 => 0x7F,
            WasmVal::I64 => 0x7E,
            WasmVal::F64 => 0x7C,
        }
    }
}

/// The WASM representation of an NXL type, or `None` for anything this
/// backend cannot hold (strings, arrays, objects, Void, Unknown, …).
fn wasm_val(ty: Type) -> Option<WasmVal> {
    match ty {
        Type::Int => Some(WasmVal::I64),
        Type::Float => Some(WasmVal::F64),
        Type::Bool => Some(WasmVal::I32),
        _ => None,
    }
}

/// The result type a function produces to its caller. `Void` and `Unknown`
/// both mean "no result", mirroring how `create_function` terminates them
/// with `Return { value: None }`.
fn return_type(function: &Function) -> Option<WasmVal> {
    match function.return_type {
        Type::Void | Type::Unknown => None,
        other => wasm_val(other),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WasmFuncType {
    params: Vec<WasmVal>,
    results: Vec<WasmVal>,
}

impl Default for WasmFuncType {
    fn default() -> Self {
        Self {
            params: Vec::new(),
            results: Vec::new(),
        }
    }
}

impl WasmFuncType {
    fn of_function(function: &Function) -> Result<Self, Diagnostic> {
        let mut params = Vec::with_capacity(function.parameters.len());
        for (name, ty) in &function.parameters {
            match wasm_val(*ty) {
                Some(val) => params.push(val),
                None => return Err(non_scalar("parameter", name, function)),
            }
        }
        let results = return_type(function).into_iter().collect();
        Ok(Self { params, results })
    }
}

fn function_type(function: &Function) -> Result<WasmFuncType, Diagnostic> {
    WasmFuncType::of_function(function)
}

fn non_scalar(what: &str, name: &str, function: &Function) -> Diagnostic {
    Diagnostic::error(
        "NXL2007",
        format!(
            "{what} '{name}' of function '{}' has a type the WebAssembly backend cannot hold; \
             only Int, Float and Bool are supported — run it with `nxl run` (VM) or build the \
             JavaScript target",
            function.name
        ),
        function.debug.function_span,
    )
}

fn intern_type(types: &mut Vec<WasmFuncType>, ty: WasmFuncType) -> u32 {
    if let Some(index) = types.iter().position(|existing| *existing == ty) {
        return index as u32;
    }
    types.push(ty);
    (types.len() - 1) as u32
}

// ── Host imports ──────────────────────────────────────────────────────────────

/// Helpers the module imports from the host. `print`'s helpers format one
/// value each and `Nl` terminates the line; the host tracks spacing so the
/// output matches the VM's `println!("{}", args.join(" "))` exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum HostImport {
    PrintI64,
    PrintF64,
    PrintBool,
    Nl,
    Math(MathFn),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MathFn {
    Cbrt,
    Exp,
    Ln,
    Log2,
    Log10,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Pow,
}

impl MathFn {
    fn field(self) -> &'static str {
        match self {
            MathFn::Cbrt => "math_cbrt",
            MathFn::Exp => "math_exp",
            MathFn::Ln => "math_ln",
            MathFn::Log2 => "math_log2",
            MathFn::Log10 => "math_log10",
            MathFn::Sin => "math_sin",
            MathFn::Cos => "math_cos",
            MathFn::Tan => "math_tan",
            MathFn::Asin => "math_asin",
            MathFn::Acos => "math_acos",
            MathFn::Atan => "math_atan",
            MathFn::Atan2 => "math_atan2",
            MathFn::Pow => "math_pow",
        }
    }

    /// Host builtins that take two Floats rather than one.
    fn takes_two(self) -> bool {
        matches!(self, MathFn::Atan2 | MathFn::Pow)
    }
}

impl HostImport {
    fn field(self) -> &'static str {
        match self {
            HostImport::PrintI64 => "print_i64",
            HostImport::PrintF64 => "print_f64",
            HostImport::PrintBool => "print_bool",
            HostImport::Nl => "nl",
            HostImport::Math(math) => math.field(),
        }
    }

    fn functype(self) -> WasmFuncType {
        match self {
            HostImport::PrintI64 => WasmFuncType {
                params: vec![WasmVal::I64],
                results: Vec::new(),
            },
            HostImport::PrintF64 => WasmFuncType {
                params: vec![WasmVal::F64],
                results: Vec::new(),
            },
            HostImport::PrintBool => WasmFuncType {
                params: vec![WasmVal::I32],
                results: Vec::new(),
            },
            HostImport::Nl => WasmFuncType::default(),
            HostImport::Math(math) => WasmFuncType {
                params: vec![WasmVal::F64; if math.takes_two() { 2 } else { 1 }],
                results: vec![WasmVal::F64],
            },
        }
    }
}

/// Deterministic import order: the `print` helpers first (so output-related
/// indices are stable), then everything else alphabetically by field name.
fn ordered_imports(used: &HashSet<HostImport>) -> Vec<HostImport> {
    let print_group = [
        HostImport::PrintI64,
        HostImport::PrintF64,
        HostImport::PrintBool,
        HostImport::Nl,
    ];
    let mut imports: Vec<HostImport> = print_group
        .into_iter()
        .filter(|import| used.contains(import))
        .collect();
    let mut rest: Vec<HostImport> = used
        .iter()
        .copied()
        .filter(|import| !print_group.contains(import))
        .collect();
    rest.sort_by_key(|import| import.field());
    imports.extend(rest);
    imports
}

// ── Per-function analysis ─────────────────────────────────────────────────────

/// Everything emission needs to know about one function: the WASM type of
/// every value, the established type of every NXL name, the host imports it
/// uses, and which value slots alias the incoming formals.
struct FnAnalysis {
    value_types: HashMap<ValueId, WasmVal>,
    name_types: HashMap<String, WasmVal>,
    imports: HashSet<HostImport>,
    /// Parameter ValueIds that alias their formal local directly (the VM
    /// aliases the parameter's value slot the same way).
    formal_aliases: HashMap<ValueId, u32>,
}

impl FnAnalysis {
    fn new() -> Self {
        Self {
            value_types: HashMap::new(),
            name_types: HashMap::new(),
            imports: HashSet::new(),
            formal_aliases: HashMap::new(),
        }
    }
}

fn unsupported(name: &str, span: Span, why: &str) -> Diagnostic {
    Diagnostic::error(
        "NXL2007",
        format!(
            "{name}: {why} — the WebAssembly backend only supports scalar code \
             (Int, Float, Bool)"
        ),
        span,
    )
}

fn vm_only(name: &str, span: Span, how: &str) -> Diagnostic {
    Diagnostic::error(
        "NXL2007",
        format!("'{name}' needs the VM (`nxl run`); the WebAssembly backend {how}"),
        span,
    )
}

/// Validate one function: register a WASM type for every value it defines,
/// check name binding types, and refuse any instruction outside the scalar
/// subset. Runs before any code is emitted so refusals never leave a
/// half-built module behind.
fn analyze_function(module: &Module, function: &Function) -> Result<FnAnalysis, Diagnostic> {
    let mut analysis = FnAnalysis::new();

    // Parameters arrive as locals 0..n-1; their names are pre-bound so a
    // later sequential rebinding assigns rather than re-declares.
    for (name, ty) in &function.parameters {
        match wasm_val(*ty) {
            Some(val) => {
                analysis.name_types.insert(name.clone(), val);
            }
            None => return Err(non_scalar("parameter", name, function)),
        }
    }
    // The function's own return type must be representable too (or absent).
    if function.return_type != Type::Void
        && function.return_type != Type::Unknown
        && return_type(function).is_none()
    {
        return Err(Diagnostic::error(
            "NXL2007",
            format!(
                "function '{}' returns a type the WebAssembly backend cannot hold; \
                 only Int, Float and Bool are supported — run it with `nxl run` (VM) or build \
                 the JavaScript target",
                function.name
            ),
            function.debug.function_span,
        ));
    }

    for block in &function.basic_blocks {
        for instruction in &block.instructions {
            analyze_instruction(module, instruction, function, &mut analysis)?;
        }
        match &block.terminator {
            Terminator::Branch { condition, .. } => {
                ensure_condition(&analysis, *condition, function)?;
            }
            Terminator::Return { value, span } => {
                analyze_return(*value, *span, function, &mut analysis)?;
            }
            Terminator::Jump { .. } | Terminator::Unreachable { .. } => {}
        }
    }
    Ok(analysis)
}

fn ensure_condition(
    analysis: &FnAnalysis,
    condition: ValueId,
    function: &Function,
) -> Result<(), Diagnostic> {
    match analysis.value_types.get(&condition) {
        Some(WasmVal::I32) => Ok(()),
        _ => Err(Diagnostic::error(
            "NXL2007",
            format!(
                "control flow in function '{}' branches on a value that is not a Bool; \
                 the WebAssembly backend only supports Bool conditions",
                function.name
            ),
            function.debug.function_span,
        )),
    }
}

fn analyze_return(
    value: Option<ValueId>,
    span: Span,
    function: &Function,
    analysis: &mut FnAnalysis,
) -> Result<(), Diagnostic> {
    match (return_type(function), value) {
        (None, None) => Ok(()),
        (Some(expected), Some(id)) => match analysis.value_types.get(&id) {
            Some(actual) if *actual == expected => Ok(()),
            Some(actual) => Err(Diagnostic::error(
                "NXL2007",
                format!(
                    "return value of function '{}' has type {:?} but the function declares a \
                     different result type; the WebAssembly backend refuses the mismatch",
                    function.name, actual
                ),
                span,
            )),
            None => Err(Diagnostic::error(
                "NXL2007",
                format!(
                    "return value in function '{}' has a type the WebAssembly backend cannot \
                     hold; only Int, Float and Bool are supported",
                    function.name
                ),
                span,
            )),
        },
        (None, Some(_)) => Err(Diagnostic::error(
            "NXL2007",
            format!(
                "function '{}' returns nothing but a return statement supplies a value; \
                 the WebAssembly backend refuses the shape",
                function.name
            ),
            span,
        )),
        (Some(_), None) => Err(Diagnostic::error(
            "NXL2007",
            format!(
                "function '{}' must return a value but a return statement supplies none; \
                 the WebAssembly backend refuses the shape",
                function.name
            ),
            span,
        )),
    }
}

/// The type of a value read at `id`, or a refusal when nothing registered
/// one (which means the defining instruction was outside the subset).
fn value_type_of(analysis: &FnAnalysis, id: ValueId, span: Span) -> Result<WasmVal, Diagnostic> {
    analysis.value_types.get(&id).copied().ok_or_else(|| {
        Diagnostic::error(
            "NXL2007",
            format!(
                "value v{id} has a type the WebAssembly backend cannot hold; \
                 only Int, Float and Bool are supported"
            ),
            span,
        )
    })
}

fn define(analysis: &mut FnAnalysis, result: ValueId, val: WasmVal) {
    analysis.value_types.insert(result, val);
}

/// Bind a name at a `Store`: the first binding fixes the name's type and
/// aliases the value's local; every later binding in the same function must
/// have the same type (a shape the VM and JS backend allow but this backend
/// cannot represent with one typed local).
fn bind_name(
    analysis: &mut FnAnalysis,
    name: &str,
    value: ValueId,
    span: Span,
) -> Result<(), Diagnostic> {
    let val = value_type_of(analysis, value, span)?;
    match analysis.name_types.get(name) {
        None => {
            analysis.name_types.insert(name.to_owned(), val);
            Ok(())
        }
        Some(existing) if *existing == val => Ok(()),
        Some(existing) => Err(Diagnostic::error(
            "NXL2007",
            format!(
                "'{name}' is rebound from {existing:?} to {val:?} in one function; \
                 the WebAssembly backend holds each name in a single typed local — \
                 run it with `nxl run` (VM) or build the JavaScript target"
            ),
            span,
        )),
    }
}

fn analyze_instruction(
    module: &Module,
    instruction: &Instruction,
    function: &Function,
    analysis: &mut FnAnalysis,
) -> Result<(), Diagnostic> {
    match instruction {
        Instruction::Parameter {
            result, index, ty, ..
        } => {
            let val = wasm_val(*ty).ok_or_else(|| {
                let name = function
                    .parameters
                    .get(*index)
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("(unknown)");
                non_scalar("parameter", name, function)
            })?;
            define(analysis, *result, val);
            analysis
                .formal_aliases
                .insert(*result, u32::try_from(*index).unwrap_or(u32::MAX));
        }

        Instruction::Constant { result, value, .. } => {
            let val = match value {
                Constant::Integer(_) => WasmVal::I64,
                Constant::Float(_) => WasmVal::F64,
                Constant::Boolean(_) => WasmVal::I32,
                Constant::String(_) => {
                    return Err(unsupported(
                        "string constant",
                        function.debug.function_span,
                        "string values need the VM (`nxl run`) or the JavaScript backend",
                    ));
                }
            };
            define(analysis, *result, val);
        }

        Instruction::Load {
            result,
            name,
            ty,
            span,
        } => {
            // For-loop bindings reach here with `Unknown` because the lowerer
            // types the iterator item late; the name's established type is
            // the authority in that case.
            let val = match wasm_val(*ty) {
                Some(val) => val,
                None => analysis.name_types.get(name).copied().ok_or_else(|| {
                    Diagnostic::error(
                        "NXL2007",
                        format!(
                            "cannot determine a scalar type for '{name}'; \
                             the WebAssembly backend only supports Int, Float and Bool"
                        ),
                        *span,
                    )
                })?,
            };
            define(analysis, *result, val);
        }

        Instruction::Store { name, value, span } => {
            bind_name(analysis, name, *value, *span)?;
        }

        Instruction::Assign { name, value, span } => {
            if !analysis.name_types.contains_key(name) {
                return Err(Diagnostic::error(
                    "NXL2007",
                    format!(
                        "assignment to '{name}' before it is bound; \
                         the WebAssembly backend refuses the shape"
                    ),
                    *span,
                ));
            }
            bind_name(analysis, name, *value, *span)?;
        }

        Instruction::Unary {
            result,
            operator,
            operand,
            span,
            ..
        } => {
            let val = value_type_of(analysis, *operand, *span)?;
            let result_type = match operator {
                UnaryOperator::Negate => match val {
                    WasmVal::I64 | WasmVal::F64 => val,
                    _ => {
                        return Err(unsupported(
                            "unary negation",
                            *span,
                            "negation needs an Int or Float operand",
                        ))
                    }
                },
                UnaryOperator::Not => {
                    if val != WasmVal::I32 {
                        return Err(unsupported(
                            "logical not",
                            *span,
                            "'not' needs a Bool operand",
                        ));
                    }
                    WasmVal::I32
                }
                UnaryOperator::Reference | UnaryOperator::Borrow => {
                    return Err(unsupported(
                        "borrow operator",
                        *span,
                        "references have no scalar representation",
                    ))
                }
            };
            define(analysis, *result, result_type);
        }

        Instruction::Binary {
            result,
            operator,
            left,
            right,
            span,
            ..
        } => {
            let lhs = value_type_of(analysis, *left, *span)?;
            let rhs = value_type_of(analysis, *right, *span)?;
            if lhs != rhs {
                return Err(unsupported(
                    "mixed-type binary operation",
                    *span,
                    "binary operands must share one scalar type",
                ));
            }
            use BinaryOperator as Op;
            let result_type = match operator {
                Op::Add | Op::Subtract | Op::Multiply | Op::Divide | Op::Remainder => {
                    if !matches!(lhs, WasmVal::I64 | WasmVal::F64) {
                        return Err(unsupported(
                            "arithmetic",
                            *span,
                            "arithmetic needs Int or Float operands",
                        ));
                    }
                    if *operator == Op::Remainder && lhs == WasmVal::F64 {
                        return Err(unsupported(
                            "float remainder",
                            *span,
                            "the VM raises a runtime type error for Remainder on Float \
                             (NXL9001), so this backend refuses it at compile time \
                             instead of computing a value `nxl run` would reject",
                        ));
                    }
                    lhs
                }
                Op::Equal | Op::NotEqual => WasmVal::I32,
                Op::Less | Op::LessEqual | Op::Greater | Op::GreaterEqual => {
                    if !matches!(lhs, WasmVal::I64 | WasmVal::F64) {
                        return Err(unsupported(
                            "comparison",
                            *span,
                            "ordering comparisons need Int or Float operands",
                        ));
                    }
                    WasmVal::I32
                }
                Op::And | Op::Or => {
                    if lhs != WasmVal::I32 {
                        return Err(unsupported(
                            "logical operation",
                            *span,
                            "'and'/'or' need Bool operands",
                        ));
                    }
                    WasmVal::I32
                }
                Op::BitAnd | Op::BitOr | Op::BitXor | Op::ShiftLeft | Op::ShiftRight => {
                    if lhs != WasmVal::I64 {
                        return Err(unsupported(
                            "bit operation",
                            *span,
                            "bitwise operators need Int operands",
                        ));
                    }
                    WasmVal::I64
                }
                Op::In | Op::Is | Op::Range => {
                    return Err(unsupported(
                        "operator",
                        *span,
                        "membership and type tests need values beyond Int, Float and Bool",
                    ));
                }
            };
            define(analysis, *result, result_type);
        }

        Instruction::Call {
            result,
            callee,
            arguments,
            return_type: call_return,
            span,
            ..
        } => {
            analyze_call(
                module,
                analysis,
                *result,
                callee,
                arguments,
                *call_return,
                *span,
            )?;
        }

        Instruction::IteratorNext {
            has_next,
            item,
            iterable,
            index,
            span,
        } => {
            // Only Int ranges are iterable here; arrays and channels are
            // refused. The VM's `IterNext` for a range computes
            // `has_next = index < limit`, `item = index`, `index += 1`.
            if value_type_of(analysis, *iterable, *span)? != WasmVal::I64 {
                return Err(unsupported(
                    "iteration",
                    *span,
                    "only an Int upper bound can be iterated (arrays and channels need the VM \
                     or the JavaScript backend)",
                ));
            }
            if value_type_of(analysis, *index, *span)? != WasmVal::I64 {
                return Err(unsupported(
                    "iteration counter",
                    *span,
                    "the loop counter must be an Int",
                ));
            }
            define(analysis, *has_next, WasmVal::I32);
            define(analysis, *item, WasmVal::I64);
        }

        Instruction::Borrow {
            result,
            value,
            span,
            ..
        } => {
            // Scalar borrows are plain copies, exactly as in the JS backend.
            let val = value_type_of(analysis, *value, *span)?;
            define(analysis, *result, val);
        }

        Instruction::Drop { .. } => {}

        Instruction::Return { value, span } => {
            analyze_return(*value, *span, function, analysis)?;
        }

        // The CfgBuilder flattens structured control flow; these only survive
        // inside `Try` regions, and `Try` itself is refused below, so seeing
        // them at the top level of a block is a shape this backend refuses
        // loudly rather than silently dropping (the JS backend ignores them
        // defensively).
        Instruction::If { span, .. }
        | Instruction::Loop { span, .. }
        | Instruction::Break { span }
        | Instruction::Continue { span } => {
            return Err(unsupported(
                "structured control flow",
                *span,
                "flattened blocks were expected instead",
            ));
        }

        Instruction::ArrayNew { span, .. }
        | Instruction::IndexGet { span, .. }
        | Instruction::IndexSet { span, .. }
        | Instruction::PatternIndex { span, .. } => {
            return Err(unsupported(
                "array operation",
                *span,
                "arrays need the VM (`nxl run`) or the JavaScript backend",
            ));
        }

        Instruction::StructNew { span, .. }
        | Instruction::FieldGet { span, .. }
        | Instruction::FieldSet { span, .. }
        | Instruction::Allocate { span, .. }
        | Instruction::LoadMemory { span, .. }
        | Instruction::StoreMemory { span, .. } => {
            return Err(unsupported(
                "aggregate value",
                *span,
                "structs and reference cells need the VM (`nxl run`) or the JavaScript backend",
            ));
        }

        Instruction::EnumNew { span, .. }
        | Instruction::VariantName { span, .. }
        | Instruction::PatternField { span, .. } => {
            return Err(unsupported(
                "enum value",
                *span,
                "enums need the VM (`nxl run`) or the JavaScript backend",
            ));
        }

        Instruction::MethodCall { method, span, .. } => {
            return Err(unsupported(
                "method call",
                *span,
                &format!("method '{method}' needs the VM (`nxl run`) or the JavaScript backend"),
            ));
        }

        Instruction::MakeClosure { callee, span, .. } => {
            return Err(unsupported(
                "closure",
                *span,
                &format!("closure '{callee}' needs the VM (`nxl run`) or the JavaScript backend"),
            ));
        }

        Instruction::CallIndirect { span, .. } => {
            return Err(unsupported(
                "indirect call",
                *span,
                "function values need the VM or JavaScript",
            ));
        }

        Instruction::SpawnCall { callee, span, .. }
        | Instruction::SpawnMethod {
            method: callee,
            span,
            ..
        } => {
            return Err(vm_only(callee, *span, "cannot schedule threads"));
        }

        Instruction::CompleteTask { span, .. } | Instruction::AwaitTask { span, .. } => {
            return Err(unsupported(
                "task",
                *span,
                "concurrent tasks need the VM (`nxl run`)",
            ));
        }

        Instruction::ChannelNew { span, .. }
        | Instruction::ChannelSend { span, .. }
        | Instruction::ChannelReceive { span, .. }
        | Instruction::ChannelClose { span, .. }
        | Instruction::ActorDispatch { span, .. }
        | Instruction::CancelTask { span, .. } => {
            return Err(unsupported(
                "channel",
                *span,
                "channels and actors need the VM (`nxl run`)",
            ));
        }

        Instruction::Spawn { span, .. } | Instruction::Await { span, .. } => {
            return Err(unsupported(
                "task",
                *span,
                "concurrent tasks need the VM (`nxl run`)",
            ));
        }

        Instruction::ForeignCall { callee, span, .. } => {
            return Err(vm_only(callee, *span, "has no host ABI to call into"));
        }

        Instruction::ToString { span, .. } | Instruction::TypeName { span, .. } => {
            return Err(unsupported(
                "string conversion",
                *span,
                "string values need the VM (`nxl run`) or the JavaScript backend",
            ));
        }

        Instruction::Catch { span, .. }
        | Instruction::Throw { span, .. }
        | Instruction::PushHandler { span, .. }
        | Instruction::PopHandler { span, .. }
        | Instruction::Try { span, .. } => {
            return Err(unsupported(
                "exception handling",
                *span,
                "try/catch and throw need the VM (`nxl run`) or the JavaScript backend",
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn analyze_call(
    module: &Module,
    analysis: &mut FnAnalysis,
    result: ValueId,
    callee: &str,
    arguments: &[ValueId],
    call_return: Type,
    span: Span,
) -> Result<(), Diagnostic> {
    // Resolve argument types first: every argument must already hold a
    // scalar, otherwise its defining instruction would have been refused.
    for argument in arguments {
        value_type_of(analysis, *argument, span)?;
    }

    let result_type = if call_return == Type::Void || call_return == Type::Unknown {
        None
    } else {
        Some(wasm_val(call_return).ok_or_else(|| {
            unsupported(
                "call result",
                span,
                &format!("'{callee}' returns a value the WebAssembly backend cannot hold"),
            )
        })?)
    };

    if callee == "print" {
        if result_type.is_some() {
            return Err(Diagnostic::error(
                "NXL2007",
                "'print' returns no value".to_owned(),
                span,
            ));
        }
        analysis.imports.insert(HostImport::Nl);
        for argument in arguments {
            match value_type_of(analysis, *argument, span)? {
                WasmVal::I64 => {
                    analysis.imports.insert(HostImport::PrintI64);
                }
                WasmVal::F64 => {
                    analysis.imports.insert(HostImport::PrintF64);
                }
                WasmVal::I32 => {
                    analysis.imports.insert(HostImport::PrintBool);
                }
            }
        }
        return Ok(());
    }

    if crate::stdlib::is_vm_only_builtin(callee) {
        return Err(vm_only(callee, span, "cannot perform blocking network I/O"));
    }

    if crate::stdlib::builtin(callee).is_some() {
        if let Some(math) = callee.strip_prefix("std::math::") {
            analyze_math(analysis, math, arguments, span)?;
        } else {
            return Err(unsupported(
                "stdlib call",
                span,
                &format!(
                    "'{callee}' produces or consumes values beyond Int, Float and Bool and needs \
                     the VM (`nxl run`) or the JavaScript backend"
                ),
            ));
        }
    } else {
        // A plain call to another function in this module.
        let target = module
            .functions
            .iter()
            .find(|f| f.name == callee)
            .ok_or_else(|| {
                Diagnostic::error(
                    "NXL2007",
                    format!("unknown function '{callee}' in the WebAssembly backend"),
                    span,
                )
            })?;
        if arguments.len() != target.parameters.len() {
            return Err(Diagnostic::error(
                "NXL2007",
                format!(
                    "'{callee}' expects {} argument(s), got {}",
                    target.parameters.len(),
                    arguments.len()
                ),
                span,
            ));
        }
        // The callee's declared return type decides the caller's slot; it has
        // already been validated by `analyze_function` for every function.
        if result_type.is_some() && return_type(target).is_none() {
            return Err(Diagnostic::error(
                "NXL2007",
                format!(
                    "'{callee}' returns no value but its result is used; \
                     the WebAssembly backend refuses the shape"
                ),
                span,
            ));
        }
        if result_type.is_none() && return_type(target).is_some() {
            return Err(Diagnostic::error(
                "NXL2007",
                format!(
                    "'{callee}' returns a value the WebAssembly backend cannot hold; \
                     only Int, Float and Bool are supported"
                ),
                span,
            ));
        }
    }

    if let Some(val) = result_type {
        define(analysis, result, val);
    }
    Ok(())
}

/// The23 `std.math` builtins. Native WASM operations cover the elementary
/// ones; `round` is written as `trunc(x + copysign(0.5, x))` to match Rust's
/// half-away-from-zero rounding (WASM's `f64.nearest` is banker's rounding),
/// and the transcendentals route through host imports.
fn analyze_math(
    analysis: &mut FnAnalysis,
    math: &str,
    arguments: &[ValueId],
    span: Span,
) -> Result<(), Diagnostic> {
    let unary_native = matches!(
        math,
        "sqrt"
            | "floor"
            | "ceil"
            | "trunc"
            | "round"
            | "is_nan"
            | "is_infinite"
            | "is_finite"
            | "to_radians"
            | "to_degrees"
    );
    let unary_host = matches!(
        math,
        "cbrt" | "exp" | "ln" | "log2" | "log10" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan"
    );
    let binary_host = matches!(math, "atan2" | "pow_float");

    if unary_native || unary_host {
        if arguments.len() != 1 {
            return Err(Diagnostic::error(
                "NXL2007",
                format!(
                    "'std::math::{math}' expects 1 argument, got {}",
                    arguments.len()
                ),
                span,
            ));
        }
        match value_type_of(analysis, arguments[0], span)? {
            WasmVal::I64 | WasmVal::F64 => {}
            WasmVal::I32 => {
                return Err(unsupported(
                    "math call",
                    span,
                    &format!("'std::math::{math}' needs an Int or Float argument"),
                ));
            }
        }
        if unary_host {
            let field = match math {
                "cbrt" => MathFn::Cbrt,
                "exp" => MathFn::Exp,
                "ln" => MathFn::Ln,
                "log2" => MathFn::Log2,
                "log10" => MathFn::Log10,
                "sin" => MathFn::Sin,
                "cos" => MathFn::Cos,
                "tan" => MathFn::Tan,
                "asin" => MathFn::Asin,
                "acos" => MathFn::Acos,
                _ => MathFn::Atan,
            };
            analysis.imports.insert(HostImport::Math(field));
        }
    } else if binary_host {
        if arguments.len() != 2 {
            return Err(Diagnostic::error(
                "NXL2007",
                format!(
                    "'std::math::{math}' expects 2 arguments, got {}",
                    arguments.len()
                ),
                span,
            ));
        }
        for argument in arguments {
            match value_type_of(analysis, *argument, span)? {
                WasmVal::I64 | WasmVal::F64 => {}
                WasmVal::I32 => {
                    return Err(unsupported(
                        "math call",
                        span,
                        &format!("'std::math::{math}' needs Int or Float arguments"),
                    ));
                }
            }
        }
        let field = if math == "atan2" {
            MathFn::Atan2
        } else {
            MathFn::Pow
        };
        analysis.imports.insert(HostImport::Math(field));
    } else {
        return Err(unsupported(
            "math call",
            span,
            &format!("'std::math::{math}' is not one of the supported math builtins"),
        ));
    }
    Ok(())
}

// ── Binary encoding helpers ───────────────────────────────────────────────────

/// The WASM opcodes and block type byte this subset uses, named as in the
/// core specification.
struct Op;

#[allow(dead_code)]
impl Op {
    const UNREACHABLE: u8 = 0x00;
    const BLOCK: u8 = 0x02;
    const LOOP: u8 = 0x03;
    const IF: u8 = 0x04;
    const ELSE: u8 = 0x05;
    const END: u8 = 0x0B;
    const BR: u8 = 0x0C;
    const BR_IF: u8 = 0x0D;
    const RETURN: u8 = 0x0F;
    const CALL: u8 = 0x10;
    const DROP: u8 = 0x1A;
    const LOCAL_GET: u8 = 0x20;
    const LOCAL_SET: u8 = 0x21;
    const I32_CONST: u8 = 0x41;
    const I64_CONST: u8 = 0x42;
    const F64_CONST: u8 = 0x44;
    const I32_EQZ: u8 = 0x45;
    const I32_EQ: u8 = 0x46;
    const I32_NE: u8 = 0x47;
    const I32_AND: u8 = 0x71;
    const I32_OR: u8 = 0x72;
    const I64_EQ: u8 = 0x51;
    const I64_NE: u8 = 0x52;
    const I64_LT_S: u8 = 0x53;
    const I64_GT_S: u8 = 0x55;
    const I64_LE_S: u8 = 0x57;
    const I64_GE_S: u8 = 0x59;
    const F64_EQ: u8 = 0x61;
    const F64_NE: u8 = 0x62;
    const F64_LT: u8 = 0x63;
    const F64_GT: u8 = 0x64;
    const F64_LE: u8 = 0x65;
    const F64_GE: u8 = 0x66;
    const I64_ADD: u8 = 0x7C;
    const I64_SUB: u8 = 0x7D;
    const I64_MUL: u8 = 0x7E;
    const I64_DIV_S: u8 = 0x7F;
    const I64_REM_S: u8 = 0x81;
    const I64_AND: u8 = 0x83;
    const I64_OR: u8 = 0x84;
    const I64_XOR: u8 = 0x85;
    const I64_SHL: u8 = 0x86;
    const I64_SHR_S: u8 = 0x87;
    const F64_ABS: u8 = 0x99;
    const F64_NEG: u8 = 0x9A;
    const F64_CEIL: u8 = 0x9B;
    const F64_FLOOR: u8 = 0x9C;
    const F64_TRUNC: u8 = 0x9D;
    const F64_SQRT: u8 = 0x9F;
    const F64_ADD: u8 = 0xA0;
    const F64_SUB: u8 = 0xA1;
    const F64_MUL: u8 = 0xA2;
    const F64_DIV: u8 = 0xA3;
    const F64_COPYSIGN: u8 = 0xA6;
    const F64_CONVERT_I64_S: u8 = 0xB9;
}

/// Block type for blocks, loops and `if`s with no parameters or results.
const EMPTY_BLOCKTYPE: u8 = 0x40;

fn push_op(out: &mut Vec<u8>, opcode: u8) {
    out.push(opcode);
}

/// Unsigned LEB128.
fn push_uleb(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

/// Signed LEB128 (two's-complement continuation rule).
fn push_sleb(out: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        let sign_bit = byte & 0x40 != 0;
        if (value == 0 && !sign_bit) || (value == -1 && sign_bit) {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

// ── Emission ──────────────────────────────────────────────────────────────────

/// One recovered loop, tracked while its body emits so `br` can target the
/// enclosing `block` (break) or `loop` (continue).
#[derive(Clone)]
struct LoopFrame {
    header: usize,
    exit: Option<usize>,
    blocks: HashSet<usize>,
}

/// A construct that owns a WASM label while open, so branch depths can be
/// computed relative to the innermost one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Open {
    Break(usize),
    Cont(usize),
    IfArm,
}

struct Emitter<'a> {
    function: &'a Function,
    analysis: &'a FnAnalysis,
    import_indices: &'a HashMap<HostImport, u32>,
    func_indices: &'a HashMap<String, u32>,
    /// Number of host imports: module function `i` lives at `import_count + i`.
    import_count: u32,
    locals: HashMap<ValueId, u32>,
    name_locals: HashMap<String, u32>,
    code: Vec<u8>,
    loops: HashMap<usize, LoopInfo>,
    frames: Vec<LoopFrame>,
    inlined: HashSet<usize>,
    emitted: HashSet<usize>,
    suppressed: Option<BasicBlockId>,
    labels: Vec<Open>,
    fn_result: Option<WasmVal>,
}

/// Emit one function body (locals declaration included, trailing `end`
/// included) from a function the analysis pass already accepted.
fn emit_function(
    function: &Function,
    analysis: &FnAnalysis,
    import_indices: &HashMap<HostImport, u32>,
    func_indices: &HashMap<String, u32>,
) -> Result<Vec<u8>, Diagnostic> {
    // Local index space: formals at 0..n, then one local per remaining
    // value, grouped by type so the locals declaration stays compact.
    // Value ids sort ascending so repeated compiles emit identical bytes.
    let mut locals: HashMap<ValueId, u32> = HashMap::new();
    for (value, index) in &analysis.formal_aliases {
        locals.insert(*value, *index);
    }
    let mut remaining: Vec<(ValueId, WasmVal)> = analysis
        .value_types
        .iter()
        .filter(|(value, _)| !analysis.formal_aliases.contains_key(value))
        .map(|(value, val)| (*value, *val))
        .collect();
    remaining.sort_by_key(|(value, _)| *value);
    let mut groups = [
        (WasmVal::I32, Vec::new()),
        (WasmVal::I64, Vec::new()),
        (WasmVal::F64, Vec::new()),
    ];
    for (value, val) in remaining {
        let group = groups
            .iter_mut()
            .find(|(group_val, _)| *group_val == val)
            .expect("every value has a scalar type");
        group.1.push(value);
    }
    let mut next_local = u32::try_from(function.parameters.len()).unwrap_or(0);
    let mut locals_decl = Vec::new();
    for (val, values) in &groups {
        if values.is_empty() {
            continue;
        }
        locals_decl.push((u32::try_from(values.len()).unwrap_or(0), *val));
        for value in values {
            locals.insert(*value, next_local);
            next_local += 1;
        }
    }

    let mut emitter = Emitter {
        function,
        analysis,
        import_indices,
        func_indices,
        import_count: import_indices.len() as u32,
        locals,
        name_locals: HashMap::new(),
        code: Vec::new(),
        loops: analyze_loops(function),
        frames: Vec::new(),
        inlined: HashSet::new(),
        emitted: HashSet::new(),
        suppressed: None,
        labels: Vec::new(),
        fn_result: return_type(function),
    };
    // Parameter names read and write their formal local directly.
    for (index, (name, _)) in function.parameters.iter().enumerate() {
        emitter
            .name_locals
            .insert(name.clone(), u32::try_from(index).unwrap_or(0));
    }
    // Every other bound name gets its own local, and Store/Assign copy into
    // it — the same one-slot-per-name semantics the VM's StoreLocal has, so
    // even a conditional `let` rebinding inside an arm stays path-exact.
    let mut name_bindings: Vec<(&String, WasmVal)> = analysis
        .name_types
        .iter()
        .filter(|(name, _)| !emitter.name_locals.contains_key(*name))
        .map(|(name, val)| (name, *val))
        .collect();
    name_bindings.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (name, val) in name_bindings {
        emitter.name_locals.insert(name.clone(), next_local);
        locals_decl.push((1, val));
        next_local += 1;
    }

    let block_count = function.basic_blocks.len();
    for idx in 0..block_count {
        if emitter.inlined.contains(&idx) || emitter.emitted.contains(&idx) {
            continue;
        }
        if let Some(info) = emitter.loops.get(&idx).cloned() {
            emitter.emit_loop(&info)?;
            continue;
        }
        emitter.emit_block(idx)?;
    }
    // A non-void function that falls off the end of the body only does so
    // through territory where every path already returned; the trailing
    // `unreachable` is dead but keeps the validator's type rules happy.
    if emitter.fn_result.is_some() {
        push_op(&mut emitter.code, Op::UNREACHABLE);
    }
    push_op(&mut emitter.code, Op::END);

    let mut body = Vec::new();
    push_uleb(&mut body, locals_decl.len() as u64);
    for (count, val) in locals_decl {
        push_uleb(&mut body, u64::from(count));
        body.push(val.byte());
    }
    body.extend(emitter.code);
    Ok(body)
}

impl<'a> Emitter<'a> {
    fn push(&mut self, opcode: u8) {
        self.code.push(opcode);
    }

    fn local(&self, value: ValueId, span: Span) -> Result<u32, Diagnostic> {
        self.locals.get(&value).copied().ok_or_else(|| {
            Diagnostic::error(
                "NXL2007",
                format!("the WebAssembly backend lost track of value v{value}"),
                span,
            )
        })
    }

    fn local_get(&mut self, value: ValueId, span: Span) -> Result<(), Diagnostic> {
        let index = self.local(value, span)?;
        self.push(Op::LOCAL_GET);
        push_uleb(&mut self.code, u64::from(index));
        Ok(())
    }

    fn local_set(&mut self, value: ValueId, span: Span) -> Result<(), Diagnostic> {
        let index = self.local(value, span)?;
        self.push(Op::LOCAL_SET);
        push_uleb(&mut self.code, u64::from(index));
        Ok(())
    }

    fn name_local(&self, name: &str, span: Span) -> Result<u32, Diagnostic> {
        self.name_locals.get(name).copied().ok_or_else(|| {
            Diagnostic::error(
                "NXL2007",
                format!("the WebAssembly backend lost track of variable '{name}'"),
                span,
            )
        })
    }

    fn type_of(&self, value: ValueId, span: Span) -> Result<WasmVal, Diagnostic> {
        self.analysis
            .value_types
            .get(&value)
            .copied()
            .ok_or_else(|| {
                Diagnostic::error(
                    "NXL2007",
                    format!(
                        "the WebAssembly backend lost track of the type of value v{value}; \
                     only Int, Float and Bool are supported"
                    ),
                    span,
                )
            })
    }

    fn call_import(&mut self, import: HostImport, span: Span) -> Result<(), Diagnostic> {
        let index = self.import_indices.get(&import).copied().ok_or_else(|| {
            Diagnostic::error(
                "NXL2007",
                format!(
                    "the WebAssembly backend did not register import '{}'",
                    import.field()
                ),
                span,
            )
        })?;
        self.push(Op::CALL);
        push_uleb(&mut self.code, u64::from(index));
        Ok(())
    }

    fn require_block(&self, id: BasicBlockId, span: Span) -> Result<usize, Diagnostic> {
        block_index(self.function, id).ok_or_else(|| {
            Diagnostic::error(
                "NXL2007",
                format!("the WebAssembly backend cannot resolve control-flow block bb{id}"),
                span,
            )
        })
    }

    fn label_depth(&self, target: Open, span: Span) -> Result<u32, Diagnostic> {
        self.labels
            .iter()
            .rev()
            .position(|open| *open == target)
            .map(|depth| depth as u32)
            .ok_or_else(|| {
                Diagnostic::error(
                    "NXL2007",
                    "the WebAssembly backend lost track of a loop label",
                    span,
                )
            })
    }

    fn push_br(&mut self, target: Open, span: Span) -> Result<(), Diagnostic> {
        let depth = self.label_depth(target, span)?;
        self.push(Op::BR);
        push_uleb(&mut self.code, u64::from(depth));
        Ok(())
    }

    fn push_br_if(&mut self, target: Open, span: Span) -> Result<(), Diagnostic> {
        let depth = self.label_depth(target, span)?;
        self.push(Op::BR_IF);
        push_uleb(&mut self.code, u64::from(depth));
        Ok(())
    }

    fn emit_block(&mut self, idx: usize) -> Result<(), Diagnostic> {
        let function = self.function;
        for instruction in &function.basic_blocks[idx].instructions {
            self.emit_instruction(instruction)?;
        }
        let terminator = &function.basic_blocks[idx].terminator;
        self.emit_terminator(terminator, idx)
    }

    fn emit_terminator(
        &mut self,
        terminator: &'a Terminator,
        block_idx: usize,
    ) -> Result<(), Diagnostic> {
        match terminator {
            Terminator::Return { value, span } => self.emit_return(*value, *span),
            Terminator::Jump { target, span } => self.emit_jump(*target, *span, block_idx),
            Terminator::Branch {
                condition,
                then_block,
                else_block,
                span,
            } => {
                let then_idx = self.require_block(*then_block, *span)?;
                let else_idx = self.require_block(*else_block, *span)?;
                self.local_get(*condition, *span)?;
                self.push(Op::IF);
                self.code.push(EMPTY_BLOCKTYPE);
                self.labels.push(Open::IfArm);
                self.emit_inline_region(then_idx)?;
                self.push(Op::ELSE);
                self.emit_inline_region(else_idx)?;
                self.push(Op::END);
                self.labels.pop();
                Ok(())
            }
            Terminator::Unreachable { .. } => {
                self.push(Op::UNREACHABLE);
                Ok(())
            }
        }
    }

    fn emit_return(&mut self, value: Option<ValueId>, span: Span) -> Result<(), Diagnostic> {
        if let (Some(_), Some(id)) = (self.fn_result, value) {
            self.local_get(id, span)?;
        }
        self.push(Op::RETURN);
        Ok(())
    }

    /// Emit one block (or recovered loop) at a jump target. Mirrors the
    /// JavaScript backend's algorithm: loops become `block { loop { … } }`,
    /// forward targets are inlined at the jump site, and the join block of
    /// an `if` whose arms both jump to the same place is held back and
    /// emitted after the `if`.
    fn emit_inline_region(&mut self, block_idx: usize) -> Result<(), Diagnostic> {
        if let Some(info) = self.loops.get(&block_idx).cloned() {
            self.emit_loop(&info)?;
            if let Some(exit) = info.exit {
                let in_enclosing_loop = self
                    .frames
                    .last()
                    .is_some_and(|frame| frame.blocks.contains(&exit));
                if in_enclosing_loop && !self.inlined.contains(&exit) {
                    return self.emit_inline_region(exit);
                }
            }
            return Ok(());
        }
        self.inlined.insert(block_idx);
        let function = self.function;
        let block = &function.basic_blocks[block_idx];
        for instruction in &block.instructions {
            self.emit_instruction(instruction)?;
        }
        match &block.terminator {
            Terminator::Return { value, span } => self.emit_return(*value, *span)?,
            Terminator::Jump { target, span } => self.emit_jump(*target, *span, block_idx)?,
            Terminator::Branch {
                condition,
                then_block,
                else_block,
                span,
            } => {
                let then_idx = self.require_block(*then_block, *span)?;
                let else_idx = self.require_block(*else_block, *span)?;
                let follow = if self.frames.last().is_some() {
                    let jump_target = |idx: usize| match &function.basic_blocks[idx].terminator {
                        Terminator::Jump { target, .. } => Some(*target),
                        _ => None,
                    };
                    let then_jump = jump_target(then_idx);
                    let else_jump = jump_target(else_idx);
                    if then_jump.is_some() && then_jump == else_jump {
                        then_jump
                    } else {
                        None
                    }
                } else {
                    None
                };
                let follow_index = follow.and_then(|id| block_index(self.function, id));
                let previous_suppressed = self.suppressed;
                self.suppressed = follow;
                self.local_get(*condition, *span)?;
                self.push(Op::IF);
                self.code.push(EMPTY_BLOCKTYPE);
                self.labels.push(Open::IfArm);
                self.emit_inline_region(then_idx)?;
                self.push(Op::ELSE);
                self.emit_inline_region(else_idx)?;
                self.push(Op::END);
                self.labels.pop();
                self.suppressed = previous_suppressed;
                if let Some(follow_index) = follow_index {
                    let in_enclosing_loop = self
                        .frames
                        .last()
                        .is_some_and(|frame| frame.blocks.contains(&follow_index));
                    if in_enclosing_loop && !self.inlined.contains(&follow_index) {
                        return self.emit_inline_region(follow_index);
                    }
                }
            }
            Terminator::Unreachable { .. } => {
                self.push(Op::UNREACHABLE);
            }
        }
        Ok(())
    }

    fn emit_jump(
        &mut self,
        target: BasicBlockId,
        span: Span,
        block_idx: usize,
    ) -> Result<(), Diagnostic> {
        let target_idx = self.require_block(target, span)?;
        if let Some(frame) = self.frames.last().cloned() {
            if target_idx == frame.header {
                return self.push_br(Open::Cont(frame.header), span);
            }
            if frame.exit == Some(target_idx) {
                return self.push_br(Open::Break(frame.header), span);
            }
            // The join point of the `if` being emitted comes right after it.
            if self.suppressed == Some(target) {
                return Ok(());
            }
            if self.inlined.contains(&target_idx) {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the WebAssembly backend cannot emit a loop body that jumps back into an \
                     already emitted block",
                    span,
                ));
            }
            return self.emit_inline_region(target_idx);
        }
        if target_idx != block_idx + 1
            && (self.inlined.contains(&target_idx) || self.emitted.contains(&target_idx))
        {
            return Err(Diagnostic::error(
                "NXL2007",
                format!(
                    "the WebAssembly backend cannot emit a jump from bb{} back into bb{target}",
                    self.function.basic_blocks[block_idx].id
                ),
                span,
            ));
        }
        // A jump to the block the main walk emits next needs nothing; control
        // falls through.
        Ok(())
    }

    /// Emit one recovered loop as `block $break { loop $cont { … } }`,
    /// consuming every block in its body so the walk never sees them again.
    fn emit_loop(&mut self, info: &LoopInfo) -> Result<(), Diagnostic> {
        for block in &info.blocks {
            self.emitted.insert(*block);
        }
        self.frames.push(LoopFrame {
            header: info.header,
            exit: info.exit,
            blocks: info.blocks.clone(),
        });
        let result = self.emit_loop_body(info);
        self.frames.pop();
        result
    }

    fn emit_loop_body(&mut self, info: &LoopInfo) -> Result<(), Diagnostic> {
        let function = self.function;
        let header = &function.basic_blocks[info.header];
        self.push(Op::BLOCK);
        self.code.push(EMPTY_BLOCKTYPE);
        self.labels.push(Open::Break(info.header));
        self.push(Op::LOOP);
        self.code.push(EMPTY_BLOCKTYPE);
        self.labels.push(Open::Cont(info.header));
        // Header instructions first: for an Int range, `IteratorNext` lowers
        // inline to `has_next = index < limit; item = index; index += 1`,
        // which is exactly what the VM's range arm computes.
        for instruction in &header.instructions {
            self.emit_instruction(instruction)?;
        }
        // The header's condition decides exit versus another iteration —
        // `if (!cond) break`, the same shape the JS backend emits. For a
        // `for` loop the condition is `has_next` (Loop.condition from the
        // lowering); for a `while` it is the condition expression itself.
        match &header.terminator {
            Terminator::Branch {
                condition,
                then_block,
                span,
                ..
            } => {
                self.local_get(*condition, *span)?;
                self.push(Op::I32_EQZ);
                self.push_br_if(Open::Break(info.header), *span)?;
                let body_idx = self.require_block(*then_block, *span)?;
                self.emit_inline_region(body_idx)?;
            }
            Terminator::Jump { target, span } => {
                let body_idx = self.require_block(*target, *span)?;
                self.emit_inline_region(body_idx)?;
            }
            Terminator::Return { value, span } => self.emit_return(*value, *span)?,
            Terminator::Unreachable { .. } => {
                self.push(Op::UNREACHABLE);
            }
        }
        self.push(Op::END);
        self.labels.pop(); // loop
        self.push(Op::END);
        self.labels.pop(); // block
        Ok(())
    }
}

impl<'a> Emitter<'a> {
    fn emit_instruction(&mut self, instruction: &'a Instruction) -> Result<(), Diagnostic> {
        match instruction {
            // Parameters already live in their formal locals (aliased in
            // `emit_function`); nothing to emit.
            Instruction::Parameter { .. } => Ok(()),

            Instruction::Constant {
                result,
                value,
                span,
                ..
            } => {
                match value {
                    Constant::Integer(number) => {
                        self.push(Op::I64_CONST);
                        push_sleb(&mut self.code, *number);
                    }
                    Constant::Float(number) => {
                        self.push(Op::F64_CONST);
                        self.code.extend_from_slice(&number.to_le_bytes());
                    }
                    Constant::Boolean(flag) => {
                        self.push(Op::I32_CONST);
                        push_sleb(&mut self.code, i64::from(*flag as i32));
                    }
                    Constant::String(_) => {
                        return Err(unsupported(
                            "string constant",
                            *span,
                            "strings have no scalar representation",
                        ));
                    }
                }
                self.local_set(*result, *span)
            }

            Instruction::Load {
                result, name, span, ..
            } => {
                let index = self.name_local(name, *span)?;
                self.push(Op::LOCAL_GET);
                push_uleb(&mut self.code, u64::from(index));
                self.local_set(*result, *span)
            }

            // Store and Assign both copy the value into the name's own
            // local — exactly the VM's StoreLocal one-slot-per-name
            // semantics, so conditional rebinding inside an arm stays
            // path-exact.
            Instruction::Store {
                name, value, span, ..
            }
            | Instruction::Assign {
                name, value, span, ..
            } => {
                let index = self.name_local(name, *span)?;
                self.local_get(*value, *span)?;
                self.push(Op::LOCAL_SET);
                push_uleb(&mut self.code, u64::from(index));
                Ok(())
            }

            Instruction::Unary {
                result,
                operator,
                operand,
                span,
                ..
            } => {
                let val = self.type_of(*operand, *span)?;
                match operator {
                    UnaryOperator::Negate => match val {
                        WasmVal::I64 => {
                            self.push(Op::I64_CONST);
                            push_sleb(&mut self.code, 0);
                            self.local_get(*operand, *span)?;
                            self.push(Op::I64_SUB);
                        }
                        WasmVal::F64 => {
                            self.local_get(*operand, *span)?;
                            self.push(Op::F64_NEG);
                        }
                        WasmVal::I32 => {
                            return Err(unsupported(
                                "unary negation",
                                *span,
                                "negation needs an Int or Float operand",
                            ));
                        }
                    },
                    UnaryOperator::Not => {
                        self.local_get(*operand, *span)?;
                        self.push(Op::I32_EQZ);
                    }
                    UnaryOperator::Reference | UnaryOperator::Borrow => {
                        return Err(unsupported(
                            "borrow operator",
                            *span,
                            "references have no scalar representation",
                        ));
                    }
                }
                self.local_set(*result, *span)
            }

            Instruction::Binary {
                result,
                operator,
                left,
                right,
                span,
                ..
            } => {
                let lhs_type = self.type_of(*left, *span)?;
                if lhs_type != self.type_of(*right, *span)? {
                    return Err(unsupported(
                        "binary operation",
                        *span,
                        "the operands disagree on type",
                    ));
                }
                self.local_get(*left, *span)?;
                self.local_get(*right, *span)?;
                // Analysis already refused Remainder on Float (the VM rejects
                // it at runtime); `binary_opcode` maps only i64 remainder.
                let opcode = self.binary_opcode(*operator, lhs_type, *span)?;
                self.push(opcode);
                self.local_set(*result, *span)
            }

            Instruction::Call {
                result,
                callee,
                arguments,
                return_type: call_return,
                span,
                ..
            } => {
                if callee == "print" {
                    for argument in arguments {
                        let arg_type = self.type_of(*argument, *span)?;
                        let import = match arg_type {
                            WasmVal::I64 => HostImport::PrintI64,
                            WasmVal::F64 => HostImport::PrintF64,
                            WasmVal::I32 => HostImport::PrintBool,
                        };
                        self.local_get(*argument, *span)?;
                        self.call_import(import, *span)?;
                    }
                    self.call_import(HostImport::Nl, *span)?;
                } else if crate::stdlib::builtin(callee).is_some() {
                    self.emit_math(callee, arguments, *span)?;
                    if *call_return != Type::Void && *call_return != Type::Unknown {
                        self.local_set(*result, *span)?;
                    }
                } else {
                    for argument in arguments {
                        self.local_get(*argument, *span)?;
                    }
                    let position = self.func_indices.get(callee).copied().ok_or_else(|| {
                        Diagnostic::error(
                            "NXL2007",
                            format!("unknown function '{callee}' in the WebAssembly backend"),
                            *span,
                        )
                    })?;
                    self.push(Op::CALL);
                    push_uleb(&mut self.code, u64::from(self.import_count + position));
                    if *call_return != Type::Void && *call_return != Type::Unknown {
                        self.local_set(*result, *span)?;
                    }
                }
                Ok(())
            }

            Instruction::IteratorNext {
                has_next,
                item,
                iterable,
                index,
                span,
            } => {
                // The VM's range arm of IterNext: `has_next = index < limit`,
                // `item = index`, `index += 1` — in the same order, so even
                // the final header visit (limit reached) binds the name the
                // same way the VM does.
                self.local_get(*index, *span)?;
                self.local_get(*iterable, *span)?;
                self.push(Op::I64_LT_S);
                self.local_set(*has_next, *span)?;
                self.local_get(*index, *span)?;
                self.local_set(*item, *span)?;
                self.local_get(*index, *span)?;
                self.push(Op::I64_CONST);
                push_sleb(&mut self.code, 1);
                self.push(Op::I64_ADD);
                self.local_set(*index, *span)
            }

            Instruction::Borrow {
                result,
                value,
                span,
                ..
            } => {
                // Scalars are copied; the analysis guarantees the result
                // type matches the borrowed value.
                self.local_get(*value, *span)?;
                self.local_set(*result, *span)
            }

            Instruction::Drop { .. } => Ok(()),

            Instruction::Return { value, span, .. } => self.emit_return(*value, *span),

            other => Err(unsupported(
                "instruction",
                self.function.debug.function_span,
                &format!(
                    "the WebAssembly backend cannot emit {other:?}; \
                     analysis should have refused it"
                ),
            )),
        }
    }

    /// The opcode for one scalar binary operation.
    fn binary_opcode(
        &self,
        operator: BinaryOperator,
        operand: WasmVal,
        span: Span,
    ) -> Result<u8, Diagnostic> {
        let opcode = match (operator, operand) {
            (BinaryOperator::Add, WasmVal::I64) => Op::I64_ADD,
            (BinaryOperator::Subtract, WasmVal::I64) => Op::I64_SUB,
            (BinaryOperator::Multiply, WasmVal::I64) => Op::I64_MUL,
            (BinaryOperator::Divide, WasmVal::I64) => Op::I64_DIV_S,
            (BinaryOperator::Remainder, WasmVal::I64) => Op::I64_REM_S,
            (BinaryOperator::Add, WasmVal::F64) => Op::F64_ADD,
            (BinaryOperator::Subtract, WasmVal::F64) => Op::F64_SUB,
            (BinaryOperator::Multiply, WasmVal::F64) => Op::F64_MUL,
            (BinaryOperator::Divide, WasmVal::F64) => Op::F64_DIV,
            (BinaryOperator::Equal, WasmVal::I64) => Op::I64_EQ,
            (BinaryOperator::NotEqual, WasmVal::I64) => Op::I64_NE,
            (BinaryOperator::Less, WasmVal::I64) => Op::I64_LT_S,
            (BinaryOperator::LessEqual, WasmVal::I64) => Op::I64_LE_S,
            (BinaryOperator::Greater, WasmVal::I64) => Op::I64_GT_S,
            (BinaryOperator::GreaterEqual, WasmVal::I64) => Op::I64_GE_S,
            (BinaryOperator::Equal, WasmVal::F64) => Op::F64_EQ,
            (BinaryOperator::NotEqual, WasmVal::F64) => Op::F64_NE,
            (BinaryOperator::Less, WasmVal::F64) => Op::F64_LT,
            (BinaryOperator::LessEqual, WasmVal::F64) => Op::F64_LE,
            (BinaryOperator::Greater, WasmVal::F64) => Op::F64_GT,
            (BinaryOperator::GreaterEqual, WasmVal::F64) => Op::F64_GE,
            (BinaryOperator::Equal, WasmVal::I32) => Op::I32_EQ,
            (BinaryOperator::NotEqual, WasmVal::I32) => Op::I32_NE,
            (BinaryOperator::And, WasmVal::I32) => Op::I32_AND,
            (BinaryOperator::Or, WasmVal::I32) => Op::I32_OR,
            (BinaryOperator::BitAnd, WasmVal::I64) => Op::I64_AND,
            (BinaryOperator::BitOr, WasmVal::I64) => Op::I64_OR,
            (BinaryOperator::BitXor, WasmVal::I64) => Op::I64_XOR,
            (BinaryOperator::ShiftLeft, WasmVal::I64) => Op::I64_SHL,
            (BinaryOperator::ShiftRight, WasmVal::I64) => Op::I64_SHR_S,
            _ => {
                return Err(unsupported(
                    "binary operation",
                    span,
                    &format!("operator {operator:?} on {operand:?} has no WebAssembly encoding"),
                ));
            }
        };
        Ok(opcode)
    }

    /// One `std.math` builtin: native opcodes where WASM has them, exact
    /// constant-folded sequences for `round`/`is_*`/unit conversion, host
    /// imports for the transcendentals.
    fn emit_math(
        &mut self,
        callee: &str,
        arguments: &[ValueId],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let math = callee.strip_prefix("std::math::").ok_or_else(|| {
            Diagnostic::error(
                "NXL2007",
                format!("the WebAssembly backend cannot emit stdlib call '{callee}'"),
                span,
            )
        })?;
        match math {
            "sqrt" | "floor" | "ceil" | "trunc" => {
                self.push_float(arguments[0], span)?;
                let opcode = match math {
                    "sqrt" => Op::F64_SQRT,
                    "floor" => Op::F64_FLOOR,
                    "ceil" => Op::F64_CEIL,
                    _ => Op::F64_TRUNC,
                };
                self.push(opcode);
            }
            "to_radians" | "to_degrees" => {
                self.push_float(arguments[0], span)?;
                self.push(Op::F64_CONST);
                let scale = if math == "to_radians" {
                    std::f64::consts::PI / 180.0
                } else {
                    180.0 / std::f64::consts::PI
                };
                self.code.extend_from_slice(&scale.to_le_bytes());
                self.push(Op::F64_MUL);
            }
            "round" => {
                // trunc(x + copysign(0.5, x)) — Rust's half-away-from-zero
                // rounding, which f64.nearest (banker's) would not give.
                self.push_float(arguments[0], span)?;
                self.push(Op::F64_CONST);
                self.code.extend_from_slice(&0.5f64.to_le_bytes());
                self.push_float(arguments[0], span)?;
                self.push(Op::F64_COPYSIGN);
                self.push(Op::F64_ADD);
                self.push(Op::F64_TRUNC);
            }
            "is_nan" => {
                // x != x is true only for NaN.
                self.push_float(arguments[0], span)?;
                self.push_float(arguments[0], span)?;
                self.push(Op::F64_NE);
            }
            "is_infinite" => {
                self.push_float(arguments[0], span)?;
                self.push(Op::F64_ABS);
                self.push(Op::F64_CONST);
                self.code.extend_from_slice(&f64::INFINITY.to_le_bytes());
                self.push(Op::F64_EQ);
            }
            "is_finite" => {
                self.push_float(arguments[0], span)?;
                self.push_float(arguments[0], span)?;
                self.push(Op::F64_EQ); // not NaN
                self.push_float(arguments[0], span)?;
                self.push(Op::F64_ABS);
                self.push(Op::F64_CONST);
                self.code.extend_from_slice(&f64::INFINITY.to_le_bytes());
                self.push(Op::F64_NE); // not infinite
                self.push(Op::I32_AND);
            }
            host => {
                self.push_float(arguments[0], span)?;
                if math == "atan2" || math == "pow_float" {
                    self.push_float(arguments[1], span)?;
                }
                let import = match host {
                    "cbrt" => MathFn::Cbrt,
                    "exp" => MathFn::Exp,
                    "ln" => MathFn::Ln,
                    "log2" => MathFn::Log2,
                    "log10" => MathFn::Log10,
                    "sin" => MathFn::Sin,
                    "cos" => MathFn::Cos,
                    "tan" => MathFn::Tan,
                    "asin" => MathFn::Asin,
                    "acos" => MathFn::Acos,
                    "atan" => MathFn::Atan,
                    "atan2" => MathFn::Atan2,
                    "pow_float" => MathFn::Pow,
                    _ => {
                        return Err(unsupported(
                            "math call",
                            span,
                            &format!(
                                "'std::math::{math}' is not one of the supported math builtins"
                            ),
                        ));
                    }
                };
                self.call_import(HostImport::Math(import), span)?;
            }
        }
        Ok(())
    }

    /// Push one value as `f64`, converting Int arguments the way the VM's
    /// math builtins coerce them.
    fn push_float(&mut self, value: ValueId, span: Span) -> Result<(), Diagnostic> {
        let val = self.type_of(value, span)?;
        self.local_get(value, span)?;
        if val == WasmVal::I64 {
            self.push(Op::F64_CONVERT_I64_S);
        }
        Ok(())
    }
}

// ── Binary assembly ───────────────────────────────────────────────────────────

fn encode_string(out: &mut Vec<u8>, text: &str) {
    push_uleb(out, text.len() as u64);
    out.extend_from_slice(text.as_bytes());
}

fn encode_functype(out: &mut Vec<u8>, ty: &WasmFuncType) {
    out.push(0x60); // functype
    push_uleb(out, ty.params.len() as u64);
    for param in &ty.params {
        out.push(param.byte());
    }
    push_uleb(out, ty.results.len() as u64);
    for result in &ty.results {
        out.push(result.byte());
    }
}

/// Write `id` + byte length + payload as one section.
fn push_section(out: &mut Vec<u8>, id: u8, payload: &[u8]) {
    out.push(id);
    push_uleb(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

/// Assemble the final module: magic header, then Type, Import, Function,
/// Export and Code sections in that order. Section contents are fully
/// determined by the inputs, so repeated compiles of the same program emit
/// byte-identical modules. No name section is written.
#[allow(clippy::too_many_arguments)]
fn assemble(
    types: &[WasmFuncType],
    imports: &[HostImport],
    import_type_ids: &[u32],
    function_type_ids: &[u32],
    start_type_id: u32,
    start_index: u32,
    bodies: &[Vec<u8>],
    start_body: &[u8],
) -> WasmArtifact {
    let mut wasm: Vec<u8> = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

    // Type section (id 1): interned signatures in first-use order.
    {
        let mut payload = Vec::new();
        push_uleb(&mut payload, types.len() as u64);
        for ty in types {
            encode_functype(&mut payload, ty);
        }
        push_section(&mut wasm, 1, &payload);
    }

    // Import section (id 2): host helpers, `env` module, function imports.
    if !imports.is_empty() {
        let mut payload = Vec::new();
        push_uleb(&mut payload, imports.len() as u64);
        for (index, import) in imports.iter().enumerate() {
            encode_string(&mut payload, "env");
            encode_string(&mut payload, import.field());
            payload.push(0x00); // externkind: function
            push_uleb(&mut payload, u64::from(import_type_ids[index]));
        }
        push_section(&mut wasm, 2, &payload);
    }

    // Function section (id 3): every module function, then `_start`.
    {
        let mut payload = Vec::new();
        push_uleb(&mut payload, (function_type_ids.len() + 1) as u64);
        for type_id in function_type_ids {
            push_uleb(&mut payload, u64::from(*type_id));
        }
        push_uleb(&mut payload, u64::from(start_type_id));
        push_section(&mut wasm, 3, &payload);
    }

    // Export section (id 7): only `_start`, so hosts always have the same
    // entry hook even when the program defines no entry point.
    {
        let mut payload = Vec::new();
        push_uleb(&mut payload, 1);
        encode_string(&mut payload, "_start");
        payload.push(0x00); // externkind: function (0x00=func, 0x01=table, 0x02=memory, 0x03=global)
        push_uleb(&mut payload, u64::from(start_index));
        push_section(&mut wasm, 7, &payload);
    }

    // Code section (id 10): function bodies in declaration order, then the
    // `_start` wrapper. Each entry is a byte length plus the body bytes
    // (locals declaration + instructions + end).
    {
        let mut payload = Vec::new();
        push_uleb(&mut payload, (bodies.len() + 1) as u64);
        for body in bodies {
            push_uleb(&mut payload, body.len() as u64);
            payload.extend_from_slice(body);
        }
        push_uleb(&mut payload, start_body.len() as u64);
        payload.extend_from_slice(start_body);
        push_section(&mut wasm, 10, &payload);
    }

    WasmArtifact {
        target: "wasm".to_owned(),
        bytes: wasm,
    }
}
