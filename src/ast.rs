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

/// One formal type parameter of a template declaration (clause 8.1):
/// `<[tag']type-category>` with an optional default (`=<type>`), the
/// default only for trailing parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateParam {
    /// The raw parameter text between the angle brackets (`type`,
    /// `key'type`).
    pub name: String,
    pub name_span: Span,
    pub default: Option<TypeExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub name: String,
    pub name_span: Span,
    /// `type name: <type>;` — a scalar/list type declaration (clause 5):
    /// the underlying representation, recorded only. Distinct-type
    /// semantics over it are a later milestone.
    pub underlying: Option<TypeExpr>,
    /// `template struct name of <params> { .. }` (clause 8): the formal
    /// type parameters, recorded. Instantiation (monomorphization) is a
    /// later milestone.
    pub template_params: Vec<TemplateParam>,
    /// `package` visibility prefix recorded (clause 22); not enforced.
    pub packaged: bool,
    /// `struct derived like base { .. }` inheritance clause. For
    /// template declarations the base name; the full parameterized base
    /// type is in [`Declaration::like_type`].
    pub like: Option<String>,
    pub like_span: Option<Span>,
    /// `template struct t of <type> like container of <type> { .. }` —
    /// the parameterized base type, recorded (clause 8.1.3).
    pub like_type: Option<TypeExpr>,
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
    /// A `cover <event> is { .. };` coverage group (clause 14.1) —
    /// recorded, not collected: the coverage data model is a later
    /// milestone.
    Cover(CoverGroupDecl),
    /// An `expect`/`assume` temporal property (clause 11.4) — recorded
    /// uninterpreted: the temporal-expression grammar (clause 12) and
    /// the evaluation engine are later milestones.
    Expect(ExpectDecl),
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
    /// `define <name['category]> "match expression" as [computed] { body };`
    /// — a macro definition (clause 15.2/15.3). Recorded uninterpreted:
    /// the match-expression grammar (clause 15.4) and expansion are later
    /// milestones.
    Macro(MacroDecl),
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
    /// `package name: ...;` / `private name: ...;` visibility prefix
    /// (clause 22), recorded only.
    pub visibility: Option<String>,
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
    /// `name: type`; a bare `name` parameter (clause 17) has none and
    /// accepts anything.
    pub ty: Option<TypeExpr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodModifier {
    Also,
    First,
    Only,
}

/// A `define <name['category]> "match expression" as [computed] { body };`
/// macro definition (clause 15.2/15.3). The header's category is the
/// non-terminal type the macro returns (`exp`, `any`, ...); the match
/// expression and body are recorded as written — the body's raw tokens
/// are kept because simple macros expand them as a template and computed
/// macros run them as a parse-time action block, both later milestones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroDecl {
    pub name: String,
    pub name_span: Span,
    pub category: Option<String>,
    /// The match expression string as written (escapes preserved).
    pub pattern: String,
    /// `define ... as computed { ... };`
    pub computed: bool,
    /// The braced body's raw tokens, without the enclosing braces.
    pub body: Vec<crate::lexer::Token>,
    pub span: Span,
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

/// A `cover <event> is { .. };` coverage group (clause 14.1). The
/// sampling event is recorded; items, crosses and transitions are
/// recorded structurally, and every option clause (`using ...`) is kept
/// as raw tokens — the option semantics (weights, at_least, per_instance)
/// belong to the coverage data model, a later milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverGroupDecl {
    pub name: String,
    pub name_span: Span,
    /// The sampling event expression as written.
    pub event: Expr,
    /// `cover name is empty;` — extendable empty group.
    pub empty: bool,
    /// `cover name is also { .. };` — a clause 14.5 extension of the
    /// group defined for the same event.
    pub is_also: bool,
    pub items: Vec<CoverItemDecl>,
    /// Group-level option entries that are not items, recorded as raw
    /// tokens (one entry per statement).
    pub options: Vec<Vec<crate::lexer::Token>>,
    pub span: Span,
}

/// One `item` / `transition` / `cross` entry of a coverage group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverItemDecl {
    /// `item` / `transition` / `cross`.
    pub kind: CoverItemKind,
    /// The item name; a cross carries the crossed names here.
    pub names: Vec<(String, Span)>,
    /// The optional declared type (`item name: uint (bits: 16) = ...`).
    pub ty: Option<TypeExpr>,
    /// The sampled expression (`= cur_trans.address`).
    pub expr: Option<Expr>,
    /// The `using ...` option clause as raw tokens, if present.
    pub using: Vec<crate::lexer::Token>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverItemKind {
    Item,
    Cross,
    Transition,
}

/// An `expect`/`assume` struct member (clause 11.4): the temporal
/// expression is recorded as raw tokens (clause 12's grammar is a later
/// milestone), and the `else` failure body parses as ordinary actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectDecl {
    /// `expect` or `assume` — simulation tools treat them alike; formal
    /// tools read them differently.
    pub kind: ExpectKind,
    /// The temporal expression as written, without the leading keyword
    /// and without the `else` clause.
    pub te: Vec<crate::lexer::Token>,
    /// The `else` failure body (usually a `dut_error` call); empty when
    /// absent — the default failure reports the rule name.
    pub else_body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectKind {
    Expect,
    Assume,
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
        /// `var x: t ...` declares the type; `var x := e ...` infers it
        /// (clause 18) — `None` with a mandatory initializer.
        ty: Option<TypeExpr>,
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
    /// `for each (m) in list do { .. };` binds `m` instead;
    /// `for each <type> (m) in list` records the element type filter;
    /// `for each (m) using index (idx) in list` also binds `idx` to the
    /// element position, with the `using` expression recorded; `in
    /// reverse list` iterates in reverse order.
    ForEach {
        var: Option<String>,
        type_filter: Option<String>,
        index_var: Option<String>,
        using: Option<Expr>,
        reversed: bool,
        list: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `for i from a to b { .. };` — inclusive bounds; `down to` counts
    /// down (`down: true`).
    ForTo {
        var: String,
        from: Expr,
        to: Expr,
        down: bool,
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
    /// `compute <call>;` — calls a TCM and runs it to completion before
    /// continuing (clause 13). Without TCM suspension, execution is the
    /// same inline call as `start`.
    Compute {
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
            | Stmt::Compute { span, .. }
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
    /// `list of <element>`, with the optional keyed-list key expression
    /// (`list (key:it) of <type>`), recorded.
    List {
        element: Box<TypeExpr>,
        key: Option<Box<Expr>>,
        span: Span,
    },
    /// A user-defined type name.
    Named { name: String, span: Span },
    /// An inline enumeral list type: `[imm, reg]`.
    InlineEnum { names: Vec<String>, span: Span },
    /// A template's formal parameter used in a type position
    /// (`<type>`, `<key'type>`) — the raw text between the angle
    /// brackets. Bound during template instantiation, a later milestone.
    Formal { name: String, span: Span },
    /// A template instantiation in a type position (`deque of int`,
    /// `map of (key_t, value_t)`).
    TemplateRef {
        name: String,
        args: Vec<TypeExpr>,
        span: Span,
    },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Scalar { span, .. }
            | TypeExpr::List { span, .. }
            | TypeExpr::Named { span, .. }
            | TypeExpr::InlineEnum { span, .. }
            | TypeExpr::Formal { span, .. }
            | TypeExpr::TemplateRef { span, .. } => *span,
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
    /// `new [<type>] [with { <assignments> }]` — instance creation
    /// (clause 6.2). The `with` block records field assignments for the
    /// fresh instance.
    New {
        ty: Option<TypeExpr>,
        inits: Vec<Stmt>,
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
    /// (clause 4.5), with the optional `(name)` binding of a true match
    /// to a local. Runtime type dispatch is exact-name only; there is
    /// no inheritance-aware type system yet.
    IsA {
        expr: Box<Expr>,
        ty: TypeExpr,
        binding: Option<String>,
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
            | Expr::New { span, .. }
            | Expr::HdlPath { span, .. } => *span,
        }
    }
}
