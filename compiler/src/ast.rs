use crate::source::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub annotation: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub annotation: String,
    pub value: Option<Expression>,
    pub is_const: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub name: Option<String>,
    pub type_parameters: Vec<String>,
    pub constraints: Vec<GenericConstraint>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<String>,
    pub body: Option<Block>,
    pub is_async: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenericConstraint {
    pub parameter: String,
    pub bounds: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclarationKind {
    Struct,
    Class,
    Actor,
    Interface,
    Trait,
    Enum,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeDeclaration {
    pub kind: DeclarationKind,
    pub name: String,
    pub type_parameters: Vec<String>,
    pub constraints: Vec<GenericConstraint>,
    pub constructor_parameters: Vec<Parameter>,
    pub fields: Vec<Field>,
    pub methods: Vec<Function>,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeAlias {
    pub name: String,
    pub type_parameters: Vec<String>,
    pub target: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImplBlock {
    pub trait_name: Option<String>,
    pub target: String,
    pub methods: Vec<Function>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub payload: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatchClause {
    pub name: String,
    pub annotation: Option<String>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub guard: Option<Expression>,
    pub body: MatchBody,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MatchBody {
    Expression(Expression),
    Block(Block),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatternKind {
    Wildcard,
    Literal(Box<Expression>),
    Binding(String),
    Tuple(Vec<Pattern>),
    Array(Vec<Pattern>),
    Variant {
        name: String,
        fields: Vec<Pattern>,
    },
    Struct {
        name: String,
        fields: Vec<(String, Pattern)>,
    },
    Or(Vec<Pattern>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Let {
        name: String,
        annotation: Option<String>,
        is_const: bool,
        is_mut: bool,
        value: Option<Expression>,
        span: Span,
    },
    Return {
        value: Option<Expression>,
        span: Span,
    },
    Expression(Expression),
    Block(Block),
    Function(Function),
    TypeDeclaration(TypeDeclaration),
    TypeAlias(TypeAlias),
    ImplBlock(ImplBlock),
    Module {
        path: Vec<String>,
        span: Span,
    },
    Import {
        path: Vec<String>,
        alias: Option<String>,
        selected: Vec<String>,
        span: Span,
    },
    Export(Box<Statement>),
    If {
        condition: Expression,
        then_block: Block,
        else_branch: Option<Box<Statement>>,
        span: Span,
    },
    For {
        binding: String,
        iterable: Expression,
        body: Block,
        span: Span,
    },
    While {
        condition: Expression,
        body: Block,
        span: Span,
    },
    Loop {
        body: Block,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    Match {
        value: Expression,
        cases: Vec<MatchCase>,
        span: Span,
    },
    Try {
        body: Block,
        catches: Vec<CatchClause>,
        finally: Option<Block>,
        span: Span,
    },
    Throw {
        value: Expression,
        span: Span,
    },
    Yield {
        value: Option<Expression>,
        span: Span,
    },
    Unsafe(Block),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StringPart {
    Text(String),
    Expression(Expression),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExpressionKind {
    Integer(i64),
    Float(f64),
    String(String),
    Character(char),
    InterpolatedString(Vec<StringPart>),
    Boolean(bool),
    Null,
    Name(String),
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
    Assignment {
        target: Box<Expression>,
        operator: AssignmentOperator,
        value: Box<Expression>,
    },
    Conditional {
        condition: Box<Expression>,
        then_value: Box<Expression>,
        else_value: Box<Expression>,
    },
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    Member {
        target: Box<Expression>,
        name: String,
        null_safe: bool,
    },
    Index {
        target: Box<Expression>,
        index: Box<Expression>,
    },
    Array(Vec<Expression>),
    Tuple(Vec<Expression>),
    Set(Vec<Expression>),
    Map(Vec<(Expression, Expression)>),
    Function(Function),
    AsyncBlock(Block),
    Await(Box<Expression>),
    Spawn(Box<Expression>),
    Match {
        value: Box<Expression>,
        cases: Vec<MatchCase>,
    },
    Object {
        type_name: String,
        fields: Vec<(String, Expression)>,
    },
    New {
        type_name: String,
        arguments: Vec<Expression>,
    },
    Channel {
        element_type: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOperator {
    Negate,
    Not,
    Reference,
    Borrow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignmentOperator {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    In,
    Is,
    Range,
    ShiftLeft,
    ShiftRight,
    BitAnd,
    BitOr,
    BitXor,
    And,
    Or,
}
