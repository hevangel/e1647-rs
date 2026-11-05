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

/// One formal type parameter of a template declaration (clause 8.2):
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
    /// `type name: <scalar> [low..high];` — the range qualification of a
    /// numeric subtype declaration (clause 5.2.3.1), recorded only. The
    /// well-formedness check rejects subtypes of `longint`/`longuint`
    /// (E0207); the range does not narrow values yet.
    pub subtype_range: Option<(Expr, Expr)>,
    /// `template struct name of <params> { .. }` (clause 8): the formal
    /// type parameters, recorded. Instantiation (monomorphization) is a
    /// later milestone.
    pub template_params: Vec<TemplateParam>,
    /// `template extend name of <params> { .. };` (clause 8.6): a
    /// template extension declaration. Its members apply to the
    /// instances whose actuals comply with the parameters' types or
    /// categories (8.3's NOTE 3), substituted with the instance's
    /// actuals — elaboration applies them, not name composition.
    pub template_extend: bool,
    /// `package` visibility prefix recorded (clause 22); not enforced.
    pub packaged: bool,
    /// `struct derived like base { .. }` inheritance clause. For
    /// template declarations the base name; the full parameterized base
    /// type is in [`Declaration::like_type`].
    pub like: Option<String>,
    pub like_span: Option<Span>,
    /// `template struct t of <type> like container of <type> { .. }` —
    /// the parameterized base type, recorded (clause 8.5.2).
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
    /// A `cover <event> is { .. };` coverage group (clause 14.2) —
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

/// A subtype qualifier of a dot access (`p.red'good'x`, clause 4.17.3).
/// The same shape as a `when` block's [`SubtypeModifier`]: the resolution
/// matches a qualifier word against a synthesized when view's
/// discriminating (field id, value) pairs — a short word (`red`) may name
/// the pair's value (`red'color`) or, for a Boolean determinant, the pair's
/// field id (`good` for `TRUE'good`).
pub type WhenQualifier = SubtypeModifier;

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

/// A `cover <event> is { .. };` coverage group (clause 14.2). The
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
    /// `cover name is also { .. };` — a clause 14.6 extension of the
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
/// expression parses into the clause 12 grammar ([`TemporalExpr`]); the
/// `else` failure body parses as ordinary actions. Evaluation needs the
/// temporal engine — a later milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectDecl {
    /// `expect` or `assume` — simulation tools treat them alike; formal
    /// tools read them differently.
    pub kind: ExpectKind,
    /// The temporal expression.
    pub te: TemporalExpr,
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
    /// `repeat [do] { body } until cond;` — the do-while loop (clause
    /// 20.2.3): the body executes at least once, then repeatedly until
    /// `cond` is TRUE.
    RepeatUntil {
        body: Vec<Stmt>,
        cond: Expr,
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
    /// `wait @event;`, `wait {..} @clk;`, `wait [n]*@event;`,
    /// `wait until <te>;` — a temporal wait (clause 13.2.4; `until` is
    /// readability-only): suspends until the temporal expression
    /// succeeds (12.3.2) over the run's event log, under the wait
    /// budget.
    WaitTemporal {
        te: TemporalExpr,
        span: Span,
    },
    /// `sync;` / `sync <te>;` — a synchronization action (13.2.3): the
    /// bare form suspends for one occurrence of the default sampling
    /// event; the TE form behaves like the temporal wait.
    Sync {
        te: Option<TemporalExpr>,
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
            | Stmt::RepeatUntil { span, .. }
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
            | Stmt::WaitTemporal { span, .. }
            | Stmt::Sync { span, .. }
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
    /// Signed 64-bit (Table 17, clause 5.2.3.1). Not the same type as
    /// `int(bits: 64)` — the two differ in expression sizing and
    /// arithmetic rules (the T7.4 milestone; not modeled yet).
    Longint,
    /// Unsigned 64-bit (Table 17, clause 5.2.3.1). Not the same type as
    /// `uint(bits: 64)` — see [`ScalarBase::Longint`].
    Longuint,
    Bit,
    Byte,
    Nibble,
    Time,
    Bool,
    Real,
    String,
}

impl ScalarBase {
    /// The predefined scalar base named by a type word. `int`, `uint` and
    /// friends lex as keywords; `longint`/`longuint` are Table 17 names
    /// that lex as identifiers and reach here from the identifier branch
    /// of the type grammar (the reference grammar treats type names as
    /// identifiers, so they are deliberately not in the lexer's
    /// [`crate::lexer::KEYWORDS`]).
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Some(match keyword {
            "uint" => Self::Uint,
            "int" => Self::Int,
            "longint" => Self::Longint,
            "longuint" => Self::Longuint,
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
            Self::Longint => "longint",
            Self::Longuint => "longuint",
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
    /// A when-qualified type (`NO_CONSTRAINT env` — clause 6.7): the
    /// subtype variant of a base type. Instances of the field carry the
    /// subtype's synthesized view (its when members composed onto the
    /// base).
    WhenQualified {
        value: String,
        base: Box<TypeExpr>,
        span: Span,
    },
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
    /// `typeof(exp)` in a type position (clause 5.9): the static type of
    /// `exp` — the expression is never evaluated (purely compile-time;
    /// typical use is inside macros). Resolution against the current
    /// minimal type information happens during elaboration: a field path,
    /// a local `var`, a method parameter or a literal resolves; anything
    /// else is reported (`E0208`) and the placeholder stays.
    TypeOf { expr: Box<Expr>, span: Span },
    /// `typeof_item(exp)` in a type position (clause 5.9): the element
    /// type of a list-typed `exp`. A non-list argument is `E0209`.
    TypeOfItem { expr: Box<Expr>, span: Span },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Scalar { span, .. }
            | TypeExpr::List { span, .. }
            | TypeExpr::Named { span, .. }
            | TypeExpr::WhenQualified { span, .. }
            | TypeExpr::InlineEnum { span, .. }
            | TypeExpr::Formal { span, .. }
            | TypeExpr::TemplateRef { span, .. }
            | TypeExpr::TypeOf { span, .. }
            | TypeExpr::TypeOfItem { span, .. } => *span,
        }
    }

    /// A canonical textual form, used to name monomorphized template
    /// instances (8.3: "any two instances of a template, if their
    /// actual parameters refer to exactly the same types, are
    /// considered to be the same instance").
    pub fn render(&self) -> String {
        match self {
            TypeExpr::Scalar { base, width, .. } => match width.as_deref() {
                Some(Expr::IntLiteral { text, .. }) => {
                    format!("{}(bits: {})", base.name(), text)
                }
                _ => base.name().to_owned(),
            },
            TypeExpr::List { element, .. } => format!("list of {}", element.render()),
            TypeExpr::Named { name, .. } => name.clone(),
            TypeExpr::WhenQualified { value, base, .. } => {
                format!("{value} {}", base.render())
            }
            TypeExpr::InlineEnum { names, .. } => format!("[{}]", names.join(", ")),
            TypeExpr::Formal { name, .. } => format!("<{name}>"),
            TypeExpr::TemplateRef { name, args, .. } => template_instance_name(name, args),
            TypeExpr::TypeOf { expr, .. } => format!("typeof({})", render_expr_compact(expr)),
            TypeExpr::TypeOfItem { expr, .. } => {
                format!("typeof_item({})", render_expr_compact(expr))
            }
        }
    }
}

/// A compact source-like rendering of an expression, used only for
/// diagnostics and canonical type names (there is no general expression
/// printer in this milestone). Shapes beyond these render as `...`.
pub fn render_expr_compact(expr: &Expr) -> String {
    match expr {
        Expr::IntLiteral { text, .. } | Expr::RealLiteral { text, .. } => text.clone(),
        Expr::BitLiteral { text, .. } => text.clone(),
        Expr::BoolLiteral { value, .. } => {
            if *value {
                "TRUE".to_owned()
            } else {
                "FALSE".to_owned()
            }
        }
        Expr::StringLiteral { value, .. } => format!("\"{value}\""),
        Expr::Ident { name, .. } => name.clone(),
        Expr::MemberAccess { object, name, .. } => {
            format!("{}.{}", render_expr_compact(object), name)
        }
        Expr::Paren { expr, .. } => format!("({})", render_expr_compact(expr)),
        _ => "...".to_owned(),
    }
}

/// The canonical name of a template instance (8.3):
/// `name of (arg, arg)` with each argument rendered canonically.
pub fn template_instance_name(name: &str, args: &[TypeExpr]) -> String {
    let rendered: Vec<String> = args.iter().map(|arg| arg.render()).collect();
    format!("{name} of ({})", rendered.join(", "))
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
    /// `lhs list_ptr== rhs` — TRUE iff both operands denote the same list
    /// object (pointer identity, clause 4.11.4); the regular [`BinaryOp::Eq`]
    /// stays item-by-item. Both operands must be lists of the same type.
    ListPtrEq,
    /// `lhs !list_ptr= rhs` — the negation of [`BinaryOp::ListPtrEq`]
    /// (clause 4.11.4).
    ListPtrNe,
    IdentityEq,
    IdentityNe,
    Lt,
    Le,
    Gt,
    Ge,
    /// `lhs ~ pattern` — native string matching (clause 4.12), case
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
    /// (clause 6.3). The `with` block records field assignments for the
    /// fresh instance.
    New {
        ty: Option<TypeExpr>,
        inits: Vec<Stmt>,
        span: Span,
    },
    /// `select { <weight> : <choice>; .. }` — a weighted distribution
    /// expression (clause 10), typically the right side of a
    /// `keep soft <field> == select { .. }`. Weights are structural;
    /// every choice (`min`, `max`, `others`, `edges`,
    /// `normal(mean, stddev)`, or a value expression) is recorded as
    /// raw tokens — the sampler treats select constraints as no-ops
    /// until the distribution arc.
    Select {
        arms: Vec<SelectArm>,
        span: Span,
    },
    /// `expr.as_a(type)` cast.
    CastAs {
        expr: Box<Expr>,
        ty: TypeExpr,
        span: Span,
    },
    /// `set_of_values(<type>)` / `full_set_of_values(<type>)`
    /// (5.10.5/5.10.6) — the set of all legal (or possible) values of a
    /// numeric or enumeral type. The argument parses as a type, not an
    /// expression.
    SetOfValues {
        ty: TypeExpr,
        full: bool,
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
    /// `{ e1; e2; ... }` — the list-concat operator of the precedence
    /// table (clause 4.6, Table 12): a fresh list; each element
    /// contributes itself, and a list element contributes its items. The
    /// element type is not known at the literal (a variable's declared
    /// type names it at run time).
    ListConcat {
        items: Vec<Expr>,
        span: Span,
    },
    /// `expr is a ty` / `expr is not a ty` — the type check operator
    /// (clause 4.17.1), with the optional `(name)` binding of a true match
    /// to a local. Runtime type dispatch is exact-name only; there is
    /// no inheritance-aware type system yet.
    IsA {
        expr: Box<Expr>,
        ty: TypeExpr,
        binding: Option<String>,
        negated: bool,
        span: Span,
    },
    /// `(exp1, ..., expN) [not] in_table { row; row; ... }` (clause
    /// 4.11.8). TRUE iff some row matches the tuple element-wise — a
    /// scalar row element by `==`, a set row element by `in`; `not`
    /// negates. A tuple is an expression only in this construct, so the
    /// parser produces [`Expr::InTable`] atomically (never an
    /// intermediate tuple expression).
    InTable {
        tuple: Vec<Expr>,
        rows: Vec<Vec<TableElem>>,
        negated: bool,
        span: Span,
    },
    /// `evaluate [type] [(var)] { action; ... }` (clause 4.17.7) — an
    /// expression: the actions run, and the value of the named variable
    /// (`value` by default) after the block is the value. The variable
    /// is visible only inside the block and starts at its type's
    /// default. `type` given → that type; otherwise the type is
    /// inferred at run time from the context (the declared type of the
    /// assignment/`var` target or of the call's parameter); when
    /// neither exists the interpreter reports `E0122`. `return`/`wait`/
    /// `sync` inside are rejected at parse time (`E0123`).
    Evaluate {
        ty: Option<TypeExpr>,
        var: Option<String>,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `struct-exp.qual'qual'...'field` — a field access qualified by
    /// when-subtype words (clause 4.17.3). The qualifier sequence is
    /// legal iff the same words separated by spaces would form a legal
    /// when-subtype name; qualifiers already implied by the static type
    /// of the base expression may be omitted. Each qualifier is a value
    /// word (`red`) or a full `value'id` pair (`red'color`) — both
    /// spellings lex identically after the dot here, so every qualifier
    /// is recorded value-only and resolution matches a word against the
    /// synthesized when view's (field id, value) pairs, by value or (for
    /// a Boolean determinant's short form) by field id.
    QualifiedField {
        base: Box<Expr>,
        qualifiers: Vec<WhenQualifier>,
        field: String,
        field_span: Span,
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

/// One element of an `in_table` row (clause 4.11.8): a scalar expression
/// (matched against the tuple element by `==`) or a set (matched by
/// `in` — a set literal for enum/bool elements, any set expression for
/// numeric elements).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableElem {
    /// A scalar row element.
    Scalar(Expr),
    /// A set literal `[a, b]` / `[lo..hi]`.
    Set(InSetExpr),
    /// `set_of_values(T)` / `full_set_of_values(T)` (5.10.5/5.10.6).
    TypeSet { ty: TypeExpr, full: bool },
}

/// One weighted arm of a [`Expr::Select`] distribution: the weight
/// expression and the choice's raw tokens (clause 10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectArm {
    pub weight: Expr,
    pub choice: Vec<crate::lexer::Token>,
}

/// A temporal expression (clause 12): the grammar behind `expect`/
/// `assume` members. Structure only — sampling propagation (12.1.4) and
/// evaluation over paths (12.3) are the temporal engine, a later
/// milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalExpr {
    /// `@event` — the unary event operator (12.2.5); dotted path, the
    /// last segment may carry the HDL `$` suffix.
    EventRef { path: Vec<String> },
    /// `cycle` (12.2.3).
    Cycle,
    /// `true(exp)` — a proposition (12.2.4).
    True { expr: Expr },
    /// `change(exp)` / `rise(exp)` / `fall(exp)` edge samples (12.2.18).
    EdgeSample { routine: EdgeRoutine, expr: Expr },
    /// `{ a; b; .. }` sequence (12.2.9).
    Sequence { items: Vec<TemporalExpr> },
    /// `lhs => rhs` yield (12.2.14) — the expect-operator shape.
    Yield {
        lhs: Box<TemporalExpr>,
        rhs: Box<TemporalExpr>,
    },
    /// `a and b` (12.2.7).
    And(Box<TemporalExpr>, Box<TemporalExpr>),
    /// `a or b` (12.2.8).
    Or(Box<TemporalExpr>, Box<TemporalExpr>),
    /// `not te` (12.2.17).
    Not(Box<TemporalExpr>),
    /// `fail te` (12.2.13).
    Fail(Box<TemporalExpr>),
    /// `eventually te` (12.2.15).
    Eventually(Box<TemporalExpr>),
    /// `[n]*te`, `~[l..h]*te`, `[l..h]*te` — fixed, true-match and
    /// first-match repeats (12.2.10–12.2.12).
    Repeat {
        kind: RepeatKind,
        low: Option<Expr>,
        high: Option<Expr>,
        te: Box<TemporalExpr>,
    },
    /// `te @event` — the binary sampling operator (12.2.6); the sampling
    /// event applies to all sub-expressions.
    Sampled {
        te: Box<TemporalExpr>,
        event: Box<TemporalExpr>,
    },
    /// `( te )` — grouping.
    Group(Box<TemporalExpr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeRoutine {
    Change,
    Rise,
    Fall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatKind {
    /// `[n]` / `[n..n]` fixed repeat.
    Fixed,
    /// `~[l..h]` true-match repeat.
    TrueMatch,
    /// `[l..h]` first-match repeat (sequence items only).
    FirstMatch,
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
            | Expr::SetOfValues { span, .. }
            | Expr::BitSlice { span, .. }
            | Expr::InSet { span, .. }
            | Expr::ListConcat { span, .. }
            | Expr::IsA { span, .. }
            | Expr::InTable { span, .. }
            | Expr::Evaluate { span, .. }
            | Expr::QualifiedField { span, .. }
            | Expr::New { span, .. }
            | Expr::Select { span, .. }
            | Expr::HdlPath { span, .. } => *span,
        }
    }
}
