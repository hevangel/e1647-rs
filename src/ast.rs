//! Typed AST for the parsed subset of the e language.
//!
//! The subset covers `struct` / `unit` / `extend` / `type` declarations with
//! fields (single names and name groups), `like` inheritance clauses, `keep`
//! constraint expressions, enumeral type declarations, `when` subtype
//! members, and methods with parameter lists and statement bodies (including
//! `while`/`break`/`continue`). Temporal expressions, events and coverage
//! are later milestones and not representable here.

use crate::lexer::Position;

pub type Span = (Position, Position);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationKind {
    Struct,
    Unit,
    Extend,
    /// `type name: [A, B, C];` — its members are [`Member::Enumeral`].
    Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub name: String,
    pub name_span: Span,
    /// `struct derived like base { .. }` inheritance clause.
    pub like: Option<String>,
    pub like_span: Option<Span>,
    pub members: Vec<Member>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    Field(Field),
    Constraint(KeepConstraint),
    Method(MethodDecl),
    /// One enumerand of a `type` declaration.
    Enumeral(Enumeral),
    /// A `when <modifier> .. { members };` subtype member.
    When(WhenDecl),
}

/// A `when` subtype member: one or more `value'id` (or bare `id`) modifiers
/// followed by the members that apply under that subtype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhenDecl {
    pub modifiers: Vec<SubtypeModifier>,
    pub members: Vec<Member>,
    pub span: Span,
}

/// One subtype modifier: `BUSY'kind` (the conventional form) or a bare
/// identifier. The reference grammar's `struct_type_modifier`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtypeModifier {
    pub value: String,
    pub value_span: Span,
    /// The identifier after `'`, e.g. `kind`.
    pub id: Option<(String, Span)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeExpr,
}

/// A `keep <expr>;` or `keep soft <expr>;` constraint. The expression is
/// stored as written; solving it is a later milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepConstraint {
    pub expr: Expr,
    pub is_soft: bool,
    pub span: Span,
}

/// A method member: `name(params): return_type is [also|first|only] { .. };`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodDecl {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    /// `is also` / `is first` / `is only` extension modifier.
    pub modifier: Option<MethodModifier>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodModifier {
    Also,
    First,
    Only,
}

/// One enumerand of a `type` declaration, with its optional explicit value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enumeral {
    pub name: String,
    pub name_span: Span,
    pub value: Option<Expr>,
}

/// A statement in a method body. The subset covers local variable
/// declarations, assignments, expression statements (including method
/// calls), `return`, braced `if`/`else`, `while` loops with `break`/
/// `continue`, `print` and `gen`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    VarDecl {
        name: String,
        name_span: Span,
        ty: TypeExpr,
        init: Option<Expr>,
        span: Span,
    },
    Assign {
        target: Expr,
        value: Expr,
        span: Span,
    },
    ExprStmt {
        expr: Expr,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
        span: Span,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Print {
        exprs: Vec<Expr>,
        span: Span,
    },
    Gen {
        expr: Expr,
        span: Span,
    },
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::VarDecl { span, .. }
            | Stmt::Assign { span, .. }
            | Stmt::ExprStmt { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::If { span, .. }
            | Stmt::While { span, .. }
            | Stmt::Break { span }
            | Stmt::Continue { span }
            | Stmt::Print { span, .. }
            | Stmt::Gen { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarBase {
    Uint,
    Int,
    Bit,
    Byte,
    Nibble,
    Time,
    Bool,
    Real,
    String,
}

impl ScalarBase {
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Some(match keyword {
            "uint" => Self::Uint,
            "int" => Self::Int,
            "bit" => Self::Bit,
            "byte" => Self::Byte,
            "nibble" => Self::Nibble,
            "time" => Self::Time,
            "bool" => Self::Bool,
            "real" => Self::Real,
            "string" => Self::String,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Uint => "uint",
            Self::Int => "int",
            Self::Bit => "bit",
            Self::Byte => "byte",
            Self::Nibble => "nibble",
            Self::Time => "time",
            Self::Bool => "bool",
            Self::Real => "real",
            Self::String => "string",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    /// A scalar base type, optionally with a `uint(bits: N)`-style width.
    Scalar {
        base: ScalarBase,
        width: Option<Box<Expr>>,
        span: Span,
    },
    /// `list of <element>`.
    List { element: Box<TypeExpr>, span: Span },
    /// A user-defined type name.
    Named { name: String, span: Span },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Scalar { span, .. } | TypeExpr::List { span, .. } => *span,
            TypeExpr::Named { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    LogicalOr,
    LogicalAnd,
    BitOr,
    BitXor,
    BitAnd,
    Eq,
    NotEq,
    IdentityEq,
    IdentityNe,
    Lt,
    Le,
    Gt,
    Ge,
    ShiftLeft,
    ShiftRight,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Negate,
    Plus,
    BitNot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    IntLiteral {
        text: String,
        span: Span,
    },
    RealLiteral {
        text: String,
        span: Span,
    },
    /// Sized/unsized based literal such as `8'b1010` or `'hFF` (raw text).
    BitLiteral {
        text: String,
        span: Span,
    },
    BoolLiteral {
        value: bool,
        span: Span,
    },
    StringLiteral {
        value: String,
        span: Span,
    },
    Ident {
        name: String,
        span: Span,
    },
    MemberAccess {
        object: Box<Expr>,
        name: String,
        name_span: Span,
        span: Span,
    },
    /// `callee(args)` — the callee is a member access or bare identifier.
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    /// `base[index]` list indexing.
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Paren {
        expr: Box<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::IntLiteral { span, .. }
            | Expr::RealLiteral { span, .. }
            | Expr::BitLiteral { span, .. }
            | Expr::BoolLiteral { span, .. }
            | Expr::StringLiteral { span, .. }
            | Expr::Ident { span, .. }
            | Expr::MemberAccess { span, .. }
            | Expr::Call { span, .. }
            | Expr::Index { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Paren { span, .. } => *span,
        }
    }
}
