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
    /// When-modifier prefixes of the declared/extended type
    /// (`extend imm cpu_instr_s`, `extend ACTIVE TX uart_agent_u`) —
    /// `value'id` pairs flattened to their modifier text. When-scoping of
    /// extensions is not modeled yet; the modifiers are recorded only.
    pub modifiers: Vec<String>,
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
    /// A `simple_port` member.
    Port(PortDecl),
    /// An `event` member.
    Event(EventDecl),
    /// An `on <event-path> { actions };` struct member (clause 11.2).
    On(OnDecl),
    /// `import a, b;` — recorded; no cross-file resolution yet.
    Import {
        names: Vec<String>,
        span: Span,
    },
    /// `define name value;` — a compile-time constant; the name resolves
    /// to the value's literal in the interpreter.
    Define {
        name: String,
        value: Option<Expr>,
        span: Span,
    },
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

/// Port direction of a [`PortDecl`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    In,
    Out,
    Inout,
}

/// An `event` member: `event name;` (explicitly emitted) or
/// `event name is <trigger>;` (e.g. `is fall(clk$) @sim`,
/// `is cycle @p_smp.clock_f`, `is @sys.any`). The trigger is stored as
/// written; edge scheduling is a later milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDecl {
    pub name: String,
    pub name_span: Span,
    pub trigger: Option<EventTrigger>,
    pub span: Span,
}

/// The `is …` trigger of an [`EventDecl`]: an optional `only`/`unexp`
/// modifier, the `cycle` keyword and one or more sampled items joined by
/// `or`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventTrigger {
    /// `only` / `unexp` modifier as written, if present.
    pub modifier: Option<String>,
    /// `cycle` keyword before the first item, if present.
    pub cycle: bool,
    pub items: Vec<EventTriggerItem>,
    /// A Boolean combination guarding the whole trigger (`is
    /// (fall(a) and true(b)) @clk`) — the joined sample conditions are
    /// stored in the items, the combination operator here.
    pub group_op: Option<String>,
}

/// One sampled item of an [`EventTrigger`]: `@p_smp.clk` on its own or a
/// `change(cond) @sim`-style edge sample. Routine names (`rise`, `fall`,
/// `change`, `true`) are validated by the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventTriggerItem {
    /// `rise` / `fall` / `change` / `true` for edge samples, absent for
    /// plain event attachments.
    pub routine: Option<String>,
    /// Dotted sampled path after `@`; the last segment may carry the HDL
    /// `$` suffix (`@off_p$`).
    pub path: Vec<String>,
    /// `rise(cond)`-style condition guarding this item, if any.
    pub condition: Option<Expr>,
}

/// A `simple_port` member: `name: inout simple_port of bit is instance;`.
/// The HDL location comes from a `keep name.hdl_path() == "path"`
/// constraint and is resolved during elaboration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDecl {
    pub name: String,
    pub name_span: Span,
    pub direction: PortDirection,
    pub element: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeExpr,
    /// `%name` physical field (packing-relevant).
    pub physical: bool,
    /// `!name` do-not-generate field (skipped by `gen`).
    pub no_gen: bool,
}

/// A `keep <expr>;` or `keep soft <expr>;` constraint. The expression is
/// stored as written; solving it is a later milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepConstraint {
    pub expr: Expr,
    pub is_soft: bool,
    /// `keep bind (port, hdl_kind);` — hierarchical HDL binding recorded
    /// as a `bind(a, b)` call. Skipped by identifier checking and by `gen`.
    pub is_bind: bool,
    pub span: Span,
}

/// A method member: `name(params): return_type is [also|first|only] { .. };`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodDecl {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    /// `@event` suffix of a time-consuming method (TCM); the event
    /// expression as written (e.g. `@sys.any`, `@driver.clock`).
    pub tcm_event: Option<Expr>,
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

/// An `on <event-path> { actions };` struct member (clause 11.2): the
/// actions run when the named event fires, in written order. Extending an
/// `on` member is a repeated declaration (the `is also` effect), so the
/// composed list is ordered. The clause's restriction — no
/// time-consuming actions or TCMs in the block — is not enforced here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnDecl {
    /// The dotted event path (`reset_done`, `driver.clock`).
    pub path: Vec<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
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
    /// `stop_run();` — ends the simulation run.
    StopRun {
        span: Span,
    },
    /// `do <item-field>;` — execute one sequence item inside a sequence
    /// body (`do trans keeping { … };` records the constraint block).
    /// Scheduling the item through the sequencer handshake is a later
    /// milestone; execution currently generates the item in place.
    SeqDo {
        item: String,
        span: Span,
    },
    /// `check that <cond> else { .. }` — on a false condition the else
    /// body runs (usually a `dut_error` call) and the DUT-error counter
    /// increments; the run itself continues.
    Check {
        condition: Expr,
        else_body: Vec<Stmt>,
        span: Span,
    },
    /// `for each in list do { .. };` binds `it` to each element;
    /// `for each (m) in list do { .. };` binds `m` instead.
    ForEach {
        var: Option<String>,
        list: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `for i from a to b { .. };` — inclusive bounds.
    ForTo {
        var: String,
        from: Expr,
        to: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `wait cycle;` / `wait [n];` / `wait [n] * cycle;` — advances
    /// simulation time by the evaluated cycle count.
    Wait {
        cycles: Expr,
        span: Span,
    },
    /// `emit <target>;` — fires an event.
    Emit {
        target: Expr,
        span: Span,
    },
    /// `start <call>;` — starts a TCM. Executed inline (synchronously)
    /// with a shared cycle budget in this milestone; event-triggered
    /// suspension is a later milestone.
    Start {
        call: Expr,
        span: Span,
    },
    /// `case expr { val: {..}; [a, b]: {..}; default: {..}; };`
    Case {
        scrutinee: Expr,
        /// Each arm carries its alternative expressions (one or more).
        arms: Vec<(Vec<Expr>, Vec<Stmt>)>,
        default: Option<Vec<Stmt>>,
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
            | Stmt::Gen { span, .. }
            | Stmt::StopRun { span }
            | Stmt::SeqDo { span, .. }
            | Stmt::Check { span, .. }
            | Stmt::ForEach { span, .. }
            | Stmt::ForTo { span, .. }
            | Stmt::Wait { span, .. }
            | Stmt::Emit { span, .. }
            | Stmt::Start { span, .. }
            | Stmt::Case { span, .. } => *span,
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
    /// An inline enumeral list type: `[imm, reg]`.
    InlineEnum { names: Vec<String>, span: Span },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Scalar { span, .. } | TypeExpr::List { span, .. } => *span,
            TypeExpr::Named { span, .. } => *span,
            TypeExpr::InlineEnum { span, .. } => *span,
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
    /// `lhs ~ pattern` — native string matching (clause 4.11), case
    /// insensitive over the full string.
    Match,
    /// `lhs !~ pattern` — the negation of [`BinaryOp::Match`].
    NotMatch,
    ShiftLeft,
    ShiftRight,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    /// `=>` constraint implication (`a => b` is `!a || b`).
    Imply,
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
    /// `cond ? then_expr : else_expr`
    Ternary {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        span: Span,
    },
    /// A quoted hierarchical HDL path (`'memory_tb.address'`) — read or
    /// assigned through the simulator bridge ([`crate::hdl`]).
    HdlPath {
        path: String,
        span: Span,
    },
    /// `expr.as_a(type)` cast.
    CastAs {
        expr: Box<Expr>,
        ty: TypeExpr,
        span: Span,
    },
    /// Bit slice `base[high:low]` (integer extraction).
    BitSlice {
        base: Box<Expr>,
        high: Box<Expr>,
        low: Box<Expr>,
        span: Span,
    },
    /// `element in [a, b]` / `element in [low .. high]` membership.
    InSet {
        element: Box<Expr>,
        set: InSetExpr,
        span: Span,
    },
    /// `expr is a ty` / `expr is not a ty` — the type check operator
    /// (clause 4.5). Runtime type dispatch is exact-name only; there is no
    /// inheritance-aware type system yet.
    IsA {
        expr: Box<Expr>,
        ty: TypeExpr,
        negated: bool,
        span: Span,
    },
}

/// The right-hand side of an `in` membership expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InSetExpr {
    /// `[a, b, c]` — any match.
    Items(Vec<Expr>),
    /// `[low .. high]` — inclusive numeric range.
    Range(Box<Expr>, Box<Expr>),
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
            | Expr::Paren { span, .. }
            | Expr::Ternary { span, .. }
            | Expr::CastAs { span, .. }
            | Expr::BitSlice { span, .. }
            | Expr::InSet { span, .. }
            | Expr::IsA { span, .. }
            | Expr::HdlPath { span, .. } => *span,
        }
    }
}
