use std::collections::HashMap;

use crate::ast::{
    BinaryOperator, Expression, ExpressionKind, MatchBody, Pattern, PatternKind, Program,
    Statement, StringPart, UnaryOperator,
};
use crate::diagnostic::Diagnostic;
use crate::source::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Void,
    Unknown,
    Array(TypeId),
    Tuple(TypeId),
    Map(TypeId),
    Struct(TypeId),
    Enum(TypeId),
    Reference(TypeId, bool),
    Task(TypeId),
    Channel(TypeId),
    Optional(TypeId),
    Result(TypeId, TypeId),
    /// A named user type, or the uninstantiated base of a generic type
    /// (`Box` in `Box<Int>`). The id is the hash of the (qualified) name.
    Generic(TypeId),
    /// An unsubstituted type parameter (`T` inside `fn id<T>` or
    /// `struct Box<T>`). It behaves like `Unknown` for body checking while
    /// the declaration keeps it visible in HIR/NIR signatures.
    Param(TypeId),
    /// An instantiated generic user type: `Box<Int>`. The first id is the
    /// base type, the second is a fingerprint of the argument types.
    Instance(TypeId, TypeId),
    Function(TypeId),
    ForeignHandle(TypeId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub name: String,
    pub functions: Vec<Function>,
    pub signatures: Vec<FunctionSignature>,
    pub foreign_functions: Vec<ForeignFunction>,
    pub type_definitions: Vec<TypeDefinition>,
    pub globals: Vec<Global>,
    pub initializers: Vec<Instruction>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub metadata: ModuleMetadata,
    pub entry_point: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeDefinition {
    pub id: TypeId,
    pub name: String,
    pub kind: TypeDefinitionKind,
    pub parameters: Vec<TypeId>,
    pub fields: Vec<(String, Type)>,
    pub variants: Vec<(String, Vec<Type>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeDefinitionKind {
    Struct,
    Class,
    Actor,
    Enum,
    Interface,
    Trait,
    Alias,
}

pub type TypeId = u32;

#[derive(Clone, Debug, PartialEq)]
pub struct Global {
    pub name: String,
    pub ty: Type,
    pub initializer: Option<ValueId>,
    pub mutable: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForeignAbiBinding {
    pub name: String,
    pub return_type: Type,
    pub parameter_types: Vec<Type>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConcurrencyOp {
    pub has_async: bool,
    pub has_spawn: bool,
    pub has_channel_ops: bool,
    pub task_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleDebugInfo {
    pub build_timestamp: Option<u64>,
    pub compiler_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleMetadata {
    pub format_version: u32,
    pub source_name: String,
    pub concurrency: ConcurrencyOp,
    pub foreign_abis: Vec<ForeignAbiBinding>,
    pub debug: ModuleDebugInfo,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDebugInfo {
    pub source_name: String,
    pub function_span: Span,
    pub instruction_spans: Vec<Span>,
    pub local_names: Vec<String>,
    pub line_numbers: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ownership {
    Unspecified,
    Owned,
    Borrowed,
    MutableBorrow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugMetadata {
    pub value: ValueId,
    pub ownership: Ownership,
    pub lifetime: Option<u32>,
}

/// Ownership and lifetime information associated with an NIR value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueMetadata {
    pub value: ValueId,
    pub ownership: Ownership,
    pub lifetime: Option<u32>,
}

// #[derive(Clone, Debug, PartialEq, Eq)]
// pub enum ConcurrencyOp {
//     Spawn { task: ValueId, span: Span },
//     Await { task: ValueId, span: Span },
//     ChannelSend { channel: ValueId, span: Span },
//     ChannelReceive { channel: ValueId, span: Span },
//     ActorDispatch { actor: ValueId, span: Span },
//     Cancellation { task: ValueId, span: Span },
// }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConcurrencyMetadata {
    Spawn { task: ValueId, span: Span },
    Await { task: ValueId, span: Span },
    ChannelSend { channel: ValueId, span: Span },
    ChannelReceive { channel: ValueId, span: Span },
    ActorDispatch { actor: ValueId, span: Span },
    Cancellation { task: ValueId, span: Span },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallingConvention {
    C,
    System,
    Nxl,
}

/// A scalar C-ABI type supported by `extern` bindings. Maps to `int64_t`
/// (`Int`), `double` (`Float`), `const char *` (`Str`), and a 0/1 integer
/// class value (`Bool`). Compound types, pointers-to-pointer, and variadic
/// signatures are out of scope for the C ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FfiType {
    Int,
    Float,
    Str,
    Bool,
}

/// A resolved `extern` declaration: the C symbol to call in `library`, with
/// its scalar ABI signature. `return_type` is `None` for `void`.
#[derive(Clone, Debug, PartialEq)]
pub struct ForeignFunction {
    /// The NXL-visible name used to call the function.
    pub name: String,
    /// The C symbol looked up in the library (may differ from `name`).
    pub symbol: String,
    /// The shared library to load.
    pub library: String,
    pub parameters: Vec<FfiType>,
    pub return_type: Option<FfiType>,
    pub calling_convention: CallingConvention,
}

/// Map an `extern` scalar type to the NIR type used for inference.
pub fn ffi_scalar_type(ty: FfiType) -> Type {
    match ty {
        FfiType::Int => Type::Int,
        FfiType::Float => Type::Float,
        FfiType::Str => Type::String,
        FfiType::Bool => Type::Bool,
    }
}

/// Map a declared `extern` return (`None` = `void`) to its NIR type.
pub fn ffi_return_type(ret: Option<FfiType>) -> Type {
    ret.map_or(Type::Void, ffi_scalar_type)
}

/// Parse a bare NXL type name used in an `extern` signature to its C-ABI
/// scalar. Returns `None` for unsupported types.
fn parse_ffi_type(name: &str) -> Option<FfiType> {
    match name {
        "Int" => Some(FfiType::Int),
        "Float" => Some(FfiType::Float),
        "String" => Some(FfiType::Str),
        "Bool" => Some(FfiType::Bool),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionSignature {
    pub name: String,
    pub parameters: Vec<(String, Type)>,
    pub return_type: Type,
    /// Declared type parameters (`T` in `fn id<T>`), in source order.
    pub type_parameters: Vec<String>,
    /// Written parameter annotations, canonicalized against the declaring
    /// file's type aliases. Kept so explicit instantiation can substitute
    /// type arguments into structural types (`Array<T>`, `Box<T>`) that the
    /// hash-based `Type` representation cannot rewrite.
    pub parameter_annotations: Vec<Option<String>>,
    /// Written return annotation, canonicalized like `parameter_annotations`.
    pub return_annotation: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<(String, Type)>,
    pub return_type: Type,
    pub basic_blocks: Vec<BasicBlock>,
    pub debug: FunctionDebugInfo,
    pub values: Vec<ValueMetadata>,
    pub concurrency: Vec<ConcurrencyMetadata>,
    /// Loops lowered into this function, outermost first. Backends use it to
    /// rebuild native loops instead of reverse-engineering the CFG.
    pub loops: Vec<LoopMetadata>,
}

/// Where a lowered loop starts, where its body starts, and where it leaves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopMetadata {
    /// Block re-evaluated on every iteration (holds the condition or the
    /// `for` iterator step).
    pub header: BasicBlockId,
    /// First block of the loop body.
    pub body: BasicBlockId,
    /// Block control reaches after the loop finishes or breaks out.
    pub exit: BasicBlockId,
}

pub type BasicBlockId = u32;

#[derive(Clone, Debug, PartialEq)]
pub struct BasicBlock {
    pub id: BasicBlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Terminator {
    Jump {
        target: BasicBlockId,
        span: Span,
    },
    Branch {
        condition: ValueId,
        then_block: BasicBlockId,
        else_block: BasicBlockId,
        span: Span,
    },
    Return {
        value: Option<ValueId>,
        span: Span,
    },
    Unreachable {
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    Parameter {
        result: ValueId,
        index: usize,
        ty: Type,
        span: Span,
    },
    Constant {
        result: ValueId,
        value: Constant,
        ty: Type,
        span: Span,
    },
    Load {
        result: ValueId,
        name: String,
        ty: Type,
        span: Span,
    },
    Store {
        name: String,
        value: ValueId,
        span: Span,
    },
    Unary {
        result: ValueId,
        operator: UnaryOperator,
        operand: ValueId,
        ty: Type,
        span: Span,
    },
    Binary {
        result: ValueId,
        operator: BinaryOperator,
        left: ValueId,
        right: ValueId,
        ty: Type,
        span: Span,
    },
    Call {
        result: ValueId,
        callee: String,
        arguments: Vec<ValueId>,
        /// Concrete type arguments for an explicitly instantiated generic
        /// call; empty for ordinary calls. Retained so NIR verification can
        /// check the call against the instantiated signature.
        type_arguments: Vec<Type>,
        return_type: Type,
        span: Span,
    },
    IteratorNext {
        has_next: ValueId,
        item: ValueId,
        iterable: ValueId,
        /// Loop-counter slot, initialised to 0 before the loop.
        index: ValueId,
        span: Span,
    },
    /// Build an array value from element values.
    ArrayNew {
        result: ValueId,
        elements: Vec<ValueId>,
        span: Span,
    },
    /// Read an element by integer index (throws when out of bounds).
    IndexGet {
        result: ValueId,
        object: ValueId,
        index: ValueId,
        /// Statically inferred element type (`Unknown` when the array
        /// element type could not be recovered).
        ty: Type,
        span: Span,
    },
    /// Copy-on-write element update; `result` aliases the new array.
    IndexSet {
        result: ValueId,
        object: ValueId,
        index: ValueId,
        value: ValueId,
        span: Span,
    },
    /// Build a struct value; fields must match the declaration exactly.
    /// `ty` is the constructed type, including instantiation identity
    /// (`Box<Int>`), so callers and verification see the same type.
    StructNew {
        result: ValueId,
        type_name: String,
        ty: Type,
        fields: Vec<(String, ValueId)>,
        span: Span,
    },
    /// Read a struct field (throws on unknown field / non-object).
    FieldGet {
        result: ValueId,
        object: ValueId,
        field: String,
        /// Statically inferred field type (`Unknown` when the object type
        /// or field declaration could not be resolved).
        ty: Type,
        span: Span,
    },
    /// Copy-on-write field update; `result` aliases the new object.
    FieldSet {
        result: ValueId,
        object: ValueId,
        field: String,
        value: ValueId,
        span: Span,
    },
    /// Build an enum variant value with payload arguments.
    EnumNew {
        result: ValueId,
        type_name: String,
        variant: String,
        /// Static type of the constructed value (`Generic` for a raw enum,
        /// `Instance` for an explicitly instantiated generic enum).
        ty: Type,
        arguments: Vec<ValueId>,
        span: Span,
    },
    /// Push the variant name of an enum value ("" for anything else).
    VariantName {
        result: ValueId,
        value: ValueId,
        span: Span,
    },
    /// Total element read for match patterns: Unit when not an
    /// indexable payload instead of throwing.
    PatternIndex {
        result: ValueId,
        object: ValueId,
        index: ValueId,
        span: Span,
    },
    /// Total field read for match patterns: Unit for unknown fields
    /// or non-objects instead of throwing.
    PatternField {
        result: ValueId,
        object: ValueId,
        field: String,
        span: Span,
    },
    /// Dynamic method call. `receiver_out` receives the receiver as it stood
    /// when the call returned, so the caller observes receiver mutations;
    /// `result` receives the return value.
    MethodCall {
        result: ValueId,
        receiver_out: ValueId,
        receiver: ValueId,
        method: String,
        arguments: Vec<ValueId>,
        span: Span,
    },
    /// Create a closure value capturing variables.
    MakeClosure {
        result: ValueId,
        callee: String,
        captures: Vec<ValueId>,
        span: Span,
    },
    /// Call a function value (closure or function reference) indirectly.
    CallIndirect {
        result: ValueId,
        callee: ValueId,
        arguments: Vec<ValueId>,
        return_type: Type,
        span: Span,
    },
    /// Spawn a thread running a function; `result` is the task handle.
    SpawnCall {
        result: ValueId,
        callee: String,
        arguments: Vec<ValueId>,
        span: Span,
    },
    /// Spawn a thread running a method on a cloned receiver.
    SpawnMethod {
        result: ValueId,
        receiver: ValueId,
        method: String,
        arguments: Vec<ValueId>,
        span: Span,
    },
    /// Wrap an already-computed value in a completed task.
    CompleteTask {
        result: ValueId,
        value: ValueId,
        span: Span,
    },
    /// Block until a task completes (re-raising failures); `result` is its
    /// value. Non-tasks pass through untouched.
    AwaitTask {
        result: ValueId,
        task: ValueId,
        span: Span,
    },
    /// Create a fresh channel.
    ChannelNew {
        result: ValueId,
        span: Span,
    },
    Allocate {
        result: ValueId,
        ty: Type,
        span: Span,
    },
    LoadMemory {
        result: ValueId,
        pointer: ValueId,
        ty: Type,
        span: Span,
    },
    StoreMemory {
        pointer: ValueId,
        value: ValueId,
        span: Span,
    },
    Borrow {
        result: ValueId,
        value: ValueId,
        mutable: bool,
        ty: Type,
        span: Span,
    },
    Drop {
        value: ValueId,
        span: Span,
    },
    Spawn {
        result: ValueId,
        task: ValueId,
        ty: Type,
        span: Span,
    },
    Await {
        result: ValueId,
        task: ValueId,
        ty: Type,
        span: Span,
    },
    ChannelSend {
        channel: ValueId,
        value: ValueId,
        span: Span,
    },
    ChannelReceive {
        result: ValueId,
        channel: ValueId,
        ty: Type,
        span: Span,
    },
    ActorDispatch {
        actor: ValueId,
        message: ValueId,
        span: Span,
    },
    CancelTask {
        task: ValueId,
        span: Span,
    },
    ChannelClose {
        channel: ValueId,
        span: Span,
    },
    ForeignCall {
        result: ValueId,
        /// The NXL-visible callee name, used for diagnostics only.
        callee: String,
        /// The shared library loaded for this call.
        library: String,
        /// The C symbol looked up in `library`.
        symbol: String,
        arguments: Vec<ValueId>,
        /// The declared scalar ABI parameter types, for marshalling.
        parameter_types: Vec<FfiType>,
        /// The declared scalar ABI return type; `None` means `void`.
        return_type: Option<FfiType>,
        calling_convention: CallingConvention,
        span: Span,
    },
    Assign {
        name: String,
        value: ValueId,
        span: Span,
    },
    If {
        condition: ValueId,
        then_branch: Vec<Instruction>,
        else_branch: Option<Vec<Instruction>>,
        span: Span,
    },
    Loop {
        condition: Option<ValueId>,
        condition_instructions: Vec<Instruction>,
        body: Vec<Instruction>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Return {
        value: Option<ValueId>,
        span: Span,
    },
    /// Convert a value to its string representation (string interpolation).
    ToString {
        result: ValueId,
        value: ValueId,
        span: Span,
    },
    /// Push the runtime type name of a value ("Int", "String", ...) as a string.
    TypeName {
        result: ValueId,
        value: ValueId,
        span: Span,
    },
    /// Pop the in-flight exception (pushed by the unwinder) into a slot.
    /// Only valid as the first instruction of an exception-dispatch block.
    Catch {
        result: ValueId,
        span: Span,
    },
    /// Raise a value as an exception; unwinds to the nearest handler.
    Throw {
        value: ValueId,
        span: Span,
    },
    /// Register the current execution state as an exception handler that
    /// resumes at `catch_block` (which must start with `Catch`).
    PushHandler {
        catch_block: BasicBlockId,
        span: Span,
    },
    /// Remove the most recently registered exception handler (normal path).
    PopHandler {
        span: Span,
    },
    /// Structured exception handling. Expanded into blocks by `CfgBuilder`;
    /// never survives into finished basic blocks.
    Try {
        body: Vec<Instruction>,
        catches: Vec<TryCatch>,
        finally: Option<Vec<Instruction>>,
        /// Slot that receives the in-flight exception on dispatch.
        tmp: ValueId,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TryCatch {
    pub name: String,
    pub annotation: Option<String>,
    pub body: Vec<Instruction>,
    pub span: Span,
}

pub type ValueId = u32;

impl Instruction {
    pub fn span(&self) -> Span {
        match self {
            Instruction::Parameter { span, .. } => *span,
            Instruction::Constant { span, .. } => *span,
            Instruction::Load { span, .. } => *span,
            Instruction::Store { span, .. } => *span,
            Instruction::Unary { span, .. } => *span,
            Instruction::Binary { span, .. } => *span,
            Instruction::Call { span, .. } => *span,
            Instruction::CallIndirect { span, .. } => *span,
            Instruction::MakeClosure { span, .. } => *span,
            Instruction::IteratorNext { span, .. } => *span,
            Instruction::ArrayNew { span, .. } => *span,
            Instruction::IndexGet { span, .. } => *span,
            Instruction::IndexSet { span, .. } => *span,
            Instruction::StructNew { span, .. } => *span,
            Instruction::FieldGet { span, .. } => *span,
            Instruction::FieldSet { span, .. } => *span,
            Instruction::EnumNew { span, .. } => *span,
            Instruction::VariantName { span, .. } => *span,
            Instruction::PatternIndex { span, .. } => *span,
            Instruction::PatternField { span, .. } => *span,
            Instruction::MethodCall { span, .. } => *span,
            Instruction::SpawnCall { span, .. } => *span,
            Instruction::SpawnMethod { span, .. } => *span,
            Instruction::CompleteTask { span, .. } => *span,
            Instruction::AwaitTask { span, .. } => *span,
            Instruction::ChannelNew { span, .. } => *span,
            Instruction::Allocate { span, .. } => *span,
            Instruction::LoadMemory { span, .. } => *span,
            Instruction::StoreMemory { span, .. } => *span,
            Instruction::Borrow { span, .. } => *span,
            Instruction::Drop { span, .. } => *span,
            Instruction::Spawn { span, .. } => *span,
            Instruction::Await { span, .. } => *span,
            Instruction::ChannelSend { span, .. } => *span,
            Instruction::ChannelReceive { span, .. } => *span,
            Instruction::ActorDispatch { span, .. } => *span,
            Instruction::CancelTask { span, .. } => *span,
            Instruction::ChannelClose { span, .. } => *span,
            Instruction::ForeignCall { span, .. } => *span,
            Instruction::Assign { span, .. } => *span,
            Instruction::If { span, .. } => *span,
            Instruction::Loop { span, .. } => *span,
            Instruction::Break { span } => *span,
            Instruction::Continue { span } => *span,
            Instruction::Return { span, .. } => *span,
            Instruction::ToString { span, .. } => *span,
            Instruction::TypeName { span, .. } => *span,
            Instruction::Catch { span, .. } => *span,
            Instruction::Throw { span, .. } => *span,
            Instruction::PushHandler { span, .. } => *span,
            Instruction::PopHandler { span } => *span,
            Instruction::Try { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
}

pub fn lower(program: &Program) -> Result<Module, Diagnostic> {
    let hir = crate::hir::analyze(program, "<memory>")?;
    lower_hir(&hir)
}

pub fn lower_hir(hir: &crate::hir::HirModule) -> Result<Module, Diagnostic> {
    // Single-file lowering still needs the file's imports so qualified stdlib
    // calls (`std.math.sqrt`) and selected imports (`from std.strings import …`)
    // resolve; the entry file is the root namespace.
    let imports = crate::project::parse_imports(&hir.program);
    lower_project_files(&[ProjectFile {
        path: String::new(),
        hir: hir.clone(),
        imports,
        aliases: HashMap::new(),
    }])
}

/// Lower several files (dependency order) into one module. The entry file
/// keeps the root namespace; every other file owns its dotted path.
pub fn lower_project_files(files: &[ProjectFile]) -> Result<Module, Diagnostic> {
    validate_ownership_declarations(files)?;
    let mut lowerer = Lowerer {
        functions: Vec::new(),
        signatures: files
            .iter()
            .flat_map(|file| file.hir.functions.clone())
            .collect(),
        instructions: Vec::new(),
        values: HashMap::new(),
        mutability: HashMap::new(),
        scope_stack: vec![HashMap::new()],
        mutability_stack: vec![HashMap::new()],
        next_value: 0,
        in_function: false,
        current_function_return_type: None,
        current_return_types: Vec::new(),
        current_type_parameters: Vec::new(),
        in_loop: false,
        loop_depth: 0,
        try_stack: Vec::new(),
        function_has_return: false,
        source_name: String::new(),
        type_names: files
            .iter()
            .flat_map(|file| file.hir.type_names.iter().cloned())
            .collect(),
        value_types: HashMap::new(),
        value_field_types: HashMap::new(),
        instance_arguments: HashMap::new(),
        identity_types: HashMap::new(),
        conditional_counter: 0,
        program: Program {
            statements: files
                .iter()
                .flat_map(|file| file.hir.program.statements.clone())
                .collect(),
        },
        current_module: String::new(),
        file_imports: Vec::new(),
        type_aliases: HashMap::new(),
        async_counter: 0,
        closure_counter: 0,
        externs: HashMap::new(),
        closure_parameters: HashMap::new(),
        borrow_scopes: vec![HashMap::new()],
        binding_states: vec![HashMap::new()],
    };

    // Register every `extern` block before lowering so calls resolve
    // regardless of declaration order, and collect the module's foreign
    // signature table.
    let mut foreign_table: Vec<ForeignFunction> = Vec::new();
    for file in files {
        for statement in &file.hir.program.statements {
            lowerer.register_externs(statement, &mut foreign_table)?;
        }
    }

    // Resolve unannotated return types before lowering bodies, so calls made
    // earlier in the file see the same inferred result as later calls.
    lowerer.infer_return_types(files);

    for file in files {
        lowerer.current_module = file.path.clone();
        lowerer.file_imports = file.imports.clone();
        lowerer.type_aliases = file.aliases.clone();
        lowerer.source_name = file.hir.source_name.clone();
        for statement in &file.hir.program.statements {
            lowerer.lower_statement(statement)?;
        }
    }
    // Metadata names the entry file (loaded last).
    let entry_source = files
        .last()
        .map(|file| file.hir.source_name.clone())
        .unwrap_or_default();

    let merged = lowerer.program.clone();
    let (type_definitions, imports, exports) = module_declarations(&merged);
    let globals = collect_globals(&merged, &lowerer);
    let signatures = lowerer.signatures.clone();
    let initializers = lowerer.instructions.clone();
    let mut functions = std::mem::take(&mut lowerer.functions);
    if !lowerer.instructions.is_empty() {
        let init_name = if functions.iter().any(|function| function.name == "main") {
            "__module_init"
        } else {
            "main"
        };
        functions.push(create_function(
            init_name.to_owned(),
            Vec::new(),
            Type::Void,
            std::mem::take(&mut lowerer.instructions),
            Span::default(),
            &entry_source,
        ));
    } else if functions.is_empty() {
        functions.push(create_function(
            "main".to_owned(),
            Vec::new(),
            Type::Void,
            Vec::new(),
            Span::default(),
            &entry_source,
        ));
    }
    let entry_point = functions
        .iter()
        .find(|function| function.name == "main")
        .or_else(|| {
            functions
                .iter()
                .find(|function| function.name == "__module_init")
        })
        .map(|function| function.name.clone());
    let module = Module {
        name: "main".to_owned(),
        functions,
        signatures,
        foreign_functions: foreign_table,
        type_definitions,
        globals,
        initializers,
        imports,
        exports,
        metadata: ModuleMetadata {
            format_version: 1,
            source_name: entry_source.clone(),
            concurrency: ConcurrencyOp {
                has_async: false,
                has_spawn: false,
                has_channel_ops: false,
                task_count: 0,
            },
            foreign_abis: Vec::new(),
            debug: ModuleDebugInfo {
                build_timestamp: None,
                compiler_version: None,
            },
        },
        entry_point,
    };
    module.verify()?;
    Ok(module)
}

fn create_function(
    name: String,
    parameters: Vec<(String, Type)>,
    return_type: Type,
    instructions: Vec<Instruction>,
    span: Span,
    source_name: &str,
) -> Function {
    let mut builder = CfgBuilder::new();
    let entry = builder.create_block();
    let end = builder.lower_sequence(&instructions, entry);
    if !builder.is_terminated[end as usize] {
        let terminator = if matches!(return_type, Type::Void | Type::Unknown) {
            Terminator::Return { value: None, span }
        } else {
            Terminator::Unreachable { span }
        };
        builder.terminate(end, terminator);
    }

    let loops = std::mem::take(&mut builder.loops);
    let basic_blocks = builder.finish();
    let mut function = Function {
        values: Vec::new(),
        concurrency: Vec::new(),
        loops,
        name,
        parameters,
        return_type,
        basic_blocks,
        debug: FunctionDebugInfo {
            source_name: source_name.to_owned(),
            function_span: span,
            instruction_spans: Vec::new(),
            local_names: Vec::new(),
            line_numbers: Vec::new(),
        },
    };
    function.refresh_metadata();
    function
}

fn collect_value_metadata(instructions: &[Instruction]) -> Vec<ValueMetadata> {
    // Flatten nested try regions so definitions inside them are tracked.
    let mut flat: Vec<&Instruction> = Vec::new();
    for instruction in instructions {
        push_flattened(instruction, &mut flat);
    }
    flat.into_iter()
        .flat_map(|instruction| {
            let ownership = match instruction {
                Instruction::Borrow { mutable: true, .. } => Ownership::MutableBorrow,
                Instruction::Borrow { .. } => Ownership::Borrowed,
                _ => Ownership::Unspecified,
            };
            instruction_results(instruction)
                .into_iter()
                .map(move |value| ValueMetadata {
                    value,
                    ownership,
                    lifetime: None,
                })
        })
        .collect()
}

impl Function {
    pub fn refresh_metadata(&mut self) {
        let instructions = self
            .basic_blocks
            .iter()
            .flat_map(|block| block.instructions.iter().cloned())
            .collect::<Vec<_>>();
        self.values = collect_value_metadata(&instructions);
        self.concurrency = collect_concurrency_metadata(&instructions);
        self.debug.instruction_spans = self
            .basic_blocks
            .iter()
            .flat_map(|block| {
                block
                    .instructions
                    .iter()
                    .map(instruction_span)
                    .chain(std::iter::once(terminator_span(&block.terminator)))
            })
            .collect();
    }
}

fn collect_concurrency_metadata(instructions: &[Instruction]) -> Vec<ConcurrencyMetadata> {
    // Flatten nested try regions so concurrency ops inside them are tracked.
    let mut flat: Vec<&Instruction> = Vec::new();
    for instruction in instructions {
        push_flattened(instruction, &mut flat);
    }
    flat.into_iter()
        .filter_map(|instruction| match instruction {
            Instruction::Spawn { task, span, .. } => Some(ConcurrencyMetadata::Spawn {
                task: *task,
                span: *span,
            }),
            Instruction::Await { task, span, .. } => Some(ConcurrencyMetadata::Await {
                task: *task,
                span: *span,
            }),
            Instruction::ChannelSend { channel, span, .. } => {
                Some(ConcurrencyMetadata::ChannelSend {
                    channel: *channel,
                    span: *span,
                })
            }
            Instruction::ChannelReceive { channel, span, .. } => {
                Some(ConcurrencyMetadata::ChannelReceive {
                    channel: *channel,
                    span: *span,
                })
            }
            Instruction::ActorDispatch { actor, span, .. } => {
                Some(ConcurrencyMetadata::ActorDispatch {
                    actor: *actor,
                    span: *span,
                })
            }
            Instruction::CancelTask { task, span } => Some(ConcurrencyMetadata::Cancellation {
                task: *task,
                span: *span,
            }),
            _ => None,
        })
        .collect()
}

/// True when a written type annotation denotes a reference (`&T` / `&mut T`).
fn is_reference_annotation(annotation: &str) -> bool {
    annotation.trim_start().starts_with('&')
}

/// Reject reference types in positions that would let a reference outlive the
/// borrow scope. NXL borrows are lexical: a reference may be read and written,
/// passed to a function, and held in a local binding, but it may not be
/// returned, stored in a field or enum payload, or captured by a closure.
fn validate_ownership_declarations(files: &[ProjectFile]) -> Result<(), Diagnostic> {
    for file in files {
        for statement in &file.hir.program.statements {
            validate_ownership_statement(statement)?;
        }
    }
    Ok(())
}

fn validate_ownership_statement(statement: &Statement) -> Result<(), Diagnostic> {
    match statement {
        Statement::Export(inner) => validate_ownership_statement(inner),
        Statement::Function(function) => validate_function_ownership(function),
        Statement::ImplBlock(block) => {
            for method in &block.methods {
                validate_function_ownership(method)?;
            }
            Ok(())
        }
        Statement::TypeDeclaration(declaration) => {
            for method in &declaration.methods {
                validate_function_ownership(method)?;
            }
            for field in &declaration.fields {
                if is_reference_annotation(&field.annotation) {
                    return Err(Diagnostic::error(
                        "NXL2017",
                        format!(
                            "field '{}' cannot hold a reference: references may not escape the scope of the value they borrow",
                            field.name
                        ),
                        field.span,
                    ));
                }
            }
            for variant in &declaration.variants {
                for payload in &variant.payload {
                    if is_reference_annotation(payload) {
                        return Err(Diagnostic::error(
                            "NXL2017",
                            format!(
                                "enum payload of '{}' cannot hold a reference: references may not escape the scope of the value they borrow",
                                variant.name
                            ),
                            variant.span,
                        ));
                    }
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_function_ownership(function: &crate::ast::Function) -> Result<(), Diagnostic> {
    if let Some(return_type) = &function.return_type {
        if is_reference_annotation(return_type) {
            return Err(Diagnostic::error(
                "NXL2017",
                "a function cannot return a reference: the borrow ends when the function returns",
                function.span,
            )
            .with_help("return the owned value, or let the caller borrow it"));
        }
    }
    Ok(())
}

fn module_declarations(program: &Program) -> (Vec<TypeDefinition>, Vec<String>, Vec<String>) {
    let mut type_definitions = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    for statement in &program.statements {
        match statement {
            Statement::TypeDeclaration(declaration) => {
                let kind = match declaration.kind {
                    crate::ast::DeclarationKind::Struct => TypeDefinitionKind::Struct,
                    crate::ast::DeclarationKind::Class => TypeDefinitionKind::Class,
                    crate::ast::DeclarationKind::Actor => TypeDefinitionKind::Actor,
                    crate::ast::DeclarationKind::Enum => TypeDefinitionKind::Enum,
                    crate::ast::DeclarationKind::Interface => TypeDefinitionKind::Interface,
                    crate::ast::DeclarationKind::Trait => TypeDefinitionKind::Trait,
                };
                let parameters = declaration
                    .type_parameters
                    .iter()
                    .map(|parameter| stable_type_id(parameter))
                    .collect::<Vec<_>>();
                type_definitions.push(TypeDefinition {
                    id: stable_type_id(&declaration.name),
                    name: declaration.name.clone(),
                    kind,
                    parameters,
                    fields: declaration
                        .fields
                        .iter()
                        .map(|field| {
                            let ty = type_from_name_with(
                                &field.annotation,
                                &HashMap::new(),
                                &declaration.type_parameters,
                            )
                            .unwrap_or_else(|| Type::Generic(stable_type_id(&field.annotation)));
                            (field.name.clone(), ty)
                        })
                        .collect(),
                    variants: declaration
                        .variants
                        .iter()
                        .map(|variant| {
                            (
                                variant.name.clone(),
                                variant
                                    .payload
                                    .iter()
                                    .map(|ty| {
                                        type_from_name_with(
                                            ty,
                                            &HashMap::new(),
                                            &declaration.type_parameters,
                                        )
                                        .unwrap_or_else(|| Type::Generic(stable_type_id(ty)))
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                });
            }
            Statement::TypeAlias(alias) => type_definitions.push(TypeDefinition {
                id: stable_type_id(&alias.name),
                name: alias.name.clone(),
                kind: TypeDefinitionKind::Alias,
                parameters: alias
                    .type_parameters
                    .iter()
                    .map(|parameter| stable_type_id(parameter))
                    .collect(),
                fields: vec![(
                    "target".to_owned(),
                    type_from_name(&alias.target, &HashMap::new())
                        .unwrap_or_else(|| Type::Generic(stable_type_id(&alias.target))),
                )],
                variants: Vec::new(),
            }),
            Statement::Import { path, .. } => imports.push(path.join(".")),
            Statement::Export(inner) => {
                if let Some(name) = exported_name(inner) {
                    exports.push(name);
                }
            }
            _ => {}
        }
    }
    (type_definitions, imports, exports)
}

fn exported_name(statement: &Statement) -> Option<String> {
    match statement {
        Statement::Function(function) => function.name.clone(),
        Statement::Let { name, .. } => Some(name.clone()),
        Statement::TypeDeclaration(declaration) => Some(declaration.name.clone()),
        Statement::TypeAlias(alias) => Some(alias.name.clone()),
        Statement::Export(inner) => exported_name(inner),
        _ => None,
    }
}

fn collect_globals(program: &Program, lowerer: &Lowerer) -> Vec<Global> {
    program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Let {
                name,
                annotation,
                is_mut,
                span,
                ..
            } => {
                let initializer = lowerer.lookup_value(name);
                let ty = annotation
                    .as_ref()
                    .and_then(|annotation| type_from_name(annotation, &HashMap::new()))
                    .or_else(|| initializer.map(|value| lowerer.value_type(value)))
                    .unwrap_or(Type::Unknown);
                Some(Global {
                    name: name.clone(),
                    ty,
                    initializer,
                    mutable: *is_mut,
                    span: *span,
                })
            }
            _ => None,
        })
        .collect()
}

struct CfgBuilder {
    blocks: Vec<BasicBlock>,
    is_terminated: Vec<bool>,
    loop_targets: Vec<(BasicBlockId, BasicBlockId)>,
    loops: Vec<LoopMetadata>,
}

impl CfgBuilder {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            is_terminated: Vec::new(),
            loop_targets: Vec::new(),
            loops: Vec::new(),
        }
    }

    fn create_block(&mut self) -> BasicBlockId {
        let id = self.blocks.len() as BasicBlockId;
        self.blocks.push(BasicBlock {
            id,
            instructions: Vec::new(),
            terminator: Terminator::Unreachable {
                span: Span::default(),
            },
        });
        self.is_terminated.push(false);
        id
    }

    fn terminate(&mut self, block: BasicBlockId, terminator: Terminator) {
        self.blocks[block as usize].terminator = terminator;
        self.is_terminated[block as usize] = true;
    }

    fn lower_sequence(
        &mut self,
        instructions: &[Instruction],
        mut current: BasicBlockId,
    ) -> BasicBlockId {
        for instruction in instructions {
            if self.is_terminated[current as usize] {
                current = self.create_block();
            }
            match instruction {
                Instruction::If {
                    condition,
                    then_branch,
                    else_branch,
                    span,
                } => {
                    let then_block = self.create_block();
                    let else_block = self.create_block();
                    let merge_block = self.create_block();
                    self.terminate(
                        current,
                        Terminator::Branch {
                            condition: *condition,
                            then_block,
                            else_block,
                            span: *span,
                        },
                    );
                    let then_end = self.lower_sequence(then_branch, then_block);
                    if !self.is_terminated[then_end as usize] {
                        self.terminate(
                            then_end,
                            Terminator::Jump {
                                target: merge_block,
                                span: *span,
                            },
                        );
                    }
                    let else_end = else_branch
                        .as_ref()
                        .map_or(else_block, |items| self.lower_sequence(items, else_block));
                    if !self.is_terminated[else_end as usize] {
                        self.terminate(
                            else_end,
                            Terminator::Jump {
                                target: merge_block,
                                span: *span,
                            },
                        );
                    }
                    current = merge_block;
                }
                Instruction::Loop {
                    condition,
                    condition_instructions,
                    body,
                    span,
                } => {
                    let header = self.create_block();
                    let body_block = self.create_block();
                    let exit = self.create_block();
                    self.terminate(
                        current,
                        Terminator::Jump {
                            target: header,
                            span: *span,
                        },
                    );
                    self.blocks[header as usize]
                        .instructions
                        .extend(condition_instructions.iter().cloned());
                    self.loop_targets.push((header, exit));
                    self.loops.push(LoopMetadata {
                        header,
                        body: body_block,
                        exit,
                    });
                    if let Some(condition) = condition {
                        self.terminate(
                            header,
                            Terminator::Branch {
                                condition: *condition,
                                then_block: body_block,
                                else_block: exit,
                                span: *span,
                            },
                        );
                    } else {
                        self.terminate(
                            header,
                            Terminator::Jump {
                                target: body_block,
                                span: *span,
                            },
                        );
                    }
                    let body_end = self.lower_sequence(body, body_block);
                    if !self.is_terminated[body_end as usize] {
                        self.terminate(
                            body_end,
                            Terminator::Jump {
                                target: header,
                                span: *span,
                            },
                        );
                    }
                    self.loop_targets.pop();
                    current = exit;
                }
                Instruction::Return { value, span } => {
                    self.terminate(
                        current,
                        Terminator::Return {
                            value: *value,
                            span: *span,
                        },
                    );
                }
                Instruction::Break { span } => {
                    if let Some((_, exit)) = self.loop_targets.last().copied() {
                        self.terminate(
                            current,
                            Terminator::Jump {
                                target: exit,
                                span: *span,
                            },
                        );
                    } else {
                        self.blocks[current as usize]
                            .instructions
                            .push(instruction.clone());
                    }
                }
                Instruction::Continue { span } => {
                    if let Some((header, _)) = self.loop_targets.last().copied() {
                        self.terminate(
                            current,
                            Terminator::Jump {
                                target: header,
                                span: *span,
                            },
                        );
                    } else {
                        self.blocks[current as usize]
                            .instructions
                            .push(instruction.clone());
                    }
                }
                _ => self.blocks[current as usize]
                    .instructions
                    .push(instruction.clone()),
            }
        }
        current
    }

    fn finish(self) -> Vec<BasicBlock> {
        self.blocks
    }
}

impl Module {
    pub fn verify(&self) -> Result<(), Diagnostic> {
        if self.metadata.format_version == 0 || self.metadata.source_name.is_empty() {
            return Err(Diagnostic::error(
                "NXL3001",
                "module is missing valid NIR format/source metadata",
                Span::default(),
            ));
        }
        let mut function_names = std::collections::HashSet::new();
        let mut signature_names = std::collections::HashSet::new();
        for signature in &self.signatures {
            if !signature_names.insert(signature.name.as_str()) {
                return Err(Diagnostic::error(
                    "NXL3001",
                    format!("module contains duplicate signature '{}'", signature.name),
                    Span::default(),
                ));
            }
        }
        for function in &self.functions {
            if !function_names.insert(function.name.as_str()) {
                return Err(Diagnostic::error(
                    "NXL3001",
                    format!("module contains duplicate function '{}'", function.name),
                    function.debug.function_span,
                ));
            }
        }
        if let Some(entry_point) = &self.entry_point {
            if !function_names.contains(entry_point.as_str()) {
                return Err(Diagnostic::error(
                    "NXL3001",
                    format!("module entry point '{entry_point}' is not defined"),
                    Span::default(),
                ));
            }
        }
        let mut type_ids = std::collections::HashSet::new();
        let mut type_names = std::collections::HashSet::new();
        for definition in &self.type_definitions {
            if !type_ids.insert(definition.id) || !type_names.insert(definition.name.as_str()) {
                return Err(Diagnostic::error(
                    "NXL3001",
                    format!("module contains duplicate type '{}'", definition.name),
                    Span::default(),
                ));
            }
        }
        let mut foreign_names = std::collections::HashSet::new();
        for foreign in &self.foreign_functions {
            if !foreign_names.insert(foreign.name.as_str()) {
                return Err(Diagnostic::error(
                    "NXL3001",
                    format!("module contains duplicate foreign ABI '{}'", foreign.name),
                    Span::default(),
                ));
            }
        }
        for function in &self.functions {
            if let Some(signature) = self
                .signatures
                .iter()
                .find(|signature| signature.name == function.name)
            {
                if signature.parameters != function.parameters
                    || signature.return_type != function.return_type
                {
                    return Err(Diagnostic::error(
                        "NXL3001",
                        format!(
                            "function '{}' does not match its module signature",
                            function.name
                        ),
                        function.debug.function_span,
                    ));
                }
            }
        }
        for function in &self.functions {
            verify_function(function, &self.signatures, &self.foreign_functions)?;
        }
        Ok(())
    }
}

fn verify_function(
    function: &Function,
    signatures: &[FunctionSignature],
    foreign_functions: &[ForeignFunction],
) -> Result<(), Diagnostic> {
    if function.basic_blocks.is_empty() {
        return Err(Diagnostic::error(
            "NXL3001",
            format!("function '{}' has no basic blocks", function.name),
            Span::default(),
        ));
    }

    let mut seen_param_names = std::collections::HashSet::new();
    for (pname, _) in &function.parameters {
        if !seen_param_names.insert(pname) {
            return Err(Diagnostic::error(
                "NXL3001",
                format!(
                    "function '{}' has duplicate parameter name '{pname}'",
                    function.name
                ),
                function.debug.function_span,
            ));
        }
    }

    let block_ids = function
        .basic_blocks
        .iter()
        .map(|block| block.id)
        .collect::<std::collections::HashSet<_>>();
    if block_ids.len() != function.basic_blocks.len()
        || !block_ids.contains(&function.basic_blocks[0].id)
    {
        return Err(Diagnostic::error(
            "NXL3001",
            format!(
                "function '{}' has duplicate block identifiers",
                function.name
            ),
            Span::default(),
        ));
    }
    if function.debug.source_name.is_empty()
        || function.debug.instruction_spans.len()
            != function
                .basic_blocks
                .iter()
                .map(|block| block.instructions.len() + 1)
                .sum::<usize>()
    {
        return Err(Diagnostic::error(
            "NXL3001",
            format!(
                "function '{}' has inconsistent debug metadata",
                function.name
            ),
            function.debug.function_span,
        ));
    }

    let entry_id = function.basic_blocks[0].id;
    let mut definitions = std::collections::HashMap::new();
    let mut value_types = std::collections::HashMap::new();
    // Nested try regions are indexed in flattened execution order so that
    // definitions inside them strictly precede their later uses.
    for block in &function.basic_blocks {
        let mut flat: Vec<&Instruction> = Vec::new();
        for instruction in &block.instructions {
            push_flattened(instruction, &mut flat);
        }
        for (index, instruction) in flat.iter().enumerate() {
            for result in instruction_results(instruction) {
                if definitions.insert(result, (block.id, index)).is_some() {
                    return Err(Diagnostic::error(
                        "NXL3001",
                        format!(
                            "function '{}' defines value {} more than once",
                            function.name, result
                        ),
                        instruction_span(instruction),
                    ));
                }
            }
            for (value, ty) in instruction_value_types(instruction) {
                value_types.insert(value, ty);
            }
        }
    }
    let parameter_instructions = function.basic_blocks[0]
        .instructions
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::Parameter {
                index, ty, span, ..
            } => Some((*index, *ty, *span)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if parameter_instructions.len() != function.parameters.len()
        || parameter_instructions
            .iter()
            .enumerate()
            .any(|(expected, (actual, ty, _))| {
                *actual != expected || *ty != function.parameters[expected].1
            })
    {
        return Err(Diagnostic::error(
            "NXL3001",
            format!(
                "function '{}' has inconsistent parameter instructions",
                function.name
            ),
            parameter_instructions
                .first()
                .map_or(function.debug.function_span, |(_, _, span)| *span),
        ));
    }
    let metadata_values = function
        .values
        .iter()
        .map(|metadata| metadata.value)
        .collect::<std::collections::HashSet<_>>();
    if metadata_values.len() != function.values.len()
        || metadata_values != definitions.keys().copied().collect()
    {
        return Err(Diagnostic::error(
            "NXL3001",
            format!(
                "function '{}' has inconsistent value metadata",
                function.name
            ),
            function.debug.function_span,
        ));
    }

    let mut predecessors = std::collections::HashMap::<BasicBlockId, Vec<BasicBlockId>>::new();
    for block in &function.basic_blocks {
        let targets = match &block.terminator {
            Terminator::Jump { target, .. } => vec![*target],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            Terminator::Return { .. } | Terminator::Unreachable { .. } => Vec::new(),
        };
        if targets.iter().any(|target| !block_ids.contains(target)) {
            return Err(Diagnostic::error(
                "NXL3001",
                format!(
                    "function '{}' block {} branches to a missing block",
                    function.name, block.id
                ),
                terminator_span(&block.terminator),
            ));
        }
        for target in &targets {
            predecessors.entry(*target).or_default().push(block.id);
        }
    }

    let reachable = reachable_blocks(entry_id, function);
    let dominators = compute_dominators(entry_id, &reachable, &predecessors);
    for block in &function.basic_blocks {
        if !reachable.contains(&block.id) {
            continue;
        }
        let mut flat: Vec<&Instruction> = Vec::new();
        for instruction in &block.instructions {
            push_flattened(instruction, &mut flat);
        }
        for (index, instruction) in flat.iter().enumerate() {
            for operand in instruction_operands(instruction) {
                verify_use(
                    function,
                    operand,
                    block.id,
                    index,
                    &definitions,
                    &dominators,
                    instruction_span(instruction),
                )?;
            }
        }
        let terminator_span = terminator_span(&block.terminator);
        match &block.terminator {
            Terminator::Branch { condition, .. } => {
                verify_use(
                    function,
                    *condition,
                    block.id,
                    flat.len(),
                    &definitions,
                    &dominators,
                    terminator_span,
                )?;
                if value_types
                    .get(condition)
                    .is_some_and(|ty| *ty != Type::Bool && !is_dynamic(*ty))
                {
                    return Err(Diagnostic::error(
                        "NXL3001",
                        format!(
                            "function '{}' branches on a non-boolean value",
                            function.name
                        ),
                        terminator_span,
                    ));
                }
            }
            Terminator::Return { value, .. } => {
                if let Some(value) = value {
                    verify_use(
                        function,
                        *value,
                        block.id,
                        flat.len(),
                        &definitions,
                        &dominators,
                        terminator_span,
                    )?;
                    let actual = value_types.get(value).copied().unwrap_or(Type::Unknown);
                    // `types_compatible`, not `!=`: an array literal is inferred
                    // as `Array(0)` (element type not pinned down), while the
                    // declared type is `Array(<id>)`. A raw comparison rejected
                    // every `fn f() -> [String]`, so the return contract could
                    // not be satisfied by any array-returning function.
                    if !types_compatible(actual, function.return_type) {
                        return Err(Diagnostic::error(
                            "NXL3001",
                            format!(
                                "function '{}' returns {actual:?}, expected {:?}",
                                function.name, function.return_type
                            ),
                            terminator_span,
                        ));
                    }
                } else if !matches!(function.return_type, Type::Void | Type::Unknown) {
                    return Err(Diagnostic::error(
                        "NXL3001",
                        format!(
                            "function '{}' has a value-less return for {:?}",
                            function.name, function.return_type
                        ),
                        terminator_span,
                    ));
                }
            }
            Terminator::Jump { .. } | Terminator::Unreachable { .. } => {}
        }
    }

    // Validate loop metadata if present.
    if !function.loops.is_empty() {
        let block_ids: std::collections::HashSet<BasicBlockId> =
            function.basic_blocks.iter().map(|b| b.id).collect();
        for (index, loop_info) in function.loops.iter().enumerate() {
            if !block_ids.contains(&loop_info.header) {
                return Err(Diagnostic::error(
                    "NXL3001",
                    format!(
                        "function '{}' loop {}: header bb{} is not a valid block",
                        function.name, index, loop_info.header
                    ),
                    Span::default(),
                ));
            }
            if !block_ids.contains(&loop_info.body) {
                return Err(Diagnostic::error(
                    "NXL3001",
                    format!(
                        "function '{}' loop {}: body bb{} is not a valid block",
                        function.name, index, loop_info.body
                    ),
                    Span::default(),
                ));
            }
            if !block_ids.contains(&loop_info.exit) {
                return Err(Diagnostic::error(
                    "NXL3001",
                    format!(
                        "function '{}' loop {}: exit bb{} is not a valid block",
                        function.name, index, loop_info.exit
                    ),
                    Span::default(),
                ));
            }
            // Strict dominance (header dominates body, body dominates exit) is
            // deliberately not required: `break`/`continue` lowering can make a
            // body block reachable from outside the natural loop nest, and the
            // backends only use this metadata as a reconstruction hint.
        }
    }

    verify_instruction_types(function, signatures, foreign_functions, &value_types)?;
    Ok(())
}

fn verify_use(
    function: &Function,
    value: ValueId,
    use_block: BasicBlockId,
    use_index: usize,
    definitions: &std::collections::HashMap<ValueId, (BasicBlockId, usize)>,
    dominators: &std::collections::HashMap<BasicBlockId, std::collections::HashSet<BasicBlockId>>,
    span: Span,
) -> Result<(), Diagnostic> {
    let Some((definition_block, definition_index)) = definitions.get(&value).copied() else {
        return Err(Diagnostic::error(
            "NXL3001",
            format!(
                "function '{}' uses undefined value {}",
                function.name, value
            ),
            span,
        ));
    };
    let valid = if definition_block == use_block {
        definition_index < use_index
    } else {
        dominators
            .get(&use_block)
            .is_some_and(|set| set.contains(&definition_block))
    };
    if !valid {
        return Err(Diagnostic::error(
            "NXL3001",
            format!(
                "value {} does not dominate its use in block {}",
                value, use_block
            ),
            span,
        ));
    }
    Ok(())
}

fn reachable_blocks(
    entry: BasicBlockId,
    function: &Function,
) -> std::collections::HashSet<BasicBlockId> {
    let mut reachable = std::collections::HashSet::new();
    let mut pending = vec![entry];
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let Some(block) = function.basic_blocks.iter().find(|block| block.id == id) else {
            continue;
        };
        match &block.terminator {
            Terminator::Jump { target, .. } => pending.push(*target),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                pending.push(*then_block);
                pending.push(*else_block);
            }
            Terminator::Return { .. } | Terminator::Unreachable { .. } => {}
        }
    }
    reachable
}

fn compute_dominators(
    entry: BasicBlockId,
    reachable: &std::collections::HashSet<BasicBlockId>,
    predecessors: &std::collections::HashMap<BasicBlockId, Vec<BasicBlockId>>,
) -> std::collections::HashMap<BasicBlockId, std::collections::HashSet<BasicBlockId>> {
    let mut dominators = reachable
        .iter()
        .map(|id| {
            (
                *id,
                if *id == entry {
                    [entry].into_iter().collect()
                } else {
                    reachable.clone()
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();

    loop {
        let mut changed = false;
        for block in reachable.iter().filter(|id| **id != entry) {
            let preds = predecessors
                .get(block)
                .into_iter()
                .flatten()
                .filter_map(|pred| dominators.get(pred))
                .collect::<Vec<_>>();
            let mut next = if let Some(first) = preds.first() {
                (*first).clone()
            } else {
                std::collections::HashSet::new()
            };
            for pred in preds.iter().skip(1) {
                next.retain(|id| pred.contains(id));
            }
            next.insert(*block);
            if dominators.get(block) != Some(&next) {
                dominators.insert(*block, next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    dominators
}

fn verify_instruction_types(
    function: &Function,
    signatures: &[FunctionSignature],
    foreign_functions: &[ForeignFunction],
    types: &std::collections::HashMap<ValueId, Type>,
) -> Result<(), Diagnostic> {
    for block in &function.basic_blocks {
        // Nested try regions are checked as if their instructions appeared
        // inline at the Try's position; execution order matches the flattened
        // sequence, so positional diagnostics stay accurate.
        let mut flat: Vec<&Instruction> = Vec::new();
        for instruction in &block.instructions {
            push_flattened(instruction, &mut flat);
        }
        for instruction in flat {
            let span = instruction_span(instruction);
            let mismatch = |message: String| Diagnostic::error("NXL3001", message, span);
            match instruction {
                Instruction::Unary {
                    operator,
                    operand,
                    ty,
                    ..
                } => {
                    let operand_type = types.get(operand).copied().unwrap_or(Type::Unknown);
                    let expected = match operator {
                        UnaryOperator::Negate
                            if matches!(operand_type, Type::Int | Type::Float) =>
                        {
                            operand_type
                        }
                        UnaryOperator::Not
                            if matches!(operand_type, Type::Bool | Type::Unknown) =>
                        {
                            // `Unknown` is accepted here, exactly as the
                            // comparison operators accept it. A value read out
                            // of a dynamic structure is not statically known to
                            // be Bool; the runtime still checks it.
                            operand_type
                        }
                        _ => {
                            return Err(mismatch(format!(
                                "unary operation {:?} is invalid for {operand_type:?}",
                                operator
                            )))
                        }
                    };
                    // `expected` can be Unknown too (when the operand was), so this
                    // compares the two the same way every other contract
                    // check does rather than one-directionally.
                    if !types_compatible(*ty, expected) {
                        return Err(mismatch("unary result type is inconsistent".to_owned()));
                    }
                }
                Instruction::Binary {
                    operator,
                    left,
                    right,
                    ty,
                    ..
                } => {
                    let left_ty = types.get(left).copied().unwrap_or(Type::Unknown);
                    let right_ty = types.get(right).copied().unwrap_or(Type::Unknown);
                    // Arrays compare structurally regardless of element precision.
                    let both_arrays =
                        matches!((&left_ty, &right_ty), (Type::Array(_), Type::Array(_)));
                    if !both_arrays
                        && left_ty != right_ty
                        && !is_dynamic(left_ty)
                        && !is_dynamic(right_ty)
                    {
                        return Err(mismatch(format!(
                            "binary operands have incompatible types {left_ty:?} and {right_ty:?}"
                        )));
                    }
                    let boolean_result = matches!(
                        operator,
                        BinaryOperator::Equal
                            | BinaryOperator::NotEqual
                            | BinaryOperator::Less
                            | BinaryOperator::LessEqual
                            | BinaryOperator::Greater
                            | BinaryOperator::GreaterEqual
                            | BinaryOperator::And
                            | BinaryOperator::Or
                    );
                    if (boolean_result && *ty != Type::Bool)
                        || (!boolean_result
                            && *ty != left_ty
                            && !is_dynamic(*ty)
                            && !is_dynamic(left_ty))
                    {
                        return Err(mismatch(
                            "binary instruction result type is inconsistent".to_owned(),
                        ));
                    }
                }
                Instruction::Call {
                    callee,
                    type_arguments,
                    arguments,
                    return_type,
                    ..
                } => {
                    if let Some(signature) = signatures.iter().find(|sig| sig.name == *callee) {
                        if signature.type_parameters.is_empty() {
                            if arguments.len() != signature.parameters.len()
                                || (*return_type != signature.return_type
                                    && !is_dynamic(*return_type))
                            {
                                return Err(mismatch(format!(
                                    "call to '{callee}' does not match its NIR signature"
                                )));
                            }
                            for (argument, (_, expected)) in
                                arguments.iter().zip(&signature.parameters)
                            {
                                let actual = types.get(argument).copied().unwrap_or(Type::Unknown);
                                // Same reasoning as the return contract: an array
                                // literal is `Array(0)`, and a parameter declared as
                                // `[T]` is `Array(<id>)`, so a raw comparison
                                // rejected every call passing an array literal.
                                if !types_compatible(actual, *expected) {
                                    return Err(mismatch(format!(
                                        "call argument has type {actual:?}, expected {expected:?}"
                                    )));
                                }
                            }
                        } else if type_arguments.len() != signature.type_parameters.len()
                            || arguments.len() != signature.parameters.len()
                        {
                            // The lowerer substitutes explicit arguments into the
                            // signature, so the erased `Param` contract cannot be
                            // compared here; arity consistency still can.
                            return Err(mismatch(format!(
                                "generic call to '{callee}' does not match its NIR signature"
                            )));
                        }
                    }
                    if let Some(foreign) = foreign_functions
                        .iter()
                        .find(|signature| signature.name == *callee)
                    {
                        if arguments.len() != foreign.parameters.len() {
                            return Err(mismatch(format!(
                                "foreign call to '{callee}' does not match its ABI signature"
                            )));
                        }
                    }
                }
                Instruction::ForeignCall {
                    callee,
                    arguments,
                    parameter_types,
                    ..
                } => {
                    if arguments.len() != parameter_types.len() {
                        return Err(mismatch(format!(
                            "foreign call to '{callee}' does not match its ABI signature"
                        )));
                    }
                }
                Instruction::Allocate { ty, .. } => {
                    // Any value type may be placed in a borrow cell; only a
                    // reference-to-reference would be meaningless here.
                    if matches!(ty, Type::Reference(..)) {
                        return Err(mismatch(format!("cannot allocate value of type {ty:?}")));
                    }
                }
                Instruction::LoadMemory { pointer, ty, .. } => {
                    let pointer_type = types.get(pointer).copied().unwrap_or(Type::Unknown);
                    if let Type::Reference(target, _) = pointer_type {
                        if target != type_identity(*ty) {
                            return Err(mismatch(
                                "loaded type does not match reference target".to_owned(),
                            ));
                        }
                    } else if pointer_type != Type::Unknown {
                        return Err(mismatch(
                            "memory load requires a reference value".to_owned(),
                        ));
                    }
                }
                Instruction::StoreMemory { pointer, value, .. } => {
                    let pointer_type = types.get(pointer).copied().unwrap_or(Type::Unknown);
                    let value_type = types.get(value).copied().unwrap_or(Type::Unknown);
                    match pointer_type {
                        Type::Reference(target, true)
                            if value_type == Type::Unknown
                                || target == type_identity(value_type)
                                // An array literal carries the placeholder
                                // `Array(0)` until its element type is pinned;
                                // it is compatible with any array cell.
                                || matches!(value_type, Type::Array(0)) => {}
                        Type::Reference(_, false) => {
                            return Err(mismatch(
                                "cannot store through an immutable reference".to_owned(),
                            ))
                        }
                        Type::Reference(_, true) => {
                            return Err(mismatch(
                                "stored value does not match reference target".to_owned(),
                            ))
                        }
                        Type::Unknown => {}
                        _ => {
                            return Err(mismatch(
                                "memory store requires a mutable reference".to_owned(),
                            ))
                        }
                    }
                }
                Instruction::Borrow {
                    value, mutable, ty, ..
                } => {
                    let value_type = types.get(value).copied().unwrap_or(Type::Unknown);
                    // The source is normally an allocated cell (`Allocate`
                    // yields a mutable reference to the slot), but a plain
                    // value or an existing reference is also accepted as long
                    // as the recorded target agrees.
                    let source_target = match value_type {
                        Type::Unknown => None,
                        Type::Reference(target, _) => Some(target),
                        other => Some(type_identity(other)),
                    };
                    match ty {
                        Type::Reference(target, is_mutable)
                            if *is_mutable == *mutable
                                && source_target.is_none_or(|source| *target == source) => {}
                        _ => {
                            return Err(mismatch(
                                "borrow result type does not match its source".to_owned(),
                            ))
                        }
                    }
                }
                Instruction::Spawn { task, ty, .. } => {
                    let task_type = types.get(task).copied().unwrap_or(Type::Unknown);
                    if !matches!(task_type, Type::Task(_) | Type::Unknown)
                        || !matches!(ty, Type::Task(_) | Type::Unknown)
                    {
                        return Err(mismatch(
                            "spawn requires and returns a task value".to_owned(),
                        ));
                    }
                }
                Instruction::Await { task, ty, .. } => {
                    let task_type = types.get(task).copied().unwrap_or(Type::Unknown);
                    match task_type {
                        Type::Task(result) if result == type_identity(*ty) => {}
                        Type::Task(_) => {
                            return Err(mismatch(
                                "await result type does not match task type".to_owned(),
                            ))
                        }
                        Type::Unknown => {}
                        _ => return Err(mismatch("await requires a task value".to_owned())),
                    }
                }
                Instruction::ChannelSend { channel, value, .. } => {
                    let channel_type = types.get(channel).copied().unwrap_or(Type::Unknown);
                    let value_type = types.get(value).copied().unwrap_or(Type::Unknown);
                    match channel_type {
                        Type::Channel(element)
                            if value_type == Type::Unknown
                                || element == type_identity(value_type) => {}
                        Type::Channel(_) => {
                            return Err(mismatch(
                                "sent value does not match channel element type".to_owned(),
                            ))
                        }
                        Type::Unknown => {}
                        _ => {
                            return Err(mismatch(
                                "channel send requires a channel value".to_owned(),
                            ))
                        }
                    }
                }
                Instruction::ChannelReceive { channel, ty, .. } => {
                    let channel_type = types.get(channel).copied().unwrap_or(Type::Unknown);
                    match channel_type {
                        Type::Channel(element) if element == type_identity(*ty) => {}
                        Type::Channel(_) => {
                            return Err(mismatch(
                                "receive result type does not match channel element type"
                                    .to_owned(),
                            ))
                        }
                        Type::Unknown => {}
                        _ => {
                            return Err(mismatch(
                                "channel receive requires a channel value".to_owned(),
                            ))
                        }
                    }
                }
                Instruction::ChannelClose { channel, .. } => {
                    let channel_type = types.get(channel).copied().unwrap_or(Type::Unknown);
                    if !matches!(channel_type, Type::Channel(_) | Type::Unknown) {
                        return Err(mismatch(
                            "channel close requires a channel value".to_owned(),
                        ));
                    }
                }
                Instruction::ToString { value, .. } | Instruction::TypeName { value, .. } => {
                    let operand_type = types.get(value).copied().unwrap_or(Type::Unknown);
                    if operand_type == Type::Void {
                        return Err(mismatch(
                            "cannot convert a void value to a string".to_owned(),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Expand nested `try` regions into execution order for verification.
/// The `Try` itself is also emitted (it matches no check arm).
fn push_flattened<'a>(instruction: &'a Instruction, out: &mut Vec<&'a Instruction>) {
    out.push(instruction);
    if let Instruction::Try {
        body,
        catches,
        finally,
        ..
    } = instruction
    {
        for instr in body {
            push_flattened(instr, out);
        }
        for catch in catches {
            for instr in &catch.body {
                push_flattened(instr, out);
            }
        }
        if let Some(fin) = finally {
            for instr in fin {
                push_flattened(instr, out);
            }
        }
    }
}

/// Types that carry no checkable identity: unknown inference results and
/// unsubstituted generic parameters. Both act as wildcards for body checks.
fn is_dynamic(ty: Type) -> bool {
    matches!(ty, Type::Unknown | Type::Param(_))
}

/// Erase an unsubstituted generic parameter to the dynamic type for body
/// checks. The concrete type is supplied at the instantiation site.
fn erased_for_check(ty: Type) -> Type {
    if is_dynamic(ty) {
        Type::Unknown
    } else {
        ty
    }
}

/// Unify two inferred types into the most precise type that describes both.
/// `Err(())` means inference is ambiguous or the two types are incompatible.
/// Used by return-type inference and by conditional expressions so that each
/// branch agrees on a single result type.
fn unify_inferred_types(left: Type, right: Type) -> Result<Type, ()> {
    if left == right {
        return Ok(left);
    }
    if is_dynamic(left) && is_dynamic(right) {
        return Ok(Type::Unknown);
    }
    if is_dynamic(left) {
        return Ok(right);
    }
    if is_dynamic(right) {
        return Ok(left);
    }
    match (left, right) {
        (Type::Array(0), Type::Array(other)) | (Type::Array(other), Type::Array(0)) => {
            Ok(Type::Array(other))
        }
        (Type::Generic(base), Type::Instance(instance, _))
        | (Type::Instance(instance, _), Type::Generic(base))
            if base == instance =>
        {
            Ok(if matches!(left, Type::Instance(_, _)) {
                left
            } else {
                right
            })
        }
        _ => Err(()),
    }
}

/// Infer a single result type from the observed `return` types of a function.
/// A bare `return;` (or `return void`) cannot be mixed with a value return.
fn infer_return_type(returns: &[Option<Type>]) -> Result<Type, ()> {
    let mut has_bare = false;
    let mut inferred: Option<Type> = None;
    for result in returns {
        match result {
            None => has_bare = true,
            Some(Type::Void) => has_bare = true,
            Some(ty) => {
                inferred = Some(match inferred {
                    None => *ty,
                    Some(existing) => unify_inferred_types(existing, *ty)?,
                });
            }
        }
    }
    if has_bare {
        if inferred.is_some() {
            return Err(());
        }
        return Ok(Type::Void);
    }
    Ok(inferred.unwrap_or(Type::Void))
}

/// Combine two type identities into one, order-dependently. Used to build a
/// stable fingerprint for a sequence of generic arguments without needing to
/// reverse a hash back into a name.
fn mix_type_id(left: TypeId, right: TypeId) -> TypeId {
    left.wrapping_mul(16_777_619) ^ right.rotate_left(7)
}

/// A stable fingerprint for the argument list of an instantiated generic type.
fn argument_fingerprint(arguments: &[Type]) -> TypeId {
    arguments.iter().fold(1, |hash, argument| {
        mix_type_id(hash, type_identity(*argument))
    })
}

/// Whether a produced value may flow into an expected type.
/// `Type::Array(0)` (empty or fully dynamic array) unifies with any array
/// element type; precise element types must otherwise agree. An erased
/// unsubstituted parameter (`Type::Param`) is treated like `Unknown`.
fn types_compatible(actual: Type, expected: Type) -> bool {
    if actual == expected || is_dynamic(actual) || is_dynamic(expected) {
        return true;
    }
    match (actual, expected) {
        (Type::Array(_), Type::Array(_)) => {
            matches!(
                (actual, expected),
                (Type::Array(0), _) | (_, Type::Array(0))
            )
        }
        // An uninstantiated generic (`Box`) accepts any instantiation of the
        // same base type (`Box<Int>`), and vice versa.
        (Type::Generic(base), Type::Instance(instance, _))
        | (Type::Instance(instance, _), Type::Generic(base)) => base == instance,
        _ => false,
    }
}

/// Tag mixed into an array's element identity. `Type::Array` stores the
/// identity of its element; without a tag, `Array<Int>` and `Int` would have
/// the same id and `&Int` would accept a borrow of `[Int]`.
const ARRAY_ID_TAG: TypeId = 0x9E37_79B9;

/// The identity of an array type: its element identity, mixed with a tag so
/// it cannot collide with the element type itself.
fn array_identity(element: Type) -> TypeId {
    array_identity_from_id(type_identity(element))
}

fn array_identity_from_id(element: TypeId) -> TypeId {
    if element == 0 {
        0
    } else {
        mix_type_id(ARRAY_ID_TAG, element)
    }
}

fn type_identity(ty: Type) -> TypeId {
    match ty {
        Type::Int => stable_type_id("Int"),
        Type::Float => stable_type_id("Float"),
        Type::Bool => stable_type_id("Bool"),
        Type::String => stable_type_id("String"),
        Type::Void => stable_type_id("Void"),
        Type::Array(id)
        | Type::Tuple(id)
        | Type::Map(id)
        | Type::Struct(id)
        | Type::Enum(id)
        | Type::Reference(id, _)
        | Type::Task(id)
        | Type::Channel(id)
        | Type::Optional(id)
        | Type::Generic(id)
        | Type::Param(id)
        | Type::Function(id)
        | Type::ForeignHandle(id) => id,
        Type::Result(ok, error) => ok ^ error.rotate_left(1),
        Type::Instance(base, arguments) => mix_type_id(base, arguments),
        Type::Unknown => 0,
    }
}

fn instruction_results(instruction: &Instruction) -> Vec<ValueId> {
    match instruction {
        Instruction::Parameter { result, .. }
        | Instruction::Constant { result, .. }
        | Instruction::Load { result, .. }
        | Instruction::Unary { result, .. }
        | Instruction::Binary { result, .. }
        | Instruction::Call { result, .. } => vec![*result],
        Instruction::Allocate { result, .. }
        | Instruction::LoadMemory { result, .. }
        | Instruction::Borrow { result, .. }
        | Instruction::Spawn { result, .. }
        | Instruction::Await { result, .. }
        | Instruction::ChannelReceive { result, .. }
        | Instruction::ForeignCall { result, .. } => vec![*result],
        Instruction::IteratorNext { has_next, item, .. } => vec![*has_next, *item],
        Instruction::ArrayNew { result, .. }
        | Instruction::IndexGet { result, .. }
        | Instruction::IndexSet { result, .. }
        | Instruction::StructNew { result, .. }
        | Instruction::FieldGet { result, .. }
        | Instruction::FieldSet { result, .. }
        | Instruction::EnumNew { result, .. }
        | Instruction::VariantName { result, .. }
        | Instruction::PatternIndex { result, .. }
        | Instruction::PatternField { result, .. }
        | Instruction::ToString { result, .. }
        | Instruction::TypeName { result, .. }
        | Instruction::Catch { result, .. } => vec![*result],
        Instruction::MethodCall {
            result,
            receiver_out,
            ..
        } => vec![*result, *receiver_out],
        Instruction::MakeClosure { result, .. } | Instruction::CallIndirect { result, .. } => {
            vec![*result]
        }
        Instruction::SpawnCall { result, .. }
        | Instruction::SpawnMethod { result, .. }
        | Instruction::CompleteTask { result, .. }
        | Instruction::AwaitTask { result, .. }
        | Instruction::ChannelNew { result, .. } => vec![*result],
        // Nested regions are visited separately through flattened walks;
        // only the exception slot is reported here.
        Instruction::Try { tmp, .. } => vec![*tmp],
        _ => Vec::new(),
    }
}

fn instruction_value_types(instruction: &Instruction) -> Vec<(ValueId, Type)> {
    match instruction {
        Instruction::Parameter { result, ty, .. }
        | Instruction::Constant { result, ty, .. }
        | Instruction::Load { result, ty, .. }
        | Instruction::Unary { result, ty, .. }
        | Instruction::Binary { result, ty, .. } => vec![(*result, *ty)],
        Instruction::Call {
            result,
            return_type,
            ..
        } => vec![(*result, *return_type)],
        Instruction::LoadMemory { result, ty, .. }
        | Instruction::Borrow { result, ty, .. }
        | Instruction::Spawn { result, ty, .. }
        | Instruction::Await { result, ty, .. }
        | Instruction::ChannelReceive { result, ty, .. } => vec![(*result, *ty)],
        Instruction::Allocate { result, ty, .. } => {
            vec![(*result, Type::Reference(type_identity(*ty), true))]
        }
        Instruction::ForeignCall {
            result,
            return_type,
            ..
        } => vec![(*result, ffi_return_type(*return_type))],
        Instruction::CallIndirect {
            result,
            return_type,
            ..
        } => vec![(*result, *return_type)],
        Instruction::MakeClosure { result, .. } => vec![(*result, Type::Function(0))],
        Instruction::IteratorNext { has_next, item, .. } => {
            vec![(*has_next, Type::Bool), (*item, Type::Unknown)]
        }
        Instruction::ArrayNew { result, .. } => vec![(*result, Type::Array(0))],
        Instruction::IndexGet { result, ty, .. } => vec![(*result, *ty)],
        Instruction::IndexSet { result, .. } => vec![(*result, Type::Unknown)],
        Instruction::StructNew { result, ty, .. } => vec![(*result, *ty)],
        Instruction::FieldGet { result, ty, .. } => vec![(*result, *ty)],
        Instruction::FieldSet { result, .. } => vec![(*result, Type::Unknown)],
        Instruction::EnumNew { result, ty, .. } => vec![(*result, *ty)],
        Instruction::VariantName { result, .. } => vec![(*result, Type::String)],
        Instruction::PatternIndex { result, .. } => vec![(*result, Type::Unknown)],
        Instruction::PatternField { result, .. } => vec![(*result, Type::Unknown)],
        Instruction::SpawnCall { result, .. }
        | Instruction::SpawnMethod { result, .. }
        | Instruction::CompleteTask { result, .. }
        | Instruction::AwaitTask { result, .. }
        | Instruction::ChannelNew { result, .. } => vec![(*result, Type::Unknown)],
        Instruction::MethodCall {
            result,
            receiver_out,
            ..
        } => vec![(*result, Type::Unknown), (*receiver_out, Type::Unknown)],
        Instruction::ToString { result, .. } | Instruction::TypeName { result, .. } => {
            vec![(*result, Type::String)]
        }
        Instruction::Catch { result, .. } => vec![(*result, Type::Unknown)],
        Instruction::Try { tmp, .. } => vec![(*tmp, Type::Unknown)],
        _ => Vec::new(),
    }
}

fn instruction_operands(instruction: &Instruction) -> Vec<ValueId> {
    match instruction {
        Instruction::Store { value, .. } | Instruction::Assign { value, .. } => vec![*value],
        Instruction::Unary { operand, .. } => vec![*operand],
        Instruction::Binary { left, right, .. } => vec![*left, *right],
        Instruction::Call { arguments, .. } => arguments.clone(),
        Instruction::CallIndirect {
            callee, arguments, ..
        } => {
            let mut operands = vec![*callee];
            operands.extend(arguments.iter().copied());
            operands
        }
        Instruction::MakeClosure { captures, .. } => captures.clone(),
        Instruction::IteratorNext {
            iterable, index, ..
        } => vec![*iterable, *index],
        Instruction::ArrayNew { elements, .. } => elements.clone(),
        Instruction::IndexGet { object, index, .. } => vec![*object, *index],
        Instruction::IndexSet {
            object,
            index,
            value,
            ..
        } => vec![*object, *index, *value],
        Instruction::StructNew { fields, .. } => fields.iter().map(|(_, value)| *value).collect(),
        Instruction::FieldGet { object, .. } => vec![*object],
        Instruction::FieldSet { object, value, .. } => vec![*object, *value],
        Instruction::EnumNew { arguments, .. } => arguments.clone(),
        Instruction::VariantName { value, .. } => vec![*value],
        Instruction::PatternIndex { object, index, .. } => vec![*object, *index],
        Instruction::PatternField { object, .. } => vec![*object],
        Instruction::MethodCall {
            receiver,
            arguments,
            ..
        } => {
            let mut operands = vec![*receiver];
            operands.extend(arguments.iter().copied());
            operands
        }
        Instruction::SpawnCall { arguments, .. } => arguments.clone(),
        Instruction::SpawnMethod {
            receiver,
            arguments,
            ..
        } => {
            let mut operands = vec![*receiver];
            operands.extend(arguments.iter().copied());
            operands
        }
        Instruction::CompleteTask { value, .. } | Instruction::AwaitTask { task: value, .. } => {
            vec![*value]
        }
        Instruction::LoadMemory { pointer, .. } | Instruction::Borrow { value: pointer, .. } => {
            vec![*pointer]
        }
        Instruction::StoreMemory { pointer, value, .. } => vec![*pointer, *value],
        Instruction::Drop { value, .. }
        | Instruction::Spawn { task: value, .. }
        | Instruction::Await { task: value, .. }
        | Instruction::ChannelClose { channel: value, .. }
        | Instruction::CancelTask { task: value, .. } => vec![*value],
        Instruction::ChannelSend { channel, value, .. }
        | Instruction::ActorDispatch {
            actor: channel,
            message: value,
            ..
        } => vec![*channel, *value],
        Instruction::ChannelReceive { channel, .. } => vec![*channel],
        Instruction::ForeignCall { arguments, .. } => arguments.clone(),
        Instruction::ToString { value, .. }
        | Instruction::TypeName { value, .. }
        | Instruction::Throw { value, .. } => vec![*value],
        // Nested regions are visited separately through flattened walks.
        Instruction::Try { .. } => Vec::new(),
        _ => Vec::new(),
    }
}

fn instruction_span(instruction: &Instruction) -> Span {
    match instruction {
        Instruction::Constant { span, .. }
        | Instruction::Load { span, .. }
        | Instruction::Store { span, .. }
        | Instruction::Unary { span, .. }
        | Instruction::Binary { span, .. }
        | Instruction::Call { span, .. }
        | Instruction::Parameter { span, .. }
        | Instruction::IteratorNext { span, .. }
        | Instruction::Allocate { span, .. }
        | Instruction::LoadMemory { span, .. }
        | Instruction::StoreMemory { span, .. }
        | Instruction::Borrow { span, .. }
        | Instruction::Drop { span, .. }
        | Instruction::Spawn { span, .. }
        | Instruction::Await { span, .. }
        | Instruction::ChannelSend { span, .. }
        | Instruction::ChannelReceive { span, .. }
        | Instruction::ChannelClose { span, .. }
        | Instruction::ActorDispatch { span, .. }
        | Instruction::CancelTask { span, .. }
        | Instruction::ForeignCall { span, .. }
        | Instruction::Assign { span, .. }
        | Instruction::If { span, .. }
        | Instruction::Loop { span, .. }
        | Instruction::Break { span }
        | Instruction::Continue { span }
        | Instruction::ArrayNew { span, .. }
        | Instruction::IndexGet { span, .. }
        | Instruction::IndexSet { span, .. }
        | Instruction::StructNew { span, .. }
        | Instruction::FieldGet { span, .. }
        | Instruction::FieldSet { span, .. }
        | Instruction::EnumNew { span, .. }
        | Instruction::VariantName { span, .. }
        | Instruction::PatternIndex { span, .. }
        | Instruction::PatternField { span, .. }
        | Instruction::MethodCall { span, .. }
        | Instruction::MakeClosure { span, .. }
        | Instruction::CallIndirect { span, .. }
        | Instruction::SpawnCall { span, .. }
        | Instruction::SpawnMethod { span, .. }
        | Instruction::CompleteTask { span, .. }
        | Instruction::AwaitTask { span, .. }
        | Instruction::ChannelNew { span, .. }
        | Instruction::Return { span, .. }
        | Instruction::ToString { span, .. }
        | Instruction::TypeName { span, .. }
        | Instruction::Catch { span, .. }
        | Instruction::Throw { span, .. }
        | Instruction::PushHandler { span, .. }
        | Instruction::PopHandler { span }
        | Instruction::Try { span, .. } => *span,
    }
}

fn terminator_span(terminator: &Terminator) -> Span {
    match terminator {
        Terminator::Jump { span, .. }
        | Terminator::Branch { span, .. }
        | Terminator::Return { span, .. }
        | Terminator::Unreachable { span } => *span,
    }
}

#[derive(Clone)]
struct Lowerer {
    functions: Vec<Function>,
    signatures: Vec<FunctionSignature>,
    instructions: Vec<Instruction>,
    values: HashMap<String, ValueId>,
    mutability: HashMap<String, bool>,
    scope_stack: Vec<HashMap<String, ValueId>>,
    mutability_stack: Vec<HashMap<String, bool>>,
    next_value: ValueId,
    in_function: bool,
    current_function_return_type: Option<Type>,
    /// Types of every `return` expression seen in the function body being
    /// lowered. `None` records a bare `return;`. Used to infer the return
    /// type of functions declared without an annotation.
    current_return_types: Vec<Option<Type>>,
    /// Type parameters of the function body currently being lowered. Used to
    /// resolve generic-body annotations and to reject non-concrete explicit
    /// type arguments with a clear diagnostic.
    current_type_parameters: Vec<String>,
    in_loop: bool,
    loop_depth: u32,
    /// Stack of loop depths captured at each lexically enclosing `try`.
    /// Used to reject control flow that would cross a handler boundary.
    try_stack: Vec<u32>,
    function_has_return: bool,
    source_name: String,
    type_names: std::collections::HashSet<String>,
    value_types: HashMap<ValueId, Type>,
    /// Per-value struct field types for generic instantiations, whose
    /// `Type::Instance` fingerprint cannot be decoded back into arguments.
    value_field_types: HashMap<ValueId, HashMap<String, Type>>,
    /// Concrete arguments behind each `Type::Instance` fingerprint, so an
    /// inferred generic argument can be written back as an annotation.
    instance_arguments: HashMap<(TypeId, TypeId), Vec<Type>>,
    /// Concrete types behind `Type::Array`/`Type::Tuple` element ids.
    identity_types: HashMap<TypeId, Type>,
    /// Monotonic counter for synthesized conditional-expression temporaries.
    conditional_counter: u32,
    /// Full program, for resolving struct/enum declarations by name.
    program: Program,
    /// Dotted path of the file currently being lowered ("" for the entry).
    current_module: String,
    /// Monotonic counter for synthesized async-worker names.
    async_counter: u32,
    /// Monotonic counter for synthesized closure names.
    closure_counter: u32,
    /// Import statements of the file currently being lowered.
    file_imports: Vec<crate::project::ModuleImport>,
    /// Bare-to-qualified type names for the current file (own types plus
    /// selected imports), used when resolving generic annotations.
    type_aliases: HashMap<String, String>,
    /// Registered `extern` (C ABI) declarations, keyed by NXL-visible name.
    externs: HashMap<String, ForeignFunction>,
    /// Parameter types of each closure *value*, keyed by the `MakeClosure`
    /// result. `CallIndirect` has no callee name, so this is the only way to
    /// see a closure's declared parameters at its call site.
    closure_parameters: HashMap<ValueId, Vec<Type>>,
    /// Lexical borrow state: for each scope, the owner bindings currently
    /// borrowed, the kind of borrow, and the scope that declared the owner.
    /// The owner scope distinguishes shadowed bindings with the same name.
    /// A borrow lasts until the end of the scope that created it.
    borrow_scopes: Vec<HashMap<String, (BorrowKind, usize)>>,
    /// Per-scope binding state, so shadowing restores the outer binding.
    binding_states: Vec<HashMap<String, BindingState>>,
}

/// Ownership-related state of a binding.
#[derive(Clone, Debug, Default)]
struct BindingState {
    /// Backed by an allocated cell because it was borrowed.
    boxed: bool,
    /// When the binding holds a reference: its target and mutability.
    reference: Option<(Type, bool)>,
    /// Explicitly moved out and not reassigned since.
    moved: bool,
}

/// The kind of an active borrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BorrowKind {
    Immutable,
    Mutable,
}

/// How a binding is being used, for borrow-conflict checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BindingUse {
    Read,
    Write,
    Move,
}

/// One file of a multi-file lowering unit: its HIR plus module context.
pub struct ProjectFile {
    pub path: String,
    pub hir: crate::hir::HirModule,
    pub imports: Vec<crate::project::ModuleImport>,
    pub aliases: HashMap<String, String>,
}

/// Short display name for diagnostics (`app::models#User` -> `User`).
pub fn short_name(qualified: &str) -> &str {
    qualified
        .rsplit('#')
        .next()
        .and_then(|segment| segment.rsplit("::").next())
        .unwrap_or(qualified)
}

/// A declared type summary: whether it is an enum, its qualified name, and
/// its variant-name/payload-type list.
type DeclaredTypeInfo = (bool, String, Vec<(String, Vec<String>)>);

/// A resolved enum variant: (type name, payload type names).
fn find_enum_variant(program: &Program, variant: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    fn walk(statements: &[Statement], variant: &str, out: &mut Vec<(String, Vec<String>)>) {
        for statement in statements {
            match statement {
                Statement::TypeDeclaration(declaration)
                    if matches!(declaration.kind, crate::ast::DeclarationKind::Enum) =>
                {
                    for candidate in &declaration.variants {
                        if candidate.name == variant {
                            out.push((declaration.name.clone(), candidate.payload.clone()));
                        }
                    }
                }
                Statement::Export(inner) => walk(std::slice::from_ref(inner), variant, out),
                _ => {}
            }
        }
    }
    walk(&program.statements, variant, &mut out);
    out
}

fn find_type_declaration<'a>(
    program: &'a Program,
    name: &str,
) -> Option<&'a crate::ast::TypeDeclaration> {
    fn walk<'a>(
        statements: &'a [Statement],
        name: &str,
    ) -> Option<&'a crate::ast::TypeDeclaration> {
        for statement in statements {
            match statement {
                Statement::TypeDeclaration(declaration) if declaration.name == name => {
                    return Some(declaration);
                }
                Statement::Export(inner) => {
                    if let Some(found) = walk(std::slice::from_ref(inner), name) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(&program.statements, name)
}

impl Lowerer {
    /// Register every `fn` in a top-level `extern` block as a C-ABI foreign
    /// declaration. Types are validated to the supported scalar surface and
    /// the arity is capped; the declaration is recorded both in the lowerer's
    /// call-resolution table and the module's foreign signature table.
    fn register_externs(
        &mut self,
        statement: &Statement,
        table: &mut Vec<ForeignFunction>,
    ) -> Result<(), Diagnostic> {
        let Statement::ExternBlock {
            library,
            functions,
            span,
        } = statement
        else {
            return Ok(());
        };
        for function in functions {
            if function.parameters.len() > crate::cabi::MAX_FFI_ARITY {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!(
                        "extern function '{}' declares {} parameters; the C ABI supports at most {}",
                        function.name,
                        function.parameters.len(),
                        crate::cabi::MAX_FFI_ARITY
                    ),
                    function.span,
                ));
            }
            let parameters = function
                .parameters
                .iter()
                .map(|(name, ty)| {
                    parse_ffi_type(ty).ok_or_else(|| {
                        Diagnostic::error(
                            "NXL2001",
                            format!(
                                "extern function '{}' parameter '{name}' has unsupported C type '{ty}'; use Int, Float, String, or Bool",
                                function.name
                            ),
                            function.span,
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let return_type = match &function.return_type {
                None => None,
                Some(ty) => Some(parse_ffi_type(ty).ok_or_else(|| {
                    Diagnostic::error(
                        "NXL2001",
                        format!(
                            "extern function '{}' has unsupported C return type '{ty}'; use Int, Float, String, Bool, or omit it for void",
                            function.name
                        ),
                        function.span,
                    )
                })?),
            };
            let symbol = function
                .symbol
                .clone()
                .unwrap_or_else(|| function.name.clone());
            let declaration = ForeignFunction {
                name: function.name.clone(),
                symbol,
                library: library.clone(),
                parameters,
                return_type,
                calling_convention: CallingConvention::C,
            };
            if self.externs.contains_key(&declaration.name) {
                return Err(Diagnostic::error(
                    "NXL2005",
                    format!("duplicate extern function '{}'", declaration.name),
                    *span,
                ));
            }
            self.externs
                .insert(declaration.name.clone(), declaration.clone());
            table.push(declaration);
        }
        Ok(())
    }

    fn lower_statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::TypeDeclaration(declaration)
                if matches!(
                    declaration.kind,
                    crate::ast::DeclarationKind::Class | crate::ast::DeclarationKind::Actor
                ) =>
            {
                // Register signatures first so methods can mutually recurse.
                self.register_methods_for(
                    &declaration.name,
                    &declaration.methods,
                    declaration.span,
                )?;
                for method in &declaration.methods {
                    let method_name = method.name.clone().unwrap_or_default();
                    let mut owned = method.clone();
                    owned.name = Some(format!("{}#{}", declaration.name, method_name));
                    owned.parameters.insert(
                        0,
                        crate::ast::Parameter {
                            name: "self".to_owned(),
                            annotation: None,
                            span: method.span,
                        },
                    );
                    self.lower_statement(&Statement::Function(owned))?;
                }
                Ok(())
            }
            Statement::TypeDeclaration(_) | Statement::TypeAlias(_) | Statement::Import { .. } => {
                Ok(())
            }
            Statement::ImplBlock(implementation) => {
                if find_type_declaration(&self.program, &implementation.target).is_none() {
                    return Err(Diagnostic::error(
                        "NXL2004",
                        format!("unknown type '{}'", implementation.target),
                        implementation.span,
                    ));
                }
                if let Some(trait_name) = &implementation.trait_name {
                    let contract = find_type_declaration(&self.program, trait_name).cloned();
                    let Some(contract) = contract else {
                        return Err(Diagnostic::error(
                            "NXL2004",
                            format!("unknown type '{trait_name}'"),
                            implementation.span,
                        ));
                    };
                    if !matches!(
                        contract.kind,
                        crate::ast::DeclarationKind::Interface | crate::ast::DeclarationKind::Trait
                    ) {
                        return Err(Diagnostic::error(
                            "NXL2001",
                            format!("'{trait_name}' is not an interface or trait"),
                            implementation.span,
                        ));
                    }
                    for required in &contract.methods {
                        let Some(required_name) = required.name.as_ref() else {
                            continue;
                        };
                        let Some(provided) = implementation
                            .methods
                            .iter()
                            .find(|method| method.name.as_deref() == Some(required_name.as_str()))
                        else {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!(
                                    "missing method '{required_name}' required by '{trait_name}'"
                                ),
                                implementation.span,
                            ));
                        };
                        if provided.parameters.len() != required.parameters.len() {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!(
                                    "method '{required_name}' takes {} argument(s), '{trait_name}' requires {}",
                                    provided.parameters.len(),
                                    required.parameters.len()
                                ),
                                provided.span,
                            ));
                        }
                    }
                }
                self.register_methods_for(
                    &implementation.target,
                    &implementation.methods,
                    implementation.span,
                )?;
                for method in &implementation.methods {
                    let method_name = method.name.clone().unwrap_or_default();
                    let mut owned = method.clone();
                    owned.name = Some(format!("{}#{}", implementation.target, method_name));
                    owned.parameters.insert(
                        0,
                        crate::ast::Parameter {
                            name: "self".to_owned(),
                            annotation: None,
                            span: method.span,
                        },
                    );
                    self.lower_statement(&Statement::Function(owned))?;
                }
                Ok(())
            }
            Statement::Export(inner) => self.lower_statement(inner),
            Statement::Module { .. } => Ok(()),
            // `extern` blocks are registered up front by `register_externs`;
            // there is no code to lower here.
            Statement::ExternBlock { .. } => Ok(()),
            Statement::Let {
                name,
                annotation,
                is_const: _,
                is_mut,
                value,
                span,
            } => {
                if self.lookup_declaration(name).is_some() {
                    return Err(Diagnostic::error(
                        "NXL2005",
                        format!("duplicate declaration of '{name}'"),
                        *span,
                    ));
                }
                let Some(value) = value else {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "uninitialized declarations are not lowered by this compiler stage",
                        *span,
                    ));
                };
                let value_id = self.expression(value)?;
                if self.value_type(value_id) == Type::Void {
                    return Err(Diagnostic::error(
                        "NXL2006",
                        format!("cannot bind the void result of '{name}'"),
                        *span,
                    ));
                }
                if let Some(annotation) = annotation {
                    let actual = self.value_type(value_id);
                    let Some(expected) = self.resolve_type(annotation) else {
                        return Err(Diagnostic::error(
                            "NXL2004",
                            format!("unknown type '{annotation}'"),
                            *span,
                        ));
                    };
                    if !types_compatible(actual, expected) {
                        return Err(Diagnostic::error(
                            "NXL2001",
                            format!(
                                "type mismatch for '{name}': expected {expected:?}, found {actual:?}"
                            ),
                            *span,
                        ));
                    }
                }
                self.instructions.push(Instruction::Store {
                    name: name.clone(),
                    value: value_id,
                    span: *span,
                });
                self.values.insert(name.clone(), value_id);
                self.mutability.insert(name.clone(), *is_mut);
                let current_scope = self.scope_stack.last_mut().unwrap();
                current_scope.insert(name.clone(), value_id);
                let current_mutability = self.mutability_stack.last_mut().unwrap();
                current_mutability.insert(name.clone(), *is_mut);
                // Track whether the new binding is a reference; a borrow was
                // registered by the initializer (if any).
                self.declare_binding(name);
                let declared_type = match annotation.as_deref() {
                    Some(annotation) => match self.resolve_type(annotation) {
                        Some(ty) => {
                            self.register_annotation_type(annotation, ty);
                            ty
                        }
                        None => self.value_type(value_id),
                    },
                    None => self.value_type(value_id),
                };
                if let Type::Reference(target, mutable) = declared_type {
                    match self.value_type(value_id) {
                        Type::Reference(value_target, value_mutable) => {
                            if mutable && !value_mutable {
                                return Err(Diagnostic::error(
                                    "NXL2015",
                                    format!(
                                        "binding '{name}' is declared as a mutable reference but is initialised with an immutable borrow"
                                    ),
                                    *span,
                                )
                                .with_help("borrow the owner mutably with 'borrow mut' or 'ref mut'"));
                            }
                            if target != value_target {
                                return Err(Diagnostic::error(
                                    "NXL2001",
                                    format!(
                                        "binding '{name}' borrows a different type than its annotation"
                                    ),
                                    *span,
                                ));
                            }
                        }
                        Type::Unknown => {}
                        other => {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!(
                                    "binding '{name}' is declared as a reference but is initialised with {other:?}"
                                ),
                                *span,
                            ));
                        }
                    }
                    let target = self.type_by_identity(target).unwrap_or(Type::Unknown);
                    self.record_identity(target);
                    if let Some(state) = self.binding_state_mut(name) {
                        state.reference = Some((target, mutable));
                    }
                }
                Ok(())
            }
            Statement::Return { value, span } => {
                // Validate return only in function context
                if !self.in_function {
                    return Err(Diagnostic::error(
                        "NXL2010",
                        "return statement can only appear inside a function",
                        *span,
                    ));
                }
                // A return would bypass pending handlers/finally blocks.
                if !self.try_stack.is_empty() {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "return across a try boundary is not supported yet; assign the result to a variable and return after the try block",
                        *span,
                    ));
                }
                self.function_has_return = true;

                let value = value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .transpose()?;
                // Record the observed return types so an unannotated function
                // can infer its result type after the body is lowered.
                self.current_return_types
                    .push(value.map(|value| self.value_type(value)));

                // References may not escape through a return value: the
                // borrowed owner ends before the caller resumes.
                if let Some(value_id) = value {
                    let actual = self.value_type(value_id);
                    if self.type_contains_reference(actual, 0) {
                        return Err(Diagnostic::error(
                            "NXL2017",
                            "cannot return a reference: it would outlive the value it borrows",
                            *span,
                        )
                        .with_help("return the owned value instead, or let the caller borrow it"));
                    }
                }

                // Validate return type matches function's declared return type
                if let Some(expected_return_type) = self.current_function_return_type {
                    match (value, expected_return_type) {
                        (None, Type::Void) | (None, Type::Unknown) => {
                            // Bare return allowed when returning Void or Unknown
                        }
                        (None, _) => {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!(
                                    "missing return value for function declared to return {expected_return_type:?}"
                                ),
                                *span,
                            ));
                        }
                        (Some(value_id), Type::Unknown) => {
                            // Unknown return type accepts any value
                            self.instructions.push(Instruction::Return {
                                value: Some(value_id),
                                span: *span,
                            });
                        }
                        (Some(value_id), expected) => {
                            let actual = self.value_type(value_id);
                            if !types_compatible(actual, expected) {
                                return Err(Diagnostic::error(
                                    "NXL2001",
                                    format!(
                                        "return value has type {actual:?}, expected {expected:?}"
                                    ),
                                    *span,
                                ));
                            }
                            self.instructions.push(Instruction::Return {
                                value: Some(value_id),
                                span: *span,
                            });
                        }
                    }
                } else {
                    self.instructions
                        .push(Instruction::Return { value, span: *span });
                }
                Ok(())
            }
            Statement::Expression(expression) => {
                self.expression(expression)?;
                Ok(())
            }
            Statement::Block(block) => {
                self.push_scope();
                for statement in &block.statements {
                    self.lower_statement(statement)?;
                }
                self.pop_scope();
                Ok(())
            }
            Statement::If {
                condition,
                then_block,
                else_branch,
                span,
            } => {
                let condition_value = self.expression(condition)?;
                let condition_type = self.value_type(condition_value);
                if condition_type != Type::Bool && !is_dynamic(condition_type) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!("if condition has type {condition_type:?}, expected Bool"),
                        *span,
                    ));
                }

                let saved_instructions = std::mem::take(&mut self.instructions);
                let then_saved_scope = self.scope_stack.clone();
                let then_saved_mutability = self.mutability_stack.clone();
                let then_saved_borrow = self.borrow_scopes.clone();
                let then_saved_binding = self.binding_states.clone();
                let then_saved_values = self.values.clone();
                let then_saved_mutability_map = self.mutability.clone();
                self.push_scope();
                self.instructions = Vec::new();
                for statement in &then_block.statements {
                    self.lower_statement(statement)?;
                }
                let then_branch = std::mem::take(&mut self.instructions);
                self.instructions = saved_instructions.clone();
                self.scope_stack = then_saved_scope;
                self.mutability_stack = then_saved_mutability;
                self.borrow_scopes = then_saved_borrow;
                self.binding_states = then_saved_binding;
                self.values = then_saved_values;
                self.mutability = then_saved_mutability_map;

                let else_branch = match else_branch {
                    Some(branch) => {
                        let else_saved_scope = self.scope_stack.clone();
                        let else_saved_mutability = self.mutability_stack.clone();
                        let else_saved_borrow = self.borrow_scopes.clone();
                        let else_saved_binding = self.binding_states.clone();
                        let else_saved_values = self.values.clone();
                        let else_saved_mutability_map = self.mutability.clone();
                        self.push_scope();
                        self.instructions = Vec::new();
                        match branch.as_ref() {
                            Statement::Block(block) => {
                                for statement in &block.statements {
                                    self.lower_statement(statement)?;
                                }
                            }
                            other => {
                                self.lower_statement(other)?;
                            }
                        }
                        let else_instructions = std::mem::take(&mut self.instructions);
                        self.instructions = saved_instructions;
                        self.scope_stack = else_saved_scope;
                        self.mutability_stack = else_saved_mutability;
                        self.borrow_scopes = else_saved_borrow;
                        self.binding_states = else_saved_binding;
                        self.values = else_saved_values;
                        self.mutability = else_saved_mutability_map;
                        Some(else_instructions)
                    }
                    None => None,
                };

                self.instructions.push(Instruction::If {
                    condition: condition_value,
                    then_branch,
                    else_branch,
                    span: *span,
                });
                Ok(())
            }
            Statement::Function(function) => {
                let Some(function_name) = function.name.as_ref() else {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "anonymous functions are not yet supported",
                        function.span,
                    ));
                };

                if self.lookup_declaration(function_name).is_some() {
                    return Err(Diagnostic::error(
                        "NXL2005",
                        format!("duplicate declaration of '{function_name}'"),
                        function.span,
                    ));
                }

                let signature = self
                    .signatures
                    .iter()
                    .find(|signature| signature.name == *function_name)
                    .cloned()
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "NXL2002",
                            format!("function '{function_name}' is missing its HIR signature"),
                            function.span,
                        )
                    })?;
                let param_types = signature.parameters;
                let parameter_annotations = signature.parameter_annotations.clone();
                let return_annotation = signature.return_annotation.clone();
                let return_type = signature.return_type;

                // Validate function body can be lowered
                let Some(body) = &function.body else {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "function declaration without body is not yet supported",
                        function.span,
                    ));
                };

                // Store function name for scope
                self.values.insert(function_name.clone(), u32::MAX);
                let current_scope = self.scope_stack.last_mut().unwrap();
                current_scope.insert(function_name.clone(), u32::MAX);

                // Save current state for the function body
                let saved_instructions = std::mem::take(&mut self.instructions);
                let saved_scope_stack = std::mem::take(&mut self.scope_stack);
                let saved_mutability_stack = std::mem::take(&mut self.mutability_stack);
                let saved_values = std::mem::take(&mut self.values);
                let saved_mutability = std::mem::take(&mut self.mutability);
                let saved_function_has_return = self.function_has_return;
                let saved_value_types = std::mem::take(&mut self.value_types);
                let saved_return_types =
                    std::mem::replace(&mut self.current_return_types, Vec::new());
                let saved_try_stack = std::mem::take(&mut self.try_stack);
                let saved_loop_depth = self.loop_depth;
                let saved_borrow_scopes = std::mem::take(&mut self.borrow_scopes);
                let saved_binding_states = std::mem::take(&mut self.binding_states);
                let saved_type_parameters = std::mem::replace(
                    &mut self.current_type_parameters,
                    function.type_parameters.clone(),
                );
                // A nested function body is a fresh control-flow region: handler
                // boundaries and loop nesting of the enclosing scope do not apply.
                self.loop_depth = 0;

                // Reset for function body
                self.instructions = Vec::new();
                self.scope_stack = vec![HashMap::new()];
                self.mutability_stack = vec![HashMap::new()];
                self.borrow_scopes = vec![HashMap::new()];
                self.binding_states = vec![HashMap::new()];
                self.values = HashMap::new();
                self.mutability = HashMap::new();
                self.in_function = true;
                self.current_function_return_type = Some(return_type);
                self.function_has_return = false;

                // Add parameters to function scope
                let mut parameter_instructions = Vec::with_capacity(param_types.len());
                for (index, (param_name, param_type)) in param_types.iter().enumerate() {
                    let value_id = self.allocate();
                    let span = function
                        .parameters
                        .get(index)
                        .map_or(function.span, |parameter| parameter.span);
                    let param_instruction = Instruction::Parameter {
                        result: value_id,
                        index,
                        ty: *param_type,
                        span,
                    };
                    self.record_value_type(&param_instruction);
                    if let Some(annotation) = parameter_annotations
                        .get(index)
                        .and_then(|annotation| annotation.as_deref())
                    {
                        self.register_annotation_type(annotation, *param_type);
                    }
                    parameter_instructions.push(param_instruction);
                    self.values.insert(param_name.clone(), value_id);
                    self.scope_stack
                        .last_mut()
                        .unwrap()
                        .insert(param_name.clone(), value_id);
                    self.declare_binding(param_name);
                    if let Type::Reference(target, mutable) = *param_type {
                        let target = self.type_by_identity(target).unwrap_or(Type::Unknown);
                        self.record_identity(target);
                        if let Some(state) = self.binding_state_mut(param_name) {
                            state.reference = Some((target, mutable));
                        }
                    }
                    // Parameters are immutable by default
                    self.mutability.insert(param_name.clone(), false);
                    self.mutability_stack
                        .last_mut()
                        .unwrap()
                        .insert(param_name.clone(), false);
                }
                self.instructions = parameter_instructions;

                // Register the declared contract before the body: field
                // lookup and inference read instance arguments back from it.
                if let Some(annotation) = return_annotation.as_deref() {
                    self.register_annotation_type(annotation, return_type);
                }

                // Lower function body
                for stmt in &body.statements {
                    self.lower_statement(stmt)?;
                }

                // Infer the result type for functions declared without a
                // return annotation. Annotated functions keep their contract.
                let effective_return_type = if signature.return_annotation.is_none() {
                    infer_return_type(&self.current_return_types).map_err(|()| {
                        Diagnostic::error(
                            "NXL2001",
                            format!(
                                "function '{function_name}' returns values of inconsistent types"
                            ),
                            function.span,
                        )
                    })?
                } else {
                    return_type
                };
                if signature.return_annotation.is_none() {
                    if let Some(stored) = self
                        .signatures
                        .iter_mut()
                        .find(|stored| stored.name == *function_name)
                    {
                        stored.return_type = effective_return_type;
                    }
                }

                if matches!(
                    effective_return_type,
                    Type::Int | Type::Float | Type::Bool | Type::String
                ) && !Self::block_returns_on_all_paths(body)
                {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "function '{function_name}' returns {effective_return_type:?} and must return a value on all paths"
                        ),
                        function.span,
                    ));
                }

                // Create function with lowered instructions
                let func_instructions = std::mem::take(&mut self.instructions);
                self.functions.push(create_function(
                    function_name.clone(),
                    param_types,
                    effective_return_type,
                    func_instructions,
                    function.span,
                    &self.source_name,
                ));

                // Restore state
                self.instructions = saved_instructions;
                self.scope_stack = saved_scope_stack;
                self.mutability_stack = saved_mutability_stack;
                self.values = saved_values;
                self.mutability = saved_mutability;
                self.in_function = false;
                self.current_function_return_type = None;
                self.function_has_return = saved_function_has_return;
                self.value_types = saved_value_types;
                self.current_return_types = saved_return_types;
                self.try_stack = saved_try_stack;
                self.loop_depth = saved_loop_depth;
                self.borrow_scopes = saved_borrow_scopes;
                self.binding_states = saved_binding_states;
                self.current_type_parameters = saved_type_parameters;

                Ok(())
            }
            Statement::For {
                binding,
                iterable,
                body,
                span,
            } => {
                let iterable_id = self.expression(iterable)?;

                if self.lookup_declaration(binding).is_some() {
                    return Err(Diagnostic::error(
                        "NXL2005",
                        format!("duplicate declaration of '{binding}' in loop binding"),
                        *span,
                    ));
                }

                let saved_in_loop = self.in_loop;
                let saved_loop_depth = self.loop_depth;
                self.in_loop = true;
                self.loop_depth += 1;
                let saved_instructions = std::mem::take(&mut self.instructions);
                let saved_scope = self.scope_stack.clone();
                let saved_mutability_scope = self.mutability_stack.clone();
                let saved_borrow_scope = self.borrow_scopes.clone();
                let saved_binding_scope = self.binding_states.clone();
                let saved_values = self.values.clone();
                let saved_mutability = self.mutability.clone();
                self.push_scope();

                let binding_id = self.allocate();
                self.values.insert(binding.clone(), binding_id);
                self.scope_stack
                    .last_mut()
                    .unwrap()
                    .insert(binding.clone(), binding_id);
                self.declare_binding(binding);
                self.mutability.insert(binding.clone(), false);
                self.mutability_stack
                    .last_mut()
                    .unwrap()
                    .insert(binding.clone(), false);

                let has_next = self.allocate();
                let item = self.allocate();
                let index = self.allocate();
                let condition_instructions = vec![
                    Instruction::IteratorNext {
                        has_next,
                        item,
                        iterable: iterable_id,
                        index,
                        span: *span,
                    },
                    Instruction::Store {
                        name: binding.clone(),
                        value: item,
                        span: *span,
                    },
                ];
                for statement in &body.statements {
                    self.lower_statement(statement)?;
                }
                let loop_body = std::mem::take(&mut self.instructions);
                self.instructions = saved_instructions;
                self.scope_stack = saved_scope;
                self.mutability_stack = saved_mutability_scope;
                self.borrow_scopes = saved_borrow_scope;
                self.binding_states = saved_binding_scope;
                self.values = saved_values;
                self.mutability = saved_mutability;
                self.in_loop = saved_in_loop;
                self.loop_depth = saved_loop_depth;
                // Initialise the loop counter before the loop so re-entering
                // an outer loop restarts iteration from the beginning.
                let init = Instruction::Constant {
                    result: index,
                    value: Constant::Integer(0),
                    ty: Type::Int,
                    span: *span,
                };
                self.record_value_type(&init);
                self.instructions.push(init);
                self.instructions.push(Instruction::Loop {
                    condition: Some(has_next),
                    condition_instructions,
                    body: loop_body,
                    span: *span,
                });
                Ok(())
            }
            Statement::While {
                condition,
                body,
                span,
            } => {
                let saved_instructions = std::mem::take(&mut self.instructions);
                let condition_value = self.expression(condition)?;
                let condition_type = self.value_type(condition_value);
                if condition_type != Type::Bool && !is_dynamic(condition_type) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!("while condition has type {condition_type:?}, expected Bool"),
                        *span,
                    ));
                }
                let condition_instructions = std::mem::take(&mut self.instructions);
                self.instructions = saved_instructions;

                let saved_in_loop = self.in_loop;
                let saved_loop_depth = self.loop_depth;
                let saved_borrow_scope = self.borrow_scopes.clone();
                let saved_binding_scope = self.binding_states.clone();
                self.in_loop = true;
                self.loop_depth += 1;
                let saved_instructions = std::mem::take(&mut self.instructions);
                for statement in &body.statements {
                    self.lower_statement(statement)?;
                }
                let loop_body = std::mem::take(&mut self.instructions);
                self.instructions = saved_instructions;
                self.borrow_scopes = saved_borrow_scope;
                self.binding_states = saved_binding_scope;
                self.in_loop = saved_in_loop;
                self.loop_depth = saved_loop_depth;
                self.instructions.push(Instruction::Loop {
                    condition: Some(condition_value),
                    condition_instructions,
                    body: loop_body,
                    span: *span,
                });
                Ok(())
            }
            Statement::Loop { body, span } => {
                let saved_in_loop = self.in_loop;
                let saved_loop_depth = self.loop_depth;
                let saved_borrow_scope = self.borrow_scopes.clone();
                let saved_binding_scope = self.binding_states.clone();
                self.in_loop = true;
                self.loop_depth += 1;
                let saved_instructions = std::mem::take(&mut self.instructions);
                for statement in &body.statements {
                    self.lower_statement(statement)?;
                }
                let loop_body = std::mem::take(&mut self.instructions);
                self.instructions = saved_instructions;
                self.borrow_scopes = saved_borrow_scope;
                self.binding_states = saved_binding_scope;
                self.in_loop = saved_in_loop;
                self.loop_depth = saved_loop_depth;
                self.instructions.push(Instruction::Loop {
                    condition: None,
                    condition_instructions: Vec::new(),
                    body: loop_body,
                    span: *span,
                });
                Ok(())
            }
            Statement::Match { value, cases, span } => {
                let value_id = self.expression(value)?;
                if self.value_type(value_id) == Type::Void {
                    return Err(Diagnostic::error(
                        "NXL2006",
                        "match value cannot be void",
                        *span,
                    ));
                }

                // Desugar to a chained if/else: first matching case wins,
                // no match runs nothing.
                let mut else_branch: Option<Vec<Instruction>> = None;
                for case in cases.iter().rev() {
                    // Sibling cases must not see each other's bindings or
                    // borrows: snapshot before the case scope and restore after.
                    let saved_values = self.values.clone();
                    let saved_mutability = self.mutability.clone();
                    let saved_borrow = self.borrow_scopes.clone();
                    let saved_binding = self.binding_states.clone();
                    self.push_scope();
                    let mut test_id = self.lower_pattern_test(&case.pattern, value_id)?;

                    if let Some(guard) = &case.guard {
                        let guard_id = self.expression(guard)?;
                        let guard_type = self.value_type(guard_id);
                        if guard_type != Type::Bool && !is_dynamic(guard_type) {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!("match guard has type {guard_type:?}, expected Bool"),
                                guard.span,
                            ));
                        }
                        test_id = self.lower_and_test(test_id, guard_id, guard.span);
                    }

                    let saved_instructions = std::mem::take(&mut self.instructions);
                    match &case.body {
                        MatchBody::Expression(expression) => {
                            let _ = self.expression(expression)?;
                        }
                        MatchBody::Block(block) => {
                            for statement in &block.statements {
                                self.lower_statement(statement)?;
                            }
                        }
                    }
                    let body_vec = std::mem::take(&mut self.instructions);
                    self.instructions = saved_instructions;
                    self.instructions.push(Instruction::If {
                        condition: test_id,
                        then_branch: body_vec,
                        else_branch: else_branch.take(),
                        span: case.span,
                    });
                    let if_instr = self.instructions.pop().expect("just pushed");
                    else_branch = Some(vec![if_instr]);
                    self.values = saved_values;
                    self.mutability = saved_mutability;
                    self.pop_scope();
                    self.borrow_scopes = saved_borrow;
                    self.binding_states = saved_binding;
                }
                // Emit the outermost if; a trailing bare fallthrough needs no
                // code (no match runs nothing).
                if let Some(chain) = else_branch {
                    self.instructions.extend(chain);
                }
                Ok(())
            }
            Statement::Break(span) => {
                if !self.in_loop {
                    return Err(Diagnostic::error(
                        "NXL2011",
                        "break statement can only appear inside a loop",
                        *span,
                    ));
                }
                if !self.try_stack.is_empty() && *self.try_stack.last().unwrap() >= self.loop_depth
                {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "break across a try boundary is not supported yet",
                        *span,
                    ));
                }
                self.instructions.push(Instruction::Break { span: *span });
                Ok(())
            }
            Statement::Continue(span) => {
                if !self.in_loop {
                    return Err(Diagnostic::error(
                        "NXL2012",
                        "continue statement can only appear inside a loop",
                        *span,
                    ));
                }
                if !self.try_stack.is_empty() && *self.try_stack.last().unwrap() >= self.loop_depth
                {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "continue across a try boundary is not supported yet",
                        *span,
                    ));
                }
                self.instructions
                    .push(Instruction::Continue { span: *span });
                Ok(())
            }
            Statement::Throw { value, span } => {
                let value_id = self.expression(value)?;
                if self.value_type(value_id) == Type::Void {
                    return Err(Diagnostic::error(
                        "NXL2006",
                        "cannot throw a void value",
                        *span,
                    ));
                }
                self.instructions.push(Instruction::Throw {
                    value: value_id,
                    span: *span,
                });
                Ok(())
            }
            Statement::Try {
                body,
                catches,
                finally,
                span,
            } => {
                // The exception slot shared by the dispatch wiring below.
                let tmp = self.allocate();
                self.try_stack.push(self.loop_depth);

                let saved_instructions = std::mem::take(&mut self.instructions);
                let saved_scope = self.scope_stack.clone();
                let saved_mutability_scope = self.mutability_stack.clone();
                let saved_borrow_scope = self.borrow_scopes.clone();
                let saved_binding_scope = self.binding_states.clone();
                let saved_values = self.values.clone();
                let saved_mutability = self.mutability.clone();

                // Body region.
                self.push_scope();
                self.instructions = Vec::new();
                for statement in &body.statements {
                    self.lower_statement(statement)?;
                }
                let body_instrs = std::mem::take(&mut self.instructions);

                // Catch regions. Each binding is visible only inside its body
                // and aliases the shared exception slot.
                let mut lowered_catches = Vec::with_capacity(catches.len());
                let mut seen_bare = false;
                for catch in catches {
                    if seen_bare {
                        return Err(Diagnostic::error(
                            "NXL2013",
                            "unreachable catch clause: a bare catch already handles all exceptions",
                            catch.span,
                        ));
                    }
                    if catch.annotation.is_none() {
                        seen_bare = true;
                    }
                    self.scope_stack = saved_scope.clone();
                    self.mutability_stack = saved_mutability_scope.clone();
                    self.borrow_scopes = saved_borrow_scope.clone();
                    self.binding_states = saved_binding_scope.clone();
                    self.values = saved_values.clone();
                    self.mutability = saved_mutability.clone();
                    self.push_scope();
                    self.values.insert(catch.name.clone(), tmp);
                    self.scope_stack
                        .last_mut()
                        .unwrap()
                        .insert(catch.name.clone(), tmp);
                    self.declare_binding(&catch.name);
                    self.mutability.insert(catch.name.clone(), false);
                    self.mutability_stack
                        .last_mut()
                        .unwrap()
                        .insert(catch.name.clone(), false);
                    self.instructions = Vec::new();
                    for statement in &catch.body.statements {
                        self.lower_statement(statement)?;
                    }
                    // Resolve the annotation through imports so
                    // `catch e: User` matches imported types too.
                    let annotation = catch
                        .annotation
                        .as_ref()
                        .map(|annotation| self.resolve_type_alias(annotation));
                    lowered_catches.push(TryCatch {
                        name: catch.name.clone(),
                        annotation,
                        body: std::mem::take(&mut self.instructions),
                        span: catch.span,
                    });
                }

                // Finally region.
                let lowered_finally = match finally {
                    Some(block) => {
                        self.scope_stack = saved_scope.clone();
                        self.mutability_stack = saved_mutability_scope.clone();
                        self.borrow_scopes = saved_borrow_scope.clone();
                        self.binding_states = saved_binding_scope.clone();
                        self.values = saved_values.clone();
                        self.mutability = saved_mutability.clone();
                        self.push_scope();
                        self.instructions = Vec::new();
                        for statement in &block.statements {
                            self.lower_statement(statement)?;
                        }
                        Some(std::mem::take(&mut self.instructions))
                    }
                    None => None,
                };

                self.instructions = saved_instructions;
                self.scope_stack = saved_scope;
                self.mutability_stack = saved_mutability_scope;
                self.borrow_scopes = saved_borrow_scope;
                self.binding_states = saved_binding_scope;
                self.values = saved_values;
                self.mutability = saved_mutability;
                self.try_stack.pop();

                self.instructions.push(Instruction::Try {
                    body: body_instrs,
                    catches: lowered_catches,
                    finally: lowered_finally,
                    tmp,
                    span: *span,
                });
                Ok(())
            }
            _ => Err(Diagnostic::error(
                "NXL2007",
                "this language construct is parsed but is not lowered yet",
                statement_span(statement),
            )),
        }
    }

    fn block_returns_on_all_paths(block: &crate::ast::Block) -> bool {
        if block.statements.is_empty() {
            return false;
        }

        let last = block.statements.last().unwrap();
        if Self::statement_returns_on_all_paths(last) {
            return true;
        }

        false
    }

    fn statement_returns_on_all_paths(statement: &Statement) -> bool {
        match statement {
            Statement::Return { .. } => true,
            Statement::Block(block) => Self::block_returns_on_all_paths(block),
            Statement::If {
                then_block,
                else_branch,
                ..
            } => {
                let then_returns = Self::block_returns_on_all_paths(then_block);
                match else_branch {
                    Some(branch) => then_returns && Self::statement_returns_on_all_paths(branch),
                    None => false,
                }
            }
            Statement::Match { cases, .. } => {
                if cases.is_empty() {
                    return false;
                }
                cases.iter().all(|case| match &case.body {
                    MatchBody::Expression(_) => false,
                    MatchBody::Block(block) => Self::block_returns_on_all_paths(block),
                })
            }
            _ => false,
        }
    }

    fn push_scope(&mut self) {
        self.scope_stack.push(HashMap::new());
        self.mutability_stack.push(HashMap::new());
        self.borrow_scopes.push(HashMap::new());
        self.binding_states.push(HashMap::new());
    }

    fn resolve_type(&self, annotation: &str) -> Option<Type> {
        type_from_name_with(
            annotation,
            &self.type_aliases,
            &self.current_type_parameters,
        )
        .or_else(|| {
            self.current_type_parameters
                .iter()
                .any(|parameter| parameter == annotation)
                .then(|| Type::Param(stable_type_id(annotation)))
        })
        .or_else(|| {
            let qualified = self.resolve_type_alias(annotation);
            self.type_names
                .contains(&qualified)
                .then(|| Type::Generic(stable_type_id(&qualified)))
        })
    }

    /// Map a bare type name to its qualified form in the current file:
    /// selected imports first, then own-module types, else unchanged.
    /// Primitives pass through untouched.
    /// Split a member chain rooted at an imported module alias:
    /// `math.Color.Red` with `import app.math as math` becomes
    /// `("app.math", ["Color", "Red"])`. Locals shadow aliases.
    fn split_module_access(&self, expression: &Expression) -> Option<(String, Vec<String>)> {
        let mut names = Vec::new();
        let mut current = expression;
        loop {
            match &current.kind {
                ExpressionKind::Member {
                    target,
                    name,
                    null_safe: false,
                } => {
                    names.push(name.clone());
                    current = target;
                }
                ExpressionKind::Name(root) => {
                    if self.lookup_declaration(root).is_some() {
                        return None;
                    }
                    for import in &self.file_imports {
                        if let crate::project::ModuleImport::Whole { path, alias } = import {
                            if alias == root {
                                names.reverse();
                                return Some((path.clone(), names));
                            }
                        }
                    }
                    // Full-path access: `app.counter.X` when `app.counter`
                    // (or a longer path below it) is imported. Longest match
                    // wins so nested modules resolve precisely.
                    names.reverse();
                    let mut dotted = root.clone();
                    for name in &names {
                        dotted.push('.');
                        dotted.push_str(name);
                    }
                    let mut best: Option<(String, Vec<String>)> = None;
                    for import in &self.file_imports {
                        if let crate::project::ModuleImport::Whole { path, .. } = import {
                            if dotted == *path {
                                best = Some((path.clone(), Vec::new()));
                            } else if let Some(rest) = dotted.strip_prefix(&format!("{path}.")) {
                                let chain = rest.split('.').map(str::to_owned).collect::<Vec<_>>();
                                if best
                                    .as_ref()
                                    .is_none_or(|(best_path, _)| path.len() > best_path.len())
                                {
                                    best = Some((path.clone(), chain));
                                }
                            }
                        }
                    }
                    return best;
                }
                _ => return None,
            }
        }
    }

    fn resolve_type_alias(&self, name: &str) -> String {
        // Primitives and unknown names pass through untouched.
        for import in &self.file_imports {
            if let crate::project::ModuleImport::Selected { path, names } = import {
                if names.iter().any(|candidate| candidate == name) {
                    return crate::project::qualify(path, name);
                }
            }
        }
        let own = crate::project::qualify(&self.current_module, name);
        if self.type_names.contains(&own) {
            return own;
        }
        name.to_owned()
    }

    /// Resolve a fully substituted annotation to a concrete type. Used for
    /// instantiated generic signatures, whose annotations were canonicalized
    /// by HIR in the declaring file's namespace.
    fn resolve_concrete_annotation(&self, annotation: &str) -> Option<Type> {
        type_from_name_with(annotation, &self.type_aliases, &[])
            .or_else(|| {
                self.type_names
                    .contains(annotation)
                    .then(|| Type::Generic(stable_type_id(annotation)))
            })
            .or_else(|| {
                let qualified = self.resolve_type_alias(annotation);
                self.type_names
                    .contains(&qualified)
                    .then(|| Type::Generic(stable_type_id(&qualified)))
            })
    }

    /// Validate and resolve one explicitly supplied type argument.
    fn resolve_type_argument(&self, written: &str, span: Span) -> Result<Type, Diagnostic> {
        let written = written.trim();
        if written.is_empty() {
            return Err(Diagnostic::error(
                "NXL2001",
                "empty generic type argument",
                span,
            ));
        }
        if self
            .current_type_parameters
            .iter()
            .any(|parameter| parameter == written)
        {
            return Err(Diagnostic::error(
                "NXL2014",
                format!(
                    "type parameter '{written}' cannot be used as an explicit type argument yet"
                ),
                span,
            )
            .with_help("explicit type arguments must name concrete types in this phase"));
        }
        if let Some(open) = written.find('<') {
            if !written.ends_with('>') {
                return Err(Diagnostic::error(
                    "NXL2004",
                    format!("unknown type '{written}' in generic argument"),
                    span,
                ));
            }
            let base = &written[..open];
            let arguments = split_type_arguments(&written[open + 1..written.len() - 1]);
            if arguments.iter().any(|argument| argument.trim().is_empty()) {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!("type '{written}' has an empty type argument"),
                    span,
                ));
            }
            if let Some(expected) = builtin_type_arity(base) {
                if expected != arguments.len() {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "type '{base}' expects {expected} type argument(s), found {}",
                            arguments.len()
                        ),
                        span,
                    ));
                }
            } else if is_primitive_type_name(base) {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!("type '{base}' is not generic"),
                    span,
                ));
            } else {
                let qualified = self.resolve_type_alias(base);
                match find_type_declaration(&self.program, &qualified) {
                    Some(declaration) => {
                        let expected = declaration.type_parameters.len();
                        if expected == 0 {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!("type '{base}' is not generic"),
                                span,
                            ));
                        }
                        if expected != arguments.len() {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!(
                                    "type '{base}' expects {expected} type argument(s), found {}",
                                    arguments.len()
                                ),
                                span,
                            ));
                        }
                    }
                    None => {
                        return Err(Diagnostic::error(
                            "NXL2004",
                            format!("unknown type '{base}' in generic argument"),
                            span,
                        ));
                    }
                }
            }
            for argument in &arguments {
                self.resolve_type_argument(argument, span)?;
            }
            return self.resolve_concrete_annotation(written).ok_or_else(|| {
                Diagnostic::error(
                    "NXL2004",
                    format!("unknown type '{written}' in generic argument"),
                    span,
                )
            });
        }

        if let Some(expected) = builtin_type_arity(written) {
            return Err(Diagnostic::error(
                "NXL2001",
                format!("type '{written}' expects {expected} type argument(s), found 0"),
                span,
            ));
        }
        let qualified = self.resolve_type_alias(written);
        if let Some(declaration) = find_type_declaration(&self.program, &qualified) {
            if !declaration.type_parameters.is_empty() {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!(
                        "type '{written}' expects {} type argument(s), found 0",
                        declaration.type_parameters.len()
                    ),
                    span,
                ));
            }
        }
        self.resolve_concrete_annotation(written).ok_or_else(|| {
            Diagnostic::error(
                "NXL2004",
                format!("unknown type '{written}' in generic argument"),
                span,
            )
        })
    }

    /// Substitute explicit type arguments into a generic function signature.
    /// Returns the instantiated parameter types, the instantiated return
    /// type, and the resolved type arguments (stored on the NIR call).
    fn instantiate_generic_signature(
        &self,
        signature: &FunctionSignature,
        type_arguments: &[String],
        display: &str,
        span: Span,
    ) -> Result<(Vec<Type>, Type, Vec<Type>), Diagnostic> {
        let expected = signature.type_parameters.len();
        if type_arguments.len() != expected {
            return Err(Diagnostic::error(
                "NXL2001",
                format!(
                    "generic function '{display}' expects {expected} type argument(s), found {}",
                    type_arguments.len()
                ),
                span,
            ));
        }
        let resolved_arguments = type_arguments
            .iter()
            .map(|argument| self.resolve_type_argument(argument, span))
            .collect::<Result<Vec<_>, _>>()?;
        let bindings = signature
            .type_parameters
            .iter()
            .cloned()
            .zip(type_arguments.iter().cloned())
            .collect::<HashMap<_, _>>();
        let erased_bindings = signature
            .type_parameters
            .iter()
            .zip(type_arguments.iter())
            .zip(resolved_arguments.iter())
            .map(|((parameter, written), ty)| (stable_type_id(parameter), (*ty, written.clone())))
            .collect::<HashMap<_, _>>();

        let mut parameters = Vec::with_capacity(signature.parameters.len());
        for (index, (_, erased)) in signature.parameters.iter().enumerate() {
            let ty =
                match signature
                    .parameter_annotations
                    .get(index)
                    .and_then(|annotation| annotation.as_deref())
                {
                    Some(annotation) => {
                        let substituted = substitute_type_parameters(annotation, &bindings);
                        self.resolve_concrete_annotation(&substituted)
                            .ok_or_else(|| {
                                Diagnostic::error(
                            "NXL2004",
                            format!("unknown type '{substituted}' in the signature of '{display}'"),
                            span,
                        )
                            })?
                    }
                    None => substitute_erased_type_parameters(*erased, &erased_bindings),
                };
            parameters.push(ty);
        }
        let return_type = match signature.return_annotation.as_deref() {
            Some(annotation) => {
                let substituted = substitute_type_parameters(annotation, &bindings);
                self.resolve_concrete_annotation(&substituted)
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "NXL2004",
                            format!("unknown type '{substituted}' in the signature of '{display}'"),
                            span,
                        )
                    })?
            }
            None => substitute_erased_type_parameters(signature.return_type, &erased_bindings),
        };
        Ok((parameters, return_type, resolved_arguments))
    }

    /// Resolve a bare function name to the qualified callee: a visible
    /// signature wins first (entry functions, builtins-in-progress), then
    /// selected imports, then own-module items.
    fn resolve_function(&self, name: &str) -> Option<String> {
        if self
            .signatures
            .iter()
            .any(|signature| signature.name == name)
        {
            return Some(name.to_owned());
        }
        for import in &self.file_imports {
            if let crate::project::ModuleImport::Selected { path, names } = import {
                if names.iter().any(|candidate| candidate == name) {
                    let qualified = crate::project::qualify(path, name);
                    if self
                        .signatures
                        .iter()
                        .any(|signature| signature.name == qualified)
                    {
                        return Some(qualified);
                    }
                    return None;
                }
            }
        }
        let own = crate::project::qualify(&self.current_module, name);
        if own != name
            && self
                .signatures
                .iter()
                .any(|signature| signature.name == own)
        {
            return Some(own);
        }
        None
    }

    /// A bare name brought in by `from std.<module> import <name>` resolves to
    /// the stdlib builtin `std::<module>::<name>`. `None` when the name is not
    /// a stdlib function (so ordinary resolution continues and then fails).
    fn resolve_stdlib_builtin(&self, name: &str) -> Option<String> {
        for import in &self.file_imports {
            if let crate::project::ModuleImport::Selected { path, names } = import {
                if names.iter().any(|candidate| candidate == name) {
                    let normalized = crate::project::normalize_path(path);
                    if let Some(module) = crate::stdlib::canonical_module(&normalized) {
                        let qualified = format!("{module}::{name}");
                        if crate::stdlib::builtin(&qualified).is_some() {
                            return Some(qualified);
                        }
                    }
                }
            }
        }
        None
    }

    /// Lower a call to a runtime builtin by its (possibly qualified) name,
    /// checking arity and stamping the declared return type.
    fn lower_builtin_call(
        &mut self,
        callee: &str,
        arguments: &[Expression],
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        if let Some(Some(arity)) = builtin_arity(callee) {
            if arguments.len() != arity {
                return Err(Diagnostic::error(
                    "NXL2002",
                    format!(
                        "built-in '{callee}' expects {arity} argument(s), got {}",
                        arguments.len()
                    ),
                    span,
                ));
            }
        }
        let mut lowered_arguments = Vec::with_capacity(arguments.len());
        for argument in arguments {
            lowered_arguments.push(self.expression(argument)?);
        }
        let return_type = builtin_return_type(callee);
        let result = self.allocate();
        let instruction = Instruction::Call {
            result,
            callee: callee.to_owned(),
            arguments: lowered_arguments,
            type_arguments: Vec::new(),
            return_type,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        Ok(result)
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
        self.mutability_stack.pop();
        self.borrow_scopes.pop();
        self.binding_states.pop();
    }

    /// Ownership state of the innermost binding called `name`.
    fn binding_state(&self, name: &str) -> Option<&BindingState> {
        self.binding_states
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
    }

    fn binding_state_mut(&mut self, name: &str) -> Option<&mut BindingState> {
        for scope in self.binding_states.iter_mut().rev() {
            if scope.contains_key(name) {
                return scope.get_mut(name);
            }
        }
        None
    }

    fn declare_binding(&mut self, name: &str) {
        self.binding_states
            .last_mut()
            .expect("a scope is always active")
            .insert(name.to_owned(), BindingState::default());
    }

    /// Whether `ty` is, or structurally contains, a reference type. References
    /// are lexical: they may be read, written and passed as arguments, but
    /// never returned or stored somewhere that outlives their borrow.
    fn type_contains_reference(&self, ty: Type, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        match ty {
            Type::Reference(..) => true,
            Type::Array(id)
            | Type::Tuple(id)
            | Type::Map(id)
            | Type::Optional(id)
            | Type::Task(id)
            | Type::Channel(id)
            | Type::Function(id) => self
                .type_by_identity(id)
                .map(|inner| self.type_contains_reference(inner, depth + 1))
                .unwrap_or(false),
            Type::Result(ok, error) => {
                let check = |id| {
                    self.type_by_identity(id)
                        .map(|inner| self.type_contains_reference(inner, depth + 1))
                        .unwrap_or(false)
                };
                check(ok) || check(error)
            }
            _ => false,
        }
    }

    /// Reject a reference argument whose destination slot cannot carry the
    /// borrow's provenance.
    ///
    /// Reference tracking is by type: a parameter declared `&T`/`&mut T` keeps
    /// the borrow visible to the checks on the callee's `return`, field,
    /// element and closure paths. An unannotated (`Unknown`) or type-parameter
    /// (`Param`) slot does not, and a call whose target cannot be resolved
    /// statically has no signature at all, so in those cases the reference
    /// would leave the caller's tracking and the program is rejected rather
    /// than silently accepted. This is a lexical constraint, not a lifetime
    /// analysis.
    fn check_reference_argument(
        &self,
        display: &str,
        index: usize,
        argument_type: Type,
        parameter_type: Option<Type>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if !self.type_contains_reference(argument_type, 0) {
            return Ok(());
        }
        if matches!(parameter_type, Some(Type::Reference(..))) {
            return Ok(());
        }
        let reason = match parameter_type {
            Some(parameter_type) => format!(
                "parameter {} of '{display}' is {parameter_type:?}, which does not name a reference type",
                index + 1
            ),
            None => format!("'{display}' has no statically known parameter types"),
        };
        Err(Diagnostic::error(
            "NXL2017",
            format!("cannot pass a reference where it cannot be tracked: {reason}"),
            span,
        )
        .with_help("declare the parameter as `&T` (or `&mut T` with `borrow mut`), or pass the owned value"))
    }

    /// The signature of `method` for a receiver whose static type names a
    /// declared type. `None` when the receiver type or the method cannot be
    /// resolved, which is the normal case for dynamic dispatch.
    fn method_signature(&self, receiver_type: Type, method: &str) -> Option<&FunctionSignature> {
        let base = match receiver_type {
            Type::Generic(id) | Type::Struct(id) | Type::Enum(id) => id,
            Type::Instance(base, _) => base,
            _ => return None,
        };
        let name = self
            .program
            .statements
            .iter()
            .find_map(|statement| {
                let declaration = match statement {
                    Statement::TypeDeclaration(declaration) => declaration,
                    Statement::Export(inner) => match inner.as_ref() {
                        Statement::TypeDeclaration(declaration) => declaration,
                        _ => return None,
                    },
                    _ => return None,
                };
                (stable_type_id(&declaration.name) == base).then(|| declaration.name.clone())
            })?;
        let qualified = format!("{name}#{method}");
        self.signatures
            .iter()
            .find(|signature| signature.name == qualified)
    }

    /// Check the arguments of a call through a method or a closure value, whose
    /// parameter types are known only when the target resolves statically.
    fn check_dynamic_arguments(
        &mut self,
        display: &str,
        receiver_type: Type,
        method: &str,
        arguments: &[Expression],
        lowered: &[ValueId],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let parameters: Option<Vec<Type>> = self
            .method_signature(receiver_type, method)
            .map(|signature| signature.parameters.iter().map(|(_, ty)| *ty).collect());
        for (index, (argument, value)) in arguments.iter().zip(lowered.iter()).enumerate() {
            let parameter_type = parameters.as_ref().and_then(|list| list.get(index).copied());
            self.check_reference_argument(
                display,
                index,
                self.value_type(*value),
                parameter_type,
                argument.span,
            )?;
        }
        Ok(())
    }

    /// Check the arguments of a call through a function *value* (`f(..)` where
    /// `f` is a closure, not a named function). The parameter types are known
    /// when the value was produced by a closure literal in this unit; anything
    /// else is an unresolved target.
    fn check_indirect_arguments(
        &mut self,
        display: &str,
        callee: ValueId,
        arguments: &[Expression],
        lowered: &[ValueId],
    ) -> Result<(), Diagnostic> {
        let parameters = self.closure_parameters.get(&callee).cloned();
        for (index, (argument, value)) in arguments.iter().zip(lowered.iter()).enumerate() {
            let parameter_type = parameters.as_ref().and_then(|list| list.get(index).copied());
            self.check_reference_argument(
                display,
                index,
                self.value_type(*value),
                parameter_type,
                argument.span,
            )?;
        }
        Ok(())
    }

    /// Active borrow kind for `name`, if any. A mutable borrow wins. Borrows
    /// recorded for a shadowed binding of the same name are ignored.
    fn active_borrow(&self, name: &str) -> Option<BorrowKind> {
        let owner_scope = self.binding_scope_index(name)?;
        let mut found = None;
        for scope in &self.borrow_scopes {
            match scope.get(name) {
                Some((BorrowKind::Mutable, borrowed_scope)) if *borrowed_scope == owner_scope => {
                    return Some(BorrowKind::Mutable)
                }
                Some((BorrowKind::Immutable, borrowed_scope)) if *borrowed_scope == owner_scope => {
                    found = Some(BorrowKind::Immutable)
                }
                _ => {}
            }
        }
        found
    }

    /// Index of the innermost active scope that declares `name`.
    fn binding_scope_index(&self, name: &str) -> Option<usize> {
        self.scope_stack
            .iter()
            .rposition(|scope| scope.contains_key(name))
    }

    /// Record a new borrow of `name`, rejecting conflicting borrows.
    fn register_borrow(
        &mut self,
        name: &str,
        kind: BorrowKind,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if self
            .binding_state(name)
            .map(|state| state.moved)
            .unwrap_or(false)
        {
            return Err(Diagnostic::error(
                "NXL2016",
                format!("cannot borrow '{name}' after it was moved"),
                span,
            ));
        }
        match (self.active_borrow(name), kind) {
            (Some(BorrowKind::Mutable), _) => {
                return Err(Diagnostic::error(
                    "NXL2015",
                    format!("cannot borrow '{name}' while it is mutably borrowed"),
                    span,
                ));
            }
            (Some(BorrowKind::Immutable), BorrowKind::Mutable) => {
                return Err(Diagnostic::error(
                    "NXL2015",
                    format!("cannot mutably borrow '{name}' while it is already borrowed"),
                    span,
                ));
            }
            _ => {}
        }
        let owner_scope = self.binding_scope_index(name).unwrap_or(0);
        self.borrow_scopes
            .last_mut()
            .expect("a scope is always active")
            .insert(name.to_owned(), (kind, owner_scope));
        Ok(())
    }

    /// Reject uses that conflict with an active borrow or a previous move.
    fn check_binding_use(
        &self,
        name: &str,
        usage: BindingUse,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if self
            .binding_state(name)
            .map(|state| state.moved)
            .unwrap_or(false)
        {
            return Err(
                Diagnostic::error("NXL2016", format!("use of moved value '{name}'"), span)
                    .with_help("reassign the binding or avoid moving it"),
            );
        }
        self.check_borrow_conflict(name, usage, span)
    }

    /// Borrow-only half of `check_binding_use`. Assignment uses this so that
    /// reassigning a moved binding is allowed: it reinitialises the binding.
    fn check_borrow_conflict(
        &self,
        name: &str,
        usage: BindingUse,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match (self.active_borrow(name), usage) {
            (Some(BorrowKind::Mutable), _) => Err(Diagnostic::error(
                "NXL2015",
                format!("cannot use '{name}' while it is mutably borrowed"),
                span,
            )),
            (Some(BorrowKind::Immutable), BindingUse::Write) => Err(Diagnostic::error(
                "NXL2015",
                format!("cannot assign to '{name}' while it is borrowed"),
                span,
            )),
            (Some(BorrowKind::Immutable), BindingUse::Move) => Err(Diagnostic::error(
                "NXL2015",
                format!("cannot move '{name}' while it is borrowed"),
                span,
            )),
            _ => Ok(()),
        }
    }

    /// The logical type of a binding (the target type when the binding is
    /// backed by a borrow cell).
    fn binding_type(&self, name: &str) -> Type {
        if let Some((target, mutable)) = self.binding_state(name).and_then(|state| state.reference)
        {
            return Type::Reference(type_identity(target), mutable);
        }
        let Some(value) = self.lookup_value(name) else {
            return Type::Unknown;
        };
        if self
            .binding_state(name)
            .map(|state| state.boxed)
            .unwrap_or(false)
        {
            if let Type::Reference(target, _) = self.value_type(value) {
                return self.type_by_identity(target).unwrap_or(Type::Unknown);
            }
        }
        self.value_type(value)
    }

    /// Ensure `name` is backed by an allocated cell so a borrow can alias the
    /// binding rather than a copy. Returns the cell value.
    fn ensure_boxed(&mut self, name: &str, span: Span) -> Result<ValueId, Diagnostic> {
        let value = self.lookup_value(name).ok_or_else(|| {
            Diagnostic::error("NXL2002", format!("use of undeclared name '{name}'"), span)
        })?;
        if self
            .binding_state(name)
            .map(|state| state.boxed)
            .unwrap_or(false)
        {
            return Ok(value);
        }
        let ty = self.value_type(value);
        let cell = self.allocate();
        let allocate = Instruction::Allocate {
            result: cell,
            ty,
            span,
        };
        self.record_value_type(&allocate);
        self.instructions.push(allocate);
        self.instructions.push(Instruction::StoreMemory {
            pointer: cell,
            value,
            span,
        });
        self.instructions.push(Instruction::Store {
            name: name.to_owned(),
            value: cell,
            span,
        });
        self.values.insert(name.to_owned(), cell);
        for scope in self.scope_stack.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_owned(), cell);
                break;
            }
        }
        if let Some(state) = self.binding_state_mut(name) {
            state.boxed = true;
        }
        Ok(cell)
    }

    /// Record the top-level type behind an identity so reference targets can
    /// be recovered later.
    fn record_identity(&mut self, ty: Type) -> TypeId {
        let id = type_identity(ty);
        self.identity_types.entry(id).or_insert(ty);
        id
    }

    /// Resolve the target of a reference type.
    fn reference_target(&self, ty: Type) -> Option<(Type, bool)> {
        match ty {
            Type::Reference(id, mutable) => {
                let target = self.type_by_identity(id)?;
                Some((target, mutable))
            }
            _ => None,
        }
    }

    /// Type of a `*reference` read.
    fn deref_type(&self, reference: ValueId, span: Span) -> Result<Type, Diagnostic> {
        let ty = self.value_type(reference);
        if let Some((target, _)) = self.reference_target(ty) {
            return Ok(target);
        }
        if is_dynamic(ty) {
            return Ok(Type::Unknown);
        }
        Err(Diagnostic::error(
            "NXL2001",
            format!("cannot dereference a value of type {ty:?}"),
            span,
        ))
    }

    /// Emit a read through a reference value.
    fn lower_reference_read(
        &mut self,
        reference: ValueId,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let ty = self.deref_type(reference, span)?;
        let result = self.allocate();
        let instruction = Instruction::LoadMemory {
            result,
            pointer: reference,
            ty,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        Ok(result)
    }

    /// Lower `borrow x`, `ref x`, `borrow mut x`, or `ref mut x`.
    fn lower_borrow(
        &mut self,
        operand: &Expression,
        mutable: bool,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let ExpressionKind::Name(name) = &operand.kind else {
            return Err(Diagnostic::error(
                "NXL2015",
                "a borrow must target a variable; borrowing fields, elements, and temporaries is not supported",
                operand.span,
            ));
        };
        if self.lookup_value(name).is_none() {
            return Err(Diagnostic::error(
                "NXL2002",
                format!("use of undeclared name '{name}'"),
                operand.span,
            ));
        }
        if self
            .binding_state(name)
            .and_then(|state| state.reference)
            .is_some()
        {
            return Err(Diagnostic::error(
                "NXL2015",
                format!("'{name}' is already a reference; pass it directly instead of borrowing it again"),
                span,
            ));
        }
        if mutable && name != "self" && !self.mutability.get(name).copied().unwrap_or(false) {
            return Err(Diagnostic::error(
                "NXL2015",
                format!("cannot mutably borrow immutable binding '{name}'"),
                span,
            )
            .with_help("declare it with 'let mut'"));
        }
        self.check_binding_use(name, BindingUse::Read, span)?;
        let target = self.binding_type(name);
        let kind = if mutable {
            BorrowKind::Mutable
        } else {
            BorrowKind::Immutable
        };
        self.register_borrow(name, kind, span)?;
        let cell = self.ensure_boxed(name, span)?;
        let target_id = self.record_identity(target);
        let result = self.allocate();
        let instruction = Instruction::Borrow {
            result,
            value: cell,
            mutable,
            ty: Type::Reference(target_id, mutable),
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        Ok(result)
    }

    /// Lower `move x`: read the value and invalidate the source binding.
    fn lower_move(&mut self, operand: &Expression, span: Span) -> Result<ValueId, Diagnostic> {
        let ExpressionKind::Name(name) = &operand.kind else {
            return Err(Diagnostic::error(
                "NXL2016",
                "ownership can only be moved out of a variable",
                operand.span,
            ));
        };
        self.check_binding_use(name, BindingUse::Move, span)?;
        let value = self.expression(operand)?;
        // A reference binding cannot outlive its owner; moving it is allowed
        // but the binding is dead afterwards either way.
        if let Some(state) = self.binding_state_mut(name) {
            state.moved = true;
        }
        Ok(value)
    }

    /// Emit a struct/array field read for an already-lowered object.
    fn lower_field_get(&mut self, object: ValueId, field: &str, ty: Type, span: Span) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::FieldGet {
            result,
            object,
            field: field.to_owned(),
            ty,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        result
    }

    /// Emit a copy-on-write field update; returns the id holding the new object.
    fn lower_field_set(
        &mut self,
        object: ValueId,
        field: &str,
        value: ValueId,
        span: Span,
    ) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::FieldSet {
            result,
            object,
            field: field.to_owned(),
            value,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        result
    }

    /// Emit an element read for an already-lowered object and index.
    fn lower_index_get(
        &mut self,
        object: ValueId,
        index: ValueId,
        ty: Type,
        span: Span,
    ) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::IndexGet {
            result,
            object,
            index,
            ty,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        result
    }

    /// Emit a total element read for match patterns (Unit on failure).
    fn lower_pattern_index(&mut self, object: ValueId, index: ValueId, span: Span) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::PatternIndex {
            result,
            object,
            index,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        result
    }

    /// Emit a total field read for match patterns (Unit on failure).
    fn lower_pattern_field(&mut self, object: ValueId, field: &str, span: Span) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::PatternField {
            result,
            object,
            field: field.to_owned(),
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        result
    }

    /// Emit a copy-on-write element update; returns the id holding the new array.
    fn lower_index_set(
        &mut self,
        object: ValueId,
        index: ValueId,
        value: ValueId,
        span: Span,
    ) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::IndexSet {
            result,
            object,
            index,
            value,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        result
    }

    /// The root variable of a member/index chain, if it is a plain name
    /// (used for mutability checks and write-back).
    fn place_root_name(expression: &Expression) -> Option<&str> {
        match &expression.kind {
            ExpressionKind::Name(name) => Some(name),
            ExpressionKind::Member { target, .. } | ExpressionKind::Index { target, .. } => {
                Self::place_root_name(target)
            }
            _ => None,
        }
    }

    /// A lowered boolean `left == right` test.
    fn lower_eq_test(&mut self, left: ValueId, right: ValueId, span: Span) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::Binary {
            result,
            operator: BinaryOperator::Equal,
            left,
            right,
            ty: Type::Bool,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        result
    }

    /// A lowered boolean `left && right`.
    fn lower_and_test(&mut self, left: ValueId, right: ValueId, span: Span) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::Binary {
            result,
            operator: BinaryOperator::And,
            left,
            right,
            ty: Type::Bool,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        result
    }

    fn bool_const(&mut self, value: bool, span: Span) -> ValueId {
        self.constant(Constant::Boolean(value), Type::Bool, span)
    }

    /// Lower a match pattern to a boolean test against an already-lowered
    /// value, binding any pattern variables in the current scope.
    fn lower_pattern_test(
        &mut self,
        pattern: &Pattern,
        value_id: ValueId,
    ) -> Result<ValueId, Diagnostic> {
        match &pattern.kind {
            PatternKind::Wildcard => Ok(self.bool_const(true, pattern.span)),
            PatternKind::Binding(name) => {
                // Nullary enum variants take precedence over bindings.
                let variants = find_enum_variant(&self.program, name);
                if !variants.is_empty() {
                    if variants.len() > 1 {
                        return Err(Diagnostic::error(
                            "NXL2002",
                            format!("ambiguous variant '{name}': matches several enums"),
                            pattern.span,
                        ));
                    }
                    let (type_name, payload) = &variants[0];
                    if !payload.is_empty() {
                        return Err(Diagnostic::error(
                            "NXL2001",
                            format!(
                                "variant '{name}' takes {} argument(s): use {name}(...)",
                                payload.len()
                            ),
                            pattern.span,
                        ));
                    }
                    return Ok(self.lower_variant_test(value_id, type_name, name, pattern.span));
                }
                if self.lookup_declaration(name).is_some() {
                    return Err(Diagnostic::error(
                        "NXL2005",
                        format!("duplicate declaration of '{name}' in match pattern"),
                        pattern.span,
                    ));
                }
                self.values.insert(name.clone(), value_id);
                self.scope_stack
                    .last_mut()
                    .unwrap()
                    .insert(name.clone(), value_id);
                self.mutability.insert(name.clone(), false);
                self.mutability_stack
                    .last_mut()
                    .unwrap()
                    .insert(name.clone(), false);
                // Materialise the binding so backends resolve the name.
                self.instructions.push(Instruction::Store {
                    name: name.clone(),
                    value: value_id,
                    span: pattern.span,
                });
                Ok(self.bool_const(true, pattern.span))
            }
            PatternKind::Literal(literal) => {
                let literal_id = self.expression(literal)?;
                Ok(self.lower_eq_test(value_id, literal_id, pattern.span))
            }
            PatternKind::Variant { name, fields } => {
                let variants = find_enum_variant(&self.program, name);
                if variants.is_empty() {
                    return Err(Diagnostic::error(
                        "NXL2002",
                        format!("unknown variant '{name}' in match pattern"),
                        pattern.span,
                    ));
                }
                if variants.len() > 1 {
                    return Err(Diagnostic::error(
                        "NXL2002",
                        format!("ambiguous variant '{name}': matches several enums"),
                        pattern.span,
                    ));
                }
                let (type_name, payload) = &variants[0];
                if payload.len() != fields.len() {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "variant '{name}' takes {} argument(s), pattern has {}",
                            payload.len(),
                            fields.len()
                        ),
                        pattern.span,
                    ));
                }
                let mut test_id = self.lower_variant_test(value_id, type_name, name, pattern.span);
                for (index, field) in fields.iter().enumerate() {
                    let index_id =
                        self.constant(Constant::Integer(index as i64), Type::Int, field.span);
                    let element_id = self.lower_pattern_index(value_id, index_id, field.span);
                    // Payloads declared with a concrete type keep it, so
                    // arithmetic and calls on bindings stay statically typed.
                    if let PatternKind::Binding(_) = &field.kind {
                        if let Some(payload_type) = payload
                            .get(index)
                            .and_then(|annotation| self.resolve_type(annotation))
                        {
                            self.value_types.insert(element_id, payload_type);
                        }
                    }
                    let field_test = self.lower_pattern_test(field, element_id)?;
                    test_id = self.lower_and_test(test_id, field_test, field.span);
                }
                Ok(test_id)
            }
            PatternKind::Struct { name, fields } => {
                let qualified = self.resolve_type_alias(name);
                let declaration = find_type_declaration(&self.program, &qualified)
                    .cloned()
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "NXL2004",
                            format!("unknown type '{name}' in match pattern"),
                            pattern.span,
                        )
                    })?;
                if !matches!(
                    declaration.kind,
                    crate::ast::DeclarationKind::Struct
                        | crate::ast::DeclarationKind::Class
                        | crate::ast::DeclarationKind::Actor
                ) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "cannot match on {:?} '{name}' as a struct",
                            declaration.kind
                        ),
                        pattern.span,
                    ));
                }
                let name_id = self.constant(
                    Constant::String(qualified.clone()),
                    Type::String,
                    pattern.span,
                );
                let tag_id = self.allocate();
                let tag_instruction = Instruction::TypeName {
                    result: tag_id,
                    value: value_id,
                    span: pattern.span,
                };
                self.record_value_type(&tag_instruction);
                self.instructions.push(tag_instruction);
                let mut test_id = self.lower_eq_test(tag_id, name_id, pattern.span);
                for (field_name, field_pattern) in fields {
                    if !declaration.fields.iter().any(|f| &f.name == field_name) {
                        return Err(Diagnostic::error(
                            "NXL2002",
                            format!("unknown field '{field_name}' on struct '{name}'"),
                            pattern.span,
                        ));
                    }
                    let field_id = self.lower_pattern_field(value_id, field_name, pattern.span);
                    let field_test = self.lower_pattern_test(field_pattern, field_id)?;
                    test_id = self.lower_and_test(test_id, field_test, pattern.span);
                }
                Ok(test_id)
            }
            PatternKind::Tuple(_) | PatternKind::Array(_) => Err(Diagnostic::error(
                "NXL2007",
                "tuple and array patterns are not supported yet",
                pattern.span,
            )),
            PatternKind::Or(alternatives) => {
                if alternatives.iter().any(Self::pattern_contains_binding) {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "bindings in or-patterns are not supported yet",
                        pattern.span,
                    ));
                }
                let mut test_id = self.bool_const(false, pattern.span);
                for alternative in alternatives {
                    let alternative_test = self.lower_pattern_test(alternative, value_id)?;
                    let result = self.allocate();
                    let instruction = Instruction::Binary {
                        result,
                        operator: BinaryOperator::Or,
                        left: test_id,
                        right: alternative_test,
                        ty: Type::Bool,
                        span: pattern.span,
                    };
                    self.record_value_type(&instruction);
                    self.instructions.push(instruction);
                    test_id = result;
                }
                Ok(test_id)
            }
        }
    }

    /// Lower an enum-variant test: true when the value is that variant.
    fn lower_variant_test(
        &mut self,
        value_id: ValueId,
        _type_name: &str,
        variant: &str,
        span: Span,
    ) -> ValueId {
        let tag_id = self.allocate();
        let tag_instruction = Instruction::VariantName {
            result: tag_id,
            value: value_id,
            span,
        };
        self.record_value_type(&tag_instruction);
        self.instructions.push(tag_instruction);
        let name_id = self.constant(Constant::String(variant.to_owned()), Type::String, span);
        self.lower_eq_test(tag_id, name_id, span)
    }

    /// Whether a pattern binds a variable (which or-patterns cannot support yet).
    fn pattern_contains_binding(pattern: &Pattern) -> bool {
        match &pattern.kind {
            PatternKind::Binding(_) => true,
            PatternKind::Variant { fields, .. }
            | PatternKind::Tuple(fields)
            | PatternKind::Array(fields) => fields.iter().any(Self::pattern_contains_binding),
            PatternKind::Struct { fields, .. } => fields
                .iter()
                .any(|(_, field)| Self::pattern_contains_binding(field)),
            PatternKind::Or(alternatives) => {
                alternatives.iter().any(Self::pattern_contains_binding)
            }
            PatternKind::Wildcard | PatternKind::Literal(_) => false,
        }
    }

    /// Lower `*r = v` / `*r op= v`: write through a mutable reference.
    fn assign_through_reference(
        &mut self,
        reference: &Expression,
        operator: crate::ast::AssignmentOperator,
        value: &Expression,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let pointer = self.expression(reference)?;
        let pointer_type = self.value_type(pointer);
        let mutable = match pointer_type {
            Type::Reference(_, mutable) => mutable,
            ty if is_dynamic(ty) => true,
            ty => {
                return Err(Diagnostic::error(
                    "NXL2015",
                    format!("cannot assign through a value of type {ty:?}; expected a reference"),
                    reference.span,
                ));
            }
        };
        if !mutable {
            return Err(Diagnostic::error(
                "NXL2015",
                "cannot write through an immutable reference",
                reference.span,
            )
            .with_help("borrow the owner mutably with 'borrow mut' or 'ref mut'"));
        }
        let assigned = match operator {
            crate::ast::AssignmentOperator::Assign => self.expression(value)?,
            _ => {
                let read = self.lower_reference_read(pointer, reference.span)?;
                let rhs = self.expression(value)?;
                let binary = match operator {
                    crate::ast::AssignmentOperator::Add => BinaryOperator::Add,
                    crate::ast::AssignmentOperator::Subtract => BinaryOperator::Subtract,
                    crate::ast::AssignmentOperator::Multiply => BinaryOperator::Multiply,
                    crate::ast::AssignmentOperator::Divide => BinaryOperator::Divide,
                    crate::ast::AssignmentOperator::Remainder => BinaryOperator::Remainder,
                    crate::ast::AssignmentOperator::And => BinaryOperator::And,
                    crate::ast::AssignmentOperator::Or => BinaryOperator::Or,
                    crate::ast::AssignmentOperator::Assign => unreachable!(),
                };
                let left_type = erased_for_check(self.value_type(read));
                let right_type = erased_for_check(self.value_type(rhs));
                if left_type != right_type
                    && left_type != Type::Unknown
                    && right_type != Type::Unknown
                {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "operator operands have incompatible types: {left_type:?} and {right_type:?}"
                        ),
                        span,
                    ));
                }
                let result = self.allocate();
                let ty = match binary {
                    BinaryOperator::And | BinaryOperator::Or => Type::Bool,
                    _ => left_type,
                };
                let instruction = Instruction::Binary {
                    result,
                    operator: binary,
                    left: read,
                    right: rhs,
                    ty,
                    span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                result
            }
        };
        // A write through `&mut T` must store a `T`; report that at the
        // assignment rather than leaving it to the NIR verifier.
        if let Some((target, _)) = self.reference_target(pointer_type) {
            let assigned_type = self.value_type(assigned);
            if !types_compatible(assigned_type, target) {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!(
                        "cannot store a value of type {assigned_type:?} through a reference to {target:?}"
                    ),
                    span,
                ));
            }
        }
        self.instructions.push(Instruction::StoreMemory {
            pointer,
            value: assigned,
            span,
        });
        Ok(assigned)
    }

    /// Lower the object of a member/index assignment. When the object is a
    /// dereference (`(*r).field = v`), return the reference value as well so
    /// the updated container can be written back through it.
    fn lower_place_object(
        &mut self,
        object: &Expression,
    ) -> Result<(ValueId, Option<ValueId>), Diagnostic> {
        if let ExpressionKind::Unary {
            operator: UnaryOperator::Deref,
            operand,
        } = &object.kind
        {
            let pointer = self.expression(operand)?;
            let mutable = match self.value_type(pointer) {
                Type::Reference(_, mutable) => mutable,
                ty if is_dynamic(ty) => true,
                ty => {
                    return Err(Diagnostic::error(
                        "NXL2015",
                        format!(
                            "cannot assign through a value of type {ty:?}; expected a reference"
                        ),
                        object.span,
                    ));
                }
            };
            if !mutable {
                return Err(Diagnostic::error(
                    "NXL2015",
                    "cannot write through an immutable reference",
                    object.span,
                )
                .with_help("borrow the owner mutably with 'borrow mut' or 'ref mut'"));
            }
            let read = self.lower_reference_read(pointer, object.span)?;
            return Ok((read, Some(pointer)));
        }
        // Deeper paths through a reference would need path-sensitive
        // write-back; reject them instead of silently dropping the update.
        if Self::place_contains_deref(object) {
            return Err(Diagnostic::error(
                "NXL2007",
                "nested assignment through a reference is not supported yet; assign a direct field or dereference the owner",
                object.span,
            ));
        }
        Ok((self.expression(object)?, None))
    }

    /// Whether a place expression dereferences anywhere along its path.
    fn place_contains_deref(expression: &Expression) -> bool {
        match &expression.kind {
            ExpressionKind::Unary {
                operator: UnaryOperator::Deref,
                ..
            } => true,
            ExpressionKind::Member { target, .. } | ExpressionKind::Index { target, .. } => {
                Self::place_contains_deref(target)
            }
            _ => false,
        }
    }

    /// Lower an assignment whose target is a member or index expression.
    /// The object evaluates once; the result is written back when the root
    /// is a plain variable or a dereferenced mutable reference. The assignment
    /// evaluates to the right-hand value.
    fn assign_to_place(
        &mut self,
        target: &Expression,
        operator: crate::ast::AssignmentOperator,
        value: &Expression,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        enum Place<'a> {
            Field { object: ValueId, field: &'a str },
            Index { object: ValueId, index: ValueId },
        }
        /// Where the updated container is written back to.
        #[derive(Clone, Copy)]
        enum Root<'a> {
            /// A plain binding (which may itself live in a borrow cell).
            Name(&'a str),
            /// Through a mutable reference: `(*r).field = v`.
            Deref(ValueId),
        }
        let (place, root) = match &target.kind {
            ExpressionKind::Member {
                target: object,
                name: field,
                null_safe,
            } => {
                if *null_safe {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "safe navigation (?.) is not supported yet",
                        target.span,
                    ));
                }
                let (object_id, deref) = self.lower_place_object(object)?;
                (
                    Place::Field {
                        object: object_id,
                        field,
                    },
                    deref
                        .map(Root::Deref)
                        .or_else(|| Self::place_root_name(object).map(Root::Name)),
                )
            }
            ExpressionKind::Index {
                target: object,
                index,
            } => {
                let (object_id, deref) = self.lower_place_object(object)?;
                let index_id = self.expression(index)?;
                let index_type = self.value_type(index_id);
                if index_type != Type::Int && !is_dynamic(index_type) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!("array index has type {index_type:?}, expected Int"),
                        index.span,
                    ));
                }
                (
                    Place::Index {
                        object: object_id,
                        index: index_id,
                    },
                    deref
                        .map(Root::Deref)
                        .or_else(|| Self::place_root_name(object).map(Root::Name)),
                )
            }
            _ => {
                return Err(Diagnostic::error(
                    "NXL2008",
                    "assignment target must be a variable, field, or index",
                    target.span,
                ));
            }
        };
        if let Some(Root::Name(root)) = root {
            // `self` is passed by mutable reference into every method: field
            // writes through it are always allowed (rebinding it is not).
            if root != "self" {
                let is_mutable = self.mutability.get(root).copied().unwrap_or(false);
                if !is_mutable {
                    return Err(Diagnostic::error(
                        "NXL2008",
                        format!("cannot assign through immutable binding '{root}'"),
                        target.span,
                    ));
                }
                if self
                    .binding_state(root)
                    .and_then(|state| state.reference)
                    .is_some()
                {
                    return Err(Diagnostic::error(
                        "NXL2015",
                        format!(
                            "cannot assign through reference binding '{root}'; dereference it first with '*{root}'"
                        ),
                        target.span,
                    ));
                }
                self.check_binding_use(root, BindingUse::Write, target.span)?;
            }
        }
        let assigned = match operator {
            crate::ast::AssignmentOperator::Assign => self.expression(value)?,
            _ => {
                let read = match place {
                    Place::Field { object, field } => {
                        let ty = self.inferred_field_type(object, &field);
                        self.lower_field_get(object, &field, ty, target.span)
                    }
                    Place::Index { object, index } => {
                        let ty = self.inferred_element_type(object);
                        self.lower_index_get(object, index, ty, target.span)
                    }
                };
                let rhs = self.expression(value)?;
                let binary = match operator {
                    crate::ast::AssignmentOperator::Add => BinaryOperator::Add,
                    crate::ast::AssignmentOperator::Subtract => BinaryOperator::Subtract,
                    crate::ast::AssignmentOperator::Multiply => BinaryOperator::Multiply,
                    crate::ast::AssignmentOperator::Divide => BinaryOperator::Divide,
                    crate::ast::AssignmentOperator::Remainder => BinaryOperator::Remainder,
                    crate::ast::AssignmentOperator::And => BinaryOperator::And,
                    crate::ast::AssignmentOperator::Or => BinaryOperator::Or,
                    crate::ast::AssignmentOperator::Assign => unreachable!(),
                };
                let left_type = erased_for_check(self.value_type(read));
                let right_type = erased_for_check(self.value_type(rhs));
                if left_type != right_type
                    && left_type != Type::Unknown
                    && right_type != Type::Unknown
                {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "operator operands have incompatible types: {left_type:?} and {right_type:?}"
                        ),
                        span,
                    ));
                }
                let result = self.allocate();
                let ty = match binary {
                    BinaryOperator::And | BinaryOperator::Or => Type::Bool,
                    _ => left_type,
                };
                let instruction = Instruction::Binary {
                    result,
                    operator: binary,
                    left: read,
                    right: rhs,
                    ty,
                    span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                result
            }
        };
        if self.type_contains_reference(self.value_type(assigned), 0) {
            return Err(Diagnostic::error(
                "NXL2017",
                "a reference cannot be stored in a field or element: references may not escape the borrow scope",
                span,
            ));
        }
        // The place keeps the type it already had: reject a value that does
        // not match instead of storing an ill-typed field or element.
        let place_type = match &place {
            Place::Field { object, field } => {
                let object_type = self.value_type(*object);
                self.field_type_of(object_type, field)
            }
            Place::Index { object, .. } => {
                let object_type = self.value_type(*object);
                self.array_element_type(object_type)
            }
        };
        if let Some(expected) = place_type {
            let assigned_type = self.value_type(assigned);
            if !types_compatible(assigned_type, expected) {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!(
                        "cannot assign a value of type {assigned_type:?} to a place of type {expected:?}"
                    ),
                    span,
                ));
            }
        }
        // Write the updated container back to the root binding.
        let updated = match place {
            Place::Field { object, field } => self.lower_field_set(object, field, assigned, span),
            Place::Index { object, index } => self.lower_index_set(object, index, assigned, span),
        };
        if let Some(root) = root {
            match root {
                // A borrowed binding lives in a cell: update the cell so live
                // references observe the change, instead of replacing the slot.
                Root::Name(root)
                    if self
                        .binding_state(root)
                        .map(|state| state.boxed)
                        .unwrap_or(false) =>
                {
                    let cell = self
                        .lookup_value(root)
                        .expect("binding exists, checked above");
                    self.instructions.push(Instruction::StoreMemory {
                        pointer: cell,
                        value: updated,
                        span,
                    });
                }
                Root::Name(root) => {
                    self.instructions.push(Instruction::Store {
                        name: root.to_owned(),
                        value: updated,
                        span,
                    });
                }
                // Write the whole updated value back through the reference.
                Root::Deref(pointer) => {
                    self.instructions.push(Instruction::StoreMemory {
                        pointer,
                        value: updated,
                        span,
                    });
                }
            }
        }
        Ok(assigned)
    }

    /// Register every method of `owner`'s declaration list as a mangled
    /// `Owner#method` signature (with an implicit `self` first parameter).
    /// Registration is lazy per declaration so nested classes work too.
    fn register_methods_for(
        &mut self,
        owner: &str,
        methods: &[crate::ast::Function],
        _span: Span,
    ) -> Result<(), Diagnostic> {
        for method in methods {
            let Some(method_name) = method.name.as_ref() else {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "anonymous methods are not supported",
                    method.span,
                ));
            };
            let mangled = format!("{owner}#{method_name}");
            if self.signatures.iter().any(|sig| sig.name == mangled) {
                return Err(Diagnostic::error(
                    "NXL2005",
                    format!("duplicate method '{method_name}' on '{owner}'"),
                    method.span,
                ));
            }
            let mut parameters = vec![("self".to_owned(), Type::Generic(stable_type_id(owner)))];
            for parameter in &method.parameters {
                if parameter.name == "self" {
                    return Err(Diagnostic::error(
                        "NXL2005",
                        "a method parameter cannot be named 'self'",
                        parameter.span,
                    ));
                }
                let ty = match &parameter.annotation {
                    Some(annotation) => self.resolve_type(annotation).ok_or_else(|| {
                        Diagnostic::error(
                            "NXL2004",
                            format!("unknown type '{annotation}'"),
                            parameter.span,
                        )
                    })?,
                    None => Type::Unknown,
                };
                if parameters.iter().any(|(name, _)| name == &parameter.name) {
                    return Err(Diagnostic::error(
                        "NXL2005",
                        format!(
                            "duplicate parameter name '{}' in method '{method_name}'",
                            parameter.name
                        ),
                        parameter.span,
                    ));
                }
                parameters.push((parameter.name.clone(), ty));
            }
            let return_type = match &method.return_type {
                Some(annotation) => self.resolve_type(annotation).ok_or_else(|| {
                    Diagnostic::error(
                        "NXL2004",
                        format!("unknown type '{annotation}'"),
                        method.span,
                    )
                })?,
                None => Type::Unknown,
            };
            self.signatures.push(FunctionSignature {
                name: mangled,
                parameters,
                return_type,
                type_parameters: method.type_parameters.clone(),
                parameter_annotations: method
                    .parameters
                    .iter()
                    .map(|parameter| parameter.annotation.clone())
                    .collect(),
                return_annotation: method.return_type.clone(),
            });
        }
        Ok(())
    }

    /// Names of the methods declared directly on a class-like type.
    fn direct_methods(&self, owner: &str) -> Vec<crate::ast::Function> {
        let mut out = Vec::new();
        if let Some(declaration) = find_type_declaration(&self.program, owner) {
            if matches!(
                declaration.kind,
                crate::ast::DeclarationKind::Class | crate::ast::DeclarationKind::Actor
            ) {
                out.extend(declaration.methods.iter().cloned());
            }
        }
        out
    }

    /// Arity (excluding `self`) of a method visible on a type, searching the
    /// class body first and then every `impl` block for the target.
    fn find_method_arity(&self, owner: &str, method: &str) -> Option<usize> {
        let mut search = self.direct_methods(owner);
        for statement in &self.program.statements {
            if let Statement::ImplBlock(implementation) = statement {
                if implementation.target == owner {
                    search.extend(implementation.methods.iter().cloned());
                }
            }
        }
        search.iter().find_map(|function| {
            (function.name.as_deref() == Some(method)).then_some(function.parameters.len())
        })
    }

    /// Lower `Type(args)` construction for class-like types.
    fn lower_construction(
        &mut self,
        type_name: &str,
        arguments: &[Expression],
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        // User-facing messages use the short name; IR keeps the qualified one.
        let display = short_name(type_name);
        let declaration = find_type_declaration(&self.program, type_name).cloned();
        // Every actor instance carries a `mailbox` channel unless it declares
        // its own, so actors can always receive messages.
        let needs_mailbox = matches!(
            declaration.as_ref().map(|decl| decl.kind),
            Some(crate::ast::DeclarationKind::Actor)
        ) && declaration
            .as_ref()
            .is_none_or(|decl| !decl.fields.iter().any(|field| field.name == "mailbox"));
        // `init` protocol when the type (or one of its impl blocks) defines it.
        if let Some(init_arity) = self.find_method_arity(type_name, "init") {
            if arguments.len() != init_arity {
                return Err(Diagnostic::error(
                    "NXL2009",
                    format!(
                        "constructor 'init' of '{display}' expects {init_arity} argument(s), found {}",
                        arguments.len()
                    ),
                    span,
                ));
            }
            let mut lowered = Vec::with_capacity(arguments.len());
            for argument in arguments {
                lowered.push(self.expression(argument)?);
            }
            let base = self.allocate();
            let base_instruction = Instruction::StructNew {
                result: base,
                type_name: type_name.to_owned(),
                ty: Type::Generic(stable_type_id(type_name)),
                fields: Vec::new(),
                span,
            };
            self.record_value_type(&base_instruction);
            self.instructions.push(base_instruction);
            let mut receiver = base;
            if needs_mailbox {
                let channel = self.allocate();
                let channel_instruction = Instruction::ChannelNew {
                    result: channel,
                    span,
                };
                self.record_value_type(&channel_instruction);
                self.instructions.push(channel_instruction);
                receiver = self.lower_field_set(base, "mailbox", channel, span);
            }
            let discard = self.allocate();
            let updated = self.allocate();
            let call = Instruction::MethodCall {
                result: discard,
                receiver_out: updated,
                receiver,
                method: "init".to_owned(),
                arguments: lowered,
                span,
            };
            self.record_value_type(&call);
            self.instructions.push(call);
            return Ok(updated);
        }
        // Auto-constructor: positional arguments bind constructor parameters
        // by name; remaining fields fall back to their defaults.
        let Some(mut declaration) = declaration else {
            return Err(Diagnostic::error(
                "NXL2004",
                format!("unknown type '{display}'"),
                span,
            ));
        };
        if needs_mailbox {
            declaration.fields.push(crate::ast::Field {
                name: "mailbox".to_owned(),
                annotation: "Channel".to_owned(),
                value: Some(Expression {
                    kind: ExpressionKind::Channel { element_type: None },
                    span,
                }),
                is_const: false,
                span,
            });
        }
        if arguments.len() != declaration.constructor_parameters.len() {
            return Err(Diagnostic::error(
                "NXL2009",
                format!(
                    "constructor of '{display}' expects {} argument(s), found {}",
                    declaration.constructor_parameters.len(),
                    arguments.len()
                ),
                span,
            ));
        }
        let mut bound: HashMap<String, ValueId> = HashMap::new();
        for (parameter, argument) in declaration
            .constructor_parameters
            .iter()
            .zip(arguments.iter())
        {
            let argument_id = self.expression(argument)?;
            if let Some(annotation) = parameter.annotation.as_ref() {
                let actual = self.value_type(argument_id);
                self.check_field_value(display, &parameter.name, annotation, actual, span)?;
            }
            bound.insert(parameter.name.clone(), argument_id);
        }
        for parameter in &declaration.constructor_parameters {
            if !declaration
                .fields
                .iter()
                .any(|field| field.name == parameter.name)
            {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!(
                        "constructor parameter '{}' has no matching field on '{display}'",
                        parameter.name
                    ),
                    span,
                ));
            }
        }
        let mut fields = Vec::with_capacity(declaration.fields.len());
        let mut field_types = HashMap::new();
        for field in &declaration.fields {
            if let Some(argument_id) = bound.get(&field.name).copied() {
                let actual = self.value_type(argument_id);
                self.check_field_value(display, &field.name, &field.annotation, actual, span)?;
                if let Some(ty) = self.resolve_type(&field.annotation) {
                    field_types.insert(field.name.clone(), ty);
                }
                fields.push((field.name.clone(), argument_id));
            } else if let Some(default) = field.value.clone() {
                let default_id = self.expression(&default)?;
                let actual = self.value_type(default_id);
                self.check_field_value(display, &field.name, &field.annotation, actual, span)?;
                if let Some(ty) = self.resolve_type(&field.annotation) {
                    field_types.insert(field.name.clone(), ty);
                }
                fields.push((field.name.clone(), default_id));
            } else {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!(
                        "'{display}' is missing a value for field '{}' (define init or a default)",
                        field.name
                    ),
                    span,
                ));
            }
        }
        let result = self.allocate();
        let instruction = Instruction::StructNew {
            result,
            type_name: type_name.to_owned(),
            ty: Type::Generic(stable_type_id(type_name)),
            fields,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        self.value_field_types.insert(result, field_types);
        Ok(result)
    }

    /// Lower a call to an already-resolved (possibly qualified) function.
    /// `display` is the source-written name used in diagnostics. With
    /// `concurrent`, the call spawns a worker thread instead of running now.
    #[allow(clippy::too_many_arguments)]
    fn lower_direct_call(
        &mut self,
        qualified: &str,
        display: &str,
        type_arguments: &[String],
        arguments: &[Expression],
        callee_span: Span,
        span: Span,
        concurrent: bool,
    ) -> Result<ValueId, Diagnostic> {
        // An `extern` (C ABI) function: emit a self-contained ForeignCall that
        // carries its library, symbol, and scalar signature inline.
        if let Some(foreign) = self.externs.get(qualified).cloned() {
            if !type_arguments.is_empty() {
                return Err(Diagnostic::error(
                    "NXL2014",
                    format!("extern function '{display}' is not generic"),
                    span,
                ));
            }
            if arguments.len() != foreign.parameters.len() {
                return Err(Diagnostic::error(
                    "NXL2009",
                    format!(
                        "extern function '{display}' expects {} argument(s), found {}",
                        foreign.parameters.len(),
                        arguments.len()
                    ),
                    span,
                ));
            }
            let mut lowered_arguments = Vec::with_capacity(arguments.len());
            for (index, (argument, parameter)) in
                arguments.iter().zip(foreign.parameters.iter()).enumerate()
            {
                let argument_value = self.expression(argument)?;
                let argument_type = self.value_type(argument_value);
                let expected = ffi_scalar_type(*parameter);
                if !types_compatible(argument_type, expected) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "argument {} to extern function '{display}' has type {argument_type:?}, expected {expected:?}",
                            index + 1
                        ),
                        argument.span,
                    ));
                }
                lowered_arguments.push(argument_value);
            }
            let result = self.allocate();
            let instruction = Instruction::ForeignCall {
                result,
                callee: display.to_owned(),
                library: foreign.library.clone(),
                symbol: foreign.symbol.clone(),
                arguments: lowered_arguments,
                parameter_types: foreign.parameters.clone(),
                return_type: foreign.return_type,
                calling_convention: foreign.calling_convention,
                span,
            };
            self.record_value_type(&instruction);
            self.instructions.push(instruction);
            return Ok(result);
        }

        // Check for user-defined function signature
        let signature = self
            .signatures
            .iter()
            .find(|sig| sig.name == qualified)
            .cloned()
            .ok_or_else(|| {
                Diagnostic::error(
                    "NXL2002",
                    format!("use of undeclared function '{display}'"),
                    callee_span,
                )
            })?;
        let mut lowered_arguments = Vec::with_capacity(arguments.len());
        let (_param_types, return_type, resolved_type_arguments) = if signature
            .type_parameters
            .is_empty()
        {
            if !type_arguments.is_empty() {
                return Err(Diagnostic::error(
                    "NXL2014",
                    format!("'{display}' is not generic and takes no type arguments"),
                    span,
                ));
            }
            let param_types = signature
                .parameters
                .iter()
                .map(|(_, ty)| *ty)
                .collect::<Vec<_>>();
            if arguments.len() != param_types.len() {
                return Err(Diagnostic::error(
                    "NXL2009",
                    format!(
                        "function '{display}' expects {} argument(s), found {}",
                        param_types.len(),
                        arguments.len()
                    ),
                    span,
                ));
            }
            for (index, (arg, param_type)) in arguments.iter().zip(param_types.iter()).enumerate()
            {
                let arg_value = self.expression(arg)?;
                let arg_type = self.value_type(arg_value);
                if !types_compatible(arg_type, *param_type) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "argument to '{display}' has type {arg_type:?}, expected {param_type:?}"
                        ),
                        arg.span,
                    ));
                }
                self.check_reference_argument(
                    display,
                    index,
                    arg_type,
                    Some(*param_type),
                    arg.span,
                )?;
                lowered_arguments.push(arg_value);
            }
            (param_types, signature.return_type, Vec::new())
        } else if !type_arguments.is_empty() {
            let (param_types, return_type, resolved) =
                self.instantiate_generic_signature(&signature, type_arguments, display, span)?;
            if arguments.len() != param_types.len() {
                return Err(Diagnostic::error(
                    "NXL2009",
                    format!(
                        "function '{display}' expects {} argument(s), found {}",
                        param_types.len(),
                        arguments.len()
                    ),
                    span,
                ));
            }
            for (index, (arg, param_type)) in arguments.iter().zip(param_types.iter()).enumerate()
            {
                let arg_value = self.expression(arg)?;
                let arg_type = self.value_type(arg_value);
                if !types_compatible(arg_type, *param_type) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "argument to '{display}' has type {arg_type:?}, expected {param_type:?}"
                        ),
                        arg.span,
                    ));
                }
                self.check_reference_argument(
                    display,
                    index,
                    arg_type,
                    Some(*param_type),
                    arg.span,
                )?;
                lowered_arguments.push(arg_value);
            }
            (param_types, return_type, resolved)
        } else {
            // Infer type arguments from the lowered argument types.
            for arg in arguments {
                lowered_arguments.push(self.expression(arg)?);
            }
            if arguments.len() != signature.parameters.len() {
                return Err(Diagnostic::error(
                    "NXL2009",
                    format!(
                        "function '{display}' expects {} argument(s), found {}",
                        signature.parameters.len(),
                        arguments.len()
                    ),
                    span,
                ));
            }
            let mut inferred = HashMap::new();
            for (index, value) in lowered_arguments.iter().enumerate() {
                let Some(Some(annotation)) = signature.parameter_annotations.get(index) else {
                    continue;
                };
                if self
                    .bind_annotation_to_type(
                        annotation,
                        self.value_type(*value),
                        &signature.type_parameters,
                        &mut inferred,
                    )
                    .is_err()
                {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "cannot infer consistent type argument(s) for '{display}': arguments disagree"
                        ),
                        span,
                    ));
                }
            }
            let written = self.inferred_arguments(
                display,
                &signature.type_parameters,
                &inferred,
                &format!(
                    "add <{}> after the name",
                    signature.type_parameters.join(", ")
                ),
                span,
            )?;
            let ordered = signature
                .type_parameters
                .iter()
                .map(|parameter| written[parameter].clone())
                .collect::<Vec<_>>();
            let (param_types, return_type, resolved) =
                self.instantiate_generic_signature(&signature, &ordered, display, span)?;
            for (index, (arg, (value, param_type))) in arguments
                .iter()
                .zip(lowered_arguments.iter().zip(param_types.iter()))
                .enumerate()
            {
                let arg_type = self.value_type(*value);
                if !types_compatible(arg_type, *param_type) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "argument to '{display}' has type {arg_type:?}, expected {param_type:?}"
                        ),
                        arg.span,
                    ));
                }
                self.check_reference_argument(
                    display,
                    index,
                    arg_type,
                    Some(*param_type),
                    arg.span,
                )?;
            }
            (param_types, return_type, resolved)
        };

        let result = self.allocate();
        if concurrent {
            let instruction = Instruction::SpawnCall {
                result,
                callee: qualified.to_owned(),
                arguments: lowered_arguments,
                span,
            };
            self.record_value_type(&instruction);
            self.instructions.push(instruction);
        } else {
            let instruction = Instruction::Call {
                result,
                callee: qualified.to_owned(),
                arguments: lowered_arguments,
                type_arguments: resolved_type_arguments,
                return_type,
                span,
            };
            self.record_value_type(&instruction);
            self.instructions.push(instruction);
        }
        Ok(result)
    }

    /// Lower `spawn expr`: plain and module-qualified function calls run on a
    /// worker thread; method calls too (receiver evaluated eagerly); anything
    /// else evaluates now and wraps in a completed task.
    fn lower_spawn(&mut self, inner: &Expression, span: Span) -> Result<ValueId, Diagnostic> {
        if let ExpressionKind::Call {
            callee,
            type_arguments,
            arguments,
        } = &inner.kind
        {
            // Plain function calls spawn concurrently (unknown names and
            // builtins like `print` fall through to eager evaluation, which
            // reports the same diagnostics as a direct call).
            if let ExpressionKind::Name(name) = &callee.kind {
                if let Some(resolved) = self.resolve_function(name) {
                    return self.lower_direct_call(
                        &resolved,
                        name,
                        type_arguments,
                        arguments,
                        callee.span,
                        span,
                        true,
                    );
                }
            }
            // Module-qualified function calls spawn concurrently.
            if let ExpressionKind::Member { .. } = &callee.kind {
                if let Some((module, chain)) = self.split_module_access(callee) {
                    if let [name] = chain.as_slice() {
                        let qualified = crate::project::qualify(&module, name);
                        if self
                            .signatures
                            .iter()
                            .any(|signature| signature.name == qualified)
                        {
                            return self.lower_direct_call(
                                &qualified,
                                name,
                                type_arguments,
                                arguments,
                                callee.span,
                                span,
                                true,
                            );
                        }
                    }
                }
            }
            // Method calls spawn concurrently with an eagerly read receiver.
            if let ExpressionKind::Member {
                target,
                name: method,
                null_safe: false,
            } = &callee.kind
            {
                if !type_arguments.is_empty() {
                    return Err(Diagnostic::error(
                        "NXL2014",
                        format!("method '{method}' does not accept type arguments"),
                        callee.span,
                    ));
                }
                if self.split_module_access(callee).is_none() {
                    let receiver_id = self.expression(target)?;
                    let receiver_type = self.value_type(receiver_id);
                    let mut lowered = Vec::with_capacity(arguments.len());
                    for argument in arguments {
                        lowered.push(self.expression(argument)?);
                    }
                    self.check_dynamic_arguments(
                        method,
                        receiver_type,
                        method,
                        arguments,
                        &lowered,
                        span,
                    )?;
                    let result = self.allocate();
                    let instruction = Instruction::SpawnMethod {
                        result,
                        receiver: receiver_id,
                        method: method.clone(),
                        arguments: lowered,
                        span,
                    };
                    self.record_value_type(&instruction);
                    self.instructions.push(instruction);
                    return Ok(result);
                }
            }
        }
        // Eager fallback: evaluate now, wrap the value in a completed task.
        // (`async {}` blocks take their own path via `lower_async_block`.)
        let value = self.expression(inner)?;
        let result = self.allocate();
        let instruction = Instruction::CompleteTask {
            result,
            value,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        Ok(result)
    }

    /// Lower `async { ... }`: snapshot every visible value binding as a
    /// by-value capture, synthesize a worker function, and spawn it.
    /// Captures keep lexical shadowing working (inner bindings win).
    fn lower_async_block(
        &mut self,
        block: &crate::ast::Block,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        // Snapshot first so lowering the body cannot disturb it.
        // Function markers (u32::MAX) are not values.
        let mut captured: Vec<String> = self
            .values
            .iter()
            .filter(|(_, id)| **id != u32::MAX)
            .map(|(name, _)| name.clone())
            .collect();
        captured.sort();
        captured.dedup();
        let worker = format!("__nxl_async_{}", self.async_counter);
        self.async_counter += 1;
        if self
            .signatures
            .iter()
            .any(|signature| signature.name == worker)
        {
            return Err(Diagnostic::error(
                "NXL2005",
                format!("async worker name '{worker}' is already taken"),
                span,
            ));
        }
        // Lower the captured values in the enclosing scope.
        let mut capture_ids = Vec::with_capacity(captured.len());
        for name in &captured {
            let value = self.lookup_value(name).ok_or_else(|| {
                Diagnostic::error("NXL2002", format!("use of undeclared name '{name}'"), span)
            })?;
            let active = self
                .scope_stack
                .iter()
                .any(|scope| scope.contains_key(name));
            if active {
                self.check_binding_use(name, BindingUse::Read, span)?;
                let state = self.binding_state(name);
                let captures_reference = state.and_then(|state| state.reference).is_some()
                    || state.map(|state| state.boxed).unwrap_or(false);
                if captures_reference {
                    return Err(Diagnostic::error(
                        "NXL2017",
                        format!(
                            "async block captures reference '{name}'; references may not escape the borrow scope"
                        ),
                        span,
                    )
                    .with_help("capture the owned value instead"));
                }
            }
            let result = self.allocate();
            let ty = self.value_type(value);
            let instruction = Instruction::Load {
                result,
                name: name.clone(),
                ty,
                span,
            };
            self.record_value_type(&instruction);
            self.instructions.push(instruction);
            capture_ids.push(result);
        }
        // Register and lower the worker: captures become its parameters.
        self.signatures.push(FunctionSignature {
            name: worker.clone(),
            parameters: captured
                .iter()
                .map(|name| (name.clone(), Type::Unknown))
                .collect(),
            return_type: Type::Unknown,
            type_parameters: Vec::new(),
            parameter_annotations: captured.iter().map(|_| None).collect(),
            return_annotation: None,
        });
        let owned = crate::ast::Function {
            name: Some(worker.clone()),
            type_parameters: Vec::new(),
            constraints: Vec::new(),
            parameters: captured
                .iter()
                .map(|name| crate::ast::Parameter {
                    name: name.clone(),
                    annotation: None,
                    span,
                })
                .collect(),
            return_type: None,
            body: Some(block.clone()),
            is_async: false,
            span: block.span,
        };
        // Lower through the ordinary function path (signature pre-registered).
        self.lower_statement(&Statement::Function(owned))?;
        // Spawn the worker over the frozen captures.
        let result = self.allocate();
        let instruction = Instruction::SpawnCall {
            result,
            callee: worker,
            arguments: capture_ids,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        Ok(result)
    }

    /// Lower `Type.Variant(args)` given the resolved qualified type name.
    fn lower_enum_variant_call(
        &mut self,
        qualified_type: &str,
        display_type: &str,
        variants: &[(String, Vec<String>)],
        variant: &str,
        type_arguments: &[String],
        arguments: &[Expression],
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let type_parameters = find_type_declaration(&self.program, qualified_type)
            .map(|declaration| declaration.type_parameters.clone())
            .unwrap_or_default();
        if !type_arguments.is_empty() {
            if type_parameters.is_empty() {
                return Err(Diagnostic::error(
                    "NXL2014",
                    format!("'{display_type}' is not generic and takes no type arguments"),
                    span,
                ));
            }
            if type_arguments.len() != type_parameters.len() {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!(
                        "generic enum '{display_type}' expects {} type argument(s), found {}",
                        type_parameters.len(),
                        type_arguments.len()
                    ),
                    span,
                ));
            }
        }
        match variants.iter().find(|(candidate, _)| candidate == variant) {
            Some((_, payload)) if payload.len() == arguments.len() => {
                let mut lowered = Vec::with_capacity(arguments.len());
                let mut actual_types = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    let argument_id = self.expression(argument)?;
                    actual_types.push(self.value_type(argument_id));
                    lowered.push(argument_id);
                }
                let (bindings, resolved_arguments) = if type_parameters.is_empty() {
                    (HashMap::new(), Vec::new())
                } else if !type_arguments.is_empty() {
                    let resolved = type_arguments
                        .iter()
                        .map(|argument| self.resolve_type_argument(argument, span))
                        .collect::<Result<Vec<_>, _>>()?;
                    let bindings = type_parameters
                        .iter()
                        .cloned()
                        .zip(type_arguments.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    (bindings, resolved)
                } else {
                    let mut inferred = HashMap::new();
                    for (actual, annotation) in actual_types.iter().zip(payload.iter()) {
                        if self
                            .bind_annotation_to_type(
                                annotation,
                                *actual,
                                &type_parameters,
                                &mut inferred,
                            )
                            .is_err()
                        {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!(
                                    "cannot infer consistent type argument(s) for '{display_type}.{variant}': payload values disagree"
                                ),
                                span,
                            ));
                        }
                    }
                    let written = self.inferred_arguments(
                        display_type,
                        &type_parameters,
                        &inferred,
                        &format!(
                            "write {display_type}.{variant}<{}>(...)",
                            type_parameters.join(", ")
                        ),
                        span,
                    )?;
                    let ordered = type_parameters
                        .iter()
                        .map(|parameter| written[parameter].clone())
                        .collect::<Vec<_>>();
                    let mut resolved = Vec::with_capacity(ordered.len());
                    for argument in &ordered {
                        resolved.push(self.resolve_type_argument(argument, span)?);
                    }
                    let bindings = type_parameters
                        .iter()
                        .cloned()
                        .zip(ordered.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    (bindings, resolved)
                };
                let ty = if type_parameters.is_empty() {
                    Type::Generic(stable_type_id(qualified_type))
                } else {
                    let base = stable_type_id(qualified_type);
                    self.record_instance_arguments(base, &resolved_arguments);
                    Type::Instance(base, argument_fingerprint(&resolved_arguments))
                };
                for ((argument, expected), actual) in arguments
                    .iter()
                    .zip(payload.iter())
                    .zip(actual_types.iter())
                {
                    let expected_type = if bindings.is_empty() {
                        self.resolve_type(expected)
                    } else {
                        let substituted = substitute_type_parameters(expected, &bindings);
                        self.resolve_concrete_annotation(&substituted)
                    };
                    if let Some(expected) = expected_type {
                        if !types_compatible(*actual, expected) {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!(
                                    "variant '{variant}' argument has type {actual:?}, expected {expected:?}"
                                ),
                                argument.span,
                            ));
                        }
                    }
                }
                let result = self.allocate();
                let instruction = Instruction::EnumNew {
                    result,
                    type_name: qualified_type.to_owned(),
                    variant: variant.to_owned(),
                    ty,
                    arguments: lowered,
                    span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                Ok(result)
            }
            Some((_, payload)) => Err(Diagnostic::error(
                "NXL2001",
                format!(
                    "variant '{variant}' takes {} argument(s), found {}",
                    payload.len(),
                    arguments.len()
                ),
                span,
            )),
            None => Err(Diagnostic::error(
                "NXL2002",
                format!("unknown variant '{variant}' on enum '{display_type}'"),
                span,
            )),
        }
    }

    /// Lower `alias.rest...(args)` where alias is an imported module.
    fn lower_module_call(
        &mut self,
        module: &str,
        chain: &[String],
        type_arguments: &[String],
        arguments: &[Expression],
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        match chain {
            [name] => {
                let qualified = crate::project::qualify(module, name);
                if self
                    .signatures
                    .iter()
                    .any(|signature| signature.name == qualified)
                {
                    return self.lower_direct_call(
                        &qualified,
                        name,
                        type_arguments,
                        arguments,
                        span,
                        span,
                        false,
                    );
                }
                if !type_arguments.is_empty() {
                    return Err(Diagnostic::error(
                        "NXL2014",
                        format!("'{name}' in module '{module}' is not generic"),
                        span,
                    ));
                }
                // `math.User(..)` constructs like `User(..)` does.
                if let Some(kind) =
                    find_type_declaration(&self.program, &qualified).map(|decl| decl.kind)
                {
                    if matches!(
                        kind,
                        crate::ast::DeclarationKind::Class | crate::ast::DeclarationKind::Actor
                    ) {
                        return self.lower_construction(&qualified, arguments, span);
                    }
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!("'{name}' is a type in module '{module}', not a function"),
                        span,
                    ));
                }
                // `import std.math;` / `import math;` expose stdlib builtins.
                if let Some(module_name) =
                    crate::stdlib::canonical_module(&crate::project::normalize_path(module))
                {
                    let builtin_name = format!("{module_name}::{name}");
                    if crate::stdlib::builtin(&builtin_name).is_some() {
                        return self.lower_builtin_call(&builtin_name, arguments, span);
                    }
                }
                Err(Diagnostic::error(
                    "NXL2002",
                    format!("unknown function '{name}' in module '{module}'"),
                    span,
                ))
            }
            [type_name, variant] => {
                let qualified_type = crate::project::qualify(module, type_name);
                let variants = find_type_declaration(&self.program, &qualified_type)
                    .filter(|declaration| {
                        matches!(declaration.kind, crate::ast::DeclarationKind::Enum)
                    })
                    .map(|declaration| {
                        declaration
                            .variants
                            .iter()
                            .map(|candidate| (candidate.name.clone(), candidate.payload.clone()))
                            .collect::<Vec<_>>()
                    });
                match variants {
                    Some(variants) => self.lower_enum_variant_call(
                        &qualified_type,
                        type_name,
                        &variants,
                        variant,
                        type_arguments,
                        arguments,
                        span,
                    ),
                    None => Err(Diagnostic::error(
                        "NXL2002",
                        format!("unknown type '{type_name}' in module '{module}'"),
                        span,
                    )),
                }
            }
            _ => Err(Diagnostic::error(
                "NXL2002",
                format!("unsupported module path in module '{module}'"),
                span,
            )),
        }
    }

    /// Check a constructed field value against its declared annotation.
    fn check_field_value(
        &self,
        owner: &str,
        field: &str,
        annotation: &str,
        actual: Type,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if let Some(expected) = self.resolve_type(annotation) {
            if !types_compatible(actual, expected) {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!(
                        "field '{field}' of '{owner}' has type {actual:?}, expected {expected:?}"
                    ),
                    span,
                ));
            }
        }
        Ok(())
    }

    /// If `name` declares a type (and no local shadows it), return whether it
    /// is an enum plus its variant-name/payload-type list.
    fn declared_type_info(&self, name: &str) -> Option<DeclaredTypeInfo> {
        if self.lookup_declaration(name).is_some() {
            return None;
        }
        let qualified = self.resolve_type_alias(name);
        if !self.type_names.contains(&qualified) {
            return None;
        }
        find_type_declaration(&self.program, &qualified).map(|decl| {
            (
                matches!(decl.kind, crate::ast::DeclarationKind::Enum),
                qualified.clone(),
                decl.variants
                    .iter()
                    .map(|variant| (variant.name.clone(), variant.payload.clone()))
                    .collect(),
            )
        })
    }

    fn expression(&mut self, expression: &Expression) -> Result<ValueId, Diagnostic> {
        match &expression.kind {
            ExpressionKind::Integer(value) => {
                Ok(self.constant(Constant::Integer(*value), Type::Int, expression.span))
            }
            ExpressionKind::Float(value) => {
                Ok(self.constant(Constant::Float(*value), Type::Float, expression.span))
            }
            ExpressionKind::String(value) => Ok(self.constant(
                Constant::String(value.clone()),
                Type::String,
                expression.span,
            )),
            ExpressionKind::InterpolatedString(parts) => {
                // Desugar to chained string concatenation, converting each
                // embedded value with ToString. Starts from "" so an empty
                // template still yields a string.
                let mut acc = self.constant(
                    Constant::String(String::new()),
                    Type::String,
                    expression.span,
                );
                for part in parts {
                    let part_id = match part {
                        StringPart::Text(text) => self.constant(
                            Constant::String(text.clone()),
                            Type::String,
                            expression.span,
                        ),
                        StringPart::Expression(inner) => {
                            let value = self.expression(inner)?;
                            let value_type = self.value_type(value);
                            if value_type == Type::Void {
                                return Err(Diagnostic::error(
                                    "NXL2006",
                                    "cannot interpolate a void value",
                                    inner.span,
                                ));
                            }
                            if value_type == Type::String {
                                value
                            } else {
                                let result = self.allocate();
                                let instruction = Instruction::ToString {
                                    result,
                                    value,
                                    span: inner.span,
                                };
                                self.record_value_type(&instruction);
                                self.instructions.push(instruction);
                                result
                            }
                        }
                    };
                    let result = self.allocate();
                    let instruction = Instruction::Binary {
                        result,
                        operator: BinaryOperator::Add,
                        left: acc,
                        right: part_id,
                        ty: Type::String,
                        span: expression.span,
                    };
                    self.record_value_type(&instruction);
                    self.instructions.push(instruction);
                    acc = result;
                }
                Ok(acc)
            }
            ExpressionKind::Boolean(value) => {
                Ok(self.constant(Constant::Boolean(*value), Type::Bool, expression.span))
            }
            ExpressionKind::Name(name) => {
                let value = match self.lookup_value(name) {
                    Some(value) => value,
                    None => {
                        // Bare nullary enum variants read as values
                        // (mirrors match-pattern resolution).
                        let variants = find_enum_variant(&self.program, name);
                        if variants.len() == 1 && variants[0].1.is_empty() {
                            let result = self.allocate();
                            let instruction = Instruction::EnumNew {
                                result,
                                type_name: variants[0].0.clone(),
                                variant: name.clone(),
                                ty: Type::Generic(stable_type_id(&variants[0].0)),
                                arguments: Vec::new(),
                                span: expression.span,
                            };
                            self.record_value_type(&instruction);
                            self.instructions.push(instruction);
                            return Ok(result);
                        }
                        return Err(Diagnostic::error(
                            "NXL2002",
                            format!("use of undeclared name '{name}'"),
                            expression.span,
                        ));
                    }
                };
                let logical_type = self.binding_type(name);
                self.check_binding_use(name, BindingUse::Read, expression.span)?;
                let result = self.allocate();
                if self
                    .binding_state(name)
                    .map(|state| state.boxed)
                    .unwrap_or(false)
                {
                    // The name slot stores the cell; read through it.
                    let reference_ty = Type::Reference(self.record_identity(logical_type), true);
                    let load = Instruction::Load {
                        result,
                        name: name.clone(),
                        ty: reference_ty,
                        span: expression.span,
                    };
                    self.record_value_type(&load);
                    self.instructions.push(load);
                    return self.lower_reference_read(result, expression.span);
                }
                let ty = if self
                    .binding_state(name)
                    .and_then(|state| state.reference)
                    .is_some()
                {
                    logical_type
                } else {
                    self.value_type(value)
                };
                // A load is a fresh value id; carry any per-value field types
                // recorded at construction so `let boxed = Box { .. }` keeps
                // precise field types.
                let field_types = self.value_field_types.get(&value).cloned();
                let instruction = Instruction::Load {
                    result,
                    name: name.clone(),
                    ty,
                    span: expression.span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                if let Some(field_types) = field_types {
                    self.value_field_types.insert(result, field_types);
                }
                Ok(result)
            }
            ExpressionKind::Unary { operator, operand } => {
                match operator {
                    UnaryOperator::Borrow | UnaryOperator::Reference | UnaryOperator::BorrowMut => {
                        let mutable = *operator == UnaryOperator::BorrowMut;
                        return self.lower_borrow(operand, mutable, expression.span);
                    }
                    UnaryOperator::Deref => {
                        let reference = self.expression(operand)?;
                        return self.lower_reference_read(reference, expression.span);
                    }
                    UnaryOperator::Move => {
                        return self.lower_move(operand, expression.span);
                    }
                    _ => {}
                }
                let operand = self.expression(operand)?;
                let operand_type = erased_for_check(self.value_type(operand));
                let result = self.allocate();
                let ty = match operator {
                    UnaryOperator::Negate
                        if matches!(operand_type, Type::Int | Type::Float | Type::Unknown) =>
                    {
                        operand_type
                    }
                    UnaryOperator::Not if matches!(operand_type, Type::Bool | Type::Unknown) => {
                        Type::Bool
                    }
                    _ => {
                        return Err(Diagnostic::error(
                            "NXL2001",
                            format!("operator is not valid for operand type {operand_type:?}"),
                            expression.span,
                        ))
                    }
                };
                let instruction = Instruction::Unary {
                    result,
                    operator: *operator,
                    operand,
                    ty,
                    span: expression.span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                Ok(result)
            }
            ExpressionKind::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.expression(left)?;
                let right = self.expression(right)?;
                let left_type = erased_for_check(self.value_type(left));
                let right_type = erased_for_check(self.value_type(right));
                if left_type != right_type
                    && left_type != Type::Unknown
                    && right_type != Type::Unknown
                {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "operator operands have incompatible types: {left_type:?} and {right_type:?}"
                        ),
                        expression.span,
                    ));
                }
                let numeric = matches!(
                    (&left_type, &right_type),
                    (Type::Int, Type::Int)
                        | (Type::Float, Type::Float)
                        | (Type::Unknown, Type::Unknown)
                        | (Type::Unknown, Type::Int)
                        | (Type::Int, Type::Unknown)
                        | (Type::Unknown, Type::Float)
                        | (Type::Float, Type::Unknown)
                );
                let ty = match operator {
                    BinaryOperator::Add
                    | BinaryOperator::Subtract
                    | BinaryOperator::Multiply
                    | BinaryOperator::Divide
                    | BinaryOperator::Remainder
                        if numeric =>
                    {
                        // Dynamically-typed operands check at runtime; the
                        // static result stays Unknown unless both sides agree.
                        if left_type == Type::Unknown || right_type == Type::Unknown
                        {
                            Type::Unknown
                        } else {
                            left_type
                        }
                    }
                    // String concatenation composes in both runtimes. An operand whose type
                    // is not known statically is accepted: array elements and
                    // values read out of a dynamic structure are `Unknown`,
                    // and rejecting `"prefix" + element` here would have been
                    // the only operator that refused them - comparisons,
                    // calls and interpolation all accept them. The result
                    // stays `Unknown` so nothing downstream is misled.
                    BinaryOperator::Add
                        if (left_type == Type::String
                            && matches!(right_type, Type::String | Type::Unknown))
                            || (matches!(left_type, Type::String | Type::Unknown)
                                && right_type == Type::String)
                            || (left_type == Type::Unknown || right_type == Type::Unknown) =>
                    {
                        if left_type == Type::Unknown || right_type == Type::Unknown {
                            Type::Unknown
                        } else {
                            Type::String
                        }
                    }
                    BinaryOperator::Equal | BinaryOperator::NotEqual
                        if left_type != Type::Void && right_type != Type::Void =>
                    {
                        Type::Bool
                    }
                    BinaryOperator::Less
                    | BinaryOperator::LessEqual
                    | BinaryOperator::Greater
                    | BinaryOperator::GreaterEqual
                        if matches!(
                            left_type,
                            Type::Int | Type::Float | Type::String | Type::Unknown
                        ) =>
                    {
                        Type::Bool
                    }
                    BinaryOperator::And | BinaryOperator::Or
                        if matches!(left_type, Type::Bool | Type::Unknown)
                            && matches!(right_type, Type::Bool | Type::Unknown) =>
                    {
                        Type::Bool
                    }
                    _ => {
                        return Err(Diagnostic::error(
                            "NXL2001",
                            format!(
                                "operator is not valid for operand types {left_type:?} and {right_type:?}"
                            ),
                            expression.span,
                        ))
                    }
                };
                let result = self.allocate();
                let instruction = Instruction::Binary {
                    result,
                    operator: *operator,
                    left,
                    right,
                    ty,
                    span: expression.span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                Ok(result)
            }
            ExpressionKind::Array(elements) => {
                let mut lowered = Vec::with_capacity(elements.len());
                for element in elements {
                    lowered.push(self.expression(element)?);
                }
                // Uniform known element types give a precise array type;
                // empty or fully dynamic literals stay Array(0), which
                // unifies with any array type. Mixed known types are rejected.
                let mut element_type: Option<Type> = None;
                for element in &lowered {
                    let ty = self.value_type(*element);
                    if ty == Type::Void {
                        return Err(Diagnostic::error(
                            "NXL2006",
                            "cannot store a void value in an array",
                            expression.span,
                        ));
                    }
                    if ty == Type::Unknown {
                        continue;
                    }
                    // A reference inside an aggregate would outlive the
                    // lexical borrow scope. Array annotations already reject
                    // nested references; reject the literal too.
                    if self.type_contains_reference(ty, 0) {
                        return Err(Diagnostic::error(
                            "NXL2017",
                            "an array cannot hold a reference: references may not escape the borrow scope",
                            expression.span,
                        ));
                    }
                    match element_type {
                        None => element_type = Some(ty),
                        Some(known) if known == ty => {}
                        Some(_) => {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                "array literal has mixed element types",
                                expression.span,
                            ));
                        }
                    }
                }
                let array_type = match element_type {
                    Some(ty) => {
                        self.identity_types.insert(type_identity(ty), ty);
                        self.identity_types.insert(array_identity(ty), ty);
                        Type::Array(array_identity(ty))
                    }
                    None => Type::Array(0),
                };
                let result = self.allocate();
                let instruction = Instruction::ArrayNew {
                    result,
                    elements: lowered,
                    span: expression.span,
                };
                self.record_value_type(&instruction);
                // Override the generic Array(0) recording with the inferred type.
                self.value_types.insert(result, array_type);
                self.instructions.push(instruction);
                Ok(result)
            }
            ExpressionKind::Index { target, index } => {
                let object_id = self.expression(target)?;
                let index_id = self.expression(index)?;
                let index_type = self.value_type(index_id);
                if index_type != Type::Int && !is_dynamic(index_type) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!("array index has type {index_type:?}, expected Int"),
                        index.span,
                    ));
                }
                let ty = self.inferred_element_type(object_id);
                Ok(self.lower_index_get(object_id, index_id, ty, expression.span))
            }
            ExpressionKind::Member {
                target,
                name,
                null_safe,
            } => {
                if *null_safe {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "safe navigation (?.) is not supported yet",
                        expression.span,
                    ));
                }
                // Module namespace: `math.Color.Red` (nullary variants only;
                // payloads need the call form).
                if let Some((module, chain)) = self.split_module_access(expression) {
                    match chain.as_slice() {
                        [type_name, variant] => {
                            let qualified_type = crate::project::qualify(&module, type_name);
                            let variants = find_type_declaration(&self.program, &qualified_type)
                                .filter(|declaration| {
                                    matches!(declaration.kind, crate::ast::DeclarationKind::Enum)
                                })
                                .map(|declaration| {
                                    declaration
                                        .variants
                                        .iter()
                                        .map(|candidate| {
                                            (candidate.name.clone(), candidate.payload.clone())
                                        })
                                        .collect::<Vec<_>>()
                                });
                            match variants {
                                Some(variants) => {
                                    return self.lower_enum_variant_call(
                                        &qualified_type,
                                        type_name,
                                        &variants,
                                        variant,
                                        &[],
                                        &[],
                                        expression.span,
                                    );
                                }
                                None => {
                                    return Err(Diagnostic::error(
                                        "NXL2002",
                                        format!("unknown type '{type_name}' in module '{module}'"),
                                        expression.span,
                                    ));
                                }
                            }
                        }
                        [name] => {
                            return Err(Diagnostic::error(
                                "NXL2002",
                                format!(
                                    "'{name}' in module '{module}' is a function or type, not a value (module globals need the bare name)"
                                ),
                                expression.span,
                            ));
                        }
                        _ => {
                            return Err(Diagnostic::error(
                                "NXL2002",
                                format!("unsupported module path in module '{module}'"),
                                expression.span,
                            ));
                        }
                    }
                }
                // Type namespace (locals shadow types): `Color.Red`.
                if let ExpressionKind::Name(type_name) = &target.kind {
                    if let Some((is_enum, qualified, variants)) = self.declared_type_info(type_name)
                    {
                        if !is_enum {
                            return Err(Diagnostic::error(
                                "NXL2002",
                                format!(
                                    "cannot access instance field '{name}' on type '{type_name}'"
                                ),
                                expression.span,
                            ));
                        }
                        match variants.iter().find(|(variant, _)| variant == name) {
                            Some((_, payload)) if payload.is_empty() => {
                                let result = self.allocate();
                                let instruction = Instruction::EnumNew {
                                    result,
                                    type_name: qualified.clone(),
                                    variant: name.clone(),
                                    ty: Type::Generic(stable_type_id(&qualified)),
                                    arguments: Vec::new(),
                                    span: expression.span,
                                };
                                self.record_value_type(&instruction);
                                self.instructions.push(instruction);
                                return Ok(result);
                            }
                            Some((_, payload)) => {
                                return Err(Diagnostic::error(
                                    "NXL2001",
                                    format!(
                                        "variant '{name}' takes {} argument(s): use {type_name}.{name}(...)",
                                        payload.len()
                                    ),
                                    expression.span,
                                ));
                            }
                            None => {
                                return Err(Diagnostic::error(
                                    "NXL2002",
                                    format!("unknown variant '{name}' on enum '{type_name}'"),
                                    expression.span,
                                ));
                            }
                        }
                    }
                }
                let object_id = self.expression(target)?;
                let ty = self.inferred_field_type(object_id, name);
                Ok(self.lower_field_get(object_id, name, ty, expression.span))
            }
            ExpressionKind::Object { type_name, fields } => {
                let (written_base, explicit_arguments) = match type_name.find('<') {
                    Some(open) if type_name.ends_with('>') => (
                        &type_name[..open],
                        Some(split_type_arguments(
                            &type_name[open + 1..type_name.len() - 1],
                        )),
                    ),
                    _ => (type_name.as_str(), None),
                };
                let qualified = self.resolve_type_alias(written_base);
                let declaration = find_type_declaration(&self.program, &qualified)
                    .cloned()
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "NXL2004",
                            format!("unknown type '{written_base}'"),
                            expression.span,
                        )
                    })?;
                if !matches!(declaration.kind, crate::ast::DeclarationKind::Struct) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "cannot construct {:?} '{written_base}' with an object literal",
                            declaration.kind
                        ),
                        expression.span,
                    ));
                }
                let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
                for (field, _) in fields {
                    if !seen.insert(field.as_str()) {
                        return Err(Diagnostic::error(
                            "NXL2005",
                            format!("duplicate field '{field}' in '{written_base}' literal"),
                            expression.span,
                        ));
                    }
                }
                // Lower the provided values once, so generic parameters can be
                // inferred from their types before the fields are checked.
                let mut provided = Vec::with_capacity(fields.len());
                for (field, value) in fields {
                    let Some(declared) = declaration.fields.iter().find(|f| &f.name == field)
                    else {
                        return Err(Diagnostic::error(
                            "NXL2002",
                            format!("unknown field '{field}' on struct '{written_base}'"),
                            expression.span,
                        ));
                    };
                    let value_id = self.expression(value)?;
                    let actual = self.value_type(value_id);
                    provided.push((
                        field.clone(),
                        declared.annotation.clone(),
                        value_id,
                        actual,
                        value.span,
                    ));
                }
                let is_generic = !declaration.type_parameters.is_empty();
                let (_type_bindings, _type_arguments_display, instance_type, field_types) =
                    if !is_generic {
                        if explicit_arguments.is_some() {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!("type '{written_base}' is not generic"),
                                expression.span,
                            ));
                        }
                        let mut field_types = HashMap::new();
                        for (field, annotation, _, actual, value_span) in &provided {
                            if let Some(expected) = self.resolve_type(annotation) {
                                field_types.insert(field.clone(), expected);
                            }
                            self.check_field_value(
                                written_base,
                                field,
                                annotation,
                                *actual,
                                *value_span,
                            )?;
                        }
                        (
                            None,
                            String::new(),
                            Type::Generic(stable_type_id(&qualified)),
                            field_types,
                        )
                    } else {
                        let (argument_strings, resolved_arguments) = match &explicit_arguments {
                            Some(arguments) => {
                                if arguments.len() != declaration.type_parameters.len() {
                                    return Err(Diagnostic::error(
                                        "NXL2001",
                                        format!(
                                            "type '{written_base}' expects {} type argument(s), found {}",
                                            declaration.type_parameters.len(),
                                            arguments.len()
                                        ),
                                        expression.span,
                                    ));
                                }
                                let mut resolved = Vec::with_capacity(arguments.len());
                                for argument in arguments {
                                    resolved.push(
                                        self.resolve_type_argument(argument, expression.span)?,
                                    );
                                }
                                (
                                    arguments
                                        .iter()
                                        .map(|argument| (*argument).to_owned())
                                        .collect::<Vec<_>>(),
                                    resolved,
                                )
                            }
                            None => {
                                let mut inferred = HashMap::new();
                                for (_, annotation, _, actual, _) in &provided {
                                    if self
                                        .bind_annotation_to_type(
                                            annotation,
                                            *actual,
                                            &declaration.type_parameters,
                                            &mut inferred,
                                        )
                                        .is_err()
                                    {
                                        return Err(Diagnostic::error(
                                            "NXL2001",
                                            format!(
                                                "cannot infer consistent type argument(s) for '{written_base}': field values disagree"
                                            ),
                                            expression.span,
                                        ));
                                    }
                                }
                                let written = self.inferred_arguments(
                                    written_base,
                                    &declaration.type_parameters,
                                    &inferred,
                                    &format!(
                                        "write {written_base}<{}>",
                                        declaration.type_parameters.join(", ")
                                    ),
                                    expression.span,
                                )?;
                                let ordered = declaration
                                    .type_parameters
                                    .iter()
                                    .map(|parameter| written[parameter].clone())
                                    .collect::<Vec<_>>();
                                let mut resolved = Vec::with_capacity(ordered.len());
                                for argument in &ordered {
                                    resolved.push(
                                        self.resolve_type_argument(argument, expression.span)?,
                                    );
                                }
                                (ordered, resolved)
                            }
                        };
                        let bindings = declaration
                            .type_parameters
                            .iter()
                            .cloned()
                            .zip(argument_strings.iter().cloned())
                            .collect::<HashMap<_, _>>();
                        let base = stable_type_id(&qualified);
                        self.record_instance_arguments(base, &resolved_arguments);
                        let instance =
                            Type::Instance(base, argument_fingerprint(&resolved_arguments));
                        let mut field_types = HashMap::new();
                        for (field, annotation, _, actual, value_span) in &provided {
                            let substituted = substitute_type_parameters(annotation, &bindings);
                            let expected = self.resolve_type_argument(&substituted, *value_span)?;
                            field_types.insert(field.clone(), expected);
                            if !types_compatible(*actual, expected) {
                                return Err(Diagnostic::error(
                                    "NXL2001",
                                    format!(
                                        "field '{field}' of '{written_base}<{}>' has type {actual:?}, expected {expected:?}",
                                        argument_strings.join(", ")
                                    ),
                                    *value_span,
                                ));
                            }
                        }
                        (
                            Some(bindings),
                            argument_strings.join(", "),
                            instance,
                            field_types,
                        )
                    };
                let lowered = provided
                    .iter()
                    .map(|(field, _, value, _, _)| (field.clone(), *value))
                    .collect::<Vec<_>>();
                let missing: Vec<&str> = declaration
                    .fields
                    .iter()
                    .map(|f| f.name.as_str())
                    .filter(|name| !seen.contains(name))
                    .collect();
                if !missing.is_empty() {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "struct '{written_base}' literal is missing field(s): {}",
                            missing.join(", ")
                        ),
                        expression.span,
                    ));
                }
                let result = self.allocate();
                let instruction = Instruction::StructNew {
                    result,
                    type_name: qualified.clone(),
                    ty: instance_type,
                    fields: lowered,
                    span: expression.span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                self.value_field_types.insert(result, field_types);
                Ok(result)
            }
            ExpressionKind::New { .. } => Err(Diagnostic::error(
                "NXL2007",
                "class instantiation with `new` is not supported yet",
                expression.span,
            )),
            ExpressionKind::Call {
                callee,
                type_arguments,
                arguments,
            } => {
                // Enum construction: `Choice.Some(1)`.
                if let ExpressionKind::Member {
                    target,
                    name: variant,
                    null_safe,
                } = &callee.kind
                {
                    if *null_safe {
                        return Err(Diagnostic::error(
                            "NXL2007",
                            "safe navigation (?.) is not supported yet",
                            callee.span,
                        ));
                    }
                    if let ExpressionKind::Name(type_name) = &target.kind {
                        if let Some((is_enum, qualified, variants)) =
                            self.declared_type_info(type_name)
                        {
                            if !is_enum {
                                return Err(Diagnostic::error(
                                    "NXL2002",
                                    format!(
                                        "'{type_name}.{variant}' is not a known enum variant or function"
                                    ),
                                    callee.span,
                                ));
                            }
                            return self.lower_enum_variant_call(
                                &qualified,
                                type_name,
                                &variants,
                                variant,
                                type_arguments,
                                arguments,
                                expression.span,
                            );
                        }
                    }
                }
                // Module-qualified call: `math.add(..)`, `math.Color.Red(..)`.
                if let ExpressionKind::Member { .. } = &callee.kind {
                    if let Some((module, chain)) = self.split_module_access(callee) {
                        return self.lower_module_call(
                            &module,
                            &chain,
                            type_arguments,
                            arguments,
                            expression.span,
                        );
                    }
                }
                // Method call: `user.greet("hi")` (dynamic dispatch on the
                // receiver's runtime type).
                if let ExpressionKind::Member {
                    target,
                    name: method,
                    null_safe,
                } = &callee.kind
                {
                    if *null_safe {
                        return Err(Diagnostic::error(
                            "NXL2007",
                            "safe navigation (?.) is not supported yet",
                            callee.span,
                        ));
                    }
                    if !type_arguments.is_empty() {
                        return Err(Diagnostic::error(
                            "NXL2014",
                            format!("method '{method}' does not accept type arguments"),
                            callee.span,
                        ));
                    }
                    let receiver_id = self.expression(target)?;
                    let receiver_type = self.value_type(receiver_id);
                    let mut lowered = Vec::with_capacity(arguments.len());
                    for argument in arguments {
                        lowered.push(self.expression(argument)?);
                    }
                    self.check_dynamic_arguments(
                        method,
                        receiver_type,
                        method,
                        arguments,
                        &lowered,
                        expression.span,
                    )?;
                    let result = self.allocate();
                    let receiver_out = self.allocate();
                    let instruction = Instruction::MethodCall {
                        result,
                        receiver_out,
                        receiver: receiver_id,
                        method: method.clone(),
                        arguments: lowered,
                        span: expression.span,
                    };
                    self.record_value_type(&instruction);
                    self.instructions.push(instruction);
                    // A mutated receiver flows back only when the receiver
                    // is a plain variable: for deeper paths (`a.b.m()`) the
                    // intermediate is a different object, and storing it
                    // into the root would corrupt the root binding. (Nested
                    // mutation needs path write-back: a later phase.)
                    if let ExpressionKind::Name(root) = &target.kind {
                        let is_reference = self
                            .binding_state(root)
                            .and_then(|state| state.reference)
                            .is_some();
                        if !is_reference {
                            self.check_binding_use(root, BindingUse::Write, expression.span)?;
                            if self
                                .binding_state(root)
                                .map(|state| state.boxed)
                                .unwrap_or(false)
                            {
                                let cell =
                                    self.lookup_value(root).expect("receiver binding exists");
                                self.instructions.push(Instruction::StoreMemory {
                                    pointer: cell,
                                    value: receiver_out,
                                    span: expression.span,
                                });
                            } else {
                                self.instructions.push(Instruction::Assign {
                                    name: root.clone(),
                                    value: receiver_out,
                                    span: expression.span,
                                });
                            }
                        }
                    }
                    return Ok(result);
                }
                // Channel construction: `channel()` (element types are
                // informational in this phase; capacity arguments reserved).
                if matches!(&callee.kind, ExpressionKind::Channel { .. }) {
                    if !type_arguments.is_empty() {
                        return Err(Diagnostic::error(
                            "NXL2014",
                            "channel() does not accept type arguments in this phase",
                            callee.span,
                        ));
                    }
                    if !arguments.is_empty() {
                        return Err(Diagnostic::error(
                            "NXL2001",
                            "channel() takes no arguments in this phase",
                            expression.span,
                        ));
                    }
                    let result = self.allocate();
                    let instruction = Instruction::ChannelNew {
                        result,
                        span: expression.span,
                    };
                    self.record_value_type(&instruction);
                    self.instructions.push(instruction);
                    return Ok(result);
                }
                // A callee that is not a bare name is a value-producing
                // expression (closure literal, call result, index, field).
                // Lower it, then call through the resulting value.
                let ExpressionKind::Name(name) = &callee.kind else {
                    if !type_arguments.is_empty() {
                        return Err(Diagnostic::error(
                            "NXL2014",
                            "type arguments require a named generic function",
                            callee.span,
                        ));
                    }
                    let callee_id = self.expression(callee)?;
                    let mut lowered_arguments = Vec::with_capacity(arguments.len());
                    for arg in arguments {
                        lowered_arguments.push(self.expression(arg)?);
                    }
                    self.check_indirect_arguments("indirect call", callee_id, arguments, &lowered_arguments)?;
                    let result = self.allocate();
                    let instruction = Instruction::CallIndirect {
                        result,
                        callee: callee_id,
                        arguments: lowered_arguments,
                        return_type: Type::Unknown,
                        span: expression.span,
                    };
                    self.record_value_type(&instruction);
                    self.instructions.push(instruction);
                    return Ok(result);
                };

                // Check for built-in functions the runtime provides directly.
                if builtin_arity(name).is_some() {
                    if !type_arguments.is_empty() {
                        return Err(Diagnostic::error(
                            "NXL2014",
                            format!("builtin function '{name}' is not generic"),
                            callee.span,
                        ));
                    }
                    return self.lower_builtin_call(name, arguments, expression.span);
                }

                // Class/actor construction: `Person("Ada")`.
                if self.lookup_declaration(name).is_none() {
                    let kind = find_type_declaration(&self.program, name)
                        .map(|declaration| declaration.kind);
                    if matches!(
                        kind,
                        Some(
                            crate::ast::DeclarationKind::Class | crate::ast::DeclarationKind::Actor
                        )
                    ) {
                        if !type_arguments.is_empty() {
                            return Err(Diagnostic::error(
                                "NXL2014",
                                format!(
                                    "class or actor '{name}' does not accept type arguments yet"
                                ),
                                callee.span,
                            ));
                        }
                        return self.lower_construction(name, arguments, expression.span);
                    }
                }

                // Class/actor construction: `Person("Ada")`.
                if self.lookup_declaration(name).is_none() {
                    let qualified = self.resolve_type_alias(name);
                    if let Some(kind) =
                        find_type_declaration(&self.program, &qualified).map(|decl| decl.kind)
                    {
                        if matches!(
                            kind,
                            crate::ast::DeclarationKind::Class | crate::ast::DeclarationKind::Actor
                        ) {
                            if !type_arguments.is_empty() {
                                return Err(Diagnostic::error(
                                    "NXL2014",
                                    format!(
                                        "class or actor '{name}' does not accept type arguments yet"
                                    ),
                                    callee.span,
                                ));
                            }
                            return self.lower_construction(&qualified, arguments, expression.span);
                        }
                    }
                }

                // Resolve bare names through selected imports + own module.
                let resolved = match self.resolve_function(name) {
                    Some(resolved) => resolved,
                    None => {
                        if let Some(value) = self.lookup_value(name) {
                            if !type_arguments.is_empty() {
                                return Err(Diagnostic::error(
                                    "NXL2014",
                                    format!(
                                        "'{name}' is a function value, not a named generic function"
                                    ),
                                    callee.span,
                                ));
                            }
                            let mut lowered_arguments = Vec::with_capacity(arguments.len());
                            for arg in arguments {
                                lowered_arguments.push(self.expression(arg)?);
                            }
                            self.check_indirect_arguments(
                                name,
                                value,
                                arguments,
                                &lowered_arguments,
                            )?;
                            let result = self.allocate();
                            let instruction = Instruction::CallIndirect {
                                result,
                                callee: value,
                                arguments: lowered_arguments,
                                return_type: Type::Unknown,
                                span: expression.span,
                            };
                            self.record_value_type(&instruction);
                            self.instructions.push(instruction);
                            return Ok(result);
                        }
                        // `from std.<module> import <name>` reaches a stdlib
                        // builtin when no user function claims the name.
                        if let Some(qualified) = self.resolve_stdlib_builtin(name) {
                            if !type_arguments.is_empty() {
                                return Err(Diagnostic::error(
                                    "NXL2014",
                                    format!("stdlib builtin '{name}' is not generic"),
                                    callee.span,
                                ));
                            }
                            return self.lower_builtin_call(&qualified, arguments, expression.span);
                        }
                        // A registered `extern` (C ABI) declaration answers a
                        // bare call when no NXL function owns the name.
                        if self.externs.contains_key(name) {
                            return self.lower_direct_call(
                                name,
                                name,
                                type_arguments,
                                arguments,
                                callee.span,
                                expression.span,
                                false,
                            );
                        }
                        return Err(Diagnostic::error(
                            "NXL2002",
                            format!("use of undeclared function '{name}'"),
                            callee.span,
                        ));
                    }
                };

                self.lower_direct_call(
                    &resolved,
                    name,
                    type_arguments,
                    arguments,
                    callee.span,
                    expression.span,
                    false,
                )
            }
            ExpressionKind::Spawn(inner) => self.lower_spawn(inner, expression.span),
            ExpressionKind::AsyncBlock(block) => self.lower_async_block(block, expression.span),
            ExpressionKind::Await(inner) => {
                let task_id = self.expression(inner)?;
                let result = self.allocate();
                let instruction = Instruction::AwaitTask {
                    result,
                    task: task_id,
                    span: expression.span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                Ok(result)
            }
            ExpressionKind::Channel { .. } => {
                let result = self.allocate();
                let instruction = Instruction::ChannelNew {
                    result,
                    span: expression.span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                Ok(result)
            }
            ExpressionKind::Assignment {
                target,
                operator,
                value,
            } => {
                // Field and element targets (`u.name = v`, `a[i] = v`).
                match &target.kind {
                    ExpressionKind::Member { .. } | ExpressionKind::Index { .. } => {
                        return self.assign_to_place(target, *operator, value, expression.span);
                    }
                    // `*r = v` writes through a mutable reference.
                    ExpressionKind::Unary {
                        operator: UnaryOperator::Deref,
                        operand,
                    } => {
                        return self.assign_through_reference(
                            operand,
                            *operator,
                            value,
                            expression.span,
                        );
                    }
                    _ => {}
                }
                let ExpressionKind::Name(name) = &target.kind else {
                    return Err(Diagnostic::error(
                        "NXL2008",
                        "assignment target must be a variable, field, or index",
                        target.span,
                    ));
                };
                let binding = self.lookup_value(name);
                let Some(_value_id) = binding else {
                    return Err(Diagnostic::error(
                        "NXL2002",
                        format!("use of undeclared name '{name}'"),
                        target.span,
                    ));
                };
                if self
                    .binding_state(name)
                    .and_then(|state| state.reference)
                    .is_some()
                {
                    return Err(Diagnostic::error(
                        "NXL2015",
                        format!("cannot rebind reference '{name}'"),
                        target.span,
                    )
                    .with_help("write through the reference with '*name = value'"));
                }
                let is_mutable = self.mutability.get(name).copied().unwrap_or(false);
                if !is_mutable {
                    return Err(Diagnostic::error(
                        "NXL2008",
                        format!("cannot assign to immutable binding '{name}'"),
                        target.span,
                    ));
                }
                self.check_borrow_conflict(name, BindingUse::Write, target.span)?;

                let assigned_value = match operator {
                    crate::ast::AssignmentOperator::Assign => self.expression(value)?,
                    crate::ast::AssignmentOperator::Add => self.expression(&Expression {
                        kind: ExpressionKind::Binary {
                            left: Box::new((**target).clone()),
                            operator: BinaryOperator::Add,
                            right: Box::new(value.as_ref().clone()),
                        },
                        span: expression.span,
                    })?,
                    crate::ast::AssignmentOperator::Subtract => self.expression(&Expression {
                        kind: ExpressionKind::Binary {
                            left: Box::new((**target).clone()),
                            operator: BinaryOperator::Subtract,
                            right: Box::new(value.as_ref().clone()),
                        },
                        span: expression.span,
                    })?,
                    crate::ast::AssignmentOperator::Multiply => self.expression(&Expression {
                        kind: ExpressionKind::Binary {
                            left: Box::new((**target).clone()),
                            operator: BinaryOperator::Multiply,
                            right: Box::new(value.as_ref().clone()),
                        },
                        span: expression.span,
                    })?,
                    crate::ast::AssignmentOperator::Divide => self.expression(&Expression {
                        kind: ExpressionKind::Binary {
                            left: Box::new((**target).clone()),
                            operator: BinaryOperator::Divide,
                            right: Box::new(value.as_ref().clone()),
                        },
                        span: expression.span,
                    })?,
                    crate::ast::AssignmentOperator::Remainder => self.expression(&Expression {
                        kind: ExpressionKind::Binary {
                            left: Box::new((**target).clone()),
                            operator: BinaryOperator::Remainder,
                            right: Box::new(value.as_ref().clone()),
                        },
                        span: expression.span,
                    })?,
                    crate::ast::AssignmentOperator::And => self.expression(&Expression {
                        kind: ExpressionKind::Binary {
                            left: Box::new((**target).clone()),
                            operator: BinaryOperator::And,
                            right: Box::new(value.as_ref().clone()),
                        },
                        span: expression.span,
                    })?,
                    crate::ast::AssignmentOperator::Or => self.expression(&Expression {
                        kind: ExpressionKind::Binary {
                            left: Box::new((**target).clone()),
                            operator: BinaryOperator::Or,
                            right: Box::new(value.as_ref().clone()),
                        },
                        span: expression.span,
                    })?,
                };

                if matches!(self.value_type(assigned_value), Type::Reference(..)) {
                    return Err(Diagnostic::error(
                        "NXL2017",
                        "a reference cannot be stored into an existing binding",
                        expression.span,
                    )
                    .with_help("let the reference be initialised in its own 'let' declaration"));
                }
                if self
                    .binding_state(name)
                    .map(|state| state.boxed)
                    .unwrap_or(false)
                {
                    // The binding is borrowed: keep the cell identity so
                    // outstanding references see the new value, and store
                    // through it instead of replacing the slot.
                    let cell = self
                        .lookup_value(name)
                        .expect("binding exists, checked above");
                    self.instructions.push(Instruction::StoreMemory {
                        pointer: cell,
                        value: assigned_value,
                        span: expression.span,
                    });
                } else {
                    self.values.insert(name.clone(), assigned_value);
                    let assign_instruction = Instruction::Assign {
                        name: name.clone(),
                        value: assigned_value,
                        span: expression.span,
                    };
                    self.instructions.push(assign_instruction);
                }
                if let Some(state) = self.binding_state_mut(name) {
                    // Reassignment restores a moved binding.
                    state.moved = false;
                }
                Ok(assigned_value)
            }
            ExpressionKind::Function(function) => {
                if function.name.is_some() {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "named function expressions are not yet supported",
                        function.span,
                    ));
                }
                let closure_name = format!("__nxl_closure_{}", self.closure_counter);
                self.closure_counter += 1;

                let mut captured: Vec<String> = self
                    .values
                    .iter()
                    .filter(|(_, id)| **id != u32::MAX)
                    .map(|(name, _)| name.clone())
                    .collect();
                captured.sort();
                captured.dedup();

                let mut capture_ids = Vec::with_capacity(captured.len());
                for name in &captured {
                    let value = self.lookup_value(name).ok_or_else(|| {
                        Diagnostic::error(
                            "NXL2002",
                            format!("use of undeclared name '{name}'"),
                            function.span,
                        )
                    })?;
                    let active = self
                        .scope_stack
                        .iter()
                        .any(|scope| scope.contains_key(name));
                    if active {
                        self.check_binding_use(name, BindingUse::Read, function.span)?;
                        let state = self.binding_state(name);
                        let captures_reference = state.and_then(|state| state.reference).is_some()
                            || state.map(|state| state.boxed).unwrap_or(false);
                        if captures_reference {
                            return Err(Diagnostic::error(
                                "NXL2017",
                                format!(
                                    "closure captures reference '{name}'; references may not escape the borrow scope"
                                ),
                                function.span,
                            )
                            .with_help("capture the owned value instead"));
                        }
                    }
                    let result = self.allocate();
                    let ty = self.value_type(value);
                    let instruction = Instruction::Load {
                        result,
                        name: name.clone(),
                        ty,
                        span: function.span,
                    };
                    self.record_value_type(&instruction);
                    self.instructions.push(instruction);
                    capture_ids.push(result);
                }

                let mut params: Vec<(String, Type)> = captured
                    .iter()
                    .map(|name| (name.clone(), Type::Unknown))
                    .collect();
                // A declared parameter annotation resolves to its type, so a
                // closure parameter written `&T` is a reference slot exactly
                // like a named function's; an unannotated one stays `Unknown`.
                let mut declared = Vec::with_capacity(function.parameters.len());
                for param in &function.parameters {
                    let ty = param
                        .annotation
                        .as_ref()
                        .and_then(|annotation| self.resolve_type(annotation))
                        .unwrap_or(Type::Unknown);
                    declared.push(ty);
                    params.push((param.name.clone(), ty));
                }
                self.signatures.push(FunctionSignature {
                    name: closure_name.clone(),
                    parameters: params.clone(),
                    return_type: Type::Unknown,
                    type_parameters: function.type_parameters.clone(),
                    parameter_annotations: captured
                        .iter()
                        .map(|_| None)
                        .chain(
                            function
                                .parameters
                                .iter()
                                .map(|param| param.annotation.clone()),
                        )
                        .collect(),
                    return_annotation: function.return_type.clone(),
                });

                let owned = crate::ast::Function {
                    name: Some(closure_name.clone()),
                    type_parameters: function.type_parameters.clone(),
                    constraints: function.constraints.clone(),
                    parameters: function.parameters.clone(),
                    return_type: function.return_type.clone(),
                    body: function.body.clone(),
                    is_async: function.is_async,
                    span: function.span,
                };
                // Closure literals get the same declaration-level ownership
                // check as a named function: `fn() -> &Int` is rejected
                // wherever it is written.
                validate_function_ownership(&owned)?;
                self.lower_statement(&Statement::Function(owned))?;

                let result = self.allocate();
                self.closure_parameters.insert(result, declared);
                let instruction = Instruction::MakeClosure {
                    result,
                    callee: closure_name,
                    captures: capture_ids,
                    span: function.span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                Ok(result)
            }
            ExpressionKind::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                let condition_id = self.expression(condition)?;
                let condition_type = self.value_type(condition_id);
                if condition_type != Type::Bool && !is_dynamic(condition_type) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "conditional expression condition has type {condition_type:?}, expected Bool"
                        ),
                        condition.span,
                    ));
                }

                // Lower each branch into its own instruction stream, then
                // merge by storing the chosen value into a synthesized local.
                let saved_instructions = std::mem::take(&mut self.instructions);
                let saved_values = self.values.clone();
                let saved_mutability = self.mutability.clone();
                let saved_value_types = self.value_types.clone();

                let then_id = self.expression(then_value)?;
                let then_type = self.value_type(then_id);
                let mut then_instructions = std::mem::take(&mut self.instructions);

                self.instructions = Vec::new();
                self.values = saved_values.clone();
                self.mutability = saved_mutability.clone();
                self.value_types = saved_value_types.clone();
                let else_id = self.expression(else_value)?;
                let else_type = self.value_type(else_id);
                let mut else_instructions = std::mem::take(&mut self.instructions);

                self.values = saved_values;
                self.mutability = saved_mutability;
                self.value_types = saved_value_types;

                if then_type == Type::Void || else_type == Type::Void {
                    return Err(Diagnostic::error(
                        "NXL2006",
                        "a conditional expression cannot produce a void value",
                        expression.span,
                    ));
                }
                let result_type =
                    unify_inferred_types(then_type, else_type).map_err(|()| {
                        Diagnostic::error(
                            "NXL2001",
                            format!(
                                "conditional branches have incompatible types {then_type:?} and {else_type:?}"
                            ),
                            expression.span,
                        )
                    })?;

                let temporary = format!("__conditional_{}", self.conditional_counter);
                self.conditional_counter += 1;
                then_instructions.push(Instruction::Store {
                    name: temporary.clone(),
                    value: then_id,
                    span: expression.span,
                });
                else_instructions.push(Instruction::Store {
                    name: temporary.clone(),
                    value: else_id,
                    span: expression.span,
                });

                self.instructions = saved_instructions;
                self.instructions.push(Instruction::If {
                    condition: condition_id,
                    then_branch: then_instructions,
                    else_branch: Some(else_instructions),
                    span: expression.span,
                });
                let result = self.allocate();
                let load = Instruction::Load {
                    result,
                    name: temporary,
                    ty: result_type,
                    span: expression.span,
                };
                self.record_value_type(&load);
                self.instructions.push(load);
                Ok(result)
            }
            _ => Err(Diagnostic::error(
                "NXL2007",
                "this expression is parsed but is not lowered yet",
                expression.span,
            )),
        }
    }

    fn lookup_value(&self, name: &str) -> Option<ValueId> {
        for scope in self.scope_stack.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(*value);
            }
        }
        self.values.get(name).copied()
    }

    fn lookup_declaration(&self, name: &str) -> Option<ValueId> {
        for scope in self.scope_stack.iter().rev() {
            if scope.contains_key(name) {
                return scope.get(name).copied();
            }
        }
        self.values.get(name).copied()
    }

    fn constant(&mut self, value: Constant, ty: Type, span: Span) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::Constant {
            result,
            value,
            ty,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        result
    }

    fn allocate(&mut self) -> ValueId {
        let value = self.next_value;
        self.next_value += 1;
        value
    }

    fn record_value_type(&mut self, instruction: &Instruction) {
        for (id, ty) in instruction_value_types(instruction) {
            self.value_types.entry(id).or_insert(ty);
        }
        if let Instruction::Allocate { result, ty, .. } = instruction {
            self.value_types
                .entry(*result)
                .or_insert(Type::Reference(type_identity(*ty), true));
        }
    }

    fn value_type(&self, value: ValueId) -> Type {
        self.value_types
            .get(&value)
            .copied()
            .unwrap_or(Type::Unknown)
    }

    /// Remember the concrete arguments behind an `Instance` fingerprint. The
    /// fingerprint is a one-way hash, so every site that builds an instance
    /// from written text records its argument list here; inference, field
    /// lookup and annotation round-tripping all read it back.
    fn record_instance_arguments(&mut self, base: TypeId, arguments: &[Type]) {
        let fingerprint = argument_fingerprint(arguments);
        for argument in arguments {
            self.register_type_identities(*argument);
        }
        self.identity_types
            .entry(mix_type_id(base, fingerprint))
            .or_insert(Type::Instance(base, fingerprint));
        self.instance_arguments
            .entry((base, fingerprint))
            .or_insert_with(|| arguments.to_vec());
    }

    /// Register a type that was resolved from a written annotation. Hashed
    /// ids cannot be reversed, so the written text is the only place the
    /// target of a reference, the element of an array, or the arguments of a
    /// generic instance are still available.
    fn register_annotation_type(&mut self, annotation: &str, ty: Type) {
        let annotation = annotation.trim();
        match ty {
            Type::Reference(id, _) => {
                let target_text = annotation
                    .strip_prefix('&')
                    .map(|rest| rest.strip_prefix("mut ").unwrap_or(rest).trim())
                    .filter(|rest| !rest.is_empty());
                if let Some(target_text) = target_text {
                    if let Some(target) = self.resolve_type(target_text) {
                        self.register_annotation_type(target_text, target);
                        self.identity_types.entry(id).or_insert(target);
                    }
                }
            }
            Type::Array(id) => {
                if let Some(element_text) = array_element_annotation(annotation) {
                    if let Some(element) = self.resolve_type(element_text) {
                        self.register_annotation_type(element_text, element);
                        self.identity_types.entry(id).or_insert(element);
                    }
                }
            }
            Type::Instance(base, fingerprint) => {
                let Some(open) = annotation.find('<') else {
                    return;
                };
                if !annotation.ends_with('>') {
                    return;
                }
                let names = split_type_arguments(&annotation[open + 1..annotation.len() - 1]);
                let mut arguments = Vec::with_capacity(names.len());
                for name in &names {
                    let Some(argument) = self.resolve_type(name) else {
                        return;
                    };
                    // Nested instances (`Box<Box<Int>>`) are only visible in
                    // their written form here, so register them as well.
                    self.register_annotation_type(name, argument);
                    arguments.push(argument);
                }
                if argument_fingerprint(&arguments) == fingerprint {
                    self.record_instance_arguments(base, &arguments);
                }
            }
            _ => {}
        }
    }

    /// Register the identities a type already knows about, so nested
    /// structural types survive a round-trip through `type_by_identity`.
    fn register_type_identities(&mut self, ty: Type) {
        match ty {
            Type::Reference(id, _)
            | Type::Array(id)
            | Type::Optional(id)
            | Type::Task(id)
            | Type::Channel(id) => {
                if let Some(inner) = self.type_by_identity(id) {
                    self.register_type_identities(inner);
                }
            }
            Type::Instance(base, fingerprint) => {
                if let Some(arguments) = self.resolve_instance_arguments(base, fingerprint) {
                    for argument in arguments {
                        self.register_type_identities(argument);
                    }
                }
            }
            _ => {}
        }
    }

    /// Recover the concrete `Type`s of an `Instance` from the fingerprint of
    /// a written annotation (`Box<Int>`), when that annotation is available.
    fn resolve_instance_arguments(&self, base: TypeId, fingerprint: TypeId) -> Option<Vec<Type>> {
        // The fingerprint is not reversible, so record arguments at every
        // construction/annotation site instead. This helper only validates
        // the cached entry (guarding against a stale hash collision).
        let arguments = self.instance_arguments.get(&(base, fingerprint))?;
        (argument_fingerprint(arguments) == fingerprint).then(|| arguments.clone())
    }

    /// Resolve a `Type::Array`/`Type::Tuple` element id back to a `Type`.
    fn type_by_identity(&self, id: TypeId) -> Option<Type> {
        if let Some(ty) = self.identity_types.get(&id) {
            return Some(*ty);
        }
        for (name, ty) in [
            ("Int", Type::Int),
            ("Float", Type::Float),
            ("Bool", Type::Bool),
            ("String", Type::String),
            ("Void", Type::Void),
        ] {
            if stable_type_id(name) == id {
                return Some(ty);
            }
        }
        // Named user types resolve by their (possibly qualified) identity.
        self.program.statements.iter().find_map(|statement| {
            let declaration = match statement {
                Statement::TypeDeclaration(declaration) => declaration,
                Statement::TypeAlias(alias) if stable_type_id(&alias.name) == id => {
                    return Some(Type::Generic(id));
                }
                Statement::Export(inner) => {
                    return match inner.as_ref() {
                        Statement::TypeDeclaration(declaration) => {
                            (stable_type_id(&declaration.name) == id).then_some(Type::Generic(id))
                        }
                        _ => None,
                    }
                }
                _ => return None,
            };
            (stable_type_id(&declaration.name) == id).then_some(Type::Generic(id))
        })
    }

    /// The declared field type of `field` on `object_ty`, if the object type
    /// and the declaration can be resolved statically. For an instantiated
    /// generic struct the declared parameters are substituted with the
    /// instance's recorded arguments, so `Box<Int>.value` is `Int`.
    fn field_type_of(&mut self, object_ty: Type, field: &str) -> Option<Type> {
        let (base, arguments) = match object_ty {
            Type::Generic(base) | Type::Struct(base) | Type::Enum(base) => (base, None),
            Type::Instance(base, fingerprint) => {
                (base, self.resolve_instance_arguments(base, fingerprint))
            }
            _ => return None,
        };
        // Copy the declaration data out so the immutable borrow of the
        // program ends before identities are registered below.
        let (annotation, type_parameters) =
            self.program.statements.iter().find_map(|statement| {
                let declaration = match statement {
                    Statement::TypeDeclaration(declaration) => declaration,
                    Statement::Export(inner) => match inner.as_ref() {
                        Statement::TypeDeclaration(declaration) => declaration,
                        _ => return None,
                    },
                    _ => return None,
                };
                if stable_type_id(&declaration.name) != base {
                    return None;
                }
                let declared = declaration
                    .fields
                    .iter()
                    .find(|candidate| candidate.name == field)?;
                Some((
                    declared.annotation.clone(),
                    declaration.type_parameters.clone(),
                ))
            })?;
        let annotation = match arguments {
            Some(arguments) if !type_parameters.is_empty() => {
                if arguments.len() != type_parameters.len() {
                    return None;
                }
                let mut bindings = HashMap::with_capacity(type_parameters.len());
                for (parameter, argument) in type_parameters.iter().zip(arguments.iter()) {
                    let rendered = self.type_to_annotation(*argument)?;
                    bindings.insert(parameter.clone(), rendered);
                }
                substitute_type_parameters(&annotation, &bindings)
            }
            _ => annotation,
        };
        let ty = type_from_name_with(&annotation, &self.type_aliases, &type_parameters)?;
        self.register_annotation_type(&annotation, ty);
        Some(ty)
    }

    /// The element type of an array type, when it can be recovered.
    fn array_element_type(&self, ty: Type) -> Option<Type> {
        match ty {
            Type::Array(id) if id != 0 => self.type_by_identity(id),
            _ => None,
        }
    }

    /// Structurally bind a declared type parameter to the concrete type of a
    /// value. Returns `Err(())` when the same parameter would be bound to two
    /// incompatible types. Annotations that do not mention a parameter are
    /// ignored here; the ordinary type check validates them.
    fn bind_annotation_to_type(
        &self,
        annotation: &str,
        actual: Type,
        params: &[String],
        bindings: &mut HashMap<String, Type>,
    ) -> Result<(), ()> {
        let annotation = annotation.trim();
        if params.iter().any(|parameter| parameter == annotation) {
            if is_dynamic(actual) {
                return Ok(());
            }
            return match bindings.get(annotation) {
                Some(existing) if *existing == actual => Ok(()),
                Some(_) => Err(()),
                None => {
                    bindings.insert(annotation.to_owned(), actual);
                    Ok(())
                }
            };
        }
        // References constrain the type they point at, not the reference
        // itself: `fn read<T>(x: &T)` called with `borrow n` binds `T = Int`.
        if let Some(inner) = annotation.strip_prefix('&') {
            let target = inner.strip_prefix("mut ").unwrap_or(inner).trim();
            let pointed = match self.reference_target(actual) {
                Some((target, _)) => target,
                None if is_dynamic(actual) => Type::Unknown,
                // A non-reference value cannot satisfy a reference parameter.
                None => return Err(()),
            };
            return self.bind_annotation_to_type(target, pointed, params, bindings);
        }
        // Array forms: `[T]`, `Array<T>`, `List<T>`.
        let element_annotation = if annotation.starts_with('[') && annotation.ends_with(']') {
            Some(annotation[1..annotation.len() - 1].to_owned())
        } else if let Some(open) = annotation.find('<') {
            let base = annotation[..open].trim();
            if annotation.ends_with('>') && matches!(base, "Array" | "List") {
                let inner = &annotation[open + 1..annotation.len() - 1];
                (!inner.contains(',')).then(|| inner.to_owned())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(inner) = element_annotation {
            if let Some(element) = self.array_element_type(actual) {
                return self.bind_annotation_to_type(&inner, element, params, bindings);
            }
            return Ok(());
        }
        // User generic form: `Base<T, ...>`.
        if let Some(open) = annotation.find('<') {
            if annotation.ends_with('>') {
                let base_name = annotation[..open].trim();
                let arguments = split_type_arguments(&annotation[open + 1..annotation.len() - 1]);
                let resolved_base = stable_type_id(
                    &self
                        .type_aliases
                        .get(base_name)
                        .cloned()
                        .unwrap_or_else(|| base_name.to_owned()),
                );
                let instance_arguments = match actual {
                    Type::Instance(base, fingerprint) if base == resolved_base => {
                        self.resolve_instance_arguments(base, fingerprint)
                    }
                    _ => None,
                };
                if let Some(arguments_actual) = instance_arguments {
                    for (declared, actual) in arguments.iter().zip(arguments_actual.iter()) {
                        self.bind_annotation_to_type(declared, *actual, params, bindings)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Turn inferred parameter bindings into annotation strings. Every
    /// parameter must be bound to a concrete type; otherwise inference is
    /// ambiguous and the caller must supply explicit type arguments.
    fn inferred_arguments(
        &self,
        display: &str,
        params: &[String],
        bindings: &HashMap<String, Type>,
        missing_help: &str,
        span: Span,
    ) -> Result<HashMap<String, String>, Diagnostic> {
        let mut written = HashMap::with_capacity(params.len());
        for parameter in params {
            let Some(ty) = bindings.get(parameter) else {
                return Err(Diagnostic::error(
                    "NXL2014",
                    format!(
                        "cannot infer type argument '{parameter}' for '{display}'; add explicit type argument(s)"
                    ),
                    span,
                )
                .with_help(missing_help));
            };
            let Some(annotation) = self.type_to_annotation(*ty) else {
                return Err(Diagnostic::error(
                    "NXL2014",
                    format!(
                        "cannot infer type argument '{parameter}' for '{display}' from a dynamic value"
                    ),
                    span,
                )
                .with_help(missing_help));
            };
            written.insert(parameter.clone(), annotation);
        }
        Ok(written)
    }

    /// Infer return types for unannotated functions before the real lowering
    /// pass. Calls are order-independent: a function that depends on another
    /// unannotated function is retried in the next round. Errors are ignored
    /// here so the main pass can report them with full span context.
    fn infer_return_types(&mut self, files: &[ProjectFile]) {
        const MAX_ROUNDS: usize = 8;
        for _ in 0..MAX_ROUNDS {
            let mut changed = false;
            for file in files {
                for statement in &file.hir.program.statements {
                    let function = match statement {
                        Statement::Function(function) => function,
                        Statement::Export(inner) => match inner.as_ref() {
                            Statement::Function(function) => function,
                            _ => continue,
                        },
                        _ => continue,
                    };
                    let Some(name) = &function.name else {
                        continue;
                    };
                    if function.return_type.is_some() {
                        continue;
                    }
                    let Some(signature) = self
                        .signatures
                        .iter()
                        .find(|signature| signature.name == *name)
                    else {
                        continue;
                    };
                    if signature.return_type != Type::Unknown {
                        continue;
                    }
                    let mut probe = self.clone();
                    probe.functions.clear();
                    probe.instructions.clear();
                    probe.values.clear();
                    probe.mutability.clear();
                    probe.scope_stack = vec![HashMap::new()];
                    probe.mutability_stack = vec![HashMap::new()];
                    probe.next_value = 0;
                    probe.in_function = false;
                    probe.current_function_return_type = None;
                    probe.current_return_types.clear();
                    probe.current_type_parameters.clear();
                    probe.in_loop = false;
                    probe.loop_depth = 0;
                    probe.try_stack.clear();
                    probe.function_has_return = false;
                    probe.value_types.clear();
                    probe.value_field_types.clear();
                    probe.conditional_counter = 0;
                    probe.async_counter = 0;
                    probe.closure_counter = 0;
                    probe.current_module = file.path.clone();
                    probe.file_imports = file.imports.clone();
                    probe.type_aliases = file.aliases.clone();
                    probe.source_name = file.hir.source_name.clone();
                    if probe.lower_statement(statement).is_err() {
                        continue;
                    }
                    // The function branch records its inferred result on the
                    // probe's own signature and restores the raw return list.
                    let inferred = probe
                        .signatures
                        .iter()
                        .find(|signature| signature.name == *name)
                        .map(|signature| signature.return_type)
                        .unwrap_or(Type::Unknown);
                    if inferred == Type::Unknown {
                        continue;
                    }
                    if let Some(signature) = self
                        .signatures
                        .iter_mut()
                        .find(|signature| signature.name == *name)
                    {
                        if signature.return_type != inferred {
                            signature.return_type = inferred;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Infer the type of `field` read from an already-lowered object value.
    fn inferred_field_type(&mut self, object: ValueId, field: &str) -> Type {
        if let Some(fields) = self.value_field_types.get(&object) {
            if let Some(ty) = fields.get(field) {
                return *ty;
            }
        }
        let object_type = self.value_type(object);
        self.field_type_of(object_type, field)
            .unwrap_or(Type::Unknown)
    }

    /// Infer the element type of an already-lowered array value.
    fn inferred_element_type(&self, object: ValueId) -> Type {
        self.array_element_type(self.value_type(object))
            .unwrap_or(Type::Unknown)
    }

    /// Render a concrete inferred type back into a type annotation that
    /// `resolve_type_argument` can read.
    fn type_to_annotation(&self, ty: Type) -> Option<String> {
        match ty {
            Type::Int => Some("Int".to_owned()),
            Type::Float => Some("Float".to_owned()),
            Type::Bool => Some("Bool".to_owned()),
            Type::String => Some("String".to_owned()),
            Type::Void => Some("Void".to_owned()),
            Type::Generic(id) => self.type_name_by_identity(id),
            Type::Instance(base, fingerprint) => {
                let arguments = self.resolve_instance_arguments(base, fingerprint)?;
                let base_name = self.type_name_by_identity(base)?;
                let rendered = arguments
                    .iter()
                    .map(|argument| self.type_to_annotation(*argument))
                    .collect::<Option<Vec<_>>>()?
                    .join(", ");
                Some(format!("{base_name}<{rendered}>"))
            }
            Type::Array(id) => {
                let element = self.type_by_identity(id)?;
                Some(format!("[{}]", self.type_to_annotation(element)?))
            }
            Type::Reference(id, mutable) => {
                let target = self.type_by_identity(id)?;
                let rendered = self.type_to_annotation(target)?;
                Some(if mutable {
                    format!("&mut {rendered}")
                } else {
                    format!("&{rendered}")
                })
            }
            Type::Tuple(id) => {
                // Tuples are keyed by their written annotation; recover the
                // element list through the same identity map used for arrays.
                let _ = id;
                None
            }
            _ => None,
        }
    }

    /// Resolve a type identity back to the declared (possibly qualified) name.
    fn type_name_by_identity(&self, id: TypeId) -> Option<String> {
        self.type_names
            .iter()
            .find(|name| stable_type_id(name) == id)
            .cloned()
    }
}

pub(crate) fn type_from_name(name: &str, aliases: &HashMap<String, String>) -> Option<Type> {
    type_from_name_with(name, aliases, &[])
}

/// Resolve a written type annotation, treating names in `params` as
/// unsubstituted type parameters (`T`). User-defined `Base<Args>` become
/// `Type::Instance`; their base is resolved through `aliases` when known.
pub(crate) fn type_from_name_with(
    name: &str,
    aliases: &HashMap<String, String>,
    params: &[String],
) -> Option<Type> {
    /// Resolve a bare argument name through aliases for hashing purposes.
    fn keyed(argument: &str, aliases: &HashMap<String, String>) -> String {
        aliases
            .get(argument)
            .cloned()
            .unwrap_or_else(|| argument.to_owned())
    }
    if params.iter().any(|param| param == name) {
        return Some(Type::Param(stable_type_id(name)));
    }
    match name {
        "Int" => Some(Type::Int),
        "Float" => Some(Type::Float),
        "Bool" => Some(Type::Bool),
        "String" => Some(Type::String),
        "Void" => Some(Type::Void),
        _ => {
            // Imported and own-module types resolve before structural parsing.
            if let Some(qualified) = aliases.get(name) {
                return Some(Type::Generic(stable_type_id(qualified)));
            }
            if let Some(inner) = name.strip_prefix('&') {
                let (mutable, target) = inner
                    .strip_prefix("mut ")
                    .map_or((false, inner), |target| (true, target));
                // The reference is keyed by the identity of what it points
                // at, not by the text written after `&`, so a borrow of a
                // `Box<Int>` and an annotation `&Box<Int>` agree.
                let target_ty = type_from_name_with(target, aliases, params)?;
                return Some(Type::Reference(type_identity(target_ty), mutable));
            }
            if name.starts_with('[') && name.ends_with(']') {
                let inner = &name[1..name.len() - 1];
                let element = type_from_name_with(inner, aliases, params)?;
                return Some(Type::Array(array_identity(element)));
            }
            if name.starts_with('(') && name.ends_with(')') {
                let contents = &name[1..name.len() - 1];
                let arguments = split_type_arguments(contents)
                    .into_iter()
                    .map(|argument| type_from_name_with(argument, aliases, params))
                    .collect::<Option<Vec<_>>>()?;
                // A reference nested inside a value type would escape the
                // borrow scope; only a direct `&T` binding or parameter is
                // supported.
                if arguments
                    .iter()
                    .any(|argument| matches!(argument, Type::Reference(..)))
                {
                    return None;
                }
                return Some(Type::Tuple(stable_type_id(name)));
            }
            let open = name.find('<')?;
            if !name.ends_with('>') {
                return None;
            }
            let constructor = &name[..open];
            let argument_names = split_type_arguments(&name[open + 1..name.len() - 1]);
            let arguments = argument_names
                .iter()
                .map(|argument| type_from_name_with(argument, aliases, params))
                .collect::<Option<Vec<_>>>()?;
            if arguments
                .iter()
                .any(|argument| matches!(argument, Type::Reference(..)))
            {
                return None;
            }
            match (constructor, arguments.as_slice()) {
                ("Array", [_]) | ("List", [_]) => Some(Type::Array(array_identity(arguments[0]))),
                ("Option", [_]) => Some(Type::Optional(stable_type_id(&keyed(
                    argument_names[0],
                    aliases,
                )))),
                ("Task", [_]) => Some(Type::Task(stable_type_id(&keyed(
                    argument_names[0],
                    aliases,
                )))),
                ("Channel", [_]) => Some(Type::Channel(stable_type_id(&keyed(
                    argument_names[0],
                    aliases,
                )))),
                ("Result", [_, _]) => Some(Type::Result(
                    stable_type_id(&keyed(argument_names[0], aliases)),
                    stable_type_id(&keyed(argument_names[1], aliases)),
                )),
                ("Map", [_, _]) => Some(Type::Map(stable_type_id(name))),
                // Known containers with the wrong arity are malformed, not
                // user-defined generic types.
                ("Array" | "List" | "Option" | "Task" | "Channel" | "Result" | "Map", _) => None,
                (base, _) => {
                    let qualified = aliases
                        .get(base)
                        .cloned()
                        .unwrap_or_else(|| base.to_owned());
                    Some(Type::Instance(
                        stable_type_id(&qualified),
                        argument_fingerprint(&arguments),
                    ))
                }
            }
        }
    }
}

pub(crate) fn stable_type_id(name: &str) -> TypeId {
    name.bytes().fold(2_166_136_261u32, |hash, byte| {
        hash.wrapping_mul(16_777_619) ^ u32::from(byte)
    })
}

/// The element annotation of an array type written as `[T]`, `Array<T>` or
/// `List<T>`. Used to register the element identity of an array built from
/// an annotation (`type_from_name_with` hashes the element's identity).
fn array_element_annotation(annotation: &str) -> Option<&str> {
    let trimmed = annotation.trim();
    if let Some(inner) = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return Some(inner.trim());
    }
    let open = trimmed.find('<')?;
    let base = trimmed[..open].trim();
    if matches!(base, "Array" | "List") && trimmed.ends_with('>') {
        return Some(trimmed[open + 1..trimmed.len() - 1].trim());
    }
    None
}

pub(crate) fn split_type_arguments(arguments: &str) -> Vec<&str> {
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut items = Vec::new();
    for (index, character) in arguments.char_indices() {
        match character {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(arguments[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if start < arguments.len() {
        items.push(arguments[start..].trim());
    }
    items
}

/// Number of type parameters a builtin container constructor takes.
fn builtin_type_arity(name: &str) -> Option<usize> {
    match name {
        "Array" | "List" | "Option" | "Task" | "Channel" => Some(1),
        "Result" | "Map" => Some(2),
        _ => None,
    }
}

fn is_primitive_type_name(name: &str) -> bool {
    matches!(name, "Int" | "Float" | "Bool" | "String" | "Void")
}

/// Replace identifier tokens that name type parameters with their written
/// type arguments. Identifiers are scanned whole, so substituting `T` does
/// not corrupt `Task<T>` or `Type`.
fn substitute_type_parameters(annotation: &str, bindings: &HashMap<String, String>) -> String {
    let mut result = String::with_capacity(annotation.len());
    let mut identifier = String::new();
    let flush = |identifier: &mut String, result: &mut String| {
        if identifier.is_empty() {
            return;
        }
        match bindings.get(identifier.as_str()) {
            Some(replacement) => result.push_str(replacement),
            None => result.push_str(identifier),
        }
        identifier.clear();
    };
    for character in annotation.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            identifier.push(character);
        } else {
            flush(&mut identifier, &mut result);
            result.push(character);
        }
    }
    flush(&mut identifier, &mut result);
    result
}

/// Best-effort substitution for signatures without written annotations.
/// Container arguments are identity hashes, so replacing them is only
/// possible when the bound argument has a recorded written form.
fn substitute_erased_type_parameters(ty: Type, bindings: &HashMap<TypeId, (Type, String)>) -> Type {
    let inner = |id: TypeId| {
        bindings
            .get(&id)
            .map(|(_, written)| stable_type_id(written))
            .unwrap_or(id)
    };
    match ty {
        Type::Param(id) => bindings.get(&id).map_or(ty, |(resolved, _)| *resolved),
        // Arrays and references only reach this path through a written
        // annotation, which is substituted structurally before resolving, so
        // their ids are already final here.
        Type::Array(_) => ty,
        Type::Tuple(id) => Type::Tuple(inner(id)),
        Type::Map(id) => Type::Map(inner(id)),
        Type::Struct(id) => Type::Struct(inner(id)),
        Type::Enum(id) => Type::Enum(inner(id)),
        Type::Reference(id, mutable) => Type::Reference(inner(id), mutable),
        Type::Task(id) => Type::Task(inner(id)),
        Type::Channel(id) => Type::Channel(inner(id)),
        Type::Optional(id) => Type::Optional(inner(id)),
        Type::Result(ok, error) => Type::Result(inner(ok), inner(error)),
        Type::Instance(_, _)
        | Type::Generic(_)
        | Type::Unknown
        | Type::Function(_)
        | Type::ForeignHandle(_)
        | Type::Int
        | Type::Float
        | Type::Bool
        | Type::String
        | Type::Void => ty,
    }
}

fn statement_span(statement: &Statement) -> Span {
    match statement {
        Statement::Let { span, .. }
        | Statement::Return { span, .. }
        | Statement::Module { span, .. }
        | Statement::Import { span, .. }
        | Statement::If { span, .. }
        | Statement::For { span, .. }
        | Statement::While { span, .. }
        | Statement::Loop { span, .. }
        | Statement::Match { span, .. }
        | Statement::Try { span, .. }
        | Statement::Throw { span, .. }
        | Statement::Yield { span, .. }
        | Statement::ExternBlock { span, .. } => *span,
        Statement::Break(span) | Statement::Continue(span) => *span,
        Statement::Function(function) => function.span,
        Statement::TypeDeclaration(declaration) => declaration.span,
        Statement::TypeAlias(alias) => alias.span,
        Statement::ImplBlock(implementation) => implementation.span,
        Statement::Expression(expression) => expression.span,
        Statement::Block(block) | Statement::Unsafe(block) => block.span,
        Statement::Export(inner) => statement_span(inner),
    }
}

impl std::fmt::Display for Module {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "module {}", self.name)?;
        for function in &self.functions {
            writeln!(
                formatter,
                "  fn {} -> {:?} {{",
                function.name, function.return_type
            )?;
            writeln!(formatter, "    cfg:")?;
            for block in &function.basic_blocks {
                writeln!(
                    formatter,
                    "      bb{}: {:?}; terminator: {:?}",
                    block.id, block.instructions, block.terminator
                )?;
            }
            writeln!(formatter, "  }}")?;
        }
        Ok(())
    }
}

/// Functions the runtime provides without a declaration in the program.
///
/// `None` means the built-in takes any number of arguments, `Some(n)` that it
/// takes exactly `n`. These must stay in step with the runtimes: the VM's
/// `call_function`, and the prelude each backend emits. Stdlib builtins
/// (`std::math::sqrt`, …) carry their arity in the `stdlib` spec table.
fn builtin_arity(name: &str) -> Option<Option<usize>> {
    match name {
        "print" => Some(None),
        "escape_html" | "strip_tags" => Some(Some(1)),
        _ => crate::stdlib::builtin(name).map(|spec| Some(spec.arity())),
    }
}

/// The declared result type of a builtin call.
fn builtin_return_type(name: &str) -> Type {
    match name {
        "print" => Type::Void,
        "escape_html" | "strip_tags" => Type::String,
        _ => match crate::stdlib::builtin(name) {
            Some(spec) => stdlib_type(spec.ret),
            None => Type::Unknown,
        },
    }
}

/// Map a stdlib builtin type onto the NXL type it lowers to.
fn stdlib_type(ty: crate::stdlib::BuiltinType) -> Type {
    use crate::stdlib::BuiltinType;
    match ty {
        BuiltinType::Int => Type::Int,
        BuiltinType::Float => Type::Float,
        BuiltinType::Str => Type::String,
        BuiltinType::Bool => Type::Bool,
        // Structured values (HTTP responses, resolver results) arrive with
        // no static shape; field reads on them are checked at runtime.
        BuiltinType::Value => Type::Unknown,
    }
}
