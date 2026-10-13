use std::collections::HashMap;

use crate::ast::{
    BinaryOperator, Expression, ExpressionKind, MatchBody, Pattern, PatternKind, Program,
    Statement, UnaryOperator,
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
    let mut lowerer = Lowerer {
        functions: Vec::new(),
        signatures: hir.functions.clone(),
        instructions: Vec::new(),
        values: HashMap::new(),
        mutability: HashMap::new(),
        scope_stack: vec![HashMap::new()],
        mutability_stack: vec![HashMap::new()],
        next_value: 0,
        in_function: false,
        current_function_return_type: None,
        in_loop: false,
        function_has_return: false,
        source_name: hir.source_name.clone(),
        type_names: hir.type_names.iter().cloned().collect(),
        value_types: HashMap::new(),
    };

    for statement in &hir.program.statements {
        lowerer.lower_statement(statement)?;
    }

    let (type_definitions, imports, exports) = module_declarations(&hir.program);
    let globals = collect_globals(&hir.program, &lowerer);
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
            &hir.source_name,
        ));
    } else if functions.is_empty() {
        functions.push(create_function(
            "main".to_owned(),
            Vec::new(),
            Type::Void,
            Vec::new(),
            Span::default(),
            &hir.source_name,
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
            source_name: hir.source_name.clone(),
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

    let basic_blocks = builder.finish();
    let mut function = Function {
        values: Vec::new(),
        concurrency: Vec::new(),
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
    instructions
        .iter()
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
    instructions
        .iter()
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
                                type_from_name(&field.annotation).unwrap_or_else(|| {
                                    Type::Generic(stable_type_id(&field.annotation))
                                }),
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
                                        type_from_name(ty)
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
                    type_from_name(&alias.target)
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
                    .and_then(|annotation| type_from_name(annotation))
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
}

impl CfgBuilder {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            is_terminated: Vec::new(),
            loop_targets: Vec::new(),
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
    for block in &function.basic_blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
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
        for (index, instruction) in block.instructions.iter().enumerate() {
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
                    block.instructions.len(),
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
                        block.instructions.len(),
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
        for instruction in &block.instructions {
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
                    if left_ty != right_ty && left_ty != Type::Unknown && right_ty != Type::Unknown
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
                _ => {}
            }
        }
    }
    Ok(())
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
        _ => Vec::new(),
    }
}

fn instruction_operands(instruction: &Instruction) -> Vec<ValueId> {
    match instruction {
        Instruction::Store { value, .. } | Instruction::Assign { value, .. } => vec![*value],
        Instruction::Unary { operand, .. } => vec![*operand],
        Instruction::Binary { left, right, .. } => vec![*left, *right],
        Instruction::Call { arguments, .. } => arguments.clone(),
        Instruction::IteratorNext { iterable, .. } => vec![*iterable],
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
        | Instruction::Return { span, .. } => *span,
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
    function_has_return: bool,
    source_name: String,
    type_names: std::collections::HashSet<String>,
    value_types: HashMap<ValueId, Type>,
}

impl Lowerer {
    fn lower_statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::TypeDeclaration(_) | Statement::TypeAlias(_) | Statement::Import { .. } => {
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
                    if actual != Type::Unknown && actual != expected {
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
                            if actual != Type::Unknown && actual != expected {
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
                self.in_loop = true;
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
                let condition_instructions = vec![
                    Instruction::IteratorNext {
                        has_next,
                        item,
                        iterable: iterable_id,
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
                self.in_loop = true;
                let saved_instructions = std::mem::take(&mut self.instructions);
                for statement in &body.statements {
                    self.lower_statement(statement)?;
                }
                let loop_body = std::mem::take(&mut self.instructions);
                self.instructions = saved_instructions;
                self.in_loop = saved_in_loop;
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
                self.in_loop = true;
                let saved_instructions = std::mem::take(&mut self.instructions);
                for statement in &body.statements {
                    self.lower_statement(statement)?;
                }
                let loop_body = std::mem::take(&mut self.instructions);
                self.instructions = saved_instructions;
                self.in_loop = saved_in_loop;
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

                for case in cases {
                    self.validate_pattern_support(&case.pattern)?;
                    self.push_scope();
                    self.bind_pattern_to_value(&case.pattern, value_id)?;

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
                    }

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
                    self.pop_scope();
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
                self.instructions
                    .push(Instruction::Continue { span: *span });
                Ok(())
            }
            _ => Err(Diagnostic::error(
                "NXL2007",
                "this language construct is parsed but is not lowered yet",
                statement_span(statement),
            )),
        }
    }

    fn bind_pattern_to_value(
        &mut self,
        pattern: &Pattern,
        value_id: ValueId,
    ) -> Result<(), Diagnostic> {
        match &pattern.kind {
            PatternKind::Binding(name) => {
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
                Ok(())
            }
            PatternKind::Or(alternatives) => {
                for alternative in alternatives {
                    self.bind_pattern_to_value(alternative, value_id)?;
                }
                Ok(())
            }
            PatternKind::Wildcard | PatternKind::Literal(_) => Ok(()),
            _ => Err(Diagnostic::error(
                "NXL2007",
                format!(
                    "pattern kind {:?} is parsed but not yet supported by the lowered semantic subset",
                    pattern.kind
                ),
                pattern.span,
            )),
        }
    }

    fn validate_pattern_support(&self, pattern: &Pattern) -> Result<(), Diagnostic> {
        match &pattern.kind {
            PatternKind::Wildcard | PatternKind::Binding(_) | PatternKind::Literal(_) => Ok(()),
            PatternKind::Or(alternatives) => {
                for alternative in alternatives {
                    self.validate_pattern_support(alternative)?;
                }
                Ok(())
            }
            _ => Err(Diagnostic::error(
                "NXL2007",
                format!(
                    "pattern kind {:?} is parsed but not yet supported by the lowered semantic subset",
                    pattern.kind
                ),
                pattern.span,
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
        type_from_name(annotation).or_else(|| {
            self.type_names
                .contains(annotation)
                .then(|| Type::Generic(stable_type_id(annotation)))
        })
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
        self.mutability_stack.pop();
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
            ExpressionKind::Boolean(value) => {
                Ok(self.constant(Constant::Boolean(*value), Type::Bool, expression.span))
            }
            ExpressionKind::Name(name) => {
                let value = self.lookup_value(name).ok_or_else(|| {
                    Diagnostic::error(
                        "NXL2002",
                        format!("use of undeclared name '{name}'"),
                        expression.span,
                    )
                })?;
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
                    UnaryOperator::Negate if matches!(operand_type, Type::Int | Type::Float) => {
                        operand_type
                    }
                    UnaryOperator::Not if operand_type == Type::Bool => Type::Bool,
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
                let ty = match operator {
                    BinaryOperator::Add
                    | BinaryOperator::Subtract
                    | BinaryOperator::Multiply
                    | BinaryOperator::Divide
                        if matches!(left_type, Type::Int | Type::Float) =>
                    {
                        left_type
                    }
                    BinaryOperator::Equal | BinaryOperator::NotEqual
                        if left_type != Type::Unknown =>
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
                        if left_type == Type::Bool && right_type == Type::Bool =>
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
            ExpressionKind::Call { callee, arguments } => {
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

                // Check for user-defined function signature
                let (param_types, return_type) = {
                    let signature = self
                        .signatures
                        .iter()
                        .find(|sig| sig.name == *name)
                        .ok_or_else(|| {
                            Diagnostic::error(
                                "NXL2002",
                                format!("use of undeclared function '{name}'"),
                                callee.span,
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
                            "function '{name}' expects {} argument(s), found {}",
                            param_types.len(),
                            arguments.len()
                        ),
                        expression.span,
                    ));
                }

                let mut lowered_arguments = Vec::with_capacity(arguments.len());
                for (arg, param_type) in arguments.iter().zip(param_types.iter()) {
                    let arg_value = self.expression(arg)?;
                    let arg_type = self.value_type(arg_value);

                    // Validate argument type compatibility
                    if *param_type != Type::Unknown
                        && arg_type != Type::Unknown
                        && *param_type != arg_type
                    {
                        return Err(Diagnostic::error(
                            "NXL2001",
                            format!(
                                "argument to '{name}' has type {arg_type:?}, expected {param_type:?}"
                            ),
                            arg.span,
                        ));
                    }
                    lowered_arguments.push(arg_value);
                }

                let result = self.allocate();
                let instruction = Instruction::Call {
                    result,
                    callee: name.clone(),
                    arguments: lowered_arguments,
                    return_type,
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
                let ExpressionKind::Name(name) = &target.kind else {
                    return Err(Diagnostic::error(
                        "NXL2008",
                        "assignment target must be a variable name",
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

pub(crate) fn type_from_name(name: &str) -> Option<Type> {
    match name {
        "Int" => Some(Type::Int),
        "Float" => Some(Type::Float),
        "Bool" => Some(Type::Bool),
        "String" => Some(Type::String),
        "Void" => Some(Type::Void),
        _ => {
            if let Some(inner) = name.strip_prefix('&') {
                let (mutable, target) = inner
                    .strip_prefix("mut ")
                    .map_or((false, inner), |target| (true, target));
                type_from_name(target)?;
                return Some(Type::Reference(stable_type_id(target), mutable));
            }
            if name.starts_with('[') && name.ends_with(']') {
                let inner = &name[1..name.len() - 1];
                type_from_name(inner)?;
                return Some(Type::Array(stable_type_id(inner)));
            }
            if name.starts_with('(') && name.ends_with(')') {
                let contents = &name[1..name.len() - 1];
                split_type_arguments(contents)
                    .into_iter()
                    .map(type_from_name)
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
                .map(|argument| type_from_name(argument))
                .collect::<Option<Vec<_>>>()?;
            match (constructor, arguments.as_slice()) {
                ("Array", [_]) | ("List", [_]) => {
                    Some(Type::Array(stable_type_id(argument_names[0])))
                }
                ("Option", [_]) => Some(Type::Optional(stable_type_id(argument_names[0]))),
                ("Task", [_]) => Some(Type::Task(stable_type_id(argument_names[0]))),
                ("Channel", [_]) => Some(Type::Channel(stable_type_id(argument_names[0]))),
                ("Result", [_, _]) => Some(Type::Result(
                    stable_type_id(argument_names[0]),
                    stable_type_id(argument_names[1]),
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
