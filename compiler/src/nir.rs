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
    Generic(TypeId),
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

#[derive(Clone, Debug, PartialEq)]
pub struct ForeignFunction {
    pub name: String,
    pub library: Option<String>,
    pub parameters: Vec<Type>,
    pub return_type: Type,
    pub calling_convention: CallingConvention,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionSignature {
    pub name: String,
    pub parameters: Vec<(String, Type)>,
    pub return_type: Type,
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
    StructNew {
        result: ValueId,
        type_name: String,
        fields: Vec<(String, ValueId)>,
        span: Span,
    },
    /// Read a struct field (throws on unknown field / non-object).
    FieldGet {
        result: ValueId,
        object: ValueId,
        field: String,
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
        callee: String,
        arguments: Vec<ValueId>,
        return_type: Type,
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
    lower_project_files(&[ProjectFile {
        path: String::new(),
        hir: hir.clone(),
        imports: Vec::new(),
        aliases: HashMap::new(),
    }])
}

/// Lower several files (dependency order) into one module. The entry file
/// keeps the root namespace; every other file owns its dotted path.
pub fn lower_project_files(files: &[ProjectFile]) -> Result<Module, Diagnostic> {
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
    };

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
        foreign_functions: Vec::new(),
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
                type_definitions.push(TypeDefinition {
                    id: stable_type_id(&declaration.name),
                    name: declaration.name.clone(),
                    kind,
                    parameters: declaration
                        .type_parameters
                        .iter()
                        .map(|parameter| stable_type_id(parameter))
                        .collect(),
                    fields: declaration
                        .fields
                        .iter()
                        .map(|field| {
                            (
                                field.name.clone(),
                                type_from_name(&field.annotation, &HashMap::new()).unwrap_or_else(
                                    || Type::Generic(stable_type_id(&field.annotation)),
                                ),
                            )
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
                                        type_from_name(ty, &HashMap::new())
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
                    .is_some_and(|ty| *ty != Type::Bool && *ty != Type::Unknown)
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
                    if function.return_type != Type::Unknown
                        && actual != Type::Unknown
                        && actual != function.return_type
                    {
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
                        UnaryOperator::Not if operand_type == Type::Bool => Type::Bool,
                        _ => {
                            return Err(mismatch(format!(
                                "unary operation {:?} is invalid for {operand_type:?}",
                                operator
                            )))
                        }
                    };
                    if *ty != expected && *ty != Type::Unknown {
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
                        && left_ty != Type::Unknown
                        && right_ty != Type::Unknown
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
                            && *ty != Type::Unknown
                            && left_ty != Type::Unknown)
                    {
                        return Err(mismatch(
                            "binary instruction result type is inconsistent".to_owned(),
                        ));
                    }
                }
                Instruction::Call {
                    callee,
                    arguments,
                    return_type,
                    ..
                } => {
                    if let Some(signature) = signatures.iter().find(|sig| sig.name == *callee) {
                        if arguments.len() != signature.parameters.len()
                            || (*return_type != signature.return_type
                                && *return_type != Type::Unknown)
                        {
                            return Err(mismatch(format!(
                                "call to '{callee}' does not match its NIR signature"
                            )));
                        }
                        for (argument, (_, expected)) in arguments.iter().zip(&signature.parameters)
                        {
                            let actual = types.get(argument).copied().unwrap_or(Type::Unknown);
                            if actual != *expected
                                && actual != Type::Unknown
                                && *expected != Type::Unknown
                            {
                                return Err(mismatch(format!(
                                    "call argument has type {actual:?}, expected {expected:?}"
                                )));
                            }
                        }
                    }
                    if let Some(foreign) = foreign_functions
                        .iter()
                        .find(|signature| signature.name == *callee)
                    {
                        if arguments.len() != foreign.parameters.len()
                            || *return_type != foreign.return_type
                        {
                            return Err(mismatch(format!(
                                "foreign call to '{callee}' does not match its ABI signature"
                            )));
                        }
                    }
                }
                Instruction::ForeignCall {
                    callee,
                    arguments,
                    return_type,
                    ..
                } => {
                    let signature = foreign_functions
                        .iter()
                        .find(|signature| signature.name == *callee)
                        .ok_or_else(|| {
                            mismatch(format!("missing foreign ABI declaration for '{callee}'"))
                        })?;
                    if arguments.len() != signature.parameters.len()
                        || *return_type != signature.return_type
                    {
                        return Err(mismatch(format!(
                            "foreign call to '{callee}' does not match its ABI signature"
                        )));
                    }
                    for (argument, expected) in arguments.iter().zip(&signature.parameters) {
                        let actual = types.get(argument).copied().unwrap_or(Type::Unknown);
                        if actual != *expected && actual != Type::Unknown {
                            return Err(mismatch(format!(
                                "foreign ABI argument has type {actual:?}, expected {expected:?}"
                            )));
                        }
                    }
                }
                Instruction::Allocate { ty, .. } => {
                    if !matches!(
                        ty,
                        Type::Unknown
                            | Type::Int
                            | Type::Float
                            | Type::Bool
                            | Type::String
                            | Type::Struct(_)
                            | Type::Enum(_)
                            | Type::Generic(_)
                            | Type::Array(_)
                            | Type::Tuple(_)
                            | Type::Map(_)
                    ) {
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
                                || target == type_identity(value_type) => {}
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
                    match ty {
                        Type::Reference(target, is_mutable)
                            if *is_mutable == *mutable
                                && (value_type == Type::Unknown
                                    || *target == type_identity(value_type)) => {}
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

/// Whether a produced value may flow into an expected type.
/// `Type::Array(0)` (empty or fully dynamic array) unifies with any array
/// element type; precise element types must otherwise agree.
fn types_compatible(actual: Type, expected: Type) -> bool {
    if actual == expected || actual == Type::Unknown || expected == Type::Unknown {
        return true;
    }
    match (actual, expected) {
        (Type::Array(_), Type::Array(_)) => {
            matches!(
                (actual, expected),
                (Type::Array(0), _) | (_, Type::Array(0))
            )
        }
        _ => false,
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
        | Type::Function(id)
        | Type::ForeignHandle(id) => id,
        Type::Result(ok, error) => ok ^ error.rotate_left(1),
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
        } => vec![(*result, *return_type)],
        Instruction::IteratorNext { has_next, item, .. } => {
            vec![(*has_next, Type::Bool), (*item, Type::Unknown)]
        }
        Instruction::ArrayNew { result, .. } => vec![(*result, Type::Array(0))],
        Instruction::IndexGet { result, .. } => vec![(*result, Type::Unknown)],
        Instruction::IndexSet { result, .. } => vec![(*result, Type::Unknown)],
        Instruction::StructNew {
            result, type_name, ..
        } => vec![(*result, Type::Generic(stable_type_id(type_name)))],
        Instruction::FieldGet { result, .. } => vec![(*result, Type::Unknown)],
        Instruction::FieldSet { result, .. } => vec![(*result, Type::Unknown)],
        Instruction::EnumNew {
            result, type_name, ..
        } => vec![(*result, Type::Generic(stable_type_id(type_name)))],
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
    in_loop: bool,
    loop_depth: u32,
    /// Stack of loop depths captured at each lexically enclosing `try`.
    /// Used to reject control flow that would cross a handler boundary.
    try_stack: Vec<u32>,
    function_has_return: bool,
    source_name: String,
    type_names: std::collections::HashSet<String>,
    value_types: HashMap<ValueId, Type>,
    /// Full program, for resolving struct/enum declarations by name.
    program: Program,
    /// Dotted path of the file currently being lowered ("" for the entry).
    current_module: String,
    /// Monotonic counter for synthesized async-worker names.
    async_counter: u32,
    /// Import statements of the file currently being lowered.
    file_imports: Vec<crate::project::ModuleImport>,
    /// Bare-to-qualified type names for the current file (own types plus
    /// selected imports), used when resolving generic annotations.
    type_aliases: HashMap<String, String>,
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
                if condition_type != Type::Bool && condition_type != Type::Unknown {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!("if condition has type {condition_type:?}, expected Bool"),
                        *span,
                    ));
                }

                let saved_instructions = std::mem::take(&mut self.instructions);
                let then_saved_scope = self.scope_stack.clone();
                let then_saved_mutability = self.mutability_stack.clone();
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
                self.values = then_saved_values;
                self.mutability = then_saved_mutability_map;

                let else_branch = match else_branch {
                    Some(branch) => {
                        let else_saved_scope = self.scope_stack.clone();
                        let else_saved_mutability = self.mutability_stack.clone();
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
                let saved_try_stack = std::mem::take(&mut self.try_stack);
                let saved_loop_depth = self.loop_depth;
                // A nested function body is a fresh control-flow region: handler
                // boundaries and loop nesting of the enclosing scope do not apply.
                self.loop_depth = 0;

                // Reset for function body
                self.instructions = Vec::new();
                self.scope_stack = vec![HashMap::new()];
                self.mutability_stack = vec![HashMap::new()];
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
                    parameter_instructions.push(param_instruction);
                    self.values.insert(param_name.clone(), value_id);
                    self.scope_stack
                        .last_mut()
                        .unwrap()
                        .insert(param_name.clone(), value_id);
                    // Parameters are immutable by default
                    self.mutability.insert(param_name.clone(), false);
                    self.mutability_stack
                        .last_mut()
                        .unwrap()
                        .insert(param_name.clone(), false);
                }
                self.instructions = parameter_instructions;

                // Lower function body
                for stmt in &body.statements {
                    self.lower_statement(stmt)?;
                }

                if matches!(
                    return_type,
                    Type::Int | Type::Float | Type::Bool | Type::String
                ) && !Self::block_returns_on_all_paths(body)
                {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "function '{function_name}' declared to return {return_type:?} must return a value on all paths"
                        ),
                        function.span,
                    ));
                }

                // Create function with lowered instructions
                let func_instructions = std::mem::take(&mut self.instructions);
                self.functions.push(create_function(
                    function_name.clone(),
                    param_types,
                    return_type,
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
                self.try_stack = saved_try_stack;
                self.loop_depth = saved_loop_depth;

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
                let saved_values = self.values.clone();
                let saved_mutability = self.mutability.clone();
                self.push_scope();

                let binding_id = self.allocate();
                self.values.insert(binding.clone(), binding_id);
                self.scope_stack
                    .last_mut()
                    .unwrap()
                    .insert(binding.clone(), binding_id);
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
                if condition_type != Type::Bool && condition_type != Type::Unknown {
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
                self.in_loop = true;
                self.loop_depth += 1;
                let saved_instructions = std::mem::take(&mut self.instructions);
                for statement in &body.statements {
                    self.lower_statement(statement)?;
                }
                let loop_body = std::mem::take(&mut self.instructions);
                self.instructions = saved_instructions;
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
                self.in_loop = true;
                self.loop_depth += 1;
                let saved_instructions = std::mem::take(&mut self.instructions);
                for statement in &body.statements {
                    self.lower_statement(statement)?;
                }
                let loop_body = std::mem::take(&mut self.instructions);
                self.instructions = saved_instructions;
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
                    self.push_scope();
                    // Sibling cases must not see each other's bindings.
                    let saved_values = self.values.clone();
                    let saved_mutability = self.mutability.clone();
                    let mut test_id = self.lower_pattern_test(&case.pattern, value_id)?;

                    if let Some(guard) = &case.guard {
                        let guard_id = self.expression(guard)?;
                        let guard_type = self.value_type(guard_id);
                        if guard_type != Type::Bool && guard_type != Type::Unknown {
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
                    self.values = saved_values.clone();
                    self.mutability = saved_mutability.clone();
                    self.push_scope();
                    self.values.insert(catch.name.clone(), tmp);
                    self.scope_stack
                        .last_mut()
                        .unwrap()
                        .insert(catch.name.clone(), tmp);
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
    }

    fn resolve_type(&self, annotation: &str) -> Option<Type> {
        type_from_name(annotation, &self.type_aliases).or_else(|| {
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

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
        self.mutability_stack.pop();
    }

    /// Emit a struct/array field read for an already-lowered object.
    fn lower_field_get(&mut self, object: ValueId, field: &str, span: Span) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::FieldGet {
            result,
            object,
            field: field.to_owned(),
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
    fn lower_index_get(&mut self, object: ValueId, index: ValueId, span: Span) -> ValueId {
        let result = self.allocate();
        let instruction = Instruction::IndexGet {
            result,
            object,
            index,
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

    /// Lower an assignment whose target is a member or index expression.
    /// The object evaluates once; the result is written back when the root
    /// is a plain variable. The assignment evaluates to the right-hand value.
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
                let object_id = self.expression(object)?;
                (
                    Place::Field {
                        object: object_id,
                        field,
                    },
                    Self::place_root_name(object),
                )
            }
            ExpressionKind::Index {
                target: object,
                index,
            } => {
                let object_id = self.expression(object)?;
                let index_id = self.expression(index)?;
                let index_type = self.value_type(index_id);
                if index_type != Type::Int && index_type != Type::Unknown {
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
                    Self::place_root_name(object),
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
        if let Some(root) = root {
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
            }
        }
        let assigned = match operator {
            crate::ast::AssignmentOperator::Assign => self.expression(value)?,
            _ => {
                let read = match place {
                    Place::Field { object, field } => {
                        self.lower_field_get(object, field, target.span)
                    }
                    Place::Index { object, index } => {
                        self.lower_index_get(object, index, target.span)
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
                let left_type = self.value_type(read);
                let right_type = self.value_type(rhs);
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
        // Write the updated container back to the root binding.
        let updated = match place {
            Place::Field { object, field } => self.lower_field_set(object, field, assigned, span),
            Place::Index { object, index } => self.lower_index_set(object, index, assigned, span),
        };
        if let Some(root) = root {
            self.instructions.push(Instruction::Store {
                name: root.to_owned(),
                value: updated,
                span,
            });
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
        for field in &declaration.fields {
            if let Some(argument_id) = bound.get(&field.name).copied() {
                let actual = self.value_type(argument_id);
                self.check_field_value(display, &field.name, &field.annotation, actual, span)?;
                fields.push((field.name.clone(), argument_id));
            } else if let Some(default) = field.value.clone() {
                let default_id = self.expression(&default)?;
                let actual = self.value_type(default_id);
                self.check_field_value(display, &field.name, &field.annotation, actual, span)?;
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
            fields,
            span,
        };
        self.record_value_type(&instruction);
        self.instructions.push(instruction);
        Ok(result)
    }

    /// Lower a call to an already-resolved (possibly qualified) function.
    /// `display` is the source-written name used in diagnostics. With
    /// `concurrent`, the call spawns a worker thread instead of running now.
    fn lower_direct_call(
        &mut self,
        qualified: &str,
        display: &str,
        arguments: &[Expression],
        callee_span: Span,
        span: Span,
        concurrent: bool,
    ) -> Result<ValueId, Diagnostic> {
        // Check for user-defined function signature
        let (param_types, return_type) = {
            let signature = self
                .signatures
                .iter()
                .find(|sig| sig.name == qualified)
                .ok_or_else(|| {
                    Diagnostic::error(
                        "NXL2002",
                        format!("use of undeclared function '{display}'"),
                        callee_span,
                    )
                })?;

            (
                signature
                    .parameters
                    .iter()
                    .map(|(_, ty)| *ty)
                    .collect::<Vec<_>>(),
                signature.return_type,
            )
        };

        // Validate argument count
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

        let mut lowered_arguments = Vec::with_capacity(arguments.len());
        for (arg, param_type) in arguments.iter().zip(param_types.iter()) {
            let arg_value = self.expression(arg)?;
            let arg_type = self.value_type(arg_value);

            // Validate argument type compatibility
            if !types_compatible(arg_type, *param_type) {
                return Err(Diagnostic::error(
                    "NXL2001",
                    format!(
                        "argument to '{display}' has type {arg_type:?}, expected {param_type:?}"
                    ),
                    arg.span,
                ));
            }
            lowered_arguments.push(arg_value);
        }

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
        if let ExpressionKind::Call { callee, arguments } = &inner.kind {
            // Plain function calls spawn concurrently (unknown names and
            // builtins like `print` fall through to eager evaluation, which
            // reports the same diagnostics as a direct call).
            if let ExpressionKind::Name(name) = &callee.kind {
                if let Some(resolved) = self.resolve_function(name) {
                    return self.lower_direct_call(
                        &resolved,
                        name,
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
                if self.split_module_access(callee).is_none() {
                    let receiver_id = self.expression(target)?;
                    let mut lowered = Vec::with_capacity(arguments.len());
                    for argument in arguments {
                        lowered.push(self.expression(argument)?);
                    }
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
        arguments: &[Expression],
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        match variants.iter().find(|(candidate, _)| candidate == variant) {
            Some((_, payload)) if payload.len() == arguments.len() => {
                let mut lowered = Vec::with_capacity(arguments.len());
                for (argument, expected) in arguments.iter().zip(payload.iter()) {
                    let argument_id = self.expression(argument)?;
                    let actual = self.value_type(argument_id);
                    if let Some(expected) = self.resolve_type(expected) {
                        if !types_compatible(actual, expected) {
                            return Err(Diagnostic::error(
                                "NXL2001",
                                format!(
                                    "variant '{variant}' argument has type {actual:?}, expected {expected:?}"
                                ),
                                argument.span,
                            ));
                        }
                    }
                    lowered.push(argument_id);
                }
                let result = self.allocate();
                let instruction = Instruction::EnumNew {
                    result,
                    type_name: qualified_type.to_owned(),
                    variant: variant.to_owned(),
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
                    return self.lower_direct_call(&qualified, name, arguments, span, span, false);
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
                let result = self.allocate();
                let ty = self.value_type(value);
                let instruction = Instruction::Load {
                    result,
                    name: name.clone(),
                    ty,
                    span: expression.span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                Ok(result)
            }
            ExpressionKind::Unary { operator, operand } => {
                let operand = self.expression(operand)?;
                let result = self.allocate();
                let operand_type = self.value_type(operand);
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
                let left_type = self.value_type(left);
                let right_type = self.value_type(right);
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
                    // String concatenation composes in both runtimes.
                    BinaryOperator::Add
                        if left_type == Type::String && right_type == Type::String =>
                    {
                        Type::String
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
                    Some(ty) => Type::Array(type_identity(ty)),
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
                if index_type != Type::Int && index_type != Type::Unknown {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!("array index has type {index_type:?}, expected Int"),
                        index.span,
                    ));
                }
                Ok(self.lower_index_get(object_id, index_id, expression.span))
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
                Ok(self.lower_field_get(object_id, name, expression.span))
            }
            ExpressionKind::Object { type_name, fields } => {
                let qualified = self.resolve_type_alias(type_name);
                let declaration = find_type_declaration(&self.program, &qualified)
                    .cloned()
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "NXL2004",
                            format!("unknown type '{type_name}'"),
                            expression.span,
                        )
                    })?;
                if !matches!(declaration.kind, crate::ast::DeclarationKind::Struct) {
                    return Err(Diagnostic::error(
                        "NXL2001",
                        format!(
                            "cannot construct {:?} '{type_name}' with an object literal",
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
                            format!("duplicate field '{field}' in '{type_name}' literal"),
                            expression.span,
                        ));
                    }
                }
                let mut lowered = Vec::with_capacity(fields.len());
                for (field, value) in fields {
                    let Some(declared) = declaration.fields.iter().find(|f| &f.name == field)
                    else {
                        return Err(Diagnostic::error(
                            "NXL2002",
                            format!("unknown field '{field}' on struct '{type_name}'"),
                            expression.span,
                        ));
                    };
                    let value_id = self.expression(value)?;
                    let actual = self.value_type(value_id);
                    self.check_field_value(
                        type_name,
                        field,
                        &declared.annotation,
                        actual,
                        value.span,
                    )?;
                    lowered.push((field.clone(), value_id));
                }
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
                            "struct '{type_name}' literal is missing field(s): {}",
                            missing.join(", ")
                        ),
                        expression.span,
                    ));
                }
                let result = self.allocate();
                let instruction = Instruction::StructNew {
                    result,
                    type_name: qualified.clone(),
                    fields: lowered,
                    span: expression.span,
                };
                self.record_value_type(&instruction);
                self.instructions.push(instruction);
                Ok(result)
            }
            ExpressionKind::New { .. } => Err(Diagnostic::error(
                "NXL2007",
                "class instantiation with `new` is not supported yet",
                expression.span,
            )),
            ExpressionKind::Call { callee, arguments } => {
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
                                arguments,
                                expression.span,
                            );
                        }
                    }
                }
                // Module-qualified call: `math.add(..)`, `math.Color.Red(..)`.
                if let ExpressionKind::Member { .. } = &callee.kind {
                    if let Some((module, chain)) = self.split_module_access(callee) {
                        return self.lower_module_call(&module, &chain, arguments, expression.span);
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
                    let receiver_id = self.expression(target)?;
                    let mut lowered = Vec::with_capacity(arguments.len());
                    for argument in arguments {
                        lowered.push(self.expression(argument)?);
                    }
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
                        self.instructions.push(Instruction::Assign {
                            name: root.clone(),
                            value: receiver_out,
                            span: expression.span,
                        });
                    }
                    return Ok(result);
                }
                // Channel construction: `channel()` (element types are
                // informational in this phase; capacity arguments reserved).
                if matches!(&callee.kind, ExpressionKind::Channel { .. }) {
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
                let ExpressionKind::Name(name) = &callee.kind else {
                    return Err(Diagnostic::error(
                        "NXL2003",
                        "only named functions are supported in calls by this compiler stage",
                        callee.span,
                    ));
                };

                // Check for built-in print function
                if name == "print" {
                    let mut lowered_arguments = Vec::with_capacity(arguments.len());
                    for argument in arguments {
                        lowered_arguments.push(self.expression(argument)?);
                    }
                    let result = self.allocate();
                    let instruction = Instruction::Call {
                        result,
                        callee: name.clone(),
                        arguments: lowered_arguments,
                        return_type: Type::Void,
                        span: expression.span,
                    };
                    self.record_value_type(&instruction);
                    self.instructions.push(instruction);
                    return Ok(result);
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
                            return self.lower_construction(&qualified, arguments, expression.span);
                        }
                    }
                }

                // Resolve bare names through selected imports + own module.
                let resolved = self.resolve_function(name).ok_or_else(|| {
                    Diagnostic::error(
                        "NXL2002",
                        format!("use of undeclared function '{name}'"),
                        callee.span,
                    )
                })?;

                self.lower_direct_call(
                    &resolved,
                    name,
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
                    _ => {}
                }
                let ExpressionKind::Name(name) = &target.kind else {
                    return Err(Diagnostic::error(
                        "NXL2008",
                        "assignment target must be a variable, field, or index",
                        target.span,
                    ));
                };
                let binding = self.values.get(name).copied();
                let Some(_value_id) = binding else {
                    return Err(Diagnostic::error(
                        "NXL2002",
                        format!("use of undeclared name '{name}'"),
                        target.span,
                    ));
                };
                let is_mutable = self.mutability.get(name).copied().unwrap_or(false);
                if !is_mutable {
                    return Err(Diagnostic::error(
                        "NXL2008",
                        format!("cannot assign to immutable binding '{name}'"),
                        target.span,
                    ));
                }

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

                self.values.insert(name.clone(), assigned_value);
                let assign_instruction = Instruction::Assign {
                    name: name.clone(),
                    value: assigned_value,
                    span: expression.span,
                };
                self.instructions.push(assign_instruction);
                Ok(assigned_value)
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
}

pub(crate) fn type_from_name(name: &str, aliases: &HashMap<String, String>) -> Option<Type> {
    /// Resolve a bare argument name through aliases for hashing purposes.
    fn keyed(argument: &str, aliases: &HashMap<String, String>) -> String {
        aliases
            .get(argument)
            .cloned()
            .unwrap_or_else(|| argument.to_owned())
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
                type_from_name(target, aliases)?;
                return Some(Type::Reference(
                    stable_type_id(&keyed(target, aliases)),
                    mutable,
                ));
            }
            if name.starts_with('[') && name.ends_with(']') {
                let inner = &name[1..name.len() - 1];
                type_from_name(inner, aliases)?;
                return Some(Type::Array(stable_type_id(&keyed(inner, aliases))));
            }
            if name.starts_with('(') && name.ends_with(')') {
                let contents = &name[1..name.len() - 1];
                split_type_arguments(contents)
                    .into_iter()
                    .map(|argument| type_from_name(argument, aliases))
                    .collect::<Option<Vec<_>>>()?;
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
                .map(|argument| type_from_name(argument, aliases))
                .collect::<Option<Vec<_>>>()?;
            match (constructor, arguments.as_slice()) {
                ("Array", [_]) | ("List", [_]) => Some(Type::Array(stable_type_id(&keyed(
                    argument_names[0],
                    aliases,
                )))),
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
                _ => Some(Type::Generic(stable_type_id(name))),
            }
        }
    }
}

pub(crate) fn stable_type_id(name: &str) -> TypeId {
    name.bytes().fold(2_166_136_261u32, |hash, byte| {
        hash.wrapping_mul(16_777_619) ^ u32::from(byte)
    })
}

fn split_type_arguments(arguments: &str) -> Vec<&str> {
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
        | Statement::Yield { span, .. } => *span,
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
