//! An interpreter for the executable subset of the parsed language.
//!
//! Entry point: [`run`]. It instantiates the built-in `sys` unit and
//! executes every composed `run()` method on it, in composition order. A
//! file without `sys` (or without `run()` on `sys`) executes to completion
//! with no output — declarations-only programs run and print nothing.
//!
//! Supported at run time: `uint`/`int`/`bit`/`byte`/`nibble`/`bool`/`real`/
//! `string` values (integers are unbounded `i128`; declared widths are not
//! truncated), enumerals, `list of` values with the clause 27 pseudo-method
//! subset (27.4.22 `size`, 27.4.12 `is_empty`, 27.4.5 `exists`, 27.4.3
//! `copy`, 27.4.21 `reverse`, 27.4.26/27.4.27 `top`/`top0`, the 27.3
//! modifiers `push`/`push0`/`add`/`add0`/`pop`/`pop0`/`insert`/`delete`/
//! `fast_delete`/`clear`, the 27.4.15-27.4.19 extremes
//! `min`/`max`/`min_index`/`max_index`, the 27.5.1/27.5.2/27.5.4/27.5.5
//! aggregates `and_all`/`or_all`/`product`/`sum`, and 27.4.11
//! `is_a_permutation`), the `{...;...}` list-concat operand (clause 4.6,
//! Table 12), the clause 4.11.4 list identity operators `list_ptr==` /
//! `!list_ptr=` (see "List identity" below), struct instances, method
//! calls (every composed block with the
//! name runs, in composition order; the last returned value wins), local
//! variables, assignments, `if`/`else`, `while` with `break`/`continue`,
//! `return`, `print`, the built-in `out` routine, and `gen`.
//!
//! `gen` is backed by [`crate::solve`]: with the `solver-pumpkin` feature
//! the type's linear `keep` constraints — conjunctive, disjunctive
//! (`||`), and their negation — are solved (seeded, deterministic Pumpkin
//! CP solve; disjunctions expand into bounded branch combinations) and
//! unsupported shapes fall back to the
//! rejection sampler, **not** a general constraint solver: fields are
//! sampled inside their declared bounds, a candidate is accepted only when
//! every top-level `keep` constraint of the enclosing type evaluates to
//! TRUE, and after 1000 failed attempts a diagnostic is reported. Soft
//! constraints are not enforced; a `when` subtype view's determinant
//! fields are fixed to the view's values (6.7) and never sampled, while
//! the enclosing type's when-block constraints stay unenforced. Every
//! run takes an explicit seed and is fully deterministic per backend.
//!
//! The clause 10.5.8 `target chosen_from read_only(list)` keep form is
//! applied when its target — a simple generative struct field path — is
//! generated (`gen <path>;` or the field's turn inside `gen` of the
//! enclosing instance): the list elements are substituted into the target
//! slot one by one, the other constraints on paths through the target
//! (declared in the structs along the path) are evaluated with each
//! candidate bound, and one satisfying element is picked uniformly with
//! the seeded RNG. An empty list, or no satisfying element, is a
//! generation contradiction (`E0417`). Pointer-equality constraints on
//! the target are an unsupported shape (`E0402`); there is no ICFS
//! machinery in this stage for the target to participate in.
//!
//! Range-generated (`~`) fields (clause 6.9.7) are excluded from that
//! sampling like `!` fields: after the other fields are concrete, each
//! receives a reduced set of values (10.3.13) — the intersection of the
//! intervals its supported constraint shapes imply — and the set's
//! minimum as its concrete value (deterministic; implementation-defined
//! per the standard). The set is kept as per-instance metadata that the
//! clause 29.13 routines (`rgf_was_generated`/`rgf_get_range`/
//! `rgf_was_reduced`) and the `rgf=` operator (18.6) read.
//!
//! ## Diagnostic codes
//!
//! - `E0401` unknown identifier, field or method at run time
//! - `E0402` type mismatch or invalid operand
//! - `E0403` division by zero
//! - `E0404` list index out of range
//! - `E0405` call to an unsupported routine or list method
//! - `E0406` recursion too deep
//! - `E0407` no solution found for the constraints (1000 attempts)
//! - `E0408` simulator bridge error from the `hdl_*` routines
//! - `E0409` cycle budget exceeded (runaway TCM)
//! - `E0410` unsupported string-match pattern shape (`/.../` or a
//!   native meta-character)
//! - `E0411` `list_ptr==` operands must be lists of the same type
//! - `E0412` `in_table` element type mismatch (clause 4.11.8): a tuple
//!   element and a row element of incompatible kinds
//! - `E0413` a subtype-qualified field access (4.17.3) on an instance
//!   that does not currently match the qualifiers' subtype — the field
//!   is not present on it
//! - `E0415` an `rgf=` value (18.6) outside the field's generated
//!   reduced set (10.3.13)
//! - `E0416` an rgf query or assign before the field was generated
//! - `E0417` `chosen_from` contradiction (10.5.8): the read-only list is
//!   empty, or none of its elements satisfies the other constraints on
//!   paths through the target
//! - `E0317` a constraint involving a range-generated field whose shape
//!   the reduced-set pass does not support (10.3.13; raised from `gen`)
//! - `E0419` a call to a recognised 2019 predefined-library name that
//!   is not implemented in this stage (see [`UNIMPLEMENTED_2019`])
//! - `E0420` `do_bind()` outside the connect_ports() generation
//!   sub-phase (9.13.3)
//! - `E0421` a blocking clause 33 call (`up()`/`down()`/`lock()`) that
//!   must suspend but runs in a non-suspendable context (an atomic
//!   region or a non-scheduled method)
//! - `E0422` a clause 33 `set_value()`/`set_max_value()` violation: a
//!   negative or over-limit value, a forbidden call after `up()`/`down()`
//!   was called, or a non-zero value on an `rdv_semaphore`
//! - `E0423` `locker.release()` by an entity that does not hold the lock,
//!   or on a locker nothing holds (31.2.5: `lock()` and `release()` are
//!   issued by the same entity)
//!
//! ## Execution phases (Table 1, 28.3; generation sub-phases 9.13)
//!
//! A run walks the full phase order: setup (`sys.setup()`, before
//! generation) → generate (the implicit gen, then
//! `pre_generate()` and `post_generate()` depth-first over the field
//! tree, then the binding passes `connect_pointers()` →
//! `connect_ports()` → `check_generation()`, 9.13.2-9.13.4) → run
//! (sys's `run()` blocks as the main coroutine; after the scheduler
//! drains, `run()` on every other live field-tree instance, depth-first
//! in field order) → extract (`extract()`, 28.3.3.6) → check
//! (`check()`, 28.3.3.7, then the 11.4 expect/assume rule pass) →
//! finalize (`quit()` then `finalize()`, 28.3.3.8). The field tree is
//! the instance fields under `sys` (lists flattened, NULLs skipped);
//! structs created outside it (a local `var`'s `gen`/`new`) take no
//! phase calls. All the hooks are initially empty — a type that does
//! not define one contributes nothing. A `stop_run()` skips the
//! remaining phases. Coverage grading needs no move: collection is
//! emit-driven (14.2) and the grade is computed lazily (the report, or
//! `covers.compute_overall_grade()`).
//!
//! The parser's `E0122` (an untyped `evaluate { ... }` whose result type
//! cannot be inferred, clause 4.17.7) is also raised here: the inference
//! contexts — the declared type of the assignment/`var` target or of the
//! call's parameter — resolve at run time, so the diagnostic surfaces
//! when an evaluation actually reaches the untyped block.
//!
//! ## List identity (clause 4.11.4)
//!
//! A list value ([`Value::List`]) holds its items behind a shared
//! `Rc<RefCell<Vec<Value>>>` handle: cloning the value — by `var l3 = l1`
//! assignment, method-argument passing or struct-field copy — **aliases**
//! the same list object, and the modifying pseudo-methods (27.3) mutate
//! that shared storage in place. `list_ptr==` compares exactly this
//! object identity: TRUE iff both operands denote the same handle, so
//! `l1 list_ptr== l1.copy()` is FALSE (a real copy mints a fresh handle)
//! while `l1 list_ptr== l3` after `var l3: list of int = l1;` is TRUE.
//! The regular `==` stays item-by-item. The handle's `RefCell` is
//! borrowed in short scopes only (no borrow is held across a nested
//! evaluation), and the recursive read paths (formatting, item-wise
//! equality) are depth-bounded so a list that was illegally stored inside
//! itself renders or compares unequal instead of overflowing the stack.
//! A list value also carries the declared element type of its creation
//! site when it is known (`list of int` → `"int"`); the value model does
//! not infer it from the items, so the `list_ptr==` same-type check is
//! skipped for a pair where either side's creation site could not know —
//! a documented approximation.
//!
//! ## Clause 27 list pseudo-methods — documented gaps
//!
//! Pseudo-methods are macros, not struct methods (27.1). The ones whose
//! semantics take a per-item expression argument cannot pass through the
//! v1 dispatch (it evaluates arguments to values only), so they report
//! `E0405` "list method ... is not supported": `apply` (27.4.2),
//! `sort`/`sort_by_field` (27.4.23/27.4.24), `first`/`last`
//! (27.4.6/27.4.13), `first_index`/`last_index` (27.4.7/27.4.14),
//! `all_different` (27.4.1), `unique` (27.4.28), `split` (27.4.25), the
//! expression forms of the boolean 27.5 aggregates
//! `and_all`/`or_all`. The 27.4.17/27.4.20/27.5.4-27.5.6 aggregates
//! (`max_value`, `min_value`, `product`, `sum`, `average`) DO take a
//! per-item expression — they intercept before the argument evaluation
//! ([`Interp::call_list_aggregate`]); their empty-list `average` and
//! `max_value`/`min_value` results are the honest `E0402` (UNDEF and
//! Table 54 are not in the value model). `min`/`max`/`min_index`/
//! `max_index` (27.4.15–27.4.19) accept an optional key expression
//! evaluated with `it` bound to each item. They select the original
//! item/index, with the latest tie winning; numeric and string keys
//! follow the runtime's relational ordering. Traversal snapshots the
//! receiver; implicit index bindings remain unsupported. `has`, `count`,
//! `all` and `all_indices` evaluate one boolean expression per item with
//! a scoped `it` binding (27.4.10/4/29/30). `has` stops at the first
//! match; the others preserve order. Their receiver is snapshotted before
//! evaluation, so mutation during a predicate does not change traversal.
//! Implicit index bindings are not modeled. Also not
//! modeled: `resize` (27.3.16 — the
//! `filler`/`keep_old` parameters are expressions and new items need the
//! element type's default value), `get_indices` (27.4.9 — the prose does
//! not state the out-of-range index behavior; element access is the `[i]`
//! index expression, `E0404`), `flatten` (27.4.8 — no language surface of
//! this subset can produce a nested list: a list argument to a modifying
//! pseudo-method is the 27.3.2/27.3.4/27.3.14/27.3.15 list form, because e
//! declares no lists of lists), the CRC family (27.6.1-27.6.3 — the prose
//! notes the polynomials but not the full algorithm parameters), and keyed
//! lists (27.7).
//!
//! `emit` records the firing and immediately executes the event's `on`
//! action blocks (clause 11.2), in composition order, on the instance
//! the event path resolves to. The simulator-side triggers fire through
//! the same machinery: `rise/fall/change(HDL expression) @sim` events
//! (11.1.4.2) and bound event ports (9.5, 9.7.3) are sampled every
//! virtual cycle while an HDL source is available — a live session or
//! the injected per-cycle test seam ([`crate::hdl::SignalSource`]);
//! without one they never fire (HDL signals without a simulator). A TCM
//! declared `@event` commences when that event occurs (17.1.2's implicit
//! entry sync); every-cycle sampling events (`@sys.any`, `@sim`) keep
//! the one-cycle implicit sync of the stand-alone model.

use crate::ast::{
    BinaryOp, DeclarationKind, EventDecl, Expr, Field, MethodDecl, OnDecl, ScalarBase, Stmt,
    TypeExpr, UnaryOp, WhenQualifier,
};
use crate::diagnostic::Diagnostic;
use crate::elab::Elaborated;
use crate::lexer::Position;
use crate::lexer::Token;
use crate::lexer::TokenKind;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::rc::Rc;

/// Outcome of one run: the text produced by `out`/`print`, any run-time
/// diagnostics, and the coverage collected by the cover groups
/// (clause 14 — buckets and grades per type).
#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    pub output: String,
    pub diagnostics: Vec<Diagnostic>,
    pub coverage: Vec<crate::coverage::TypeCoverage>,
}

#[derive(Debug, Clone)]
#[allow(clippy::derive_partial_eq_without_eq)] // Real(f64) has no total equality
pub enum Value {
    Int(i128),
    Real(f64),
    Bool(bool),
    Str(String),
    Enum {
        type_name: String,
        name: String,
    },
    List(ListValue),
    /// A set (5.2.12): inclusive intervals in canonical form — sorted,
    /// non-overlapping, non-neighboring.
    Set(Vec<(i128, i128)>),
    Object(usize),
    /// A `simple_port`/`event_port` member reference (clause 9): the
    /// owning instance and the port name. Ports are not fields — a port
    /// member read produces this value, and the 9.13 binding methods
    /// (`get_bound_set()`, `is_connected()`, `disconnect()`, ...) dispatch
    /// on it. The port's *value* is the `name$` access through the HDL
    /// bridge, not this handle.
    Port {
        owner: usize,
        name: String,
    },
    /// A `table` value (32.3, T9.3): the shared storage behind the
    /// procedural `table_manager`/`table` API. Cloning the value aliases
    /// the same table; `copy()` mints a fresh writable one.
    Table(Rc<RefCell<crate::tables::TableValue>>),
    /// A `table_row` value (32.3): the shared row handle plus, when the
    /// row came from a table, that table — the row-index pseudo-column
    /// (column 0) and the read-only rule read through it.
    TableRow {
        table: Option<Rc<RefCell<crate::tables::TableValue>>>,
        row: Rc<RefCell<crate::tables::TableRowValue>>,
    },
    /// A custom numeric type value (5.2.15, 5.8.4): the declaring
    /// `numeric_type`'s name and the implementing-struct instance whose
    /// `numeric_i` methods give the value its behavior (clause 31; the
    /// T7.1 value model). By value, not by reference: binding a custom
    /// value into a slot, passing it as an argument or copying a field
    /// mints a fresh instance and runs the struct's `numeric_assign`
    /// (a per-field deep copy when the struct does not define one) —
    /// values have value semantics even though operations are struct
    /// methods.
    Custom {
        type_name: String,
        instance: usize,
    },
    /// An `instance_iterator` value (32.2, T11.2): the shared state
    /// behind one iteration over the instance field tree. Cloning the
    /// value aliases the same iterator (a `copy()` mints an independent
    /// one positioned identically); `start()`/`start_all()` compute the
    /// full walk once and `next()` moves along it.
    InstanceIterator(Rc<RefCell<InstanceIteratorState>>),
    /// An `instance_path_item` value (32.2): one step of the path an
    /// `instance_iterator` reports. The items are immutable snapshots
    /// handed out by the iterator — `new instance_path_item` is the
    /// construction stub (E0419, clause 32.2).
    InstancePathItem(Rc<InstancePathItem>),
    /// A clause 33 resource-sharing object (`semaphore`,
    /// `rdv_semaphore` or `locker`): the shared state behind the
    /// blocking `up()`/`down()`/`lock()` TCM calls. The value aliases
    /// one object — copying it does not copy the synchronization state.
    Sync(Rc<RefCell<SyncState>>),
    Void,
}

/// Which clause 33 predefined struct a [`Value::Sync`] carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncKind {
    /// `semaphore`: value (available resources) 0 by default, maximum
    /// MAX_INT.
    Semaphore,
    /// `rdv_semaphore`: the rendezvous semaphore — the value stays 0;
    /// `up()` and `down()` block until they coincide, and the paired
    /// `up()` thread always runs first, followed immediately by the
    /// `down()` thread (31.2.1).
    RdvSemaphore,
    /// `locker`: FIFO-ordered mutual exclusion between TCMs (31.2.5).
    Locker,
}

impl SyncKind {
    fn name(self) -> &'static str {
        match self {
            SyncKind::Semaphore => "semaphore",
            SyncKind::RdvSemaphore => "rdv_semaphore",
            SyncKind::Locker => "locker",
        }
    }
}

/// Which blocking call one parked coroutine waits on. One entry of the
/// sync object's FIFO waiter queue — the queue order is the blocked
/// call order, and blocked calls are serviced in that order (31.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncWaiter {
    /// `up()` blocked at the maximum.
    Up,
    /// `down()` blocked with no resource available.
    Down,
    /// `locker.lock()` blocked while another entity holds the lock.
    Lock,
}

/// The state of one clause 33 sync object. The blocking surface is the
/// scheduler's wake pass: a parked coroutine carries the object and its
/// queue serial, and each scheduler iteration services the oldest
/// waiter whose condition holds, so blocked calls run in their request
/// order (the documented FIFO reading of 31.2.1).
#[derive(Debug)]
pub struct SyncState {
    kind: SyncKind,
    /// Available resources (the value `get_value()` reads). Always 0 on
    /// an `rdv_semaphore`; unused on a `locker`.
    available: i128,
    /// The maximum available resources (`get_max_value()`). 0 on an
    /// `rdv_semaphore`; unused on a `locker`.
    max_value: i128,
    /// 31.2.3/31.2.4: `set_value()`/`set_max_value()` cannot be called
    /// once `up()` or `down()` was called. Any `up`/`down`/`try_up`/
    /// `try_down` call — blocked or not — sets this (the conservative
    /// reading of "was previously called").
    transfer_attempted: bool,
    /// The blocked calls, front = oldest request.
    waiters: std::collections::VecDeque<(u64, SyncWaiter)>,
    /// Serial source for the waiter queue.
    next_serial: u64,
    /// `locker` only: the coroutine id of the entity that acquired the
    /// lock, `None` while free.
    holder: Option<u64>,
}

impl SyncState {
    fn new(kind: SyncKind) -> Self {
        // The default maximum is MAX_INT (31.1: "The maximum value ranges
        // between 1 and MAX_INT"), the e int maximum.
        let (available, max_value) = match kind {
            SyncKind::Semaphore => (0, 2_147_483_647),
            SyncKind::RdvSemaphore => (0, 0),
            SyncKind::Locker => (0, 0),
        };
        Self {
            kind,
            available,
            max_value,
            transfer_attempted: false,
            waiters: std::collections::VecDeque::new(),
            next_serial: 0,
            holder: None,
        }
    }
}

/// One clause 33 blocking call's outcome.
enum SyncOutcome {
    /// The call completed; the turn continues inside the statement.
    Done(Value),
    /// The turn ends at the call — a successful blocking call
    /// yields (31.2.5's non-preemptive model interrupts at TCM
    /// calls and release/lock requests) or a blocked one parks on
    /// the waiter queue until the wake pass services it.
    Yield,
}

impl SyncOutcome {
    /// The call's value (`Done`'s payload; the blocking surface is
    /// VOID, so a defensively-mapped `Yield` is VOID too).
    fn value(self) -> Value {
        match self {
            SyncOutcome::Done(value) => value,
            SyncOutcome::Yield => Value::Void,
        }
    }
}

/// One step of an `instance_iterator`'s path (32.2): the instance the
/// step arrived at, the instance field it came through and — when that
/// field is a list (nested lists recurse) — the index chain of the
/// item. The iteration root's step has no field and no indices; its
/// path string is the root's own spelling (`sys` for the run's root
/// unit).
#[derive(Debug, Clone)]
pub struct InstancePathItem {
    instance: usize,
    field: Option<String>,
    list_indices: Vec<usize>,
}

/// The state of one `instance_iterator` (32.2, T11.2): the full
/// depth-first visit order computed once by `start()`/`start_all()` —
/// each visit carrying its path from the iteration root — the spelling
/// the root renders by, and the cursor the `next()`/`get_current_*`
/// methods move and read. The walk snapshots the tree at start time;
/// fields created or re-bound afterwards are not picked up (a
/// documented approximation).
#[derive(Debug, Clone)]
pub struct InstanceIteratorState {
    visits: Vec<Rc<Vec<InstancePathItem>>>,
    /// How the root step spells (`sys` for the run's root unit).
    root_label: String,
    /// FALSE before the first `start()`/`start_all()` and after
    /// `finish()`.
    started: bool,
    /// The position of the current visit: `Some(0)` right after
    /// `start()` (the root is current without a `next()`), advanced by
    /// `next()`, `None` while never started, after the walk ran past
    /// the last visit, and after `finish()`.
    cursor: Option<usize>,
}

/// The storage of a [`Value::List`] (`list of <t>`, 5.2.11). The items
/// live behind a shared handle: cloning a list value aliases the same
/// list object (which `var l3 = l1`, argument passing and struct-field
/// copy all do), and `list_ptr==` (clause 4.11.4) compares this object
/// identity. `copy()` (27.4.3) mints a fresh handle — a real copy. The
/// 27.3 modifying pseudo-methods mutate the shared storage in place.
///
/// `element` is the declared element type rendered canonically (`"int"`,
/// `"uint"`, ...), recorded when the creation site knows it: defaults,
/// sampling and variables initialized from a typed declaration. It is
/// `None` where nothing at the site can know it (a bare `{...;...}` list
/// concat outside such an initialization). The value model does not
/// infer the element type from the items.
#[derive(Debug, Clone)]
pub struct ListValue {
    pub items: Rc<RefCell<Vec<Value>>>,
    pub element: Option<String>,
}

impl ListValue {
    /// A fresh, empty list with the given (optional) declared element
    /// type. Every call allocates a distinct object.
    fn new(element: Option<String>) -> Self {
        Self {
            items: Rc::new(RefCell::new(Vec::new())),
            element,
        }
    }
}

struct ObjectInstance {
    type_name: String,
    /// Declaration order preserved; `gen` and formatting rely on it.
    fields: Vec<(String, Value)>,
    /// `simple_port` members: port name → HDL location for the bridge.
    ports: Vec<(String, String)>,
    /// Range-generated (`~`) field metadata (clause 6.9.7, 10.3.13):
    /// field name → generated state, present once `gen` has produced
    /// the field's reduced set. The clause 29.13 rgf routines and the
    /// `rgf=` operator (18.6) read it.
    rgf: BTreeMap<String, RgfState>,
}

/// The generated state of one range-generated field: the reduced set as
/// canonical intervals, and whether the set is smaller than the
/// declared type's full set (`rgf_was_reduced`, clause 29.13).
#[derive(Debug, Clone)]
struct RgfState {
    ranges: Vec<(i128, i128)>,
    reduced: bool,
}

impl ObjectInstance {
    fn get(&self, name: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
    }

    fn set(&mut self, name: &str, value: Value) -> bool {
        for (field, slot) in &mut self.fields {
            if field == name {
                *slot = value;
                return true;
            }
        }
        false
    }

    /// The HDL location bound to a port, if any.
    fn port_path(&self, name: &str) -> Option<&str> {
        self.ports
            .iter()
            .find(|(port, _)| port == name)
            .map(|(_, path)| path.as_str())
    }
}

#[derive(Debug, Default, Clone)]
struct Frame {
    locals: std::collections::HashMap<String, Value>,
    local_types: std::collections::HashMap<String, TypeExpr>,
}

#[derive(Debug, Clone)]
struct StatementCall {
    receiver: usize,
    name: String,
    args: Vec<Value>,
    block: usize,
    frame: Frame,
    span: crate::ast::Span,
}

enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
    /// `stop_run()` — unwind to the top of the run.
    Stop,
    /// A synchronization action inside a scheduled coroutine (13.2.2):
    /// unwind to the scheduler, which queues the coroutine for a later
    /// cycle. Only raised in coroutine turns — `call_method` and
    /// `on` blocks run atomically and never produce it.
    Suspend,
}

/// Which child of a compound statement a turn's continuation lives in.
///
/// The resume-path design (13.2.2): a synchronization action delimits
/// the atomic fragments of a TCM, and resumption "commences executing
/// from the first action following the synchronization action". The
/// suspension therefore records the full descent through the statement
/// tree — one [`PathLevel`] per nesting level, each naming the
/// statement index it entered and the child its continuation descended
/// into — and resumption replays nothing that already ran: the
/// executed statement prefix is skipped, an `if`/`case` branch is
/// re-entered without re-evaluating its condition/scrutinee, and a
/// loop body is re-entered mid-iteration (the condition is re-checked
/// only when the iteration completes, exactly like the `while` model
/// this generalizes). The descent reaches every compound; only the
/// atomic regions (inline method calls, `compute`, `on` blocks, and a
/// `check` else body, which continue their enclosing fragment) keep
/// the conservative run-to-completion model.
#[derive(Debug, Clone)]
enum Child {
    /// A statement-position call, with its own locals and once-evaluated
    /// receiver/arguments. Further path levels descend into its body.
    Call(Box<StatementCall>),
    /// A `while` body mid-iteration — the condition is re-checked only
    /// when the iteration completes.
    WhileBody,
    /// A `repeat until` body mid-iteration (20.2.3) — likewise, the
    /// condition is evaluated only when the iteration completes.
    RepeatBody,
    /// A numeric `for` body mid-iteration, carrying the in-flight
    /// iteration's counter and the loop's once-evaluated end bound, so
    /// the continuation neither resets the counter nor replays the
    /// bounds (the body's reads go through the frame's bound
    /// temporary).
    ForBody { counter: i128, end: i128 },
    /// A `for each` body mid-iteration, carrying the list snapshot
    /// taken at loop entry (post-reversal) and the in-flight element's
    /// position; the element and index bindings live in the frame.
    ForeachBody { position: usize, items: Vec<Value> },
    /// The `then` branch of an `if` — the condition was evaluated
    /// before the suspension; resume does not re-evaluate it.
    IfThen,
    /// The `else` branch of an `if`.
    IfElse,
    /// The matched arm of a `case` (`None` = the default arm) — the
    /// scrutinee is not re-matched on resume.
    CaseArm { index: Option<usize> },
}

/// One level of a turn's statement walk: the statement index within
/// the enclosing block and, once the walk descends into a compound
/// statement, which child it entered. The turn's stack of levels is
/// its position in the statement tree; captured at a suspension it
/// becomes the resume path.
#[derive(Debug, Clone)]
struct PathLevel {
    index: usize,
    child: Option<Child>,
}

/// A suspension raised by a synchronization action, collected by the
/// scheduler when the turn unwinds.
struct PendingSuspension {
    path: Vec<PathLevel>,
    frame: Frame,
    eligible_at_cycle: u64,
    temporal: bool,
    span: crate::ast::Span,
    wait_start: Option<usize>,
    /// A clause 33 sync wait riding the suspension (the blocking call
    /// parks until the wake pass services it).
    wake_sync: Option<(Rc<RefCell<SyncState>>, u64)>,
}

/// One concurrently scheduled execution (clause 13.2.2): the main
/// pseudo-coroutine (the `run()` bodies, in order) and every `start`ed
/// TCM. Suspension state is the statement path into the tree (the
/// walk's position when the synchronization action suspended), the
/// captured locals frame, and the cycle gate.
struct Coroutine<'a> {
    kind: CoroutineKind<'a>,
    /// The scheduler-wide identity of this coroutine — the locker's
    /// holder identity (clause 33: `lock()`/`release()` by the same
    /// entity).
    id: u64,
    path: Vec<PathLevel>,
    frame: Frame,
    me: Option<usize>,
    eligible_at_cycle: u64,
    /// Set while the coroutine is parked for the occurrence of its TCM
    /// sampling event — the implicit entry sync of a `@event` TCM
    /// (17.1.2: "as if the action sync @event was added at the top").
    /// Cleared when the event fires.
    wake_event: Option<String>,
    /// Set while the coroutine is parked on a clause 33 blocking call
    /// (`up()` at the maximum, `down()` with no resource, `lock()` while
    /// held): the object and this call's waiter serial. The wake pass
    /// services the oldest waiter whose condition holds.
    wake_sync: Option<(Rc<RefCell<SyncState>>, u64)>,
    /// Whether the coroutine is suspended at a temporal wait (drives
    /// the per-wait runaway guard).
    suspended_temporal: bool,
    /// Cycles spent suspended at the current temporal wait.
    temporal_waited: u64,
    span: crate::ast::Span,
    wait_start: Option<usize>,
}

enum CoroutineKind<'a> {
    /// The `run()` bodies, executed in order as one coroutine.
    Main {
        blocks: Vec<&'a [Stmt]>,
        next_block: usize,
    },
    /// A started TCM: instance, method name (resolved per turn, so
    /// `is also` chains compose), evaluated arguments.
    Tcm {
        instance: usize,
        method: String,
        args: Vec<Value>,
        next_block: usize,
    },
}

enum TurnOutcome {
    Complete,
    Suspended,
    Error(Diagnostic),
}

/// splitmix64: small, seedable, deterministic — no external dependency.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u128) -> u128 {
        if bound == 0 {
            0
        } else {
            u128::from(self.next_u64()) % bound
        }
    }

    fn unit_f64(&mut self) -> f64 {
        f64::from((self.next_u64() >> 40) as u32) / f64::from(1u32 << 24)
    }
}

/// One `rise/fall/change(HDL expression) @sim` event trigger (11.1.4.2),
/// collected from the elaborated types for the per-cycle sampling.
#[derive(Clone)]
struct SimTrigger {
    /// The type that declares the event; every instance of it owns the
    /// event (11.1.2).
    type_name: String,
    event_name: String,
    /// `rise` / `fall` / `change` as written.
    routine: String,
    /// The monitored HDL expression inside the edge routine's
    /// parentheses.
    condition: Expr,
    span: crate::ast::Span,
}

struct Interp<'a> {
    types: &'a [crate::elab::ElaboratedType],
    instances: Vec<ObjectInstance>,
    rng: Rng,
    /// The run's seed, handed to the constraint-solver backend for `gen`.
    /// Only read with the `solver-pumpkin` feature.
    #[cfg_attr(not(feature = "solver-pumpkin"), allow(dead_code))]
    seed: u64,
    output: String,
    depth: usize,
    /// Where signal values come from: a live simulator session (spawned
    /// by `hdl_load`) or the injected per-cycle test seam.
    hdl: Option<crate::hdl::SignalSource>,
    /// Virtual simulation time in cycles, advanced by `wait` statements.
    cycle: u64,
    /// Remaining `wait` statements before a runaway TCM is stopped.
    wait_budget: u64,
    /// Events fired by `emit`, keyed by their path as written.
    emitted: std::collections::BTreeSet<String>,
    /// The `rise/fall/change(HDL expression) @sim` event triggers
    /// (11.1.4.2) collected from the elaborated types.
    sim_triggers: Vec<SimTrigger>,
    /// The last sampled value per monitored trigger, keyed by (instance,
    /// event-or-port path) — the edge detection's baseline. `None` marks
    /// a trigger whose expression has not been sampleable yet.
    sim_prev: BTreeMap<(usize, String), Option<i128>>,
    /// The `keep <port>.edge() == <value>` attribute values (9.7.3)
    /// per (type name, port name); the sampling default is `change`.
    port_edges: BTreeMap<(String, String), String>,
    /// The run's temporal log (clause 12): one state per virtual cycle,
    /// recording which events occurred in it. `emit` writes here and
    /// temporal `wait`s and `expect` rules read it. One global
    /// timeline: all events share the per-cycle states.
    temporal_log: Vec<crate::temporal::State>,
    /// The scheduled coroutines (13.2.2): the main `run()` chain first,
    /// then every `start`ed TCM, run round-robin.
    coroutines: Vec<Coroutine<'a>>,
    /// Set while a coroutine turn executes: statement walks record
    /// their descent for the resume path.
    tracing: bool,
    /// Atomic regions (inline method calls, `compute`, `on` blocks,
    /// `check` else bodies): waits inside keep the inline spin instead
    /// of suspending.
    atomic_depth: usize,
    /// The statement-tree descent of the current turn.
    exec_context: Vec<PathLevel>,
    /// The suspension raised by the current turn's synchronization
    /// action, collected when the turn unwinds.
    pending_suspension: Option<PendingSuspension>,
    /// The clause 33 sync wait raised by the current turn, collected
    /// with the suspension: the object and this call's waiter serial.
    pending_wake_sync: Option<(Rc<RefCell<SyncState>>, u64)>,
    /// The id of the coroutine whose turn runs — the locker's holder
    /// identity (`lock()` and `release()` are issued by the same
    /// entity, 31.2.5).
    current_coroutine_id: Option<u64>,
    /// The serial source for coroutine ids.
    next_coroutine_id: u64,
    /// The log start of a resumed temporal wait, restored before the
    /// wait statement re-executes.
    resume_wait_start: Option<usize>,
    /// Coverage data (clause 14), keyed by type; `emit` of a group's
    /// sampling event records item values into the buckets.
    coverage: Vec<crate::coverage::TypeCoverage>,
    /// Set by `stop_run()`; unwinds the whole run.
    stop_requested: bool,
    /// Failed `check that` conditions (`dut_error`), reported like
    /// Specman's DUT-error count.
    dut_errors: usize,
    /// `define` constants resolved to values.
    defines: std::collections::BTreeMap<String, Value>,
    /// The static field store (T2.1): one shared occurrence per
    /// declaring type, keyed by the canonical type name the static
    /// member is declared in, then by field name. Initialized at
    /// program start — a static field exists with zero instances — and
    /// read/written by `Type::field` accesses and by bare static names.
    ///
    /// ## Static-storage design note (T2.1–T2.3)
    ///
    /// Static storage lives HERE, in the interpreter: a per-type map
    /// keyed by the canonical name of the type that declares the
    /// member, created at program start (so a static field exists with
    /// zero instances) and initialised once from the field's init value
    /// or type default. The declaring name comes from the elaboration's
    /// static registries ([`Elaborated::static_fields`] and siblings),
    /// whose key is the type the member was declared in — the base
    /// declaration's name, or the extend target for an `extend T {
    /// static ... }` member, so extension adds to the same store. A
    /// `when` subtype never contributes: static members are rejected
    /// inside `when` blocks at the well-formedness pass (E0304/E0310),
    /// and the synthesized views' field lists may carry the base's
    /// static flags but the view's own name is never a store key — a
    /// bare static reference from a view instance resolves through the
    /// view's base chain to the owning store. `Type::field` resolves
    /// through the same registries: the member must be declared in
    /// exactly `Type` (validated by the well-formedness pass, E0307),
    /// and the access reads/writes `statics[Type][field]`; a path may
    /// start at a static field and continue through instance fields of
    /// its value. Instances never carry static fields —
    /// [`Interp::instantiate`] skips them, so `gen` skips them and
    /// instance formatting (`print`, `to_string`) never includes them.
    statics: BTreeMap<String, BTreeMap<String, Value>>,
    /// The static members by declaring type name (from elaboration):
    /// fields, methods, events, and the static `on` blocks. The
    /// `Type::member` runtime resolution reads these; a static field's
    /// store key is the same name.
    static_fields: &'a BTreeMap<String, Vec<Field>>,
    static_methods: &'a BTreeMap<String, Vec<MethodDecl>>,
    static_events: &'a BTreeMap<String, Vec<EventDecl>>,
    static_ons: &'a BTreeMap<String, Vec<OnDecl>>,
    /// The annotation types (clause 35.2) by name with their
    /// `repeatable` flag: `new` of one is rejected (E0402) and the
    /// clause 35.4 API dispatch resolves against it.
    annotation_types: &'a BTreeMap<String, bool>,
    /// The attached annotations (clause 35.3) by annotation type name —
    /// the clause 35.4 `get_all_annotations()` reads this registry.
    attached_annotations: &'a BTreeMap<String, Vec<crate::elab::AttachedAnnotation>>,
    /// The type whose static method is currently executing (T2.2):
    /// bare static-field and bare static-method references inside a
    /// static method resolve against it (`me` does not exist there).
    static_context: Option<String>,
    /// The declared type expected of the expression currently being
    /// evaluated, when the evaluation site knows one: the declared type
    /// of a `var`/assignment target or of the call argument's parameter.
    /// An untyped `evaluate { ... }` block (4.17.7) takes its result
    /// type from it; every site sets it immediately before its eval call
    /// and restores it afterwards.
    expected_type: Option<TypeExpr>,
    /// The type name an untyped `new` creates with while the enclosing
    /// `var x: t = new;` initializer evaluates (clause 18: the
    /// variable's declared type is the instance's): set just before the
    /// initializer evaluates, cleared right after.
    new_type_hint: Option<String>,
    /// Field init-value evaluation failures (clause 6.9.1/6.9.4) that
    /// surfaced where no diagnostic can propagate — instance creation
    /// returns an index, not a `Result`. Drained into the run's
    /// diagnostics; the affected fields keep their type default.
    pending_diagnostics: Vec<Diagnostic>,
    /// Type-level port bindings from `keep bind(p, q)` constraints
    /// (9.13): type name → the (port, port) edges among the type's own
    /// ports. Every instance of the type carries the same edges between
    /// its own ports.
    type_port_binds: BTreeMap<String, Vec<(String, String)>>,
    /// Type-level `keep bind(p, empty)` spellings (9.13): ports bound to
    /// nothing on purpose — `is_connected()` is FALSE for them even when
    /// an HDL path exists.
    type_port_unbound: BTreeMap<String, Vec<String>>,
    /// The dynamic disconnect state: ports that lost their bindings to
    /// `disconnect()` / `disconnect_bound_set()` (9.13) since run start,
    /// as (instance, port) pairs.
    disconnected_ports: BTreeSet<(usize, String)>,
    /// Whether the generate phases have completed (`post_generate()`
    /// has run): `covers.sample_cg()` is not permitted before it
    /// (14.8.2.10).
    post_generate_done: bool,
    /// TRUE while the `connect_ports()` sub-phase pass runs (9.13.3):
    /// `do_bind()` is legal only inside it (`E0420` elsewhere).
    in_connect_ports: bool,
    /// The last overall coverage grade computed by
    /// `covers.compute_overall_grade()` (14.9.6), read back by
    /// `covers.get_overall_grade()`.
    overall_grade: Option<f64>,
    /// The table read-cache (32.3, T9.3): a table read from a file is
    /// cached and read-only — one shared value per (file, tag), so
    /// modifying it is the copy-first runtime error. Keyed by the
    /// resolved path and the tag.
    table_cache: BTreeMap<(String, String), Rc<RefCell<crate::tables::TableValue>>>,
}

/// How many `wait` statements and scheduler cycles one run may consume
/// (the runaway-TCM guard; every suspension cycle in the clause 13.2.2
/// scheduler draws from the same budget).
const WAIT_BUDGET: u64 = 5_000;

/// How many cycles one temporal `wait` may stay unsatisfied before it
/// reports `E0409` — the temporal twin of the runaway guard, bounded
/// low because the success scan walks the whole log each cycle.
const TEMPORAL_WAIT_CYCLES: u64 = 1_000;

/// The instance-iterator walk's descent cap (32.2, T11.2) — the same
/// shape as the phase-method field tree's cap: hitting it means a
/// cyclic field structure, and that branch stops descending.
const INSTANCE_WALK_MAX_DEPTH: u32 = 64;

/// The nested-list index chain a single iterator path step records at
/// most (`grid[0][2]` chains) — a list stored inside itself cannot
/// extend a chain forever.
const INSTANCE_WALK_MAX_INDICES: usize = 16;

/// The argument-count ceiling a stub entry without a plan-stated
/// signature claims: no signature is asserted for a name whose clause
/// this stage does not model, so any practical argument count reaches
/// the E0419 message instead of an invented arity check.
const MAX_STUB_ARGS: usize = usize::MAX;

/// The recognised 2019 predefined-library names this stage does not
/// implement (T8): a call resolves to the table (so the parser accepts
/// it), the arity is checked where the migration plan states a
/// signature, and the call then reports `E0419` "'name()' is recognised
/// (clause X) but not implemented in this stage" at run time. A
/// user-defined method of the same name always wins — the table is
/// consulted only when nothing else resolves.
/// `(name, clause, arity (min, max))`.
const UNIMPLEMENTED_2019: &[(&str, &str, (usize, usize))] = &[
    // 28.3.5.1: deactivate a struct at the end of the current tick and
    // reactivate it next cycle — the coroutine model has no per-tick
    // struct activation to suspend.
    ("rerun", "28.3.5.1", (0, 0)),
    // 9.13.5-9.13.12 port-binding surface beyond the `keep bind`
    // simple_port model: unit-level binding, directed method-port
    // connection and HDL-path remapping.
    ("do_bind_unit", "9.13.5-9.13.12", (2, 2)),
    ("connect", "9.13.5-9.13.12", (1, 1)),
    ("remap_hdl_path", "9.13.5-9.13.12", (1, 1)),
    // 16.3.7: set a named check's severity/effect, rooted at a unit.
    ("set_check_by_name", "16.3.7", (2, 3)),
    // 14.9.15-14.9.17 coverage-configuration calls.
    ("set_per_inst_for_struct", "14.9.15-14.9.17", (2, 2)),
    ("set_unicov_hier", "14.9.15-14.9.17", (2, 2)),
    ("set_cross_instance_collection", "14.9.15-14.9.17", (3, 3)),
    ("get_top_size_items", "14.9.15-14.9.17", (1, 1)),
    // 30.5.16: the log-file flush of the predefined files struct.
    ("flush_log_file", "30.5.16", (0, 0)),
    // 19.1: `swap` is a predefined pack tool name; the byte-order swap
    // itself is not implemented.
    ("swap", "19.1", (1, 1)),
    // 35.4: the element-taking annotation API and the
    // definition-element surface need an rf_definition_element model;
    // only get_all_annotations() is implemented (T10.3).
    ("get_attached_annotation", "35.4", (1, 1)),
    ("get_attached_annotations_list", "35.4", (1, 1)),
    ("get_definition_element", "35.4", (0, 0)),
    ("is_applicable", "35.4", (1, 1)),
    // ----------------------------------------------------------------
    // The clause 32.1 reflection catalogue (T11.1): recognised names
    // with no reflection model — the catalogue is
    // docs/REFLECTION_2019.md, the type names live in
    // builtins::RECOGNIZED_REFLECTION_TYPES. Arity is claimed only
    // where the migration plan states an argument shape; the other
    // entries accept any argument count (`(0, MAX_STUB_ARGS)` — no
    // signature is claimed for a name whose clause this stage does not
    // model). A user-defined method of the same name always wins: the
    // table is consulted only when nothing else resolves.
    // ----------------------------------------------------------------
    // 32.1.2.3 rf_object (base of rf_struct and rf_interface).
    ("is_contained_in", "32.1.2.3", (0, MAX_STUB_ARGS)),
    ("is_instance_of_me", "32.1.2.3", (0, MAX_STUB_ARGS)),
    // 32.1.2.5.2 rf_struct_member.
    ("is_static", "32.1.2.5.2", (0, MAX_STUB_ARGS)),
    // 32.1.2.5.4 rf_range_generated_field (the value forms beside the
    // implemented clause 29.13 routines rgf_was_generated /
    // rgf_get_range / rgf_was_reduced).
    ("was_generated", "32.1.2.5.4", (0, MAX_STUB_ARGS)),
    ("was_reduced", "32.1.2.5.4", (0, MAX_STUB_ARGS)),
    ("get_range", "32.1.2.5.4", (0, MAX_STUB_ARGS)),
    ("rgf_assign_value", "32.1.2.5.4", (0, MAX_STUB_ARGS)),
    ("rgf_assign_value_unsafe", "32.1.2.5.4", (0, MAX_STUB_ARGS)),
    // 32.1.2.5.8-32.1.2.5.10 the on-method descriptors.
    (
        "get_triggering_event",
        "32.1.2.5.8-32.1.2.5.10",
        (0, MAX_STUB_ARGS),
    ),
    (
        "get_triggering_event_string",
        "32.1.2.5.8-32.1.2.5.10",
        (0, MAX_STUB_ARGS),
    ),
    // 32.1.2.5.11 rf_event.
    ("on_method", "32.1.2.5.11", (0, MAX_STUB_ARGS)),
    // 32.1.2.5.13 rf_check (get_relevant_check_actions is the 2019
    // rename).
    ("get_check_actions", "32.1.2.5.13", (0, MAX_STUB_ARGS)),
    (
        "get_relevant_check_actions",
        "32.1.2.5.13",
        (0, MAX_STUB_ARGS),
    ),
    // 32.1.2.6-32.1.2.7 rf_interface / rf_interface_member /
    // rf_interface_method.
    ("get_methods", "32.1.2.6-32.1.2.7", (0, MAX_STUB_ARGS)),
    (
        "get_declared_methods",
        "32.1.2.6-32.1.2.7",
        (0, MAX_STUB_ARGS),
    ),
    ("get_method", "32.1.2.6-32.1.2.7", (0, MAX_STUB_ARGS)),
    (
        "get_base_interfaces",
        "32.1.2.6-32.1.2.7",
        (0, MAX_STUB_ARGS),
    ),
    (
        "get_direct_sub_interfaces",
        "32.1.2.6-32.1.2.7",
        (0, MAX_STUB_ARGS),
    ),
    (
        "get_all_sub_interfaces",
        "32.1.2.6-32.1.2.7",
        (0, MAX_STUB_ARGS),
    ),
    ("get_implementors", "32.1.2.6-32.1.2.7", (0, MAX_STUB_ARGS)),
    (
        "get_declaring_interface",
        "32.1.2.6-32.1.2.7",
        (0, MAX_STUB_ARGS),
    ),
    ("get_result_type", "32.1.2.6-32.1.2.7", (0, MAX_STUB_ARGS)),
    ("get_parameters", "32.1.2.6-32.1.2.7", (0, MAX_STUB_ARGS)),
    (
        "get_implementing_methods",
        "32.1.2.6-32.1.2.7",
        (0, MAX_STUB_ARGS),
    ),
    ("invoke", "32.1.2.6-32.1.2.7", (0, MAX_STUB_ARGS)),
    ("invoke_unsafe", "32.1.2.6-32.1.2.7", (0, MAX_STUB_ARGS)),
    // 32.1.2.8.5 rf_like_struct.
    ("get_interfaces", "32.1.2.8.5", (0, MAX_STUB_ARGS)),
    // 32.1.2.10 rf_scalar / rf_numeric / rf_custom_numeric (the
    // set-of-values queries moved from rf_numeric to rf_scalar in
    // 2019; the clause 5.10.5/5.10.6 routines stay implemented).
    ("get_set_of_values", "32.1.2.10", (0, MAX_STUB_ARGS)),
    ("get_full_set_of_values", "32.1.2.10", (0, MAX_STUB_ARGS)),
    ("get_bit_size_modified", "32.1.2.10", (1, 1)),
    ("get_range_string", "32.1.2.10", (0, MAX_STUB_ARGS)),
    ("is_longint", "32.1.2.10", (0, MAX_STUB_ARGS)),
    ("get_implementing_struct", "32.1.2.10", (0, MAX_STUB_ARGS)),
    // 32.1.2.14 the template family's value/parameter queries.
    ("get_type", "32.1.2.14", (0, MAX_STUB_ARGS)),
    ("get_template_parameter", "32.1.2.14", (0, MAX_STUB_ARGS)),
    ("get_value", "32.1.2.14", (0, MAX_STUB_ARGS)),
    // 32.1.2.17 rf_manager queries (the bare-call spellings route
    // through call_builtin; the member-call form lands here too).
    (
        "get_annotation_type_by_name",
        "32.1.2.17",
        (0, MAX_STUB_ARGS),
    ),
    ("get_template_type_by_name", "32.1.2.17", (0, MAX_STUB_ARGS)),
    ("get_template_by_name", "32.1.2.17", (0, MAX_STUB_ARGS)),
    (
        "get_template_interface_by_name",
        "32.1.2.17",
        (0, MAX_STUB_ARGS),
    ),
    ("get_user_template_types", "32.1.2.17", (0, MAX_STUB_ARGS)),
    ("get_user_templates", "32.1.2.17", (0, MAX_STUB_ARGS)),
    // 32.1.3 definition elements, layers, modules.
    ("get_source_lines_count", "32.1.3", (0, MAX_STUB_ARGS)),
    ("get_added_interfaces", "32.1.3", (0, MAX_STUB_ARGS)),
    ("get_exact_subtype", "32.1.3", (0, MAX_STUB_ARGS)),
    ("get_instance_layers", "32.1.3", (0, MAX_STUB_ARGS)),
    ("get_layers", "32.1.3", (0, MAX_STUB_ARGS)),
    ("get_template_layer", "32.1.3", (0, MAX_STUB_ARGS)),
    ("get_actual_importer", "32.1.3", (0, MAX_STUB_ARGS)),
    ("is_top_module", "32.1.3", (0, MAX_STUB_ARGS)),
    ("get_full_file_name", "32.1.3", (0, MAX_STUB_ARGS)),
    ("get_file_age", "32.1.3", (0, MAX_STUB_ARGS)),
    // 32.1.4 value and manager utilities.
    ("value_to_type", "32.1.4", (2, 2)),
    ("eval_exp_string", "32.1.4", (1, 1)),
    ("insert_list_element", "32.1.4", (0, MAX_STUB_ARGS)),
    ("insert_list_element_unsafe", "32.1.4", (0, MAX_STUB_ARGS)),
    ("delete_list_element", "32.1.4", (0, MAX_STUB_ARGS)),
    ("delete_list_element_unsafe", "32.1.4", (0, MAX_STUB_ARGS)),
    ("get_field_of_unit_instance", "32.1.4.6", (1, 1)),
    ("get_list_indices_of_unit_instance", "32.1.4.6", (1, 1)),
    ("get_list_index_of_unit_instance", "32.1.4.6", (1, 1)),
];

/// Instantiate the built-in `sys` unit and execute its `run()` methods.
pub fn run(elaborated: &Elaborated, seed: u64) -> RunResult {
    run_inner(elaborated, seed, None)
}

/// [`run`] with an injected signal history standing in for the external
/// simulator: the `hdl_*` routines and the `@sim`/event-port sampling
/// read `signals` instead of a spawned `xezim` session (the simulator
/// binary is only provided in the container — tests and offline runs use
/// this seam). A `hdl_load` in the source replaces the injected source
/// with a real session.
pub fn run_with_signals(
    elaborated: &Elaborated,
    seed: u64,
    signals: crate::hdl::InjectedSignals,
) -> RunResult {
    run_inner(
        elaborated,
        seed,
        Some(crate::hdl::SignalSource::Injected(signals)),
    )
}

fn run_inner(
    elaborated: &Elaborated,
    seed: u64,
    hdl: Option<crate::hdl::SignalSource>,
) -> RunResult {
    // Resolve `define` constants (literal values only; non-literals are
    // skipped — they need the elaboration-time substitution that a macro
    // pass would provide).
    let defines: std::collections::BTreeMap<String, Value> = elaborated
        .defines
        .iter()
        .filter_map(|(name, expr)| {
            let value = match expr {
                Expr::IntLiteral { text, .. } => parse_int_literal(text).map(Value::Int),
                Expr::BitLiteral { text, .. } => parse_int_literal(text).map(Value::Int),
                Expr::StringLiteral { value, .. } => Some(Value::Str(unescape(value))),
                Expr::BoolLiteral { value, .. } => Some(Value::Bool(*value)),
                _ => None,
            }?;
            Some((name.clone(), value))
        })
        .collect();
    // The simulator-side event triggers: the `rise/fall/change(HDL
    // expression) @sim` shapes (11.1.4.2) that the run loop samples each
    // virtual cycle.
    let sim_triggers: Vec<SimTrigger> = elaborated
        .types
        .iter()
        .flat_map(|ty| {
            ty.events.iter().filter_map(|event| {
                let trigger = event.trigger.as_ref()?;
                // Only the single-item edge shapes are wired: multi-item
                // triggers, `cycle`, grouped guards and `true(exp)`
                // samples would need the temporal engine at the sampling
                // point (documented v1 limit).
                if trigger.cycle || trigger.group_op.is_some() || trigger.items.len() != 1 {
                    return None;
                }
                let item = &trigger.items[0];
                // `@sim` is the simulation tick the virtual cycles stand
                // in for; other sampling events are not wired here.
                if item.path.as_slice() != ["sim"] {
                    return None;
                }
                let routine = item.routine.as_deref()?;
                if !matches!(routine, "rise" | "fall" | "change") {
                    return None;
                }
                Some(SimTrigger {
                    type_name: ty.name.clone(),
                    event_name: event.name.clone(),
                    routine: routine.to_owned(),
                    condition: item.condition.clone()?,
                    span: event.span,
                })
            })
        })
        .collect();
    let mut interp = Interp {
        types: elaborated.types.as_slice(),
        instances: Vec::new(),
        rng: Rng::new(seed),
        seed,
        output: String::new(),
        depth: 0,
        hdl,
        cycle: 0,
        wait_budget: WAIT_BUDGET,
        emitted: std::collections::BTreeSet::new(),
        sim_triggers,
        sim_prev: BTreeMap::new(),
        port_edges: collect_port_edge_attributes(elaborated),
        temporal_log: Vec::new(),
        coroutines: Vec::new(),
        tracing: false,
        atomic_depth: 0,
        exec_context: Vec::new(),
        pending_suspension: None,
        pending_wake_sync: None,
        current_coroutine_id: None,
        next_coroutine_id: 0,
        resume_wait_start: None,
        coverage: crate::coverage::build(elaborated),
        stop_requested: false,
        dut_errors: 0,
        defines,
        statics: BTreeMap::new(),
        static_fields: &elaborated.static_fields,
        static_methods: &elaborated.static_methods,
        static_events: &elaborated.static_events,
        static_ons: &elaborated.static_ons,
        annotation_types: &elaborated.annotation_types,
        attached_annotations: &elaborated.attached_annotations,
        static_context: None,
        expected_type: None,
        new_type_hint: None,
        pending_diagnostics: Vec::new(),
        type_port_binds: collect_type_port_binds(elaborated),
        type_port_unbound: collect_type_port_unbound(elaborated),
        disconnected_ports: BTreeSet::new(),
        post_generate_done: false,
        in_connect_ports: false,
        overall_grade: None,
        table_cache: BTreeMap::new(),
    };
    // The static fields are created at declaration (clause 6.9.3):
    // every store entry exists before any instance does, initialised
    // from the init value or the type default. Init values are constant
    // expressions, so the empty-frame / no-instance context suffices;
    // an expression that cannot evaluate keeps the type default and the
    // failure is reported through the run diagnostics.
    let static_inits: Vec<(String, Field)> = elaborated
        .static_fields
        .iter()
        .flat_map(|(type_name, fields)| {
            fields
                .iter()
                .map(move |field| (type_name.clone(), field.clone()))
        })
        .collect();
    for (type_name, field) in static_inits {
        let value = match &field.init {
            Some(init) => {
                let mut frame = Frame::default();
                match interp.eval(init, &mut frame, None) {
                    Ok(value) => value,
                    Err(diagnostic) => {
                        interp.pending_diagnostics.push(diagnostic);
                        interp.default_for(&field.ty)
                    }
                }
            }
            None => interp.default_for(&field.ty),
        };
        interp
            .statics
            .entry(type_name)
            .or_default()
            .insert(field.name, value);
    }
    let types: &[crate::elab::ElaboratedType] = interp.types;
    let Some(sys) = types
        .iter()
        .find(|t| t.name == "sys" && t.kind == DeclarationKind::Unit)
    else {
        // No `sys`: a declarations-only program runs to completion with no
        // output.
        return RunResult {
            output: interp.output,
            diagnostics: interp.pending_diagnostics,
            coverage: interp.coverage,
        };
    };
    let run_blocks: Vec<&MethodDecl> = sys
        .methods
        .iter()
        .filter(|method| method.name == "run")
        .collect();
    let sys_index = interp.instantiate("sys");
    let mut diagnostics = Vec::new();
    // Table 1's setup phase (28.3): `sys.setup()` runs before any
    // generation — the root unit's own setup blocks.
    interp.call_phase_method(sys_index, "setup", &mut diagnostics);
    // Specman generates the root unit's data fields before run() starts
    // (respecting `keep` constraints and skipping `!` fields); mirror
    // that so corpus testbenches see generated lists/scalars. Both this
    // and the sequence start happen even when no explicit run() exists.
    if let Err(diagnostic) = interp.gen_object(sys_index, &[]) {
        interp
            .output
            .push_str(&format!("implicit gen: {}\n", diagnostic.render()));
    }
    // The generation sub-phases of 9.13: after the tree is generated,
    // `pre_generate()` and `post_generate()` run over the field tree
    // (depth-first, field order), then the three binding passes
    // `connect_pointers()` (9.13.2), `connect_ports()` (9.13.3 — the
    // only sub-phase `do_bind()` may run in) and `check_generation()`
    // (9.13.4).
    interp.run_phase_on_tree(sys_index, "pre_generate", &mut diagnostics);
    interp.run_phase_on_tree(sys_index, "post_generate", &mut diagnostics);
    interp.post_generate_done = true;
    interp.run_phase_on_tree(sys_index, "connect_pointers", &mut diagnostics);
    interp.in_connect_ports = true;
    interp.run_phase_on_tree(sys_index, "connect_ports", &mut diagnostics);
    interp.in_connect_ports = false;
    interp.run_phase_on_tree(sys_index, "check_generation", &mut diagnostics);
    // Start each declared sequence's MAIN body inline: Specman's
    // sequencer launches MAIN during test start; the inline subset runs
    // it synchronously with `driver` bound to the driver instance.
    let sequences: Vec<crate::elab::SequenceInfo> = elaborated.sequences.clone();
    for sequence in &sequences {
        let Some(driver_index) = interp
            .instances
            .iter()
            .position(|instance| instance.type_name == sequence.driver)
        else {
            continue;
        };
        let sequence_index = interp.instantiate(&sequence.name);
        interp.instances[sequence_index].set("driver", Value::Object(driver_index));
        let types: &[crate::elab::ElaboratedType] = interp.types;
        let Some(body) = types
            .iter()
            .find(|t| t.name == sequence.name)
            .and_then(|t| t.methods.iter().find(|m| m.name == "body").cloned())
        else {
            continue;
        };
        let mut frame = Frame::default();
        frame
            .locals
            .insert("driver".to_owned(), Value::Object(driver_index));
        if let Err(diagnostic) = interp.exec_block(&body.body, &mut frame, Some(sequence_index)) {
            interp.output.push_str(&format!(
                "sequence {}: {}\n",
                sequence.name,
                diagnostic.render()
            ));
        }
    }
    // Clause 13.2.2 scheduler: the run() bodies run as the main
    // coroutine; every `start`ed TCM joins the queue with an implicit
    // sync cycle before it commences (a TCM declared `@event` with a
    // declared sampling event waits for that event's occurrence instead
    // — 17.1.2). Round-robin turns run atomically
    // from one synchronization action to the next; when no coroutine is
    // eligible, one cycle passes.
    interp.coroutines.push(Coroutine {
        kind: CoroutineKind::Main {
            blocks: run_blocks.iter().map(|b| b.body.as_slice()).collect(),
            next_block: 0,
        },
        id: interp.next_coroutine_id,
        path: Vec::new(),
        frame: Frame::default(),
        me: Some(sys_index),
        eligible_at_cycle: 0,
        wake_event: None,
        wake_sync: None,
        suspended_temporal: false,
        temporal_waited: 0,
        span: origin_span(),
        wait_start: None,
    });
    interp.next_coroutine_id += 1;
    // Cycle 0's sample only establishes the monitored values' baseline —
    // no previous value, so no edge can fire on it.
    interp.sample_sim_events();
    // The clause 11.4 live monitoring: one pass per scheduler cycle
    // boundary, over every not-yet-fired rule.
    let expects: Vec<&crate::ast::ExpectDecl> = elaborated
        .types
        .iter()
        .flat_map(|t| t.expects.iter())
        .collect();
    let mut expect_fired = vec![false; expects.len()];
    // One streaming monitor per rule: the live passes feed it the run
    // log incrementally and the quit pass reads its end-of-run verdict.
    let mut rule_monitors: Vec<crate::temporal::RuleMonitor> = expects
        .iter()
        .map(|rule| crate::temporal::RuleMonitor::new(&rule.te))
        .collect();
    while !interp.coroutines.is_empty() && !interp.stop_requested {
        // The clause 33 wake pass: a blocking call parked a coroutine on
        // a sync object; service the oldest waiter whose condition
        // holds, before the turn pick — a resource produced in one
        // coroutine's turn reaches its blocked consumer in this same
        // scheduling round.
        interp.service_sync_waiters();
        let Some(index) = interp.coroutines.iter().position(|c| {
            c.eligible_at_cycle <= interp.cycle && c.wake_event.is_none() && c.wake_sync.is_none()
        }) else {
            // Nobody eligible: suspended coroutines wait for the next
            // occurrence of their sampling event — one cycle passes.
            if interp.wait_budget == 0 {
                diagnostics.push(interp.error(
                    "E0409",
                    format!("cycle budget exceeded at cycle {}", interp.cycle),
                    origin_span(),
                ));
                break;
            }
            interp.wait_budget -= 1;
            interp.cycle += 1;
            interp.pad_log();
            // The virtual cycle advanced: sample the simulator-side
            // triggers (11.1.4.2). A fire feeds the temporal log, runs
            // the event's `on` blocks and wakes TCMs parked on it.
            interp.sample_sim_events();
            // The clause 11.4 monitoring: a rule that has become
            // unsatisfiable over the log prefix fires here — the failure
            // is reported (else body runs, DUT error counted) at the
            // cycle it became certain, not deferred to quit.
            check_expect_rules(
                &mut interp,
                &expects,
                &mut expect_fired,
                &mut rule_monitors,
                sys_index,
            );
            let mut expired = Vec::new();
            for (position, co) in interp.coroutines.iter_mut().enumerate() {
                if co.suspended_temporal {
                    co.temporal_waited += 1;
                    if co.temporal_waited > TEMPORAL_WAIT_CYCLES {
                        expired.push(position);
                    }
                }
            }
            for position in expired.into_iter().rev() {
                let co = interp.coroutines.remove(position);
                diagnostics.push(interp.error(
                    "E0409",
                    format!(
                        "temporal wait unsatisfied after {} cycles at cycle {}",
                        co.temporal_waited, interp.cycle
                    ),
                    co.span,
                ));
            }
            continue;
        };
        let mut co = interp.coroutines.remove(index);
        interp.tracing = true;
        interp.exec_context.clear();
        interp.pending_wake_sync = None;
        interp.current_coroutine_id = Some(co.id);
        interp.resume_wait_start = co.wait_start.take();
        let name = match &co.kind {
            CoroutineKind::Main { .. } => "run".to_owned(),
            CoroutineKind::Tcm { method, .. } => method.clone(),
        };
        let outcome = interp.run_coroutine_turn(&mut co);
        interp.tracing = false;
        interp.exec_context.clear();
        interp.pending_wake_sync = None;
        interp.current_coroutine_id = None;
        interp.resume_wait_start = None;
        match outcome {
            TurnOutcome::Complete => {}
            TurnOutcome::Suspended => {
                co.temporal_waited = 0;
                interp.coroutines.push(co);
            }
            TurnOutcome::Error(diagnostic) => {
                if matches!(co.kind, CoroutineKind::Main { .. }) {
                    diagnostics.push(diagnostic);
                    break;
                }
                interp
                    .output
                    .push_str(&format!("tcm {name}: {}\n", diagnostic.render()));
            }
        }
    }
    // The Table 1 run phase on the rest of the field tree: sys's own
    // run() blocks ran as the main coroutine above; after the scheduler
    // drains, run() is called on every other live instance, depth-first
    // in field order (28.3). The extract, check and finalize phases
    // follow the same order. A `stop_run()` skips the remaining phases.
    if !interp.stop_requested {
        for index in interp.field_tree(sys_index).into_iter().skip(1) {
            interp.call_phase_method(index, "run", &mut diagnostics);
        }
        // The extract phase (28.3.3.6).
        interp.run_phase_on_tree(sys_index, "extract", &mut diagnostics);
        // The check phase (28.3.3.7): the per-struct check() hooks, then
        // the expect/assume rules re-check at quit for anything the live
        // monitoring did not already fire.
        interp.run_phase_on_tree(sys_index, "check", &mut diagnostics);
    }
    // 11.4: the quit-time re-check of the rules the live passes did not
    // already fire — the streaming monitors consume any log growth since
    // the last boundary and report their end-of-run verdicts (12.3.3:
    // the obligation windows exhaust when the run ends — the earliest
    // sound closure point).
    check_expect_rules_at_quit(
        &mut interp,
        &expects,
        &mut expect_fired,
        &mut rule_monitors,
        sys_index,
    );
    // The finalize phase (28.3.3.8): `quit()` then `finalize()` on every
    // live instance, depth-first, sys first.
    if !interp.stop_requested {
        for index in interp.field_tree(sys_index) {
            interp.call_phase_method(index, "quit", &mut diagnostics);
            interp.call_phase_method(index, "finalize", &mut diagnostics);
        }
    }
    // Specman closes every run with a DUT-error count; mirror it.
    if interp.dut_errors > 0 {
        interp.output.push_str(&format!(
            "Checking is complete - {} DUT error(s).\n",
            interp.dut_errors
        ));
    }
    // Field init-value failures collected during instance creation
    // (clause 6.9.1/6.9.4) join the run's diagnostics.
    diagnostics.append(&mut interp.pending_diagnostics);
    RunResult {
        output: interp.output,
        diagnostics,
        coverage: interp.coverage,
    }
}

fn origin() -> Position {
    Position {
        byte: 0,
        line: 1,
        column: 1,
    }
}

/// The clause 11.4 live monitoring pass: for every not-yet-fired
/// expect/assume rule, re-evaluate its propositions with the current
/// field values and check whether the rule has failed over the log so
/// far ("all possibilities to satisfy it are exhausted", 12.3.3). The
/// pass that first sees the failure fires the rule: the `else` body
/// runs (or the failure itself is the check error, 11.4) and the rule
/// retires — it is not re-checked. A rule whose TE cannot be evaluated
/// over the log reports the gap honestly and retires too, so the live
/// pass never spams.
fn check_expect_rules(
    interp: &mut Interp,
    expects: &[&crate::ast::ExpectDecl],
    expect_fired: &mut [bool],
    rule_monitors: &mut [crate::temporal::RuleMonitor],
    sys_index: usize,
) {
    for (rule_index, expect_rule) in expects.iter().enumerate() {
        if expect_fired[rule_index] || interp.stop_requested {
            continue;
        }
        let kind = format!("{:?}", expect_rule.kind).to_lowercase();
        let (booleans, numerics) =
            interp.evaluate_temporal_exprs(&expect_rule.te, &mut Frame::default(), Some(sys_index));
        let callbacks = temporal_callbacks(&booleans, &numerics);
        // The monitor consumes only FINALIZED states: the current
        // cycle's state is still in progress (this cycle's emits record
        // into it during turns), so it is handed over at the next
        // boundary, after the cycle closed. The state index equals the
        // cycle (pad_log keeps one state per cycle).
        let final_len = (interp.cycle as usize).min(interp.temporal_log.len());
        match rule_monitors[rule_index].observe(&interp.temporal_log[..final_len], &callbacks) {
            Ok(true) => {
                // The live fire: the streaming monitor matched the
                // forbidden behavior of an invariant rule — the same
                // prefix-certain verdict the previous full-rescan pass
                // computed, now incremental.
                fire_expect_rule(interp, expect_rule, &kind, sys_index);
                expect_fired[rule_index] = true;
            }
            Ok(false) => {}
            Err(err) => {
                // The rule cannot be evaluated over the log: report the
                // gap honestly, once, and retire the rule.
                expect_fired[rule_index] = true;
                interp.output.push_str(&format!(
                    "{kind} rule cannot be evaluated over the run: {err:?}\n"
                ));
            }
        }
    }
}

/// Fire one expect/assume rule's failure: the `else` body runs on the
/// sys instance (or, without one, the failure itself is the check
/// error, 11.4).
fn fire_expect_rule(
    interp: &mut Interp,
    expect_rule: &crate::ast::ExpectDecl,
    kind: &str,
    sys_index: usize,
) {
    if expect_rule.else_body.is_empty() {
        // No `dut_error` clause: the failure itself is the check error
        // (11.4 — the tool prints the rule).
        interp.dut_errors += 1;
        interp.output.push_str(&format!(
            "{kind} rule failed: temporal property not satisfied, no else body\n"
        ));
    } else {
        let mut frame = Frame::default();
        if let Err(diagnostic) =
            interp.exec_block(&expect_rule.else_body, &mut frame, Some(sys_index))
        {
            interp
                .output
                .push_str(&format!("{kind} rule else body: {}\n", diagnostic.render()));
        }
    }
}

/// The quit-time re-check of the expect/assume rules the live passes
/// (one per scheduler cycle boundary) did not already fire — 12.3.3:
/// the rule fails where all possibilities to satisfy it are exhausted.
/// The `else` body (usually a `dut_error` call) runs on
/// the sys instance; without one, the failure itself is the check
/// error. Rules whose TE cannot be evaluated over the log report the
/// gap honestly.
fn check_expect_rules_at_quit(
    interp: &mut Interp,
    expects: &[&crate::ast::ExpectDecl],
    expect_fired: &mut [bool],
    rule_monitors: &mut [crate::temporal::RuleMonitor],
    sys_index: usize,
) {
    for (rule_index, expect_rule) in expects.iter().enumerate() {
        if expect_fired[rule_index] || interp.stop_requested {
            continue;
        }
        let kind = format!("{:?}", expect_rule.kind).to_lowercase();
        let (booleans, numerics) =
            interp.evaluate_temporal_exprs(&expect_rule.te, &mut Frame::default(), Some(sys_index));
        let callbacks = temporal_callbacks(&booleans, &numerics);
        // The monitor catches any log growth since the last boundary,
        // then its end-of-run verdict decides (equivalent to the
        // previous whole-log `rule_failed` re-scan).
        match rule_monitors[rule_index].observe(&interp.temporal_log, &callbacks) {
            Ok(_) => {}
            Err(err) => {
                expect_fired[rule_index] = true;
                interp.output.push_str(&format!(
                    "{kind} rule cannot be evaluated over the run: {err:?}\n"
                ));
                continue;
            }
        }
        if rule_monitors[rule_index].failed_at_end() {
            fire_expect_rule(interp, expect_rule, &kind, sys_index);
            expect_fired[rule_index] = true;
        }
    }
}

fn origin_span() -> crate::ast::Span {
    (origin(), origin())
}

impl<'a> Interp<'a> {
    /// The struct/unit instances of the field tree under `root`, in
    /// depth-first field order — the order the Table 1 phase methods
    /// (28.3: `setup`, `run`, `extract`, `check`, `quit`, `finalize`)
    /// and the generation sub-phases (9.13) are called: the root first,
    /// then each instance-typed field's own tree in declaration order,
    /// list items flattened in index order, NULL fields skipped.
    /// Struct instances created outside the field tree (a local `var`
    /// initialized with `gen` or `new`) are not part of it.
    fn field_tree(&self, root: usize) -> Vec<usize> {
        let mut order = Vec::new();
        let mut visited = BTreeSet::new();
        self.collect_field_tree(root, &mut order, &mut visited, 0);
        order
    }

    fn collect_field_tree(
        &self,
        index: usize,
        order: &mut Vec<usize>,
        visited: &mut BTreeSet<usize>,
        depth: u32,
    ) {
        if depth > 64 || !visited.insert(index) {
            return;
        }
        order.push(index);
        // Snapshot the field values: the walk never holds a borrow (a
        // list's shared storage may be mutated inside a deeper visit).
        let fields: Vec<Value> = self.instances[index]
            .fields
            .iter()
            .map(|(_, value)| value.clone())
            .collect();
        for value in fields {
            match value {
                Value::Object(child) => {
                    self.collect_field_tree(child, order, visited, depth + 1);
                }
                Value::List(list) => {
                    let items = list.items.borrow().clone();
                    for item in items {
                        if let Value::Object(child) = item {
                            self.collect_field_tree(child, order, visited, depth + 1);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Call one Table 1 phase method (28.3) or generation sub-phase
    /// (9.13) on every field-tree instance under `root`, in the same
    /// depth-first field order as `run()`. A type that does not define
    /// the method contributes nothing — all the hooks are initially
    /// empty (any struct may declare or extend them); a run-time error
    /// inside a hook is reported and the phase continues with the next
    /// instance.
    fn run_phase_on_tree(&mut self, root: usize, name: &str, diagnostics: &mut Vec<Diagnostic>) {
        for index in self.field_tree(root) {
            self.call_phase_method(index, name, diagnostics);
        }
    }

    /// Run every composed block of the phase method `name` on one
    /// instance, if the instance's type defines any.
    fn call_phase_method(&mut self, index: usize, name: &str, diagnostics: &mut Vec<Diagnostic>) {
        let type_name = self.instances[index].type_name.clone();
        let defined = resolve_method_blocks(self.types, &type_name, name, origin_span()).is_ok();
        if !defined {
            return;
        }
        if let Err(diagnostic) = self.call_method(index, name, Vec::new(), origin_span()) {
            diagnostics.push(diagnostic);
        }
    }
}

/// The type-level `keep bind(p, q)` port-port bindings (9.13): type
/// name → (port, port) edges between the type's own ports, read from
/// the recorded bind constraints (`bind(a, b)` calls where both
/// operands name ports of the same declaration). Every instance of the
/// type carries the same edges between its own ports.
fn collect_type_port_binds(elaborated: &Elaborated) -> BTreeMap<String, Vec<(String, String)>> {
    let mut binds: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for ty in &elaborated.types {
        let port_names: BTreeSet<&str> = ty.ports.iter().map(|p| p.name.as_str()).collect();
        for constraint in &ty.constraints {
            if !constraint.is_bind {
                continue;
            }
            let Expr::Call { callee, args, .. } = &constraint.expr else {
                continue;
            };
            let Expr::Ident { name, .. } = callee.as_ref() else {
                continue;
            };
            if name != "bind" || args.len() != 2 {
                continue;
            }
            let (Expr::Ident { name: first, .. }, Expr::Ident { name: second, .. }) =
                (&args[0], &args[1])
            else {
                continue;
            };
            if first == "empty"
                || second == "empty"
                || !port_names.contains(first.as_str())
                || !port_names.contains(second.as_str())
            {
                continue;
            }
            binds
                .entry(ty.name.clone())
                .or_default()
                .push((first.clone(), second.clone()));
        }
    }
    binds
}

/// The type-level `keep bind(p, empty)` spellings (9.13): ports bound
/// to nothing on purpose, per type.
fn collect_type_port_unbound(elaborated: &Elaborated) -> BTreeMap<String, Vec<String>> {
    let mut unbound: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for ty in &elaborated.types {
        let port_names: BTreeSet<&str> = ty.ports.iter().map(|p| p.name.as_str()).collect();
        for constraint in &ty.constraints {
            if !constraint.is_bind {
                continue;
            }
            let Expr::Call { callee, args, .. } = &constraint.expr else {
                continue;
            };
            let Expr::Ident { name, .. } = callee.as_ref() else {
                continue;
            };
            if name != "bind" || args.len() != 2 {
                continue;
            }
            let (Expr::Ident { name: first, .. }, Expr::Ident { name: second, .. }) =
                (&args[0], &args[1])
            else {
                continue;
            };
            if second == "empty" && port_names.contains(first.as_str()) {
                unbound
                    .entry(ty.name.clone())
                    .or_default()
                    .push(first.clone());
            }
        }
    }
    unbound
}

/// Pre-evaluated temporal propositions and bounds, keyed by expression
/// identity (see [`Interp::evaluate_temporal_exprs`]).
type TemporalValues<'te> = (
    Vec<(&'te crate::ast::Expr, Option<bool>)>,
    Vec<(&'te crate::ast::Expr, Option<i128>)>,
);

/// Resolve a method's `is also` blocks on a type, as written in the
/// elaborated member lists (base types first, then extends).
fn resolve_method_blocks<'a>(
    types: &'a [crate::elab::ElaboratedType],
    type_name: &str,
    name: &str,
    span: crate::ast::Span,
) -> Result<Vec<&'a MethodDecl>, Diagnostic> {
    let Some(ty) = types.iter().find(|t| t.name == type_name) else {
        return Err(Diagnostic::new(
            "E0401",
            format!("unknown type '{type_name}'"),
            span.0,
            span.1,
        ));
    };
    let blocks: Vec<&'a MethodDecl> = ty
        .methods
        .iter()
        .filter(|method| method.name == name)
        .collect();
    if blocks.is_empty() {
        return Err(Diagnostic::new(
            "E0401",
            format!("method '{name}' not found on '{type_name}'"),
            span.0,
            span.1,
        ));
    }
    Ok(blocks)
}

/// The engine callback set over pre-evaluated propositions and bounds:
/// expressions are matched by identity (the engine passes back the
/// references [`Interp::evaluate_temporal_exprs`] collected).
fn temporal_callbacks<'a>(
    booleans: &'a [(&'a crate::ast::Expr, Option<bool>)],
    numerics: &'a [(&'a crate::ast::Expr, Option<i128>)],
) -> crate::temporal::EvalCallbacks<'a> {
    crate::temporal::EvalCallbacks {
        boolean: Box::new(move |expr| {
            booleans
                .iter()
                .find(|(key, _)| std::ptr::eq(*key, expr))
                .and_then(|(_, value)| *value)
        }),
        numeric: Box::new(move |expr| {
            numerics
                .iter()
                .find(|(key, _)| std::ptr::eq(*key, expr))
                .and_then(|(_, value)| *value)
        }),
    }
}

impl<'a> Interp<'a> {
    fn instantiate(&mut self, type_name: &str) -> usize {
        let mut fields = Vec::new();
        let mut ports = Vec::new();
        let mut when_of: Vec<(String, String)> = Vec::new();
        let types: &[crate::elab::ElaboratedType] = self.types;
        if let Some(ty) = types.iter().find(|t| t.name == type_name) {
            for field in &ty.fields {
                // A static field does not live in instances (clause
                // 6.9.3): its one shared occurrence is in the per-type
                // static store, so instance creation skips it — which
                // also keeps it out of `gen` and out of instance
                // formatting (`print`, `to_string`).
                if field.is_static {
                    continue;
                }
                let value = match &field.init {
                    // Clause 6.9.1/6.9.4: a field's init value is set at
                    // instance creation, before `init()` runs. Init
                    // values are constant expressions, so the empty
                    // frame / no-instance context suffices; an expression
                    // that cannot evaluate keeps the type default and the
                    // failure is reported through the run diagnostics.
                    // The value binds through the field's declared type
                    // (T7.1: a custom-numeric field converts a scalar
                    // init via `numeric_from_int`/`numeric_from_real`,
                    // and a custom value copies).
                    Some(init) => {
                        let mut frame = Frame::default();
                        match self.eval(init, &mut frame, None) {
                            Ok(value) => {
                                match self.bind_slot_value(value, Some(&field.ty), origin_span()) {
                                    Ok(value) => value,
                                    Err(diagnostic) => {
                                        self.pending_diagnostics.push(diagnostic);
                                        self.default_for(&field.ty)
                                    }
                                }
                            }
                            Err(diagnostic) => {
                                self.pending_diagnostics.push(diagnostic);
                                self.default_for(&field.ty)
                            }
                        }
                    }
                    None => self.default_for(&field.ty),
                };
                fields.push((field.name.clone(), value));
            }
            for port in &ty.ports {
                if let Some(hdl_path) = &port.hdl_path {
                    ports.push((port.name.clone(), hdl_path.clone()));
                }
            }
            when_of = ty.when_of.clone();
        } else if let Some(base_name) = type_name.rsplit(' ').next()
            && let Some(base) = types.iter().find(|t| t.name == base_name)
        {
            // A when-qualified type spelling (`good packet`) whose exact
            // subtype view was not synthesized (no `when good packet`
            // block): the instance carries the base's fields under the
            // spelling's name (6.7). Its determinants are fixed by the
            // qualifier words at the type position that named it (see
            // [`Interp::default_for`]).
            for field in &base.fields {
                let default = self.default_for(&field.ty);
                fields.push((field.name.clone(), default));
            }
        }
        let index = self.instances.len();
        self.instances.push(ObjectInstance {
            type_name: type_name.to_owned(),
            fields,
            ports,
            rgf: BTreeMap::new(),
        });
        // A subtype view instance carries its determinant values (6.7):
        // the view's (field id, value) pairs hold from instantiation on,
        // and generation never samples them (they are what the view is).
        for (id, value) in when_of {
            if let Some(v) = self.determinant_value(index, &id, &value) {
                self.instances[index].set(&id, v);
            }
        }
        // The composed `init()` blocks run at instance creation (the
        // clause 17.1 implicit call; the static-members runtime test
        // relies on it). e calls `init()` after the instance's
        // generation; this model runs it at creation — the only point
        // shared by every creation site — so a later explicit `gen` of
        // the instance can overwrite generated fields it set (a
        // documented approximation). The blocks run inline (an atomic
        // region: no suspension), with the fresh instance as `me`; a
        // block's error is collected and instance creation still
        // succeeds, like a field init failure.
        if self.depth <= 100 {
            let init_blocks: Vec<Vec<Stmt>> = types
                .iter()
                .find(|t| t.name == type_name)
                .map(|ty| {
                    ty.methods
                        .iter()
                        .filter(|method| method.name == "init")
                        .map(|method| method.body.clone())
                        .collect()
                })
                .unwrap_or_default();
            self.depth += 1;
            self.atomic_depth += 1;
            for body in &init_blocks {
                let mut frame = Frame::default();
                match self.exec_block(body, &mut frame, Some(index)) {
                    Ok(Flow::Stop) => self.stop_requested = true,
                    Ok(_) => {}
                    Err(diagnostic) => self.pending_diagnostics.push(diagnostic),
                }
            }
            self.atomic_depth -= 1;
            self.depth -= 1;
        }
        index
    }

    /// The value a determinant field of an instance takes under a
    /// subtype view's (field id, value) pair: an enumerand of the
    /// field's type, a Boolean, or a numeric literal.
    fn determinant_value(&self, index: usize, id: &str, word: &str) -> Option<Value> {
        let type_name = self.instances[index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let ty = types.iter().find(|t| t.name == type_name)?;
        let field = ty.fields.iter().find(|field| field.name == id)?;
        match &field.ty {
            TypeExpr::Scalar {
                base: ScalarBase::Bool,
                ..
            } => Some(Value::Bool(word.eq_ignore_ascii_case("true"))),
            TypeExpr::Scalar { .. } => word.parse::<i128>().ok().map(Value::Int),
            TypeExpr::Named { name, .. } => {
                let declared = types
                    .iter()
                    .find(|t| &t.name == name && t.kind == DeclarationKind::Type)?;
                // The word must be one of the type's enumerands.
                if !declared
                    .enumerals
                    .iter()
                    .any(|enumeral| enumeral.name == word)
                {
                    return None;
                }
                Some(Value::Enum {
                    type_name: name.clone(),
                    name: word.to_owned(),
                })
            }
            TypeExpr::InlineEnum { names, .. } => Some(Value::Enum {
                type_name: inline_enum_key(names),
                name: word.to_owned(),
            }),
            _ => None,
        }
    }

    fn default_for(&mut self, ty: &TypeExpr) -> Value {
        match ty {
            TypeExpr::Scalar { base, .. } => match base {
                ScalarBase::Bool => Value::Bool(false),
                ScalarBase::Real => Value::Real(0.0),
                ScalarBase::String => Value::Str(String::new()),
                _ => Value::Int(0),
            },
            TypeExpr::List { element, .. } => Value::List(ListValue::new(Some(element.render()))),
            TypeExpr::Named { name, .. } if name == "set" => {
                // The set type's default is the empty set (5.2.12: `[]`).
                Value::Set(Vec::new())
            }
            TypeExpr::InlineEnum { names, .. } => {
                // The default enumerand is the first one declared.
                match names.first() {
                    Some(first) => Value::Enum {
                        type_name: inline_enum_key(names),
                        name: first.clone(),
                    },
                    None => Value::Void,
                }
            }
            TypeExpr::Named { name, .. } => {
                // The 32.3 table types have no default value: a table
                // var's default is NULL, like an interface pointer.
                if matches!(name.as_str(), "table" | "table_row" | "table_manager") {
                    return Value::Void;
                }
                // The 32.2 instance iterator and the recognised
                // reflection types (T11.1/T11.2) default to NULL too:
                // an unstarted iterator and an unmodeled rf_* value are
                // both the null reference, and a default must never
                // construct (`new` on the reflection stubs reports
                // E0419 instead).
                if name == "instance_iterator"
                    || crate::builtins::is_recognized_reflection_type(name)
                {
                    return Value::Void;
                }
                let types: &[crate::elab::ElaboratedType] = self.types;
                if let Some(t) = types.iter().find(|t| t.name == name.as_str()) {
                    // Clause 5.2.9/5.4.4.4: an interface-typed value is a
                    // pointer to an instance that implements the
                    // interface, or NULL — the default.
                    if t.kind == DeclarationKind::Interface {
                        return Value::Void;
                    }
                    // A custom numeric type defaults to a fresh value
                    // of its implementing struct, initialized by
                    // `set_initial_value()` when the struct defines it
                    // (T7.1's defaulting rule).
                    if t.kind == DeclarationKind::NumericType {
                        return self.new_custom_numeric(name, origin_span());
                    }
                    if t.kind == DeclarationKind::Type
                        && let Some(first) = t.enumerals.first()
                    {
                        return Value::Enum {
                            type_name: name.clone(),
                            name: first.name.clone(),
                        };
                    }
                }
                if self.depth > 100 {
                    return Value::Void;
                }
                self.depth += 1;
                let index = self.instantiate(name);
                self.depth -= 1;
                Value::Object(index)
            }
            TypeExpr::WhenQualified { value, base, .. } => {
                // A when-qualified field carries the synthesized subtype
                // view (6.7): its when members apply for the instance.
                // Chained qualifiers resolve to the combined view name.
                let Some(view_name) = when_view_name(value, base) else {
                    return Value::Void;
                };
                if self.depth > 100 {
                    return Value::Void;
                }
                self.depth += 1;
                let index = self.instantiate(&view_name);
                self.depth -= 1;
                // A spelling whose view was not synthesized (no `when`
                // block for it) still fixes its qualifier words as
                // determinants on the base-carried instance: `good
                // packet` means the instance's `good` determinant is
                // TRUE from instantiation on (6.7).
                if !self.types.iter().any(|t| t.name == view_name) {
                    let words = when_spelling_words(value, base);
                    if let Some(base_name) = when_view_base(base) {
                        for word in words {
                            self.set_spelled_determinant(index, &base_name, &word);
                        }
                    }
                }
                Value::Object(index)
            }
            // A template reference instantiates its monomorphized view
            // (8.3), which elaboration materialized under the canonical
            // name; a formal is bound only inside a template's own
            // declaration and stays void here. A monomorphized
            // `template numeric_type` instance (8.4) defaults as the
            // custom numeric value it is (T7.1), not as its struct.
            TypeExpr::TemplateRef { name, args, .. } => {
                let instance = crate::ast::template_instance_name(name, args);
                let types: &[crate::elab::ElaboratedType] = self.types;
                if types
                    .iter()
                    .any(|t| t.name == instance && t.kind == DeclarationKind::NumericType)
                {
                    return self.new_custom_numeric(&instance, origin_span());
                }
                if self.depth > 100 {
                    return Value::Void;
                }
                self.depth += 1;
                let index = self.instantiate(&instance);
                self.depth -= 1;
                Value::Object(index)
            }
            // `typeof()`/`typeof_item()` placeholders resolve during
            // elaboration (5.9); reaching the interpreter with one means
            // the static type was undetermined — no value. A value
            // actual and a width-qualified formal are template shapes
            // that never reach the interpreter as types.
            TypeExpr::TypeOf { .. } | TypeExpr::TypeOfItem { .. } => Value::Void,
            TypeExpr::Formal { .. } | TypeExpr::FormalWidth { .. } | TypeExpr::Value { .. } => {
                Value::Void
            }
        }
    }

    /// Fix one qualifier word of a when-qualified type spelling as a
    /// determinant value on a freshly instantiated base-carried
    /// instance: a Boolean field's own name means TRUE (`good` for
    /// `TRUE'good`); otherwise the word is an enumerand of some field's
    /// type (`red` of `color`).
    fn set_spelled_determinant(&mut self, index: usize, base_name: &str, word: &str) {
        let types: &[crate::elab::ElaboratedType] = self.types;
        let Some(base) = types.iter().find(|t| t.name == base_name) else {
            return;
        };
        // Boolean field named by the word (the bare `TRUE'good` form).
        if let Some(field) = base.fields.iter().find(|field| field.name == word)
            && matches!(
                &field.ty,
                TypeExpr::Scalar {
                    base: ScalarBase::Bool,
                    ..
                }
            )
        {
            self.instances[index].set(word, Value::Bool(true));
            return;
        }
        // An enumerand of a field's declared or inline enumeral type.
        for field in &base.fields {
            let holds = match &field.ty {
                TypeExpr::Named { name, .. } => types.iter().any(|t| {
                    &t.name == name
                        && t.kind == DeclarationKind::Type
                        && t.enumerals.iter().any(|e| e.name == word)
                }),
                TypeExpr::InlineEnum { names, .. } => names.iter().any(|n| n == word),
                _ => false,
            };
            if holds {
                self.instances[index].set(
                    &field.name,
                    Value::Enum {
                        type_name: match &field.ty {
                            TypeExpr::Named { name, .. } => name.clone(),
                            TypeExpr::InlineEnum { names, .. } => inline_enum_key(names),
                            _ => unreachable!("matched above"),
                        },
                        name: word.to_owned(),
                    },
                );
                return;
            }
        }
    }

    fn exec_block(
        &mut self,
        statements: &[Stmt],
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Flow, Diagnostic> {
        for (index, statement) in statements.iter().enumerate() {
            self.exec_context.push(PathLevel { index, child: None });
            let flow = self.exec_statement(statement, frame, me);
            self.exec_context.pop();
            match flow? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    /// Execute a statement chain resuming at the position named by
    /// `path` (13.2.2: resumption "commences executing from the first
    /// action following the synchronization action"). The path's first
    /// level names the statement to continue at: an already-executed
    /// prefix is skipped (no replay), a level whose child is set
    /// re-enters that compound mid-flight via
    /// [`Interp::exec_statement_resumable`], and the final childless
    /// level executes its statement (a re-evaluated temporal wait, or
    /// the first action following a consumed sync). Every compound on
    /// the way keeps its context level on the stack across its whole
    /// execution — a re-suspension inside a later iteration or a fresh
    /// statement after the resumed one rebuilds the same path.
    fn exec_block_resumable(
        &mut self,
        statements: &[Stmt],
        frame: &mut Frame,
        me: Option<usize>,
        path: &[PathLevel],
    ) -> Result<Flow, Diagnostic> {
        if path.is_empty() {
            return self.exec_block(statements, frame, me);
        }
        let head = path[0].index;
        for (index, statement) in statements.iter().enumerate() {
            if index < head {
                // Already executed before the suspension: skipped, not
                // replayed.
                continue;
            }
            if index == head {
                let Some(child) = path[0].child.clone() else {
                    if path.len() > 1 {
                        return Err(self.error(
                            "E0402",
                            "internal: resume path continues past a leaf statement",
                            statement.span(),
                        ));
                    }
                    // The final level: the statement itself. A position
                    // past the block's end (a sync consumed by the
                    // block's last statement) falls out of the loop and
                    // reports the block complete.
                    self.exec_context.push(PathLevel { index, child: None });
                    let flow = self.exec_statement(statement, frame, me);
                    self.exec_context.pop();
                    match flow? {
                        Flow::Normal => continue,
                        other => return Ok(other),
                    }
                };
                // Mid-flight compound: resume its interior, then run
                // the statement to completion.
                self.exec_context.push(PathLevel {
                    index,
                    child: Some(child.clone()),
                });
                let flow = self.exec_statement_resumable(statement, &child, &path[1..], frame, me);
                self.exec_context.pop();
                match flow? {
                    Flow::Normal => continue,
                    other => return Ok(other),
                }
            }
            self.exec_context.push(PathLevel { index, child: None });
            let flow = self.exec_statement(statement, frame, me);
            self.exec_context.pop();
            match flow? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    /// Execute one compound statement whose interior is mid-flight at
    /// a suspension: `child` names the branch the continuation
    /// descended into, `rest` the path within it. The statement's own
    /// header actions (a condition evaluation, a scrutinee match, the
    /// loop-bound evaluation) ran before the suspension and are not
    /// replayed; the resumed body then flows into the statement's
    /// ordinary continuation, which keeps the context level on the
    /// stack so a re-suspension rebuilds the matching path.
    fn exec_statement_resumable(
        &mut self,
        statement: &Stmt,
        child: &Child,
        rest: &[PathLevel],
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Flow, Diagnostic> {
        if rest.is_empty() {
            // A descent always carries the position it continues at
            // (the leaf level); reaching a compound without one is a
            // malformed path, not a fresh re-entry (which would replay
            // the body).
            return Err(self.error(
                "E0402",
                "internal: resume path descends without a position",
                statement.span(),
            ));
        }
        match (statement, child) {
            (Stmt::Compute { .. } | Stmt::ExprStmt { .. }, Child::Call(call)) => {
                self.exec_statement_call(call.as_ref().clone(), frame, Some(rest))
            }
            (Stmt::While { cond, body, .. }, Child::WhileBody) => {
                let first_flow = self.exec_block_resumable(body, frame, me, rest)?;
                self.while_loop(cond, body, frame, me, first_flow)
            }
            (Stmt::RepeatUntil { body, cond, .. }, Child::RepeatBody) => {
                self.repeat_until_loop(body, cond, frame, me, Some(rest))
            }
            (
                Stmt::ForTo {
                    var, down, body, ..
                },
                Child::ForBody { counter, end },
            ) => {
                // The in-flight iteration's counter and the
                // once-evaluated end bound come from the path; the
                // frame already binds the loop's temporary (inserted
                // when the iteration started).
                let first_flow = self.exec_block_resumable(body, frame, me, rest)?;
                self.for_to_loop(
                    var,
                    *down,
                    *end,
                    body,
                    frame,
                    me,
                    *counter,
                    Some(first_flow),
                )
            }
            (
                Stmt::ForEach {
                    var,
                    index_var,
                    body,
                    ..
                },
                Child::ForeachBody { position, items },
            ) => {
                // The in-flight element's binding lives in the frame;
                // the list snapshot from loop entry rides the path.
                let binding = var.clone().unwrap_or_else(|| "it".to_owned());
                self.for_each_loop(
                    &binding,
                    index_var.as_deref(),
                    items,
                    body,
                    frame,
                    me,
                    *position,
                    Some(rest),
                )
            }
            (Stmt::If { then_body, .. }, Child::IfThen) => {
                self.exec_block_resumable(then_body, frame, me, rest)
            }
            (Stmt::If { else_body, .. }, Child::IfElse) => {
                let Some(else_body) = else_body else {
                    return Err(self.error(
                        "E0402",
                        "internal: resume path descends into a missing else body",
                        statement.span(),
                    ));
                };
                self.exec_block_resumable(else_body, frame, me, rest)
            }
            (Stmt::Case { arms, default, .. }, Child::CaseArm { index }) => {
                let body = match index {
                    Some(arm) => arms.get(*arm).map(|(_, body)| body),
                    None => default.as_ref(),
                };
                let Some(body) = body else {
                    return Err(self.error(
                        "E0402",
                        "internal: resume path descends into a missing case arm",
                        statement.span(),
                    ));
                };
                self.exec_block_resumable(body, frame, me, rest)
            }
            _ => Err(self.error(
                "E0402",
                "internal: resume path crossed a statement without a matching child",
                statement.span(),
            )),
        }
    }

    /// The shared `while` loop: `first_flow` is the outcome of an
    /// already-running iteration (a resumed body) or [`Flow::Normal`]
    /// for a fresh entry.
    fn while_loop(
        &mut self,
        cond: &Expr,
        body: &[Stmt],
        frame: &mut Frame,
        me: Option<usize>,
        first_flow: Flow,
    ) -> Result<Flow, Diagnostic> {
        let mut pending = first_flow;
        loop {
            match pending {
                Flow::Normal => {}
                Flow::Continue => {}
                Flow::Break => return Ok(Flow::Normal),
                other => return Ok(other),
            }
            let Value::Bool(keep) = self.eval(cond, frame, me)? else {
                return Err(self.error("E0402", "while condition must be a boolean", cond.span()));
            };
            if !keep {
                return Ok(Flow::Normal);
            }
            pending = self.exec_block(body, frame, me)?;
        }
    }

    /// The shared `repeat until` loop (20.2.3): the body executes at
    /// least once, then repeatedly until the condition is TRUE.
    /// `in_flight` is the resume path of an already-running iteration
    /// (a suspension inside the body) or `None` for a fresh entry.
    fn repeat_until_loop(
        &mut self,
        body: &[Stmt],
        cond: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
        in_flight: Option<&[PathLevel]>,
    ) -> Result<Flow, Diagnostic> {
        let mut pending = match in_flight {
            Some(rest) => self.exec_block_resumable(body, frame, me, rest)?,
            None => self.exec_block(body, frame, me)?,
        };
        loop {
            match pending {
                Flow::Normal | Flow::Continue => {}
                Flow::Break => return Ok(Flow::Normal),
                other => return Ok(other),
            }
            let Value::Bool(keep_going) = self.eval(cond, frame, me)? else {
                return Err(self.error(
                    "E0402",
                    "repeat-until condition must be a boolean",
                    cond.span(),
                ));
            };
            if keep_going {
                return Ok(Flow::Normal);
            }
            pending = self.exec_block(body, frame, me)?;
        }
    }

    /// The shared numeric `for` loop: `i` is the next iteration's
    /// counter (the in-flight iteration's when resuming — read back
    /// from the path, never reset to the start), `first_outcome` the
    /// outcome of an already-running iteration (`None` for a fresh
    /// entry). The loop's end bound is carried, not re-evaluated, so
    /// resumption replays none of the header's actions; the temporary
    /// is bound in the frame for the body's reads.
    #[allow(clippy::too_many_arguments)]
    fn for_to_loop(
        &mut self,
        var: &str,
        down: bool,
        end: i128,
        body: &[Stmt],
        frame: &mut Frame,
        me: Option<usize>,
        mut i: i128,
        first_outcome: Option<Flow>,
    ) -> Result<Flow, Diagnostic> {
        let mut pending = first_outcome;
        loop {
            match pending.take() {
                None => {}
                // A completed iteration advances the counter; a
                // `continue` from the body re-checks the bound without
                // advancing (the fresh arm's shape).
                Some(Flow::Normal) => {
                    if down {
                        i -= 1;
                    } else {
                        i += 1;
                    }
                }
                Some(Flow::Continue) => {}
                Some(Flow::Break) => return Ok(Flow::Normal),
                Some(other) => return Ok(other),
            }
            let in_bounds = if down { i >= end } else { i <= end };
            if !in_bounds {
                return Ok(Flow::Normal);
            }
            // The in-flight iteration's counter and the carried end
            // bound ride the context stack: a suspension inside the
            // body builds a path that resumes this iteration without
            // replaying the bounds or resetting the counter.
            self.set_ctx_child(Child::ForBody { counter: i, end });
            frame.locals.insert(var.to_owned(), Value::Int(i));
            pending = Some(self.exec_block(body, frame, me)?);
        }
    }

    /// The shared `for each` loop: `items` is the list snapshot taken
    /// at loop entry (post-reversal), `position` the next element's
    /// index, and `in_flight` the resume path of the element whose body
    /// was executing when a synchronization action suspended (`None`
    /// for a fresh entry). The element and index bindings live in the
    /// frame; the snapshot rides the path so resumption neither
    /// re-evaluates the list expression nor re-runs completed elements.
    #[allow(clippy::too_many_arguments)]
    fn for_each_loop(
        &mut self,
        binding: &str,
        index_var: Option<&str>,
        items: &[Value],
        body: &[Stmt],
        frame: &mut Frame,
        me: Option<usize>,
        mut position: usize,
        in_flight: Option<&[PathLevel]>,
    ) -> Result<Flow, Diagnostic> {
        if let Some(rest) = in_flight {
            match self.exec_block_resumable(body, frame, me, rest)? {
                Flow::Normal | Flow::Continue => position += 1,
                Flow::Break => return Ok(Flow::Normal),
                other => return Ok(other),
            }
        }
        while position < items.len() {
            self.set_ctx_child(Child::ForeachBody {
                position,
                items: items.to_vec(),
            });
            frame
                .locals
                .insert(binding.to_owned(), items[position].clone());
            if let Some(index_name) = index_var {
                frame
                    .locals
                    .insert(index_name.to_owned(), Value::Int(position as i128));
            }
            match self.exec_block(body, frame, me)? {
                Flow::Normal | Flow::Continue => position += 1,
                Flow::Break => return Ok(Flow::Normal),
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_statement(
        &mut self,
        statement: &Stmt,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Flow, Diagnostic> {
        match statement {
            Stmt::VarDecl { name, ty, init, .. } => {
                // `var c: cfg = new;` — an untyped `new` in a var
                // declaration with a declared struct type takes that
                // type (clause 6.3's typed creation: the target's type
                // is known). Other untyped `new` uses keep the explicit
                // not-supported diagnostic below.
                let inferred_new: Option<Expr> = match (ty, init) {
                    (
                        Some(
                            var_ty @ TypeExpr::Named { .. } | var_ty @ TypeExpr::TemplateRef { .. },
                        ),
                        Some(Expr::New {
                            ty: None,
                            inits,
                            span,
                        }),
                    ) => Some(Expr::New {
                        ty: Some(var_ty.clone()),
                        inits: inits.clone(),
                        span: *span,
                    }),
                    _ => None,
                };
                let mut value = match (&inferred_new, init) {
                    (Some(new_expr), _) => self.eval(new_expr, frame, me)?,
                    (None, Some(expr)) => {
                        // The declared type is the inference context for an
                        // untyped `evaluate { ... }` in the initializer
                        // (4.17.7); the `var x := e` form declares none.
                        self.eval_with_hint(ty.clone(), expr, frame, me)?
                    }
                    // A declared type without an initializer takes the
                    // type's default; the type-inferring `:=` form
                    // always carries an initializer.
                    (None, None) => match ty {
                        Some(ty) => self.default_for(ty),
                        None => {
                            return Err(self.error(
                                "E0402",
                                "type-inferring 'var' without an initializer",
                                statement.span(),
                            ));
                        }
                    },
                };
                // A bare list-concat literal does not know its element
                // type at its creation site; the declared type of the
                // variable it initializes names it (5.2.11). A value that
                // arrived from an existing list keeps that list's record.
                if let (Some(ty), Value::List(list)) = (ty, &mut value)
                    && list.element.is_none()
                    && let TypeExpr::List { element, .. } = ty
                {
                    list.element = Some(element.render());
                }
                // The declared type binds the value (T7.1: a
                // custom-numeric slot converts a scalar initializer
                // through `numeric_from_int`/`numeric_from_real`, and a
                // custom value copies — value semantics).
                value = self.bind_slot_value(value, ty.as_ref(), statement.span())?;
                if let Some(ty) = ty {
                    frame.local_types.insert(name.clone(), ty.clone());
                }
                frame.locals.insert(name.clone(), value);
                Ok(Flow::Normal)
            }
            Stmt::RgfAssign {
                target,
                value,
                span,
            } => {
                self.exec_rgf_assign(target, value, frame, me, *span)?;
                Ok(Flow::Normal)
            }
            Stmt::Assign { target, value, .. } => {
                // The target's declared type is the inference context for
                // an untyped `evaluate { ... }` in the assigned value
                // (4.17.7).
                let hint = self.static_type_of(target, frame, me);
                let value = self.eval_with_hint(hint, value, frame, me)?;
                match target {
                    Expr::HdlPath { path, span } => {
                        if self.hdl.is_none() {
                            return Err(self.error(
                                "E0408",
                                "no simulator loaded — call hdl_load first",
                                *span,
                            ));
                        }
                        let Some(int_value) = as_i64(&value) else {
                            return Err(self.error(
                                "E0402",
                                "signal assignment expects an integer (or boolean) value",
                                *span,
                            ));
                        };
                        let cycle = self.cycle;
                        match self.hdl.as_mut().unwrap().set(path, int_value, cycle) {
                            Ok(()) => Ok(Flow::Normal),
                            Err(msg) => Err(self.error("E0408", msg, *span)),
                        }
                    }
                    Expr::Index { base, index, span } => {
                        let index_value = self.eval(index, frame, me)?;
                        let Value::Int(i) = index_value else {
                            return Err(self.error(
                                "E0402",
                                "list index must be an integer",
                                *span,
                            ));
                        };
                        let list = self.read_slot(base, frame, me)?;
                        let Value::List(list) = list else {
                            return Err(self.error("E0402", "indexed value is not a list", *span));
                        };
                        // The slot's list object is shared; the assignment
                        // mutates it in place through the handle.
                        let len = list.items.borrow().len() as i128;
                        if i < 0 || i >= len {
                            return Err(self.error(
                                "E0404",
                                format!("list index {i} out of range (size {len})"),
                                *span,
                            ));
                        }
                        list.items.borrow_mut()[i as usize] = value;
                        Ok(Flow::Normal)
                    }
                    _ => {
                        self.write_slot(target, value, frame, me, statement.span())?;
                        Ok(Flow::Normal)
                    }
                }
            }
            Stmt::ExprStmt { expr, .. } => {
                // A statement-position clause 33 blocking call
                // (`p.up();`, `p.down();`, `l.lock();`, `l.release();`)
                // ends the turn at the call: a successful one yields,
                // a blocked one parks until the wake pass services it
                // (31.2.5's non-preemptive model interrupts at TCM
                // calls and release/lock requests). Any other spelling
                // evaluates through the expression evaluator.
                if let Some(flow) = self.sync_statement_call(expr, frame, me)? {
                    return Ok(flow);
                }
                if let Some(flow) = self.declared_statement_call(expr, frame, me)? {
                    return Ok(flow);
                }
                self.eval(expr, frame, me)?;
                Ok(Flow::Normal)
            }
            Stmt::Return { value, .. } => {
                let value = match value {
                    Some(expr) => self.eval(expr, frame, me)?,
                    None => Value::Void,
                };
                Ok(Flow::Return(value))
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
                ..
            } => {
                let Value::Bool(cond_value) = self.eval(cond, frame, me)? else {
                    let span = statement.span();
                    return Err(self.error("E0402", "condition is not a boolean", span));
                };
                if cond_value {
                    // The taken branch is recorded on the context stack:
                    // a suspension inside builds a path that re-enters
                    // it without re-evaluating the condition (the
                    // condition's actions ran before the suspension).
                    self.set_ctx_child(Child::IfThen);
                    self.exec_block(then_body, frame, me)
                } else if let Some(else_body) = else_body {
                    self.set_ctx_child(Child::IfElse);
                    self.exec_block(else_body, frame, me)
                } else {
                    Ok(Flow::Normal)
                }
            }
            Stmt::While { cond, body, .. } => {
                // The loop's context level stays on the stack across
                // the whole loop — including its continuation after a
                // resumed iteration — so a re-suspension rebuilds the
                // same path (the condition is re-checked only when an
                // iteration completes).
                self.set_ctx_child(Child::WhileBody);
                self.while_loop(cond, body, frame, me, Flow::Normal)
            }
            // 20.2.3: the body executes at least once, then repeatedly
            // until the condition is TRUE.
            Stmt::RepeatUntil { body, cond, .. } => {
                self.set_ctx_child(Child::RepeatBody);
                self.repeat_until_loop(body, cond, frame, me, None)
            }
            Stmt::Break { .. } => Ok(Flow::Break),
            Stmt::Continue { .. } => Ok(Flow::Continue),
            Stmt::Print { exprs, span, .. } => {
                let mut parts = Vec::new();
                for expr in exprs {
                    let value = self.eval(expr, frame, me)?;
                    // A custom numeric value renders through
                    // numeric_to_string(dec) (T7.3); the absence of the
                    // method is the checked E0418.
                    let text = match expr {
                        Expr::Ident { name, .. } => {
                            format!("{name} = {}", self.format_value_checked(&value, *span)?)
                        }
                        _ => self.format_value_checked(&value, *span)?,
                    };
                    parts.push(text);
                }
                self.output.push_str(&parts.join(" "));
                self.output.push('\n');
                Ok(Flow::Normal)
            }
            Stmt::Gen { expr, span } => self.exec_gen(expr, frame, me, *span),
            Stmt::StopRun { .. } => {
                self.stop_requested = true;
                Ok(Flow::Stop)
            }
            Stmt::SeqDo { item, span } => {
                // Inline item execution: gen the item field in place.
                // The sequencer handshake (get_next_item/item_done
                // blocking) needs the TCM scheduler — a later milestone.
                self.exec_gen(
                    &Expr::Ident {
                        name: item.clone(),
                        span: *span,
                    },
                    frame,
                    me,
                    *span,
                )
            }
            Stmt::Check {
                condition,
                else_body,
                span,
            } => {
                let Value::Bool(holds) = self.eval(condition, frame, me)? else {
                    return Err(self.error(
                        "E0402",
                        "the condition of 'check that' is not a boolean",
                        *span,
                    ));
                };
                if !holds {
                    // Specman counts DUT errors and keeps running; the
                    // else body (typically `dut_error(...)`) reports.
                    self.dut_errors += 1;
                    // The else body continues the same atomic fragment
                    // (the check is a leaf statement; the resume path
                    // does not descend into it) — waits inside keep the
                    // inline spin.
                    self.atomic_depth += 1;
                    let result = self.exec_block(else_body, frame, me);
                    self.atomic_depth -= 1;
                    result?;
                }
                Ok(Flow::Normal)
            }
            Stmt::ForEach {
                var,
                type_filter,
                index_var,
                using,
                reversed,
                list,
                body,
                span,
            } => {
                // The element type filter is recorded, not enforced.
                let _ = type_filter;
                let _ = using;
                // 20.2.5: `for each` iterates a list or a set — the
                // items in order. A set literal with enumeral bounds
                // iterates the enumerands (`for each (c) in [red..blue]`
                // runs red, green, blue); any other set iterates its
                // integer elements in order (the value model keeps
                // intervals, so a set that did not come from a literal
                // with enumeral bounds carries indices, not enumerands).
                let binding = var.clone().unwrap_or_else(|| "it".to_owned());
                let mut items: Vec<Value> = match list {
                    Expr::SetLiteral {
                        items: set_items, ..
                    } => self.set_literal_iteration_items(set_items, frame, me, *span)?,
                    _ => match self.eval(list, frame, me)? {
                        Value::List(list) => {
                            // Iterate a snapshot of the items; the loop
                            // body may modify the shared list object.
                            list.items.borrow().clone()
                        }
                        Value::Set(intervals) => intervals
                            .iter()
                            .flat_map(|(lo, hi)| (*lo..=*hi).map(Value::Int))
                            .collect(),
                        other => {
                            return Err(self.error(
                                "E0402",
                                format!(
                                    "for-each expects a list or set, got {}",
                                    self.value_kind(&other)
                                ),
                                *span,
                            ));
                        }
                    },
                };
                if *reversed {
                    items.reverse();
                }
                self.for_each_loop(
                    &binding,
                    index_var.as_deref(),
                    &items,
                    body,
                    frame,
                    me,
                    0,
                    None,
                )
            }
            Stmt::ForTo {
                var,
                from,
                to,
                down,
                body,
                span,
            } => {
                let Value::Int(start_v) = self.eval(from, frame, me)? else {
                    return Err(self.error("E0402", "for lower bound must be an integer", *span));
                };
                let Value::Int(end_v) = self.eval(to, frame, me)? else {
                    return Err(self.error("E0402", "for upper bound must be an integer", *span));
                };
                self.for_to_loop(var, *down, end_v, body, frame, me, start_v, None)
            }
            Stmt::Wait { cycles, span } => {
                let Value::Int(n) = self.eval(cycles, frame, me)? else {
                    return Err(self.error("E0402", "wait count must be an integer", *span));
                };
                if n < 0 {
                    return Err(self.error("E0402", "wait count must be non-negative", *span));
                }
                // In a coroutine turn the wait suspends for n
                // occurrences of the sampling event (13.2.2) — the
                // scheduler advances time; resumption skips this
                // statement.
                if n > 0 && self.suspend_eligible() {
                    return self.suspend_here(
                        frame,
                        *span,
                        true,
                        self.cycle + n as u64,
                        false,
                        None,
                    );
                }
                if self.wait_budget == 0 {
                    return Err(self.error(
                        "E0409",
                        format!("cycle budget exceeded at cycle {}", self.cycle),
                        *span,
                    ));
                }
                self.wait_budget -= 1;
                self.cycle = self.cycle.saturating_add(n as u64);
                self.pad_log();
                Ok(Flow::Normal)
            }
            Stmt::WaitTemporal { te, span } => self.temporal_wait(te, *span, frame, me),
            Stmt::Sync { te, span } => match te {
                Some(te) => self.temporal_wait(te, *span, frame, me),
                None => {
                    // 13.2.3: the bare sync suspends for one occurrence
                    // of the default sampling event; in this model the
                    // default event fires every cycle.
                    if self.suspend_eligible() {
                        return self.suspend_here(frame, *span, true, self.cycle + 1, false, None);
                    }
                    if self.wait_budget == 0 {
                        return Err(self.error(
                            "E0409",
                            format!("cycle budget exceeded at cycle {}", self.cycle),
                            *span,
                        ));
                    }
                    self.wait_budget -= 1;
                    self.cycle = self.cycle.saturating_add(1);
                    self.pad_log();
                    Ok(Flow::Normal)
                }
            },
            Stmt::Emit { target, span } => {
                // 11.1.3.2: emit causes the event to occur; the action
                // itself does not consume time.
                if let Expr::StaticAccess {
                    type_name, member, ..
                } = target
                {
                    // `emit Type::event` (clause 11.2): the static
                    // event of the type itself — one occurrence, no
                    // instance, the static `on` blocks run with no `me`.
                    self.fire_static_event(type_name, member, *span, frame)?;
                } else {
                    let path = self.event_path(target)?;
                    let owner = self.resolve_event_owner(target, frame, me);
                    self.fire_event(&path, owner, *span, frame)?;
                }
                Ok(Flow::Normal)
            }
            Stmt::Start { call, span } => {
                // 13.2.2: a started TCM joins the scheduler's queue with
                // an implicit sync cycle before it commences; it runs
                // concurrently (round-robin turns) with the starter.
                let Expr::Call { callee, args, .. } = call else {
                    return Err(self.error("E0402", "start expects a method call", *span));
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, frame, me)?);
                }
                let (instance, name) = match callee.as_ref() {
                    Expr::Ident { name, .. } => {
                        let Some(index) = me else {
                            return Err(self.error(
                                "E0402",
                                format!("start of '{name}' without an enclosing instance"),
                                *span,
                            ));
                        };
                        (index, name.clone())
                    }
                    Expr::MemberAccess { object, name, .. } => {
                        let object_value = self.eval(object, frame, me)?;
                        let Value::Object(index) = object_value else {
                            return Err(self.error("E0402", "start requires an object", *span));
                        };
                        (index, name.clone())
                    }
                    // A static method is not a TCM (clause 17.2.2): it
                    // has no instance to schedule and no sampling event.
                    Expr::StaticAccess {
                        type_name, member, ..
                    } => {
                        return Err(self.error(
                            "E0402",
                            format!(
                                "'{type_name}::{member}' is a static method; static methods are not TCMs and cannot be started"
                            ),
                            *span,
                        ));
                    }
                    _ => return Err(self.error("E0402", "start target not supported", *span)),
                };
                let blocks = self.resolve_method(instance, &name, *span)?;
                if blocks
                    .first()
                    .is_some_and(|block| values.len() != block.params.len())
                {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "method '{name}' expects {} argument(s), got {}",
                            blocks.first().map_or(0, |block| block.params.len()),
                            values.len()
                        ),
                        *span,
                    ));
                }
                // 17.1.2: the TCM implicitly synchronizes on its
                // designated sampling event upon entry, as if the action
                // `sync @event` were added at the top of its code. The
                // every-cycle events keep the implicit sync cycle of the
                // stand-alone model (`sys.any` occurs automatically —
                // 11.1.4.1.1; `sim` is the tick the virtual cycles stand
                // in for); any other declared event parks the TCM until
                // its occurrence — unless it already occurred this tick,
                // when the entry sync succeeds immediately and the TCM
                // is scheduled for the current cycle (17.2.1: "begins
                // execution when its sampling event is emitted or
                // immediately if the sampling event has already been
                // emitted for the current tick" — it still runs only
                // after the starter's turn yields, 17.2.2).
                let tcm_event = blocks.first().and_then(|block| block.tcm_event.clone());
                let (wake_event, eligible_at_cycle) =
                    match tcm_event.map(|event| self.event_path(&event)) {
                        Some(Ok(path)) if !is_every_cycle_event(&path) => {
                            if self.event_occurred_this_cycle(&path) {
                                (None, self.cycle)
                            } else {
                                (Some(path), self.cycle + 1)
                            }
                        }
                        _ => (None, self.cycle + 1),
                    };
                self.coroutines.push(Coroutine {
                    kind: CoroutineKind::Tcm {
                        instance,
                        method: name,
                        args: values,
                        next_block: 0,
                    },
                    id: self.next_coroutine_id,
                    path: Vec::new(),
                    frame: Frame::default(),
                    me: Some(instance),
                    // The implicit "sync cycle" before it commences; a
                    // parked TCM (wake_event set) starts when its event
                    // fires instead.
                    eligible_at_cycle,
                    wake_event,
                    wake_sync: None,
                    suspended_temporal: false,
                    temporal_waited: 0,
                    span: *span,
                    wait_start: None,
                });
                self.next_coroutine_id += 1;
                Ok(Flow::Normal)
            }
            // `compute <call>;` — without TCM suspension, the call runs
            // inline exactly like `start` (clause 13's run-to-completion
            // semantics needs the scheduler).
            Stmt::Compute { call, span } => {
                let Expr::Call { callee, args, .. } = call else {
                    return Err(self.error("E0402", "compute expects a method call", *span));
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, frame, me)?);
                }
                match callee.as_ref() {
                    Expr::Ident { name, .. } => {
                        let Some(index) = me else {
                            return Err(self.error(
                                "E0402",
                                format!("compute of '{name}' without an enclosing instance"),
                                *span,
                            ));
                        };
                        if self.suspend_eligible() {
                            return self.begin_statement_call(index, name, values, *span, frame);
                        }
                        self.call_method(index, name, values, *span)?;
                    }
                    Expr::MemberAccess { object, name, .. } => {
                        let object_value = self.eval(object, frame, me)?;
                        let Value::Object(index) = object_value else {
                            return Err(self.error("E0402", "compute requires an object", *span));
                        };
                        if self.suspend_eligible() {
                            return self.begin_statement_call(index, name, values, *span, frame);
                        }
                        self.call_method(index, name, values, *span)?;
                    }
                    // See `start`: a static method is not a TCM.
                    Expr::StaticAccess {
                        type_name, member, ..
                    } => {
                        return Err(self.error(
                            "E0402",
                            format!(
                                "'{type_name}::{member}' is a static method; static methods are not TCMs and cannot be computed"
                            ),
                            *span,
                        ));
                    }
                    _ => return Err(self.error("E0402", "compute target not supported", *span)),
                }
                Ok(Flow::Normal)
            }
            Stmt::Case {
                scrutinee,
                arms,
                default,
                span,
            } => {
                let value = self.eval(scrutinee, frame, me)?;
                let mut matched: Option<(Option<usize>, &Vec<Stmt>)> = None;
                'arms: for (arm_index, (alternatives, body)) in arms.iter().enumerate() {
                    for alternative in alternatives {
                        let alt_value = self.eval(alternative, frame, me)?;
                        if matches!(
                            self.apply_binary(
                                BinaryOp::Eq,
                                value.clone(),
                                alt_value,
                                *span,
                                &Precision::unbounded(),
                            )?,
                            Value::Bool(true)
                        ) {
                            matched = Some((Some(arm_index), body));
                            break 'arms;
                        }
                    }
                }
                let matched = matched.or_else(|| default.as_ref().map(|body| (None, body)));
                match matched {
                    Some((arm, body)) => {
                        // The matched arm is recorded on the context
                        // stack: a suspension inside builds a path that
                        // re-enters it without re-matching the
                        // scrutinee.
                        self.set_ctx_child(Child::CaseArm { index: arm });
                        self.exec_block(body, frame, me)
                    }
                    None => Ok(Flow::Normal),
                }
            }
        }
    }

    fn exec_gen(
        &mut self,
        expr: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<Flow, Diagnostic> {
        let Some(me_index) = me else {
            return Err(self.error("E0402", "gen requires an enclosing instance", span));
        };
        // A simple member path (`h.p`) — the clause 10.5.8 chosen_from
        // target spelling among others.
        if let Expr::MemberAccess { .. } = expr {
            self.exec_gen_field_path(expr, frame, me_index, span)?;
            return Ok(Flow::Normal);
        }
        let Expr::Ident { name, .. } = expr else {
            return Err(self.error(
                "E0402",
                "gen target must be a variable or a simple field path",
                span,
            ));
        };
        let slot_is_local = frame.locals.contains_key(name);
        let current = if slot_is_local {
            frame.locals.get(name).cloned()
        } else {
            self.instances[me_index].get(name).cloned()
        };
        let Some(current) = current else {
            return Err(self.error("E0401", format!("unknown variable '{name}' in gen"), span));
        };
        // A clause 10.5.8 chosen_from whose target is exactly this field
        // governs the generation: the field is assigned a pointer to a
        // list element instead of being sampled. Only a field can be
        // governed — a constraint's identifiers are checked against the
        // type's fields, so a local variable is never a constraint
        // target.
        if !slot_is_local
            && let Some((instance, remaining, list)) =
                self.find_governing_chosen_from(me_index, std::slice::from_ref(name))?
        {
            let Value::Object(_) = current else {
                return Err(self.error(
                    "E0402",
                    format!(
                        "chosen_from target '{name}' must be a generative struct field (10.5.8)"
                    ),
                    span,
                ));
            };
            let other = self.chosen_from_other_constraints(instance, &remaining);
            self.gen_chosen_from(instance, &remaining, &list, &other, span)?;
            return Ok(Flow::Normal);
        }
        // A struct instance is randomized in place, recursively — under
        // the enclosing instance's constraints that reference it
        // (10.3.6-10.3.7's connected field sets, v1: depth-2 paths).
        if let Value::Object(index) = current {
            let obj_type_name = self.instances[me_index].type_name.clone();
            let types: &[crate::elab::ElaboratedType] = self.types;
            let enclosing: Vec<Expr> = types
                .iter()
                .find(|t| t.name == obj_type_name)
                .map(|t| {
                    t.constraints
                        .iter()
                        .filter(|c| !c.is_bind)
                        .map(|c| c.expr.clone())
                        .collect()
                })
                .unwrap_or_default();
            let rebased = rebase_constraints(&enclosing, name);
            self.gen_object(index, &rebased)?;
            return Ok(Flow::Normal);
        }
        // Scalar, list or enumeral target: sample candidates and accept one
        // that satisfies every top-level constraint of the enclosing type.
        let Some(declared_ty) = self.declared_type_of(name, slot_is_local, frame, me_index) else {
            return Err(self.error(
                "E0402",
                format!("cannot determine the type of '{name}'"),
                span,
            ));
        };
        let obj_type_name = self.instances[me_index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let constraints: Vec<Expr> = types
            .iter()
            .find(|t| t.name == obj_type_name)
            .map(|t| {
                t.constraints
                    .iter()
                    .filter(|c| !c.is_bind)
                    .map(|c| c.expr.clone())
                    .collect()
            })
            .unwrap_or_default();
        // Solver-backed path: linear problems go to the backend
        // conjunction solver (pumpkin with the cargo feature; without it
        // the outcome is Unsupported and the rejection sampler below
        // takes over), non-linear problems go to the bounded enumeration
        // solver in every build (see [`crate::solve::solve`]). A
        // chosen_from constraint assigns its own target when that is
        // generated; it is not a predicate over this scalar, so it stays
        // out of the translated problem.
        {
            let bounds: std::collections::BTreeMap<String, (i64, i64)> =
                crate::solve::int_bounds(&declared_ty)
                    .map(|b| (name.clone(), b))
                    .into_iter()
                    .collect();
            let solver_constraints: Vec<Expr> = constraints
                .iter()
                .filter(|constraint| !matches!(**constraint, Expr::ChosenFrom { .. }))
                .cloned()
                .collect();
            if !bounds.is_empty()
                && let Ok(problem) = crate::solve::translate(&solver_constraints, &bounds)
            {
                match crate::solve::solve(&problem, self.seed) {
                    crate::solve::SolveOutcome::Solved(mut assignment) => {
                        let value = assignment.remove(0);
                        self.store(
                            slot_is_local,
                            name,
                            Value::Int(value),
                            frame,
                            me_index,
                            span,
                        )?;
                        return Ok(Flow::Normal);
                    }
                    crate::solve::SolveOutcome::Unsat => {
                        return Err(self.error(
                            "E0407",
                            format!("the constraints of '{obj_type_name}' are unsatisfiable"),
                            span,
                        ));
                    }
                    crate::solve::SolveOutcome::Unsupported => {}
                }
            }
        }
        let mut solved = false;
        for _ in 0..1000 {
            let candidate = self.sample_type(&declared_ty)?;
            self.store(slot_is_local, name, candidate, frame, me_index, span)?;
            let mut pass = true;
            for constraint in &constraints {
                // A chosen_from constraint is not a predicate here: it
                // assigns its own target when that field is generated.
                if matches!(constraint, Expr::ChosenFrom { .. }) {
                    continue;
                }
                match self.eval(constraint, frame, Some(me_index))? {
                    Value::Bool(true) => {}
                    Value::Bool(false) => {
                        pass = false;
                        break;
                    }
                    _ => {
                        return Err(self.error(
                            "E0402",
                            "constraint did not evaluate to a boolean",
                            span,
                        ));
                    }
                }
            }
            if pass {
                solved = true;
                break;
            }
        }
        if !solved {
            return Err(self.error(
                "E0407",
                format!(
                    "no solution found for the constraints of '{obj_type_name}' after 1000 attempts"
                ),
                span,
            ));
        }
        Ok(Flow::Normal)
    }

    /// `gen <path>;` — the target is a simple member path: a bare
    /// identifier root (the enclosing instance, one of its fields, or a
    /// local) followed by field names. A clause 10.5.8 chosen_from
    /// governing the path assigns a list element to the target slot;
    /// without one, a struct-typed final field is randomized in place
    /// under the parent type's constraints that reference it (10.3.6 to
    /// 10.3.7's connected field sets, v1: depth-2 paths).
    fn exec_gen_field_path(
        &mut self,
        expr: &Expr,
        frame: &mut Frame,
        me_index: usize,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        let Some(segments) = expr_path_segments(expr) else {
            return Err(self.error("E0402", "gen target must be a simple field path", span));
        };
        let [root, rest @ ..] = segments.as_slice() else {
            return Err(self.error("E0402", "gen target must be a simple field path", span));
        };
        enum Root {
            /// `gen me.p` — the path starts at the enclosing instance.
            Me,
            /// `gen h.p` with `h` a local variable holding the instance.
            Local,
            /// `gen h.p` with `h` a field of the enclosing instance — the
            /// path may be referenced by the enclosing type's constraints.
            Field,
        }
        let root_kind = if root == "me" || root == "self" {
            Root::Me
        } else if frame.locals.contains_key(root.as_str()) {
            Root::Local
        } else if self.instances[me_index].get(root).is_some() {
            Root::Field
        } else {
            return Err(self.error("E0401", format!("unknown variable '{root}' in gen"), span));
        };
        let base = match root_kind {
            Root::Me => me_index,
            Root::Local | Root::Field => {
                let root_value = if matches!(root_kind, Root::Local) {
                    frame.locals.get(root.as_str()).cloned()
                } else {
                    self.instances[me_index].get(root).cloned()
                };
                let Some(Value::Object(base)) = root_value else {
                    return Err(self.error(
                        "E0402",
                        format!("gen target root '{root}' is not a struct instance"),
                        span,
                    ));
                };
                base
            }
        };
        // Where the constraint walk starts: a field root extends the
        // written path one level up (the enclosing type may declare the
        // governing chosen_from and constraints on the full path); `me`
        // and a local root start at the root object itself.
        let (walk_instance, walk_remaining) = match root_kind {
            Root::Field => (me_index, segments.clone()),
            Root::Me => (base, rest.to_vec()),
            Root::Local => (base, rest.to_vec()),
        };
        if let Some((instance, remaining, list)) =
            self.find_governing_chosen_from(walk_instance, &walk_remaining)?
        {
            let other = self.chosen_from_other_constraints(walk_instance, &walk_remaining);
            self.gen_chosen_from(instance, &remaining, &list, &other, span)?;
            return Ok(());
        }
        // Plain generation of the final field: single-segment paths only —
        // the deeper enclosing-constraint scoping needs full CFS support.
        let [field] = rest else {
            return Err(self.error(
                "E0402",
                "gen of a field path deeper than one segment is only supported for a chosen_from target (10.5.8)",
                span,
            ));
        };
        let Some(Value::Object(target_index)) = self.instances[base].get(field).cloned() else {
            return Err(self.error(
                "E0402",
                format!("gen of field '{field}' requires a struct-typed field"),
                span,
            ));
        };
        let parent_type_name = self.instances[base].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let enclosing: Vec<Expr> = types
            .iter()
            .find(|t| t.name == parent_type_name)
            .map(|t| {
                t.constraints
                    .iter()
                    .filter(|c| !c.is_bind)
                    .map(|c| c.expr.clone())
                    .collect()
            })
            .unwrap_or_default();
        let rebased = rebase_constraints(&enclosing, field);
        self.gen_object(target_index, &rebased)?;
        Ok(())
    }

    /// Walk a gen target path outermost-first, looking for the clause
    /// 10.5.8 chosen_from constraint that governs it: at each instance
    /// along the path the remaining segments name the target relative to
    /// that instance's type, and a non-bind chosen_from whose target path
    /// is exactly those segments governs. The constraint may live in the
    /// target's own struct or in any struct enclosing it along the path.
    /// Returns the slot (instance and remaining segments), where the
    /// element is assigned, and the chosen_from's list expression.
    fn find_governing_chosen_from(
        &self,
        start_instance: usize,
        start_remaining: &[String],
    ) -> Result<Option<(usize, Vec<String>, Expr)>, Diagnostic> {
        let mut instance = start_instance;
        let mut remaining: Vec<String> = start_remaining.to_vec();
        loop {
            let type_name = self.instances[instance].type_name.clone();
            let Some(ty) = self.types.iter().find(|t| t.name == type_name) else {
                return Ok(None);
            };
            for constraint in &ty.constraints {
                if constraint.is_bind {
                    continue;
                }
                let Expr::ChosenFrom { target, list, .. } = &constraint.expr else {
                    continue;
                };
                // 10.5.8: a chosen_from constraint is not allowed under a
                // `when` subtype; the declaring type of the governing
                // constraint is where that is visible.
                if !ty.when_of.is_empty() {
                    return Err(self.error(
                        "E0402",
                        "chosen_from is not allowed under a when subtype (10.5.8)",
                        constraint.span,
                    ));
                }
                if constraint_path_segments(target).as_deref() == Some(remaining.as_slice()) {
                    return Ok(Some((instance, remaining, list.as_ref().clone())));
                }
            }
            if remaining.len() <= 1 {
                return Ok(None);
            }
            let Some(Value::Object(next)) = self.instances[instance].get(&remaining[0]).cloned()
            else {
                return Ok(None);
            };
            instance = next;
            remaining.remove(0);
        }
    }

    /// The constraints that gate a clause 10.5.8 chosen_from choice: every
    /// non-bind, non-chosen_from constraint declared in the types along
    /// the target path — the declaring struct and the structs it reaches
    /// through — that constrains a path extending the target path,
    /// evaluated in each declaring instance's context. Constraints in
    /// descendants of the target are never collected (10.5.8).
    fn chosen_from_other_constraints(
        &self,
        start_instance: usize,
        start_remaining: &[String],
    ) -> Vec<(usize, Expr)> {
        let mut other: Vec<(usize, Expr)> = Vec::new();
        let mut instance = start_instance;
        let mut remaining: Vec<String> = start_remaining.to_vec();
        loop {
            let type_name = self.instances[instance].type_name.clone();
            if let Some(ty) = self.types.iter().find(|t| t.name == type_name) {
                for constraint in &ty.constraints {
                    if constraint.is_bind || matches!(constraint.expr, Expr::ChosenFrom { .. }) {
                        continue;
                    }
                    if expr_extends_path(&constraint.expr, &remaining) {
                        other.push((instance, constraint.expr.clone()));
                    }
                }
            }
            if remaining.len() <= 1 {
                return other;
            }
            let Some(Value::Object(next)) = self.instances[instance].get(&remaining[0]).cloned()
            else {
                return other;
            };
            instance = next;
            remaining.remove(0);
        }
    }

    /// Apply one clause 10.5.8 `target chosen_from read_only(list)`
    /// constraint: the list elements are substituted into the target slot
    /// (`slot_instance.slot_remaining`) one by one, the other constraints
    /// on paths through the target (`(instance, constraint)` pairs, each
    /// evaluated in its declaring instance's context) must all hold with
    /// the candidate bound, and one satisfying element is picked uniformly
    /// with the seeded RNG. An empty list, or no satisfying element, is a
    /// generation contradiction (`E0417`).
    fn gen_chosen_from(
        &mut self,
        slot_instance: usize,
        slot_remaining: &[String],
        list_expr: &Expr,
        other: &[(usize, Expr)],
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        let target_text = slot_remaining.join(".");
        self.check_chosen_from_target(slot_instance, slot_remaining, &target_text, span)?;
        let list_text = expr_text(list_expr);
        let list_value = self.eval(list_expr, &mut Frame::default(), Some(slot_instance))?;
        let Value::List(list) = list_value else {
            return Err(self.error(
                "E0402",
                format!("chosen_from list '{list_text}' does not denote a list"),
                span,
            ));
        };
        let items = list.items.borrow().clone();
        // Pointer-equality (and the other whole-object identity and
        // type-narrowing comparisons) on the target are unsupported
        // 10.5.8 shapes — reported instead of silently ignored.
        for (_, constraint) in other {
            if let Some(shape) = constraint_compares_target_whole(constraint, slot_remaining) {
                return Err(self.error(
                    "E0402",
                    format!(
                        "chosen_from: a {shape} constraint on the target '{target_text}' is not supported (10.5.8)"
                    ),
                    span,
                ));
            }
        }
        let mut satisfying: Vec<Value> = Vec::new();
        for candidate in &items {
            if !self.set_field_path(slot_instance, slot_remaining, candidate.clone()) {
                return Err(self.error("E0401", format!("unknown field '{target_text}'"), span));
            }
            let mut pass = true;
            for (instance, constraint) in other {
                match self.eval(constraint, &mut Frame::default(), Some(*instance))? {
                    Value::Bool(true) => {}
                    Value::Bool(false) => {
                        pass = false;
                        break;
                    }
                    _ => {
                        return Err(self.error(
                            "E0402",
                            "constraint did not evaluate to a boolean",
                            span,
                        ));
                    }
                }
            }
            if pass {
                satisfying.push(candidate.clone());
            }
        }
        if satisfying.is_empty() {
            return Err(self.error(
                "E0417",
                format!(
                    "chosen_from: no element of '{list_text}' satisfies the constraints on '{target_text}'"
                ),
                span,
            ));
        }
        // 10.5.8: one of the satisfying elements, chosen uniformly.
        let pick = self.rng.below(satisfying.len() as u128) as usize;
        let chosen = satisfying[pick].clone();
        if !self.set_field_path(slot_instance, slot_remaining, chosen) {
            return Err(self.error("E0401", format!("unknown field '{target_text}'"), span));
        }
        Ok(())
    }

    /// The 10.5.8 target-shape rules checked where the type information
    /// lives: every segment names a field, no `!` field on the path, and
    /// the target itself is a generative struct field (not `!`, `%`,
    /// `const`, `static` or unit-typed — units are instantiated, never
    /// generated).
    fn check_chosen_from_target(
        &self,
        slot_instance: usize,
        slot_remaining: &[String],
        target_text: &str,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        let mut instance = slot_instance;
        let last = slot_remaining.len() - 1;
        for (position, segment) in slot_remaining.iter().enumerate() {
            let type_name = self.instances[instance].type_name.clone();
            let Some(ty) = self.types.iter().find(|t| t.name == type_name) else {
                return Err(self.error(
                    "E0401",
                    format!("unknown type '{type_name}' in chosen_from target '{target_text}'"),
                    span,
                ));
            };
            let Some(field) = ty.fields.iter().find(|field| &field.name == segment) else {
                return Err(self.error(
                    "E0401",
                    format!("unknown field '{segment}' in chosen_from target '{target_text}'"),
                    span,
                ));
            };
            if position < last {
                if field.no_gen {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "chosen_from target '{target_text}' goes through a '!' field (10.5.8)"
                        ),
                        span,
                    ));
                }
                let Some(Value::Object(next)) = self.instances[instance].get(segment).cloned()
                else {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "chosen_from target '{target_text}' must go through struct instances"
                        ),
                        span,
                    ));
                };
                instance = next;
            } else {
                let not_generative = field.no_gen
                    || field.physical
                    || field.is_const
                    || field.is_static
                    || self.is_unit_typed(&field.ty)
                    || matches!(field.ty, TypeExpr::List { .. });
                if not_generative {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "chosen_from target '{target_text}' must be a generative struct field (10.5.8)"
                        ),
                        span,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Assign `value` to the slot named by a dotted field path: every
    /// segment except the last must currently hold a struct instance.
    fn set_field_path(&mut self, instance: usize, remaining: &[String], value: Value) -> bool {
        let mut instance = instance;
        let last = remaining.len() - 1;
        for (position, segment) in remaining.iter().enumerate() {
            if position == last {
                return self.instances[instance].set(segment, value);
            }
            let Some(Value::Object(next)) = self.instances[instance].get(segment).cloned() else {
                return false;
            };
            instance = next;
        }
        false
    }

    fn declared_type_of(
        &self,
        name: &str,
        slot_is_local: bool,
        frame: &Frame,
        me: usize,
    ) -> Option<TypeExpr> {
        if slot_is_local {
            return frame.local_types.get(name).cloned();
        }
        let obj_type = self.instances[me].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let ty = types.iter().find(|t| t.name == obj_type)?;
        ty.fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| field.ty.clone())
    }

    fn store(
        &mut self,
        slot_is_local: bool,
        name: &str,
        value: Value,
        frame: &mut Frame,
        me: usize,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        // The slot's declared type binds the value (T7.1's value
        // semantics: a custom numeric copies; a scalar into a
        // custom-numeric slot converts). The gen paths store sampled
        // scalars, so the conversion arm never fires from there.
        let declared = self.declared_type_of(name, slot_is_local, frame, me);
        let value = self.bind_slot_value(value, declared.as_ref(), span)?;
        if slot_is_local {
            frame.locals.insert(name.to_owned(), value);
            return Ok(());
        }
        if self.instances[me].set(name, value) {
            Ok(())
        } else {
            Err(self.error("E0401", format!("unknown field '{name}'"), span))
        }
    }

    /// Randomize one struct instance in place: every field is sampled and
    /// the type's top-level `keep` constraints must all hold.
    /// Whether the type expression names a unit (instance) type — those
    /// fields are instantiated, never generated.
    /// `field rgf= exp;` (clause 18.6): assign a concrete value to an
    /// already-generated range-generated field. The value must belong to
    /// the field's generated reduced set (10.3.13) — `E0415` otherwise,
    /// and there is no automatic width cast: a value that does not fit
    /// the field's type is simply outside the set. The field must have
    /// been generated (`E0416`); a non-rgf target is `E0130` (the
    /// well-formedness pass reports it too — this is the unchecked-parse
    /// path).
    fn exec_rgf_assign(
        &mut self,
        target: &Expr,
        value: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        let (instance, field) = self.rgf_target_instance_field(target, frame, me, span)?;
        let field_decl = self.rgf_field_declaration(instance, &field, span)?;
        if !field_decl.range_gen {
            return Err(self.error("E0130", "rgf= target is not a range-generated field", span));
        }
        let type_name = self.instances[instance].type_name.clone();
        let Some(state) = self.instances[instance].rgf.get(&field).cloned() else {
            return Err(self.error(
                "E0416",
                format!(
                    "rgf query/assign before generation: field '{field}' of '{type_name}' was not generated"
                ),
                span,
            ));
        };
        let value = self.eval(value, frame, me)?;
        let numeric = match &value {
            Value::Int(v) => *v,
            Value::Bool(b) => *b as i128,
            Value::Enum { type_name, name } => {
                self.enumeral_index(type_name, name).ok_or_else(|| {
                    self.error(
                        "E0402",
                        format!("'{name}' is not an enumerand of '{type_name}'"),
                        span,
                    )
                })?
            }
            other => {
                return Err(self.error(
                    "E0402",
                    format!(
                        "rgf= expects a scalar value, got {}",
                        self.value_kind(other)
                    ),
                    span,
                ));
            }
        };
        if !state
            .ranges
            .iter()
            .any(|(low, high)| *low <= numeric && numeric <= *high)
        {
            return Err(self.error(
                "E0415",
                format!(
                    "value {numeric} is outside the generated range {} of '{field}'",
                    render_set(&state.ranges)
                ),
                span,
            ));
        }
        let concrete = self.rgf_concrete_value(&field_decl.ty, numeric, span)?;
        self.instances[instance].set(&field, concrete);
        Ok(())
    }

    /// The clause 29.13 rgf routines, intercepted before the generic
    /// call path so the field argument arrives as a path (its instance
    /// and field name), not as an evaluated value.
    /// `Ok(None)` — not an rgf routine; the caller dispatches normally.
    fn call_rgf_routine(
        &mut self,
        name: &str,
        args: &[Expr],
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<Option<Value>, Diagnostic> {
        if !matches!(
            name,
            "rgf_was_generated" | "rgf_get_range" | "rgf_was_reduced"
        ) {
            return Ok(None);
        }
        let [arg] = args else {
            return Err(self.error(
                "E0402",
                format!("'{name}' expects one field argument, got {}", args.len()),
                span,
            ));
        };
        let (instance, field) = self.rgf_target_instance_field(arg, frame, me, span)?;
        let field_decl = self.rgf_field_declaration(instance, &field, span)?;
        if !field_decl.range_gen {
            return Err(self.error(
                "E0130",
                format!("'{name}' target is not a range-generated field"),
                span,
            ));
        }
        let type_name = self.instances[instance].type_name.clone();
        let state = self.instances[instance].rgf.get(&field).cloned();
        if name == "rgf_was_generated" {
            // TRUE iff the solver generated the field; a query on a
            // generated-yet field is FALSE, not an error (clause 29.13).
            return Ok(Some(Value::Bool(state.is_some())));
        }
        let Some(state) = state else {
            return Err(self.error(
                "E0416",
                format!(
                    "rgf query/assign before generation: field '{field}' of '{type_name}' was not generated"
                ),
                span,
            ));
        };
        match name {
            "rgf_get_range" => Ok(Some(Value::Set(state.ranges))),
            "rgf_was_reduced" => Ok(Some(Value::Bool(state.reduced))),
            _ => unreachable!("routine name matched above"),
        }
    }

    /// The declared field one rgf target names: the (instance, field)
    /// path resolved, the field looked up in its instance's type.
    fn rgf_field_declaration(
        &self,
        instance: usize,
        field: &str,
        span: crate::ast::Span,
    ) -> Result<crate::ast::Field, Diagnostic> {
        let type_name = self.instances[instance].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let Some(ty) = types.iter().find(|t| t.name == type_name) else {
            return Err(self.error("E0401", format!("unknown type '{type_name}'"), span));
        };
        ty.fields
            .iter()
            .find(|f| f.name == field)
            .cloned()
            .ok_or_else(|| self.error("E0401", format!("unknown field '{field}'"), span))
    }

    /// The (instance, field) an rgf target names: a member path whose
    /// prefix evaluates to an instance and whose last segment is the
    /// field; a bare name is a field of the enclosing instance (`me`).
    fn rgf_target_instance_field(
        &mut self,
        target: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<(usize, String), Diagnostic> {
        match target {
            Expr::MemberAccess { object, name, .. } => {
                let base = self.eval(object, frame, me)?;
                match base {
                    Value::Object(index) => Ok((index, name.clone())),
                    other => Err(self.error(
                        "E0402",
                        format!(
                            "rgf target is not an instance field, got {}",
                            self.value_kind(&other)
                        ),
                        span,
                    )),
                }
            }
            Expr::Ident { name, .. } => {
                if frame.locals.contains_key(name) {
                    return Err(self.error(
                        "E0402",
                        format!("'{name}' is not a field of the enclosing instance"),
                        span,
                    ));
                }
                match me {
                    Some(index) => Ok((index, name.clone())),
                    None => {
                        Err(self.error("E0402", "rgf target requires an enclosing instance", span))
                    }
                }
            }
            _ => Err(self.error("E0402", "rgf target must be a field path", span)),
        }
    }

    fn is_unit_typed(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Named { name, .. } => self
                .types
                .iter()
                .any(|t| &t.name == name && t.kind == DeclarationKind::Unit),
            // A when-qualified unit type is unit-typed as well; its
            // view carries the base's kind.
            TypeExpr::WhenQualified { base, .. } => self.is_unit_typed(base),
            _ => false,
        }
    }

    /// A predeclared value type gen cannot construct (`event`, `file`,
    /// the clause 33 sync structs, ...): the field keeps its NULL
    /// default until the program assigns it. A declared type of the
    /// same name shadows the builtin and generates normally.
    fn is_non_generatable_builtin(&self, ty: &TypeExpr) -> bool {
        let TypeExpr::Named { name, .. } = ty else {
            return false;
        };
        crate::builtins::is_builtin_type(name) && !self.types.iter().any(|t| &t.name == name)
    }

    /// The fixed length demanded by a `keep <field>.size() == N`
    /// constraint, if any (only the equality form drives lengths here;
    /// ranges are a later milestone).
    fn required_list_size<'c>(&self, field: &str, constraints: &'c [Expr]) -> Option<usize> {
        for constraint in constraints {
            let Expr::Binary {
                op: crate::ast::BinaryOp::Eq,
                lhs,
                rhs,
                ..
            } = constraint
            else {
                continue;
            };
            let size_call = |expr: &'c Expr| -> Option<(&'c String, &'c Expr)> {
                let Expr::Call { callee, args, .. } = expr else {
                    return None;
                };
                if !args.is_empty() {
                    return None;
                }
                let Expr::MemberAccess { object, name, .. } = callee.as_ref() else {
                    return None;
                };
                if name != "size" {
                    return None;
                }
                let Expr::Ident { name, .. } = object.as_ref() else {
                    return None;
                };
                Some((name, expr))
            };
            let (accessor, size_expr) = if let Some((name, _)) = size_call(lhs) {
                (name, rhs.as_ref())
            } else if let Some((name, _)) = size_call(rhs) {
                (name, lhs.as_ref())
            } else {
                continue;
            };
            if accessor != field {
                continue;
            }
            if let Expr::IntLiteral { text, .. } = size_expr
                && let Some(length) = parse_int_literal(text)
                && let Ok(length) = usize::try_from(length)
            {
                return Some(length);
            }
        }
        None
    }

    fn gen_object(&mut self, index: usize, enclosing: &[Expr]) -> Result<(), Diagnostic> {
        let type_name = self.instances[index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let Some(ty) = types.iter().find(|t| t.name == type_name) else {
            return Err(self.error(
                "E0401",
                format!("unknown type '{type_name}'"),
                origin_span(),
            ));
        };
        let type_span = ty.span;
        // The range-generated (`~`) fields (clause 6.9.7), declaration
        // order. They are excluded from sampling below exactly like `!`
        // fields: their values come from the reduced-set pass after the
        // other fields are concrete (10.3.13).
        let rgf_fields: Vec<(String, TypeExpr)> = ty
            .fields
            .iter()
            .filter(|field| field.range_gen && !field.is_static)
            .map(|field| (field.name.clone(), field.ty.clone()))
            .collect();
        let rgf_names: BTreeSet<String> = rgf_fields.iter().map(|(name, _)| name.clone()).collect();
        // `!` fields keep their value; instance (unit-typed) fields are
        // not generated — they are instantiated structurally. A `const`
        // field keeps its value too: it is initialised once (by its init
        // value, in `init()`, or by `do_unpack()` — clause 6.9.4), so
        // generation must not overwrite it. A static field is not an
        // instance member at all (clause 6.9.3) and is never generated.
        // A subtype view's determinant fields are fixed by the view
        // (6.7) and are not sampled. A `~` field is not sampled either —
        // see the reduced-set pass below.
        let field_types: Vec<(String, TypeExpr)> = ty
            .fields
            .iter()
            .filter(|field| {
                !field.no_gen
                    && !field.range_gen
                    && !field.is_const
                    && !field.is_static
                    && !self.is_unit_typed(&field.ty)
                    && !self.is_non_generatable_builtin(&field.ty)
                    && !ty.when_of.iter().any(|(id, _)| id == &field.name)
            })
            .map(|field| (field.name.clone(), field.ty.clone()))
            .collect();
        let mut constraints: Vec<Expr> = ty
            .constraints
            .iter()
            .filter(|constraint| !constraint.is_bind)
            .map(|constraint| constraint.expr.clone())
            .collect();
        constraints.extend(enclosing.iter().cloned());
        // The clause 10.5.8 chosen_from constraints of this type: each
        // must target one of the type's own generative fields (a deeper
        // target path is applied by generating the field path explicitly,
        // `gen h.p`; there is no ICFS machinery in this stage for the
        // target to participate in). A chosen_from declared under a
        // `when` subtype, or with a target that is not a simple field
        // path of this type, is an unsupported shape.
        let mut chosen_specs: Vec<(String, Expr)> = Vec::new();
        for constraint in &constraints {
            // The bind constraints are filtered out at the collection
            // site; every entry here is a plain constraint expression.
            let Expr::ChosenFrom { target, list, .. } = constraint else {
                continue;
            };
            if !ty.when_of.is_empty() {
                return Err(self.error(
                    "E0402",
                    "chosen_from is not allowed under a when subtype (10.5.8)",
                    constraint.span(),
                ));
            }
            let Some(segments) = constraint_path_segments(target) else {
                return Err(self.error(
                    "E0402",
                    "chosen_from target must be a simple field path (10.5.8)",
                    constraint.span(),
                ));
            };
            let [field] = segments.as_slice() else {
                return Err(self.error(
                    "E0402",
                    format!(
                        "chosen_from target '{}' must be a field of '{type_name}' (10.5.8); deeper target paths are applied by generating the field path",
                        segments.join(".")
                    ),
                    constraint.span(),
                ));
            };
            chosen_specs.push((field.clone(), list.as_ref().clone()));
        }
        let chosen_names: std::collections::BTreeSet<String> = chosen_specs
            .iter()
            .map(|(field, _)| field.clone())
            .collect();
        // The constraint split (10.3.13): constraints referencing an rgf
        // field drive that field's reduced set; the rest gate the
        // sampling of the non-rgf fields as before.
        let mut rgf_constraints: Vec<Expr> = Vec::new();
        let mut plain_constraints: Vec<Expr> = Vec::new();
        for constraint in &constraints {
            if rgf_field_refs(constraint, &rgf_names).is_empty() {
                plain_constraints.push(constraint.clone());
            } else {
                rgf_constraints.push(constraint.clone());
            }
        }
        // Every rgf-involving constraint must match a supported reduced-
        // set shape (10.3.13), checked before anything is generated so an
        // unsupported shape is a deterministic E0317.
        let rgf_order: Vec<String> = rgf_fields.iter().map(|(name, _)| name.clone()).collect();
        for constraint in &rgf_constraints {
            if let Err(field) = rgf_constraint_shape(constraint, &rgf_names, &rgf_order) {
                return Err(self.error(
                    "E0317",
                    format!("unsupported constraint shape for range-generated field '{field}'"),
                    type_span,
                ));
            }
        }
        // The per-field constraint bookkeeping: each rgf-involving
        // constraint carries exactly one rgf atom (the shape check); the
        // map groups them by the field that atom constrains.
        let mut per_field: BTreeMap<String, Vec<Expr>> = BTreeMap::new();
        for constraint in &rgf_constraints {
            if let Ok(Some(field)) = rgf_constraint_atom_field(constraint, &rgf_names, &rgf_order) {
                per_field.entry(field).or_default().push(constraint.clone());
            }
        }
        // Simple range/equality constraints narrow the per-field sampling
        // domains; everything else is enforced by the rejection predicate.
        let domains = self.constraint_domains(&plain_constraints, &field_types);
        // Solver-backed path: linear problems go to the backend
        // conjunction solver (pumpkin with the cargo feature; without it
        // the outcome is Unsupported), non-linear problems to the
        // bounded enumeration solver in every build.
        if let Some(assignment) =
            self.try_solve_fields(&field_types, &plain_constraints, &type_name, type_span)?
        {
            for (name, field_ty) in &field_types {
                match assignment.iter().find(|(assigned, _)| assigned == name) {
                    Some((_, value)) => {
                        self.instances[index].set(name, Value::Int(*value));
                    }
                    None => {
                        // A chosen_from target is assigned a list element
                        // below, not sampled.
                        if chosen_names.contains(name) {
                            continue;
                        }
                        let value = self.sample_type(field_ty)?;
                        self.instances[index].set(name, value);
                    }
                }
            }
            for (field, list_expr) in &chosen_specs {
                let other = self.chosen_from_other_constraints(index, std::slice::from_ref(field));
                let mut other = other;
                other.extend(
                    enclosing
                        .iter()
                        .filter(|expr| expr_extends_path(expr, std::slice::from_ref(field)))
                        .map(|expr| (index, expr.clone())),
                );
                self.gen_chosen_from(
                    index,
                    std::slice::from_ref(field),
                    list_expr,
                    &other,
                    type_span,
                )?;
            }
            return self.generate_rgf_fields(index, &rgf_fields, &per_field, &type_name, type_span);
        }
        for _ in 0..1000 {
            for (name, field_ty) in &field_types {
                // A chosen_from target is assigned a list element after
                // the other fields are sampled (so co-constraints see
                // their values), not sampled here.
                if chosen_names.contains(name) {
                    continue;
                }
                let value = match domains.get(name) {
                    Some(domain) => self.sample_int_in(field_ty, *domain),
                    None => self.sample_type(field_ty)?,
                };
                self.instances[index].set(name, value);
                // A `keep list.size() == N` constraint drives the list
                // length: fill the sampled (empty) list with generated
                // elements so the constraint can pass.
                if let Some(length) = self.required_list_size(name, &plain_constraints)
                    && let Value::List(list) = self.instances[index]
                        .get(name)
                        .cloned()
                        .unwrap_or(Value::Void)
                {
                    let mut filled = list.items.borrow().clone();
                    filled.resize(length, Value::Void);
                    for slot in filled.iter_mut() {
                        let element_ty = list_element_type(field_ty);
                        let element = self.sample_type(&element_ty)?;
                        *slot = element;
                        if let Value::Object(element_index) = *slot {
                            self.gen_object(element_index, &[])?;
                        }
                    }
                    // Replace the shared storage contents in one short
                    // borrow (the sampling above must not run inside it).
                    *list.items.borrow_mut() = filled;
                }
            }
            // Clause 10.5.8: assign each chosen_from target now — the
            // other fields carry their sampled values, so the constraints
            // co-constraining paths through the target and another field
            // evaluate meaningfully.
            for (field, list_expr) in &chosen_specs {
                let other = self.chosen_from_other_constraints(index, std::slice::from_ref(field));
                let mut other = other;
                other.extend(
                    enclosing
                        .iter()
                        .filter(|expr| expr_extends_path(expr, std::slice::from_ref(field)))
                        .map(|expr| (index, expr.clone())),
                );
                self.gen_chosen_from(
                    index,
                    std::slice::from_ref(field),
                    list_expr,
                    &other,
                    type_span,
                )?;
            }
            let mut pass = true;
            for constraint in &plain_constraints {
                // HDL-binding constraints (`keep x.hdl_path() == "…"`,
                // `keep x.delayed() == FALSE`, the event-port
                // `keep x.edge() == …` attribute of 9.7.3) and the unit
                // attribute constraints (7.3.4 `pack_options()`, 7.3.5
                // `per_inst_cover()` — recorded by elaboration) record
                // simulator/tool facts, not data constraints; they never
                // gate gen.
                if expr_calls_any(
                    constraint,
                    &[
                        "hdl_path",
                        "delayed",
                        "soft",
                        "select",
                        "edge",
                        "pack_options",
                        "per_inst_cover",
                    ],
                ) {
                    continue;
                }
                // A chosen_from constraint assigns its target above; it
                // is not itself a predicate.
                if matches!(constraint, Expr::ChosenFrom { .. }) {
                    continue;
                }
                match self.eval(constraint, &mut Frame::default(), Some(index))? {
                    Value::Bool(true) => {}
                    Value::Bool(false) => {
                        pass = false;
                        break;
                    }
                    _ => {
                        return Err(self.error(
                            "E0402",
                            "constraint did not evaluate to a boolean",
                            type_span,
                        ));
                    }
                }
            }
            if pass {
                return self.generate_rgf_fields(
                    index,
                    &rgf_fields,
                    &per_field,
                    &type_name,
                    type_span,
                );
            }
        }
        Err(self.error(
            "E0407",
            format!("no solution found for the constraints of '{type_name}' after 1000 attempts"),
            type_span,
        ))
    }

    /// The reduced-set pass (10.3.13) over one generated instance's `~`
    /// fields, after every non-rgf field holds its concrete value.
    ///
    /// Algorithm v1 (per the 2019 migration plan, T5.2): for each rgf
    /// field, the generated set is the intersection over the field's
    /// applicable constraints of the value sets those constraints imply.
    /// A constraint's implication is computed on the constraint's shape
    /// tree — `lhs && rhs` intersects the sides' contributions, `lhs ||
    /// rhs` unions them, `bool-exp => atom` contributes the atom's set
    /// when the guard evaluates TRUE and the field type's full set when
    /// it evaluates FALSE — and an atom `f relop exp` (or `f in set`,
    /// `f not in set`) with the rgf side a bare field reference and the
    /// other side rgf-free contributes the interval(s) that `rel-op`
    /// implies with `exp` evaluated to a constant against the already
    /// generated field values. The concrete value is the set's minimum
    /// (the first interval's lower bound) — deterministic, and per the
    /// standard implementation-defined. Because a supported shape has
    /// at most one rgf atom (two atoms naming different fields are
    /// E0317), no constraint couples two rgf fields and the per-field
    /// passes are independent.
    fn generate_rgf_fields(
        &mut self,
        index: usize,
        rgf_fields: &[(String, TypeExpr)],
        per_field: &BTreeMap<String, Vec<Expr>>,
        type_name: &str,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        for (name, field_ty) in rgf_fields {
            let Some(full_range) = self.type_value_range(field_ty) else {
                return Err(self.error(
                    "E0402",
                    format!("range-generated field '{name}' has no representable value range"),
                    span,
                ));
            };
            let full_set = vec![full_range];
            let mut set = full_set.clone();
            for constraint in per_field.get(name).into_iter().flatten() {
                let contribution =
                    self.rgf_contribution(constraint, name, index, &full_set, span)?;
                set = intersect_interval_sets(&set, &contribution);
            }
            if set.is_empty() {
                return Err(self.error(
                    "E0407",
                    format!(
                        "the constraints of '{type_name}' leave no value for range-generated field '{name}'"
                    ),
                    span,
                ));
            }
            let reduced = set != full_set;
            let concrete = self.rgf_concrete_value(field_ty, set[0].0, span)?;
            self.instances[index].set(name, concrete);
            self.instances[index].rgf.insert(
                name.clone(),
                RgfState {
                    ranges: set,
                    reduced,
                },
            );
        }
        Ok(())
    }

    /// The value set one supported constraint shape contributes to rgf
    /// field `field`, with the instance's other fields concrete. The
    /// shape was validated by [`rgf_constraint_shape`]; every node that
    /// does not involve the field is rgf-free and evaluates as a Boolean
    /// constant here.
    fn rgf_contribution(
        &mut self,
        expr: &Expr,
        field: &str,
        index: usize,
        full_set: &[(i128, i128)],
        span: crate::ast::Span,
    ) -> Result<Vec<(i128, i128)>, Diagnostic> {
        match expr {
            Expr::Paren { expr, .. } => self.rgf_contribution(expr, field, index, full_set, span),
            Expr::Binary {
                op: BinaryOp::LogicalAnd,
                lhs,
                rhs,
                ..
            } => {
                let left = self.rgf_contribution(lhs, field, index, full_set, span)?;
                let right = self.rgf_contribution(rhs, field, index, full_set, span)?;
                Ok(intersect_interval_sets(&left, &right))
            }
            Expr::Binary {
                op: BinaryOp::LogicalOr,
                lhs,
                rhs,
                ..
            } => {
                let left = self.rgf_contribution(lhs, field, index, full_set, span)?;
                let right = self.rgf_contribution(rhs, field, index, full_set, span)?;
                Ok(union_interval_sets(&left, &right))
            }
            // An implication whose antecedent is FALSE contributes the
            // full type range (the constraint is vacuously satisfied).
            Expr::Binary {
                op: BinaryOp::Imply,
                lhs,
                rhs,
                ..
            } => match self.eval(lhs, &mut Frame::default(), Some(index))? {
                Value::Bool(true) => self.rgf_contribution(rhs, field, index, full_set, span),
                Value::Bool(false) => Ok(full_set.to_vec()),
                other => Err(self.error(
                    "E0402",
                    format!(
                        "constraint guard did not evaluate to a boolean, got {}",
                        self.value_kind(&other)
                    ),
                    span,
                )),
            },
            Expr::Unary {
                op: UnaryOp::Not,
                expr: inner,
                ..
            } => {
                // The `not in` spelling (the parser wraps the membership
                // in a unary `not`; parentheses unwrap); any other `not`
                // over rgf-free operands is a Boolean constant.
                if let Some((element, set)) = rgf_inset_parts(inner)
                    && rgf_bare_ref(element, field)
                {
                    let contained = self.rgf_set_contribution(set, index, span)?;
                    return Ok(complement_interval_set(&contained, full_set[0]));
                }
                match self.eval(expr, &mut Frame::default(), Some(index))? {
                    Value::Bool(true) => Ok(full_set.to_vec()),
                    Value::Bool(false) => Ok(Vec::new()),
                    other => Err(self.error(
                        "E0402",
                        format!(
                            "constraint did not evaluate to a boolean, got {}",
                            self.value_kind(&other)
                        ),
                        span,
                    )),
                }
            }
            Expr::Binary { op, lhs, rhs, .. }
                if matches!(
                    op,
                    BinaryOp::Eq
                        | BinaryOp::NotEq
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                ) && (rgf_bare_ref(lhs, field) || rgf_bare_ref(rhs, field)) =>
            {
                // One side is the bare rgf reference (shape-checked); the
                // other evaluates to a constant scalar.
                let (value, rgf_is_lhs) = if rgf_bare_ref(lhs, field) {
                    (self.rgf_scalar_operand(rhs, index, span)?, true)
                } else {
                    (self.rgf_scalar_operand(lhs, index, span)?, false)
                };
                let interval = match (op, rgf_is_lhs) {
                    (BinaryOp::Eq, _) => (value, value),
                    (BinaryOp::NotEq, _) => {
                        return Ok(complement_interval_set(&[(value, value)], full_set[0]));
                    }
                    (BinaryOp::Lt, true) | (BinaryOp::Gt, false) => {
                        (i128::MIN, value.saturating_sub(1))
                    }
                    (BinaryOp::Le, true) | (BinaryOp::Ge, false) => (i128::MIN, value),
                    (BinaryOp::Gt, true) | (BinaryOp::Lt, false) => {
                        (value.saturating_add(1), i128::MAX)
                    }
                    (BinaryOp::Ge, true) | (BinaryOp::Le, false) => (value, i128::MAX),
                    _ => unreachable!("relation op matched above"),
                };
                Ok(vec![interval])
            }
            Expr::InSet { element, set, .. } if rgf_bare_ref(element, field) => {
                let contained = self.rgf_set_contribution(set, index, span)?;
                Ok(contained)
            }
            // Any node that does not involve the field is rgf-free (the
            // shape check guarantees at most one rgf atom per
            // constraint): it evaluates to a Boolean constant — TRUE
            // contributes the full range, FALSE the empty set.
            _ => match self.eval(expr, &mut Frame::default(), Some(index))? {
                Value::Bool(true) => Ok(full_set.to_vec()),
                Value::Bool(false) => Ok(Vec::new()),
                other => Err(self.error(
                    "E0402",
                    format!(
                        "constraint did not evaluate to a boolean, got {}",
                        self.value_kind(&other)
                    ),
                    span,
                )),
            },
        }
    }

    /// The value set an `in` right side contributes: a bracket literal
    /// (items or range) with constant bounds, or a general set
    /// expression evaluated to a set value.
    fn rgf_set_contribution(
        &mut self,
        set: &crate::ast::InSetExpr,
        index: usize,
        span: crate::ast::Span,
    ) -> Result<Vec<(i128, i128)>, Diagnostic> {
        match set {
            crate::ast::InSetExpr::Range(low, high) => {
                let low = self.rgf_scalar_operand(low, index, span)?;
                let high = self.rgf_scalar_operand(high, index, span)?;
                Ok(vec![(low, high)])
            }
            crate::ast::InSetExpr::Items(items) => {
                let mut intervals = Vec::with_capacity(items.len());
                for item in items {
                    let value = self.rgf_scalar_operand(item, index, span)?;
                    intervals.push((value, value));
                }
                Ok(canonical_set(intervals))
            }
            crate::ast::InSetExpr::Expr(expr) => {
                let value = self.eval(expr, &mut Frame::default(), Some(index))?;
                let Value::Set(intervals) = value else {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "constraint set expression does not evaluate to a set, got {}",
                            self.value_kind(&value)
                        ),
                        span,
                    ));
                };
                Ok(intervals)
            }
        }
    }

    /// The scalar constant one non-rgf constraint operand evaluates to
    /// against the generated field values: an integer, a boolean (0/1)
    /// or an enumerand (its index).
    fn rgf_scalar_operand(
        &mut self,
        expr: &Expr,
        index: usize,
        span: crate::ast::Span,
    ) -> Result<i128, Diagnostic> {
        let value = self.eval(expr, &mut Frame::default(), Some(index))?;
        match &value {
            Value::Int(v) => Ok(*v),
            Value::Bool(b) => Ok(*b as i128),
            Value::Enum { type_name, name } => {
                self.enumeral_index(type_name, name).ok_or_else(|| {
                    self.error(
                        "E0402",
                        format!("'{name}' is not an enumerand of '{type_name}'"),
                        span,
                    )
                })
            }
            other => Err(self.error(
                "E0402",
                format!(
                    "constraint operand is not a scalar value, got {}",
                    self.value_kind(other)
                ),
                span,
            )),
        }
    }

    /// The concrete value an rgf field takes: the set's minimum (the
    /// first canonical interval's lower bound), mapped to the field's
    /// type — an integer, a boolean, or the enumerand at that index.
    fn rgf_concrete_value(
        &mut self,
        ty: &TypeExpr,
        minimum: i128,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        match ty {
            TypeExpr::Scalar {
                base: ScalarBase::Bool,
                ..
            } => Ok(Value::Bool(minimum != 0)),
            TypeExpr::Scalar { .. } => Ok(Value::Int(minimum)),
            TypeExpr::Named { name, .. } => {
                let values = self.enumeral_values(name);
                if values.is_empty() {
                    return Ok(Value::Int(minimum));
                }
                let Some(pick) = values
                    .get(minimum.unsigned_abs() as usize)
                    .filter(|_| minimum >= 0)
                else {
                    return Err(self.error(
                        "E0402",
                        format!("index {minimum} is not an enumerand of '{name}'"),
                        span,
                    ));
                };
                Ok(Value::Enum {
                    type_name: name.clone(),
                    name: pick.clone(),
                })
            }
            TypeExpr::InlineEnum { names, .. } => {
                let Some(pick) = names
                    .get(minimum.unsigned_abs() as usize)
                    .filter(|_| minimum >= 0)
                else {
                    return Err(self.error(
                        "E0402",
                        format!("index {minimum} is not an enumerand of the inline enumeral list"),
                        span,
                    ));
                };
                Ok(Value::Enum {
                    type_name: inline_enum_key(names),
                    name: pick.clone(),
                })
            }
            TypeExpr::WhenQualified { base, .. } => self.rgf_concrete_value(base, minimum, span),
            _ => Ok(Value::Int(minimum)),
        }
    }

    /// Translate the type's `keep` constraints over its integer scalar
    /// fields and solve them: linear problems via the backend
    /// conjunction solver (pumpkin with the cargo feature), non-linear
    /// problems via the bounded enumeration solver (every build).
    /// `Ok(None)` means the caller must use the rejection sampler:
    /// translation unsupported, backend unavailable, or search unknown.
    fn try_solve_fields(
        &self,
        field_types: &[(String, TypeExpr)],
        constraints: &[Expr],
        type_name: &str,
        span: crate::ast::Span,
    ) -> Result<Option<Vec<(String, i128)>>, Diagnostic> {
        let bounds: std::collections::BTreeMap<String, (i64, i64)> = field_types
            .iter()
            .filter_map(|(name, ty)| crate::solve::int_bounds(ty).map(|b| (name.clone(), b)))
            .collect();
        if bounds.is_empty() {
            return Ok(None);
        }
        // A chosen_from constraint assigns its own target when that is
        // generated; it is not a linear predicate, and its presence must
        // not push the type's other fields off the solver.
        let solver_constraints: Vec<Expr> = constraints
            .iter()
            .filter(|constraint| !matches!(**constraint, Expr::ChosenFrom { .. }))
            .cloned()
            .collect();
        let problem = match crate::solve::translate(&solver_constraints, &bounds) {
            Ok(problem) => problem,
            Err(_) => return Ok(None),
        };
        match crate::solve::solve(&problem, self.seed) {
            crate::solve::SolveOutcome::Solved(assignment) => Ok(Some(
                problem
                    .vars
                    .iter()
                    .map(|var| var.name.clone())
                    .zip(assignment)
                    .collect(),
            )),
            crate::solve::SolveOutcome::Unsat => Err(self.error(
                "E0407",
                format!("the constraints of '{type_name}' are unsatisfiable"),
                span,
            )),
            crate::solve::SolveOutcome::Unsupported => Ok(None),
        }
    }

    /// Per-field sampling domains implied by the constraint list's simple
    /// shapes: `f in [lo..hi]`, `f == c`, and a constant comparison
    /// `f < c` / `f <= c` / `f > c` / `f >= c` (either orientation), where
    /// the bounds evaluate to integer constants without an instance
    /// context. Conjuncts contribute independently; everything else —
    /// soft shapes included, which the rejection predicate enforces
    /// as written — leaves the field's domain untouched. An empty
    /// intersection is left as-is: sampling then uses the declared
    /// bounds and the predicate fails the attempt (E0407), which is the
    /// honest outcome for contradictory constraints. A comparison against
    /// a constant narrows only the sampling domain; the rejection
    /// predicate still enforces the constraint as written.
    fn constraint_domains(
        &mut self,
        constraints: &[Expr],
        field_types: &[(String, TypeExpr)],
    ) -> BTreeMap<String, (i128, i128)> {
        let mut domains = BTreeMap::new();
        for constraint in constraints {
            self.collect_domain_leaves(constraint, &mut domains);
        }
        // 10.3.12.3: a `select` distribution biases the choice within
        // the current range of legal values — the range already
        // narrowed by the other constraints. The drawn value narrows
        // the field to a point; the rejection predicate skips
        // select-shaped constraints, so this bias is their whole
        // effect. When no other constraint narrowed the field, the
        // bias draws over the field type's natural range.
        for constraint in constraints {
            for (field, arms) in select_leaves(constraint) {
                let base = domains.get(&field).copied().or_else(|| {
                    field_types
                        .iter()
                        .find(|(name, _)| *name == field)
                        .and_then(|(_, ty)| natural_range(ty))
                });
                let Some(base) = base else { continue };
                let Some(value) = self.sample_select(arms, base) else {
                    continue;
                };
                intersect_domain(&mut domains, &field, (value, value));
            }
        }
        domains
    }

    fn collect_domain_leaves(&mut self, expr: &Expr, domains: &mut BTreeMap<String, (i128, i128)>) {
        match expr {
            Expr::Binary {
                op: BinaryOp::LogicalAnd,
                lhs,
                rhs,
                ..
            } => {
                self.collect_domain_leaves(lhs, domains);
                self.collect_domain_leaves(rhs, domains);
            }
            Expr::Paren { expr: inner, .. } => self.collect_domain_leaves(inner, domains),
            Expr::InSet {
                element,
                set: crate::ast::InSetExpr::Range(low, high),
                ..
            } => {
                if let Expr::Ident { name, .. } = element.as_ref()
                    && let (Some(low), Some(high)) =
                        (self.const_int_eval(low), self.const_int_eval(high))
                {
                    intersect_domain(domains, name, (low, high));
                }
            }
            Expr::Binary {
                op: BinaryOp::Eq,
                lhs,
                rhs,
                ..
            } => {
                for (side, other) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Ident { name, .. } = side.as_ref()
                        && let Some(value) = self.const_int_eval(other)
                    {
                        intersect_domain(domains, name, (value, value));
                    }
                }
            }
            // A constant comparison narrows the side that names a field:
            // `a < 10` and its mirror `10 > a` narrow identically. Bounds
            // saturate so extreme constants stay representable; sampling
            // clamps the domain to the field's natural range anyway.
            Expr::Binary { op, lhs, rhs, .. }
                if matches!(
                    op,
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
                ) =>
            {
                for (side, other, direct) in [(lhs, rhs, true), (rhs, lhs, false)] {
                    let Some(value) = self.const_int_eval(other) else {
                        continue;
                    };
                    // `a < 10` / `a <= 10` bound the field above; the
                    // mirrored `10 > a` / `10 >= a` bound it below.
                    let upper = if direct {
                        matches!(op, BinaryOp::Lt | BinaryOp::Le)
                    } else {
                        matches!(op, BinaryOp::Gt | BinaryOp::Ge)
                    };
                    let strict = matches!(op, BinaryOp::Lt | BinaryOp::Gt);
                    let (lower, high) = if upper {
                        let bound = if strict {
                            value.saturating_sub(1)
                        } else {
                            value
                        };
                        (i128::MIN, bound)
                    } else {
                        let bound = if strict {
                            value.saturating_add(1)
                        } else {
                            value
                        };
                        (bound, i128::MAX)
                    };
                    if let Expr::Ident { name, .. } = side.as_ref() {
                        intersect_domain(domains, name, (lower, high));
                    }
                }
            }
            Expr::InTable { tuple, rows, .. } => {
                // 4.11.8 in a `keep`: the disjunction of the rows, enforced
                // as written by the rejection predicate. For the sampling
                // domains, the projection of a disjunction onto one tuple
                // element is the union of the rows' projections, so a
                // bare-identifier element narrows to that union — sound
                // (every row-satisfying assignment stays inside it) and
                // necessary in practice: the rejection sampler alone does
                // not target the rows, so a wide scalar field would make
                // the disjunction practically unsampleable. A row whose
                // element for the position is not a constant (an enumerand
                // included) leaves the projection unknown — the narrowing
                // for that field is dropped entirely, because that row
                // could match with any value of the field.
                if rows.is_empty() {
                    return;
                }
                let mut projections: Vec<Vec<(i128, i128)>> = vec![Vec::new(); tuple.len()];
                for row in rows {
                    for (position, element) in row.iter().enumerate() {
                        let Some(Expr::Ident { .. }) = tuple.get(position) else {
                            continue;
                        };
                        if let Some(domain) = self.table_row_element_domain(element) {
                            projections[position].push(domain);
                        }
                    }
                }
                for (position, element_domains) in projections.into_iter().enumerate() {
                    if element_domains.len() != rows.len() {
                        continue;
                    }
                    if let Expr::Ident { name, .. } = &tuple[position] {
                        let low = element_domains
                            .iter()
                            .map(|d| d.0)
                            .min()
                            .unwrap_or(i128::MIN);
                        let high = element_domains
                            .iter()
                            .map(|d| d.1)
                            .max()
                            .unwrap_or(i128::MAX);
                        intersect_domain(domains, name, (low, high));
                    }
                }
            }
            _ => {}
        }
    }

    /// The constant integer domain one `in_table` row element constrains
    /// its tuple element to, when computable: a scalar element by
    /// constant evaluation, a set literal from its bounds, a type set
    /// from the type's value range. `None` for anything non-constant
    /// (enumerands included — those fields keep their full natural
    /// range). An item list's span `[min, max]` is a superset of its
    /// points, which keeps the narrowing sound.
    fn table_row_element_domain(
        &mut self,
        element: &crate::ast::TableElem,
    ) -> Option<(i128, i128)> {
        match element {
            crate::ast::TableElem::Scalar(expr) => {
                let value = self.const_int_eval(expr)?;
                Some((value, value))
            }
            crate::ast::TableElem::Set(crate::ast::InSetExpr::Range(low, high)) => {
                Some((self.const_int_eval(low)?, self.const_int_eval(high)?))
            }
            crate::ast::TableElem::Set(crate::ast::InSetExpr::Items(items)) => {
                let mut low = i128::MAX;
                let mut high = i128::MIN;
                for item in items {
                    let value = self.const_int_eval(item)?;
                    low = low.min(value);
                    high = high.max(value);
                }
                Some((low, high))
            }
            // A general set expression's span is not statically known
            // here; the row keeps its full projection (the narrowing for
            // this position is dropped, like a non-constant element's).
            crate::ast::TableElem::Set(crate::ast::InSetExpr::Expr(_)) => None,
            crate::ast::TableElem::TypeSet { ty, .. } => self.type_value_range(ty),
        }
    }

    /// Evaluate an expression expected to be an instance-independent
    /// integer constant (`2*K`, `100`); `None` on anything else.
    fn const_int_eval(&mut self, expr: &Expr) -> Option<i128> {
        match self.eval(expr, &mut Frame::default(), None) {
            Ok(Value::Int(value)) => Some(value),
            _ => None,
        }
    }

    /// Draw one value from a select distribution over the given range
    /// (10.3.12.3): an arm is chosen with probability weight/(sum of
    /// all weights), weights being integers, and the arm's choice maps
    /// to a value of the range — `min`/`max` take the extremes,
    /// `edges` picks one of them, `others` draws uniformly strictly
    /// inside, and `normal(center, stddev)` draws a Box–Muller normal
    /// clamped to the range (the choice keywords' example figures are
    /// images in the local copy; these readings follow the corpus
    /// usage). `None` when an arm's weight or choice cannot be
    /// evaluated — the field then keeps its unwidened domain.
    fn sample_select(
        &mut self,
        arms: &[crate::ast::SelectArm],
        range: (i128, i128),
    ) -> Option<i128> {
        let mut weights = Vec::new();
        let mut total: i128 = 0;
        for arm in arms {
            let weight = self.const_int_eval(&arm.weight)?;
            if weight < 0 {
                return None;
            }
            total += weight;
            weights.push(weight);
        }
        if total <= 0 {
            return None;
        }
        let mut pick = self.rng.below(total as u128) as i128;
        let mut chosen = arms.len() - 1;
        for (index, weight) in weights.iter().enumerate() {
            if pick < *weight {
                chosen = index;
                break;
            }
            pick -= weight;
        }
        let (lo, hi) = range;
        let (kind, args) = select_choice(&arms[chosen].choice)?;
        let drawn = match kind {
            "min" => lo,
            "max" => hi,
            "edges" => {
                if self.rng.below(2) == 0 {
                    lo
                } else {
                    hi
                }
            }
            "others" => {
                if hi > lo + 1 {
                    lo + 1 + self.rng.below((hi - lo - 1) as u128) as i128
                } else {
                    lo
                }
            }
            "normal" => {
                let (center, stddev) = args?;
                // Box–Muller from the seeded RNG; the draw is clamped
                // into the range so the bias always lands somewhere
                // legal.
                let u1 = self.rng.unit_f64().max(f64::EPSILON);
                let u2 = self.rng.unit_f64();
                let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                let value = center as f64 + stddev as f64 * z;
                let rounded = value.round() as i128;
                rounded.clamp(lo, hi)
            }
            _ => return None,
        };
        Some(drawn.clamp(lo, hi))
    }

    /// Sample an integer-valued scalar inside the declared bounds
    /// intersected with `domain`. A contradictory domain falls back to
    /// the natural sampling (the rejection predicate then fails the
    /// attempt); booleans, reals, strings and time are not domain-sampled.
    fn sample_int_in(&mut self, ty: &TypeExpr, domain: (i128, i128)) -> Value {
        let Some((natural_low, natural_high)) = natural_range(ty) else {
            return self.sample_type(ty).unwrap_or(Value::Void);
        };
        let low = natural_low.max(domain.0);
        let high = natural_high.min(domain.1);
        if low > high {
            return self.sample_type(ty).unwrap_or(Value::Void);
        }
        if low == high {
            return Value::Int(low);
        }
        Value::Int(low + self.rng.below((high - low + 1) as u128) as i128)
    }

    /// Sample one value of the declared type within its bounds. Struct
    /// fields are instantiated and randomized recursively (so their own
    /// constraints are enforced); non-literal widths use the default.
    fn sample_type(&mut self, ty: &TypeExpr) -> Result<Value, Diagnostic> {
        match ty {
            TypeExpr::Scalar { base, width, .. } => Ok(match base {
                ScalarBase::Uint => Value::Int(self.below_width(width, 32)),
                ScalarBase::Int => {
                    let bits = width_bits(width, 32);
                    let raw = self.rng.below(1u128 << bits) as i128;
                    Value::Int(raw - ((1i128 << bits) >> 1))
                }
                // The long bases are 64-bit wide (Table 17, clause
                // 5.2.3.1): an unconstrained `longuint` samples the full
                // unsigned 64-bit range, a `longint` the full signed one.
                // i128 storage holds both without truncation.
                ScalarBase::Longuint => Value::Int(self.below_width(width, 64)),
                ScalarBase::Longint => {
                    let bits = width_bits(width, 64);
                    let raw = self.rng.below(1u128 << bits) as i128;
                    Value::Int(raw - ((1i128 << bits) >> 1))
                }
                ScalarBase::Bit => Value::Int(self.rng.below(2) as i128),
                ScalarBase::Byte => Value::Int(self.rng.below(256) as i128),
                ScalarBase::Nibble => Value::Int(self.rng.below(16) as i128),
                ScalarBase::Bool => Value::Bool(self.rng.below(2) == 1),
                ScalarBase::Real => Value::Real(self.rng.unit_f64()),
                ScalarBase::String => Value::Str(String::new()),
                ScalarBase::Time => Value::Int(0),
            }),
            TypeExpr::List { element, .. } => {
                let len = self.rng.below(4) as usize;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.sample_type(element)?);
                }
                Ok(Value::List(ListValue {
                    items: Rc::new(RefCell::new(items)),
                    element: Some(element.render()),
                }))
            }
            TypeExpr::Named { name, .. } => {
                let types: &[crate::elab::ElaboratedType] = self.types;
                if let Some(t) = types.iter().find(|t| t.name == name.as_str()) {
                    // Clause 5.2.9: no object is of an interface type — a
                    // generated interface-typed field stays NULL.
                    if t.kind == DeclarationKind::Interface {
                        return Ok(Value::Void);
                    }
                    if t.kind == DeclarationKind::Type {
                        let names: Vec<&str> =
                            t.enumerals.iter().map(|e| e.name.as_str()).collect();
                        if names.is_empty() {
                            return Ok(Value::Void);
                        }
                        let pick = names[self.rng.below(names.len() as u128) as usize];
                        return Ok(Value::Enum {
                            type_name: name.clone(),
                            name: pick.to_owned(),
                        });
                    }
                }
                if self.depth > 50 {
                    return Err(self.error("E0406", "recursion too deep", origin_span()));
                }
                self.depth += 1;
                let index = self.instantiate(name);
                let result = self.gen_object(index, &[]);
                self.depth -= 1;
                result?;
                Ok(Value::Object(index))
            }
            TypeExpr::WhenQualified { value, base, .. } => {
                // A when-qualified field generates an instance of the
                // synthesized subtype view, whose when members (fields,
                // constraints) apply for it (6.7). Chained qualifiers
                // resolve to the combined view name.
                let Some(view_name) = when_view_name(value, base) else {
                    return Ok(Value::Void);
                };
                if self.depth > 50 {
                    return Err(self.error("E0406", "recursion too deep", origin_span()));
                }
                self.depth += 1;
                let index = self.instantiate(&view_name);
                let result = self.gen_object(index, &[]);
                self.depth -= 1;
                result?;
                // A spelling whose view was not synthesized still fixes
                // its qualifier words as determinants (6.7) — but after
                // the base fields' generation, which must not overwrite
                // them back.
                if !self.types.iter().any(|t| t.name == view_name) {
                    let words = when_spelling_words(value, base);
                    if let Some(base_name) = when_view_base(base) {
                        for word in words {
                            self.set_spelled_determinant(index, &base_name, &word);
                        }
                    }
                }
                Ok(Value::Object(index))
            }
            TypeExpr::InlineEnum { names, .. } => {
                // Random enumerand from the inline list.
                if names.is_empty() {
                    return Ok(Value::Void);
                }
                let pick = names[self.rng.below(names.len() as u128) as usize].clone();
                Ok(Value::Enum {
                    type_name: inline_enum_key(names),
                    name: pick,
                })
            }
            // A template reference generates an instance of its
            // monomorphized view (8.3); formals are bound only inside a
            // template's own declaration and yield nothing here.
            TypeExpr::TemplateRef { name, args, .. } => {
                let instance = crate::ast::template_instance_name(name, args);
                if self.depth > 50 {
                    return Err(self.error("E0406", "recursion too deep", origin_span()));
                }
                self.depth += 1;
                let index = self.instantiate(&instance);
                let result = self.gen_object(index, &[]);
                self.depth -= 1;
                result?;
                Ok(Value::Object(index))
            }
            // `typeof()`/`typeof_item()` placeholders resolve during
            // elaboration (5.9); reaching the sampler with one means the
            // static type was undetermined — nothing to generate. The
            // template shapes (a formal, a width-qualified formal, a
            // value actual) never sample.
            TypeExpr::TypeOf { .. } | TypeExpr::TypeOfItem { .. } => Ok(Value::Void),
            TypeExpr::Formal { .. } | TypeExpr::FormalWidth { .. } | TypeExpr::Value { .. } => {
                Ok(Value::Void)
            }
        }
    }

    fn below_width(&mut self, width: &Option<Box<Expr>>, default_bits: u32) -> i128 {
        self.rng.below(1u128 << width_bits(width, default_bits)) as i128
    }

    /// Resolve a method's `is also` blocks on an instance's type.
    fn resolve_method(
        &self,
        object_index: usize,
        name: &str,
        span: crate::ast::Span,
    ) -> Result<Vec<&MethodDecl>, Diagnostic> {
        let type_name = self.instances[object_index].type_name.clone();
        resolve_method_blocks(self.types, &type_name, name, span)
    }

    fn peek_statement_receiver(
        &self,
        expr: &Expr,
        frame: &Frame,
        me: Option<usize>,
    ) -> Option<usize> {
        match expr {
            Expr::Ident { name, .. } if name == "me" || name == "self" => me,
            Expr::Ident { name, .. } => match frame
                .locals
                .get(name)
                .or_else(|| me.and_then(|index| self.instances[index].get(name)))
            {
                Some(Value::Object(index)) => Some(*index),
                _ => None,
            },
            Expr::MemberAccess { object, name, .. } => {
                let index = self.peek_statement_receiver(object, frame, me)?;
                match self.instances[index].get(name) {
                    Some(Value::Object(index)) => Some(*index),
                    _ => None,
                }
            }
            Expr::Paren { expr, .. } => self.peek_statement_receiver(expr, frame, me),
            _ => None,
        }
    }

    fn statement_type_name(ty: &TypeExpr) -> Option<String> {
        match ty {
            TypeExpr::Named { name, .. } => Some(name.clone()),
            TypeExpr::WhenQualified { value, base, .. } => when_view_name(value, base),
            TypeExpr::TemplateRef { name, args, .. } => {
                Some(crate::ast::template_instance_name(name, args))
            }
            _ => None,
        }
    }

    /// Resolve receiver kinds without evaluating a receiver or argument.
    /// The later call preparation evaluates each expression exactly once.
    fn statement_receiver_type(
        &self,
        expr: &Expr,
        frame: &Frame,
        me: Option<usize>,
    ) -> Option<String> {
        if let Some(index) = self.peek_statement_receiver(expr, frame, me) {
            return Some(self.instances[index].type_name.clone());
        }
        if let Some(ty) = self.static_type_of(expr, frame, me) {
            return Self::statement_type_name(&ty);
        }
        match expr {
            Expr::Paren { expr, .. } => self.statement_receiver_type(expr, frame, me),
            Expr::New { ty: Some(ty), .. } | Expr::CastAs { ty, .. } => {
                Self::statement_type_name(ty)
            }
            Expr::Index { base, .. } => match self.static_type_of(base, frame, me)? {
                TypeExpr::List { element, .. } => Self::statement_type_name(&element),
                _ => None,
            },
            Expr::Call { callee, .. } => {
                let (owner, name) = match callee.as_ref() {
                    Expr::Ident { name, .. } => (self.instances[me?].type_name.clone(), name),
                    Expr::MemberAccess { object, name, .. } => {
                        (self.statement_receiver_type(object, frame, me)?, name)
                    }
                    _ => return None,
                };
                let method = self
                    .types
                    .iter()
                    .find(|ty| ty.name == owner)?
                    .methods
                    .iter()
                    .find(|method| method.name == *name)?;
                Self::statement_type_name(method.return_type.as_ref()?)
            }
            _ => None,
        }
    }

    /// A whole call used as a statement can carry a suspension without
    /// an expression-value continuation. Builtins and unresolved value
    /// receivers keep their ordinary expression dispatch.
    fn declared_statement_call(
        &mut self,
        expr: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Option<Flow>, Diagnostic> {
        if !self.suspend_eligible() {
            return Ok(None);
        }
        let Expr::Call { callee, args, span } = expr else {
            return Ok(None);
        };
        let (type_name, name) = match callee.as_ref() {
            Expr::Ident { name, .. } if !crate::builtins::is_builtin_routine(name) => {
                let Some(index) = me else {
                    return Ok(None);
                };
                (self.instances[index].type_name.clone(), name)
            }
            Expr::MemberAccess { object, name, .. } => {
                let Some(type_name) = self.statement_receiver_type(object, frame, me) else {
                    return Ok(None);
                };
                (type_name, name)
            }
            _ => return Ok(None),
        };
        if !self.types.iter().any(|ty| {
            ty.name == type_name
                && ty.methods.iter().any(|method| {
                    method.name == *name && !method.is_static && method.tcm_event.is_some()
                })
        }) {
            return Ok(None);
        }
        let hints = self.call_arg_hints(callee, args.len(), frame, me);
        let mut values = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            values.push(self.eval_with_hint(
                hints.get(index).cloned().flatten(),
                arg,
                frame,
                me,
            )?);
        }
        let receiver = match callee.as_ref() {
            Expr::Ident { .. } => me.expect("enclosing instance was checked"),
            Expr::MemberAccess { object, .. } => match self.eval(object, frame, me)? {
                Value::Object(index) => index,
                _ => return Err(self.error("E0402", "method call requires an object", *span)),
            },
            _ => unreachable!("callee shape was checked"),
        };
        self.begin_statement_call(receiver, name, values, *span, frame)
            .map(Some)
    }

    fn begin_statement_call(
        &mut self,
        receiver: usize,
        name: &str,
        args: Vec<Value>,
        span: crate::ast::Span,
        caller: &Frame,
    ) -> Result<Flow, Diagnostic> {
        let declared = self
            .resolve_method(receiver, name, span)
            .is_ok_and(|blocks| blocks.iter().any(|block| !block.is_static));
        if !declared {
            self.call_method(receiver, name, args, span)?;
            return Ok(Flow::Normal);
        }
        self.exec_statement_call(
            StatementCall {
                receiver,
                name: name.to_owned(),
                args,
                block: 0,
                frame: Frame::default(),
                span,
            },
            caller,
            None,
        )
    }

    /// Carry one method call through all its composed blocks. The path
    /// stores each callee's locals at its call boundary; the scheduler's
    /// outer frame always remains the caller's frame.
    fn exec_statement_call(
        &mut self,
        mut call: StatementCall,
        caller: &Frame,
        mut resume: Option<&[PathLevel]>,
    ) -> Result<Flow, Diagnostic> {
        if self.depth > 200 {
            return Err(self.error("E0406", "recursion too deep", call.span));
        }
        self.depth += 1;
        let result = (|| {
            let types = self.types;
            let type_name = self.instances[call.receiver].type_name.clone();
            let blocks = resolve_method_blocks(types, &type_name, &call.name, call.span)?;
            let call_level = self.exec_context.len().checked_sub(1).ok_or_else(|| {
                self.error(
                    "E0402",
                    "internal: method continuation without a caller",
                    call.span,
                )
            })?;
            while call.block < blocks.len() {
                let block = blocks[call.block];
                if call.args.len() != block.params.len() {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "method '{}' expects {} argument(s), got {}",
                            call.name,
                            block.params.len(),
                            call.args.len()
                        ),
                        call.span,
                    ));
                }
                let mut frame = if resume.is_some() {
                    call.frame.clone()
                } else {
                    let mut frame = Frame::default();
                    for (param, arg) in block.params.iter().zip(&call.args) {
                        if let Some(ty) = &param.ty {
                            frame.local_types.insert(param.name.clone(), ty.clone());
                        }
                        let value =
                            self.bind_slot_value(arg.clone(), param.ty.as_ref(), call.span)?;
                        frame.locals.insert(param.name.clone(), value);
                    }
                    frame
                };
                call.frame = frame.clone();
                self.exec_context[call_level].child = Some(Child::Call(Box::new(call.clone())));
                let flow = match resume.take() {
                    Some(path) => self.exec_block_resumable(
                        &block.body,
                        &mut frame,
                        Some(call.receiver),
                        path,
                    )?,
                    None => self.exec_block(&block.body, &mut frame, Some(call.receiver))?,
                };
                match flow {
                    Flow::Suspend => {
                        let Some(pending) = self.pending_suspension.as_mut() else {
                            return Err(self.error(
                                "E0402",
                                "internal: method suspension without state",
                                call.span,
                            ));
                        };
                        call.frame = pending.frame.clone();
                        pending.path[call_level].child = Some(Child::Call(Box::new(call)));
                        pending.frame = caller.clone();
                        return Ok(Flow::Suspend);
                    }
                    Flow::Stop => {
                        self.stop_requested = true;
                        return Ok(Flow::Stop);
                    }
                    Flow::Normal | Flow::Break | Flow::Continue | Flow::Return(_) => {
                        call.block += 1;
                    }
                }
            }
            Ok(Flow::Normal)
        })();
        self.depth -= 1;
        result
    }

    fn call_method(
        &mut self,
        object_index: usize,
        name: &str,
        args: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let type_name = self.instances[object_index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let blocks = match resolve_method_blocks(types, &type_name, name, span) {
            Ok(blocks) => blocks,
            Err(diagnostic) => {
                // No user method of the name: the predefined any_unit
                // queries (7.4) dispatch here (a user definition of the
                // same name wins, having resolved above).
                match (name, args.as_slice()) {
                    ("e_path", []) => return self.instance_e_path(object_index, span),
                    ("get_children", []) => {
                        return Ok(self.instance_children(object_index, false));
                    }
                    ("get_children", [Value::Bool(with_ports)]) => {
                        return Ok(self.instance_children(object_index, *with_ports));
                    }
                    ("get_ports", []) => return Ok(self.instance_ports(object_index)),
                    ("get_ports_recursively", []) => {
                        return Ok(self.instance_ports_recursively(object_index));
                    }
                    _ => {}
                }
                // The recognised-but-unimplemented 2019 surface (E0419)
                // reports instead of the unknown-method error.
                if let Some(stub) = self.unimplemented_2019(name, &args, span) {
                    return Err(stub);
                }
                return Err(diagnostic);
            }
        };
        // A static method is called as `Type::name(...)` (clause 17.2.2)
        // — the instance form has no static dispatch: it would hand the
        // method a `me` that a static method must not have.
        if blocks.iter().all(|block| block.is_static) {
            return Err(self.error(
                "E0402",
                format!(
                    "'{name}' is a static method of '{type_name}'; call it as '{type_name}::{name}(...)' (clause 17.2.2)"
                ),
                span,
            ));
        }
        if self.depth > 200 {
            return Err(self.error("E0406", "recursion too deep", span));
        }
        self.depth += 1;
        // Inline calls are atomic regions (13.2.2: a called TCM runs to
        // its return before the callee resumes): waits inside keep the
        // inline spin.
        self.atomic_depth += 1;
        let mut result = Value::Void;
        for block in blocks {
            if args.len() != block.params.len() {
                self.atomic_depth -= 1;
                self.depth -= 1;
                return Err(self.error(
                    "E0402",
                    format!(
                        "method '{name}' expects {} argument(s), got {}",
                        block.params.len(),
                        args.len()
                    ),
                    span,
                ));
            }
            let mut frame = Frame::default();
            for (param, arg) in block.params.iter().zip(args.iter()) {
                if let Some(ty) = &param.ty {
                    frame.local_types.insert(param.name.clone(), ty.clone());
                }
                // A parameter binds by value (T7.1): a custom numeric
                // argument copies, and a scalar argument converts when
                // the parameter's declared type is a custom numeric
                // type. The clause 31 dispatch passes struct objects,
                // which bind unchanged.
                let bound = self.bind_slot_value(arg.clone(), param.ty.as_ref(), span)?;
                frame.locals.insert(param.name.clone(), bound);
            }
            match self.exec_block(&block.body, &mut frame, Some(object_index))? {
                Flow::Return(value) => result = value,
                Flow::Normal | Flow::Break | Flow::Continue => {}
                // stop_run() inside a TCM unwinds the whole run; the flag
                // is checked by the run loop and the calling scheduler.
                Flow::Stop => {
                    self.stop_requested = true;
                }
                // Suspension is disabled in atomic regions.
                Flow::Suspend => {}
            }
        }
        self.atomic_depth -= 1;
        self.depth -= 1;
        Ok(result)
    }

    /// `Type::method(args)` — the static method dispatch (clause
    /// 17.2.2): the composed static blocks of the declaring type run
    /// without a receiver. A static method is not a TCM: its body is an
    /// atomic region like an inline call, and bare static-field /
    /// static-method references inside it resolve against
    /// `static_context` (this type).
    fn call_static_method(
        &mut self,
        type_name: &str,
        name: &str,
        args: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let blocks: Vec<&MethodDecl> = self
            .static_methods
            .get(type_name)
            .map(|methods| {
                methods
                    .iter()
                    .filter(|method| method.name == name)
                    .collect()
            })
            .unwrap_or_default();
        if blocks.is_empty() {
            return Err(self.error(
                "E0401",
                format!("static method '{type_name}::{name}' not found"),
                span,
            ));
        }
        if self.depth > 200 {
            return Err(self.error("E0406", "recursion too deep", span));
        }
        self.depth += 1;
        self.atomic_depth += 1;
        let previous_context = self.static_context.take();
        self.static_context = Some(type_name.to_owned());
        let mut result = Value::Void;
        for block in &blocks {
            if args.len() != block.params.len() {
                self.static_context = previous_context;
                self.atomic_depth -= 1;
                self.depth -= 1;
                return Err(self.error(
                    "E0402",
                    format!(
                        "static method '{name}' expects {} argument(s), got {}",
                        block.params.len(),
                        args.len()
                    ),
                    span,
                ));
            }
            let mut frame = Frame::default();
            for (param, arg) in block.params.iter().zip(args.iter()) {
                if let Some(ty) = &param.ty {
                    frame.local_types.insert(param.name.clone(), ty.clone());
                }
                frame.locals.insert(param.name.clone(), arg.clone());
            }
            match self.exec_block(&block.body, &mut frame, None)? {
                Flow::Return(value) => result = value,
                Flow::Normal | Flow::Break | Flow::Continue => {}
                Flow::Stop => {
                    self.stop_requested = true;
                }
                // Suspension is disabled in atomic regions.
                Flow::Suspend => {}
            }
        }
        self.static_context = previous_context;
        self.atomic_depth -= 1;
        self.depth -= 1;
        Ok(result)
    }

    /// The value of `Type::field` from the static store (clause
    /// 6.9.3/4.17.4). The well-formedness pass validates the access
    /// shape; a missing entry is a runtime gap (see the store's design
    /// note on `Interp::statics`).
    fn read_static_field(
        &self,
        type_name: &str,
        member: &str,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        self.statics
            .get(type_name)
            .and_then(|fields| fields.get(member))
            .cloned()
            .ok_or_else(|| {
                self.error(
                    "E0401",
                    format!("unknown static member '{type_name}::{member}'"),
                    span,
                )
            })
    }

    /// The declaring type of a static field referenced by bare name, or
    /// `None`. The starting point is the enclosing instance's type (an
    /// instance method sees its type's and its ancestors' statics) or,
    /// inside a static method, the static context's type. The walk
    /// follows the composed `base` chain (like parents, when views to
    /// their base) to the type whose registry declares the field — the
    /// same name its store entry lives under.
    fn bare_static_field_owner(&self, me: Option<usize>, name: &str) -> Option<String> {
        let start = match (me, &self.static_context) {
            (Some(index), _) => self.instances[index].type_name.clone(),
            (None, Some(type_name)) => type_name.clone(),
            (None, None) => return None,
        };
        self.static_field_owner_from(&start, name)
    }

    fn static_field_owner_from(&self, start: &str, name: &str) -> Option<String> {
        let mut current = start.to_owned();
        // The chain is bounded (elaboration rejects cycles); the cap
        // keeps a pathological type list from looping here.
        for _ in 0..64 {
            if let Some(fields) = self.static_fields.get(&current)
                && fields.iter().any(|field| field.name == name)
            {
                return Some(current);
            }
            let base = self
                .types
                .iter()
                .find(|ty| ty.name == current)
                .and_then(|ty| ty.base.clone())?;
            current = base;
        }
        None
    }

    /// Dispatch a built-in routine call.
    fn call_builtin(
        &mut self,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
        me: Option<usize>,
    ) -> Result<Value, Diagnostic> {
        if name == "out" {
            let text = self.format_values_checked(&values, span)?;
            self.output.push_str(&text);
            return Ok(Value::Void);
        }
        if name == "outf" {
            return self.call_outf(values, span);
        }
        if name == "dut_error" || name == "error" {
            self.dut_errors += 1;
            let text = self.format_values_checked(&values, span)?;
            self.output.push_str(&format!("{name}: {text}\n"));
            return Ok(Value::Void);
        }
        if name == "message" {
            // 24.9.2.14: the message path first calls the receiving
            // instance's user_message_handler() when its type defines
            // one (an empty hook by default; `extend any_unit` refines
            // it for every unit), then the message text prints — the
            // severity argument is skipped in this subset (verbosity
            // filtering is not modeled).
            if let Some(index) = me {
                let type_name = self.instances[index].type_name.clone();
                if resolve_method_blocks(self.types, &type_name, "user_message_handler", span)
                    .is_ok()
                {
                    self.call_method(index, "user_message_handler", Vec::new(), span)?;
                }
            }
            let text = self.format_values_checked(&values[1..], span)?;
            self.output.push_str(&text);
            self.output.push('\n');
            return Ok(Value::Void);
        }
        if name == "append" {
            // String concatenation form; list append is a later milestone.
            let text = self.format_values_checked(&values, span)?;
            return Ok(Value::Str(text));
        }
        if name == "stop_run" {
            self.stop_requested = true;
            return Ok(Value::Void);
        }
        if name == "raise_objection" || name == "drop_objection" {
            // Objections coordinate end-of-test across TCMs; the inline
            // subset has nothing to wait on, so they are no-ops.
            return Ok(Value::Void);
        }
        if name == "pack" {
            // pack(NULL, item) packs the item's physical fields,
            // MSB-first in declaration order, into one integer. Only the
            // null-packing argument form occurs in the corpora; the
            // packing-options argument is ignored.
            let Some(Value::Object(index)) =
                values.iter().rev().find(|v| matches!(v, Value::Object(_)))
            else {
                return Err(self.error("E0402", "'pack' expects an object to pack", span));
            };
            return self.pack_object(*index, span);
        }
        if name.starts_with("hdl_") {
            return self.call_hdl(name, values, span);
        }
        if name == "set_check" {
            // 16.3.6: `set_check(name, severity|effect [, root_unit])`
            // sets a named check's reporting effect. The check-effect
            // model is a later milestone; the call is accepted (the
            // name carries, the root unit is ignored) and has no
            // effect in this stage.
            if values.len() < 2 || values.len() > 3 {
                return Err(self.error(
                    "E0402",
                    format!("'set_check' expects 2 or 3 arguments, got {}", values.len()),
                    span,
                ));
            }
            if !matches!(&values[0], Value::Str(_)) {
                return Err(self.error(
                    "E0402",
                    "'set_check' expects the check name as a string first",
                    span,
                ));
            }
            return Ok(Value::Void);
        }
        if name == "do_bind" {
            // 9.13.3.2: perform the declarative port bindings — legal
            // only in the connect_ports() generation sub-phase (E0420
            // elsewhere). No procedural bindings are pending in the
            // `keep bind` model: the recorded edges apply statically.
            if !values.is_empty() {
                return Err(self.error(
                    "E0402",
                    format!("'do_bind' expects no arguments, got {}", values.len()),
                    span,
                ));
            }
            if !self.in_connect_ports {
                return Err(self.error(
                    "E0420",
                    "do_bind() may only be called in the connect_ports() sub-phase (9.13.3)",
                    span,
                ));
            }
            return Ok(Value::Void);
        }
        if name == "deep_copy" {
            // 29.2.2: the general deep copy (scalars, strings, sets,
            // lists, structs).
            let [value] = values.as_slice() else {
                return Err(self.error(
                    "E0402",
                    format!("'deep_copy' expects one argument, got {}", values.len()),
                    span,
                ));
            };
            return self.deep_copy_value(value, 0, span);
        }
        // The recognised-but-unimplemented 2019 surface (E0419) sits
        // between the implemented builtins and the unknown-routine error.
        if let Some(diagnostic) = self.unimplemented_2019(name, &values, span) {
            return Err(diagnostic);
        }
        Err(self.error("E0405", format!("routine '{name}' is not supported"), span))
    }

    /// The clause 35.4 annotation API on `@T` (T10.3). `@T.
    /// get_all_annotations()` is implemented for real: the list of the
    /// type's recorded attachments (clause 35.3), each rendered
    /// source-like with its attribute values as written — a
    /// `list of string`. The element-taking calls
    /// (`get_attached_annotation`, `get_attached_annotations_list`) and
    /// the definition-element surface (`get_definition_element`,
    /// `is_applicable`) are recognised, arity-checked stubs reporting
    /// `E0419` at run time: there is no `rf_definition_element` model
    /// in this stage (the reflection catalogue is a later phase).
    fn call_annotation_type_method(
        &self,
        type_name: &str,
        name: &str,
        values: &[Value],
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let Some(_repeatable) = self.annotation_types.get(type_name) else {
            return Err(self.error(
                "E0401",
                format!("unknown annotation type '@{type_name}'"),
                span,
            ));
        };
        match name {
            "get_all_annotations" => {
                if !values.is_empty() {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "'get_all_annotations()' expects no arguments, got {}",
                            values.len()
                        ),
                        span,
                    ));
                }
                let items = self
                    .attached_annotations
                    .get(type_name)
                    .map(|annotations| {
                        annotations
                            .iter()
                            .map(|annotation| Value::Str(render_attached_annotation(annotation)))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Ok(Value::List(ListValue {
                    items: Rc::new(RefCell::new(items)),
                    element: Some("string".to_owned()),
                }))
            }
            // The recognised-but-unimplemented clause 35.4 surface: the
            // same E0419 shape the UNIMPLEMENTED_2019 table produces
            // (the table itself answers the call form first — these
            // arms serve the bare member-access spellings).
            "get_attached_annotation" | "get_attached_annotations_list" => {
                if values.len() != 1 {
                    return Err(self.error(
                        "E0402",
                        format!("'{name}()' expects one argument, got {}", values.len()),
                        span,
                    ));
                }
                Err(self.error(
                    "E0419",
                    format!(
                        "'{name}()' is recognised (clause 35.4) but not implemented in this stage"
                    ),
                    span,
                ))
            }
            "get_definition_element" => {
                if !values.is_empty() {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "'get_definition_element()' expects no arguments, got {}",
                            values.len()
                        ),
                        span,
                    ));
                }
                Err(self.error(
                    "E0419",
                    "'get_definition_element()' is recognised (clause 35.4) but not implemented in this stage"
                        .to_owned(),
                    span,
                ))
            }
            "is_applicable" => {
                if values.len() != 1 {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "'is_applicable()' expects one argument, got {}",
                            values.len()
                        ),
                        span,
                    ));
                }
                Err(self.error(
                    "E0419",
                    "'is_applicable()' is recognised (clause 35.4) but not implemented in this stage"
                        .to_owned(),
                    span,
                ))
            }
            _ => Err(self.error(
                "E0405",
                format!("annotation type '@{type_name}' has no method '{name}'"),
                span,
            )),
        }
    }

    /// The E0419 stub dispatch ([`UNIMPLEMENTED_2019`]): a recognised
    /// name with a compatible arity reports "'name()' is recognised
    /// (clause X) but not implemented in this stage"; a recognised name
    /// with the wrong arity reports the arity mismatch (the stubs parse
    /// and arity-check). `None` when the name is not in the table.
    fn unimplemented_2019(
        &self,
        name: &str,
        values: &[Value],
        span: crate::ast::Span,
    ) -> Option<Diagnostic> {
        let (_, clause, (min, max)) = UNIMPLEMENTED_2019
            .iter()
            .find(|(candidate, _, _)| *candidate == name)?;
        if values.len() < *min || values.len() > *max {
            let expected = if min == max {
                format!("{min}")
            } else {
                format!("{min}-{max}")
            };
            return Some(self.error(
                "E0402",
                format!(
                    "'{name}' expects {expected} argument(s), got {}",
                    values.len()
                ),
                span,
            ));
        }
        Some(self.error(
            "E0419",
            format!("'{name}()' is recognised (clause {clause}) but not implemented in this stage"),
            span,
        ))
    }

    /// `pack(NULL, obj)` — concatenate the object's physical fields
    /// MSB-first into one integer (per-field widths from their scalar
    /// declarations; non-physical and `!` fields are excluded).
    fn pack_object(&mut self, index: usize, span: crate::ast::Span) -> Result<Value, Diagnostic> {
        let type_name = self.instances[index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let Some(ty) = types.iter().find(|t| t.name == type_name) else {
            return Err(self.error("E0401", format!("unknown type '{type_name}'"), span));
        };
        let mut packed: i128 = 0;
        for field in &ty.fields {
            if !field.physical || field.no_gen {
                continue;
            }
            let Some(value) = self.instances[index].get(&field.name) else {
                continue;
            };
            let Value::Int(bits) = value else {
                return Err(self.error(
                    "E0402",
                    format!("field '{}' cannot be packed (not an integer)", field.name),
                    span,
                ));
            };
            let width = field_width_bits(&field.ty).unwrap_or(1) as u32;
            packed = (packed << width) | (bits & ((1i128 << width) - 1));
        }
        Ok(Value::Int(packed))
    }

    /// Whether the instance's type declares a `simple_port`/`event_port`
    /// member of the name (clause 9). The declared list drives the
    /// lookup, not the instance's HDL-bound subset — every declared
    /// port is a member, bound or not.
    fn instance_has_port(&self, index: usize, name: &str) -> bool {
        let type_name = self.instances[index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        types
            .iter()
            .find(|t| t.name == type_name)
            .is_some_and(|ty| ty.ports.iter().any(|port| port.name == name))
    }

    /// The predefined any_unit instance-path query (7.4): the dotted
    /// field path from `sys` (`sys.u1`, `sys.u2[0]`), following the
    /// same field order the field tree walks. `E0402` for an instance
    /// outside the tree under sys (a local `var`'s struct).
    fn instance_e_path(&self, target: usize, span: crate::ast::Span) -> Result<Value, Diagnostic> {
        let Some(sys_index) = self
            .instances
            .iter()
            .position(|instance| instance.type_name == "sys")
        else {
            return Err(self.error("E0402", "e_path() has no root sys instance", span));
        };
        fn walk(interp: &Interp, index: usize, path: &str, target: usize) -> Option<String> {
            if index == target {
                return Some(path.to_owned());
            }
            let fields: Vec<(String, Value)> = interp.instances[index]
                .fields
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            for (field_name, value) in fields {
                match value {
                    Value::Object(child) => {
                        let child_path = format!("{path}.{field_name}");
                        if let Some(found) = walk(interp, child, &child_path, target) {
                            return Some(found);
                        }
                    }
                    Value::List(list) => {
                        let items = list.items.borrow().clone();
                        for (position, item) in items.iter().enumerate() {
                            let Value::Object(child) = item else {
                                continue;
                            };
                            let child_path = format!("{path}.{field_name}[{position}]");
                            if let Some(found) = walk(interp, *child, &child_path, target) {
                                return Some(found);
                            }
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        match walk(self, sys_index, "sys", target) {
            Some(path) => Ok(Value::Str(path)),
            None => Err(self.error(
                "E0402",
                "e_path() has no instance path: the struct is not in the field tree under sys",
                span,
            )),
        }
    }

    /// `get_children([with_ports])` (7.4.4): the direct child unit
    /// instances in instance-field declaration order — lists flatten in
    /// index order, NULL fields are skipped — plus, with the argument
    /// TRUE, the unit's own ports (the `any_port` children).
    fn instance_children(&self, index: usize, with_ports: bool) -> Value {
        let type_name = self.instances[index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let mut children: Vec<Value> = Vec::new();
        if let Some(ty) = types.iter().find(|t| t.name == type_name) {
            for field in &ty.fields {
                let Some(value) = self.instances[index].get(&field.name) else {
                    continue;
                };
                match value {
                    Value::Object(_) if self.field_type_is_unit(&field.ty) => {
                        if !matches!(value, Value::Void) {
                            children.push(value.clone());
                        }
                    }
                    Value::List(list) => {
                        let element = match &field.ty {
                            TypeExpr::List { element, .. } => element.as_ref(),
                            _ => continue,
                        };
                        if !self.field_type_is_unit(element) {
                            continue;
                        }
                        let items = list.items.borrow().clone();
                        for item in items {
                            if matches!(item, Value::Object(_)) {
                                children.push(item);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if with_ports && let Some(ty) = types.iter().find(|t| t.name == type_name) {
            for port in &ty.ports {
                children.push(Value::Port {
                    owner: index,
                    name: port.name.clone(),
                });
            }
        }
        Value::List(ListValue {
            items: Rc::new(RefCell::new(children)),
            element: Some("any_unit".to_owned()),
        })
    }

    /// `get_ports()` (7.4.5): the unit's own ports as `any_port`
    /// handles, in declaration order.
    fn instance_ports(&self, index: usize) -> Value {
        let type_name = self.instances[index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let ports: Vec<Value> = types
            .iter()
            .find(|t| t.name == type_name)
            .map(|ty| {
                ty.ports
                    .iter()
                    .map(|port| Value::Port {
                        owner: index,
                        name: port.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Value::List(ListValue {
            items: Rc::new(RefCell::new(ports)),
            element: Some("any_port".to_owned()),
        })
    }

    /// `get_ports_recursively()` (7.4.6): the unit's ports followed by
    /// every sub-instance's, in field-tree order.
    fn instance_ports_recursively(&self, index: usize) -> Value {
        let mut ports: Vec<Value> = Vec::new();
        for member in self.field_tree(index) {
            let type_name = self.instances[member].type_name.clone();
            let types: &[crate::elab::ElaboratedType] = self.types;
            if let Some(ty) = types.iter().find(|t| t.name == type_name) {
                for port in &ty.ports {
                    ports.push(Value::Port {
                        owner: member,
                        name: port.name.clone(),
                    });
                }
            }
        }
        Value::List(ListValue {
            items: Rc::new(RefCell::new(ports)),
            element: Some("any_port".to_owned()),
        })
    }

    /// Whether the type expression names a unit type (directly or
    /// through a when-qualified spelling).
    fn field_type_is_unit(&self, ty: &TypeExpr) -> bool {
        let types: &[crate::elab::ElaboratedType] = self.types;
        match ty {
            TypeExpr::Named { name, .. } => types
                .iter()
                .any(|t| t.name == *name && t.kind == DeclarationKind::Unit),
            TypeExpr::WhenQualified { base, .. } => self.field_type_is_unit(base),
            _ => false,
        }
    }

    /// The 9.13 port binding methods over the `keep bind` simple_port
    /// model: `get_bound_set()` (9.13.9), `is_connected()` (9.13.10),
    /// `disconnect()` (9.13.11) and `disconnect_bound_set()`
    /// (9.13.12) are implemented; `connect()` and `remap_hdl_path()`
    /// fall to the E0419 table (no method-port model).
    fn call_port_method(
        &mut self,
        owner: usize,
        port: &str,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        match (name, values.as_slice()) {
            ("get_bound_set", []) => {
                // 9.13.9: the bound set contains the port itself; a
                // disconnected port's set is the port alone. The set's
                // members are the port handles of the owner instance —
                // the `keep bind` model binds a type's own ports to
                // each other.
                let members = self.port_bound_set(owner, port);
                let items = members
                    .into_iter()
                    .map(|(member_owner, member_name)| Value::Port {
                        owner: member_owner,
                        name: member_name,
                    })
                    .collect();
                Ok(Value::List(ListValue {
                    items: Rc::new(RefCell::new(items)),
                    element: Some("any_port".to_owned()),
                }))
            }
            ("is_connected", []) => Ok(Value::Bool(self.port_is_connected(owner, port))),
            ("disconnect", []) => {
                // 9.13.11: remove every connection of the port; its
                // bound set becomes the port itself.
                self.disconnected_ports.insert((owner, port.to_owned()));
                Ok(Value::Void)
            }
            ("disconnect_bound_set", []) => {
                // 9.13.12: disconnect every port of the bound set.
                let members = self.port_bound_set(owner, port);
                for (member_owner, member_name) in members {
                    self.disconnected_ports.insert((member_owner, member_name));
                }
                Ok(Value::Void)
            }
            _ => {
                if let Some(diagnostic) = self.unimplemented_2019(name, &values, span) {
                    return Err(diagnostic);
                }
                Err(self.error(
                    "E0401",
                    format!("unknown method '{name}' on port '{port}'"),
                    span,
                ))
            }
        }
    }

    /// The port's bound set (9.13.9): the connected component of the
    /// type's `keep bind(p, q)` edges that contains the port, the port
    /// itself included — over the owning instance's ports (the model's
    /// binds are intra-instance). Edges touching a port disconnected
    /// since run start drop out.
    fn port_bound_set(&self, owner: usize, port: &str) -> Vec<(usize, String)> {
        if self.disconnected_ports.contains(&(owner, port.to_owned())) {
            return vec![(owner, port.to_owned())];
        }
        let type_name = self.instances[owner].type_name.clone();
        let Some(edges) = self.type_port_binds.get(&type_name) else {
            return vec![(owner, port.to_owned())];
        };
        let mut visited: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::from([port.to_owned()]);
        let mut queue = vec![port.to_owned()];
        while let Some(current) = queue.pop() {
            for (first, second) in edges {
                let partner = if *first == current {
                    second
                } else if *second == current {
                    first
                } else {
                    continue;
                };
                // An edge to (or from) a port disconnected on this
                // instance no longer connects.
                if self.disconnected_ports.contains(&(owner, partner.clone())) {
                    continue;
                }
                if visited.insert(partner.clone()) {
                    queue.push(partner.clone());
                }
            }
        }
        visited.into_iter().map(|name| (owner, name)).collect()
    }

    /// 9.13.10: TRUE when the port is connected — a bound set larger
    /// than the port itself, or an HDL path on the port. A port bound
    /// to nothing on purpose (`keep bind(p, empty)`) and a
    /// disconnected port are FALSE either way.
    fn port_is_connected(&self, owner: usize, port: &str) -> bool {
        let type_name = self.instances[owner].type_name.clone();
        if self
            .type_port_unbound
            .get(&type_name)
            .is_some_and(|ports| ports.iter().any(|name| name == port))
        {
            return false;
        }
        if self.disconnected_ports.contains(&(owner, port.to_owned())) {
            return false;
        }
        if self.port_bound_set(owner, port).len() > 1 {
            return true;
        }
        self.instances[owner].port_path(port).is_some()
    }

    /// The `covers` singleton methods (14.9): the procedural cover
    /// group sampling (14.8.2.10), the overall grade compute/read pair
    /// (14.9.6), the coverage-database path query, and the
    /// configuration calls that fall to the E0419 table.
    fn call_covers_method(
        &mut self,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        match (name, values.as_slice()) {
            ("sample_cg", [Value::Str(spec)]) => {
                // 14.8.2.10: sample a cover group (all its instances)
                // by name, without emitting its sampling event — not
                // permitted before the post_generate() sub-phase ran.
                if !self.post_generate_done {
                    return Err(self.error(
                        "E0402",
                        "covers.sample_cg() is not permitted before the post_generate() sub-phase (14.8.2.10)",
                        span,
                    ));
                }
                self.sample_cover_group_by_name(spec, span)?;
                Ok(Value::Void)
            }
            ("sample_cg", _) => Err(self.error(
                "E0402",
                "'covers.sample_cg' expects the cover group name string (\"type.group\")",
                span,
            )),
            ("compute_overall_grade", []) => Ok(Value::Real(self.compute_overall_grade_value())),
            ("get_overall_grade", []) => {
                // 14.9.6: reads the last computed value; a run that
                // never computed one computes it once here (the read
                // cannot fail, so the split keeps the compute/write
                // pairing observable without a stale-zero state).
                if self.overall_grade.is_none() {
                    self.compute_overall_grade_value();
                }
                Ok(Value::Real(self.overall_grade.unwrap_or(0.0)))
            }
            ("get_ucd_path", []) => {
                // 14.9.15-14.9.17: the coverage-database file path —
                // coverage-database files are out of scope for this
                // stage (no coverage database is written), so the path
                // is the empty string.
                Ok(Value::Str(String::new()))
            }
            _ => {
                if let Some(diagnostic) = self.unimplemented_2019(name, &values, span) {
                    return Err(diagnostic);
                }
                Err(self.error(
                    "E0405",
                    format!("method 'covers.{name}' is not supported"),
                    span,
                ))
            }
        }
    }

    /// The normalised overall coverage grade (14.9.6): the mean grade
    /// over every collected group that has one, 0.0-1.0 (the grade
    /// model is the covered-bucket fraction; the 1M-100M raw scale the
    /// prose normalises is Specman-internal). Stores the value for
    /// `get_overall_grade()`.
    fn compute_overall_grade_value(&mut self) -> f64 {
        let mut sum = 0.0f64;
        let mut count = 0usize;
        for entry in &self.coverage {
            for group in &entry.groups {
                if let Some(grade) = group.grade() {
                    sum += grade;
                    count += 1;
                }
            }
        }
        let grade = if count == 0 { 0.0 } else { sum / count as f64 };
        self.overall_grade = Some(grade);
        grade
    }

    /// `covers.sample_cg("type.group")` (14.8.2.10): sample every live
    /// instance of the type that owns a group of the name, once per
    /// instance, without emitting the group's sampling event. Cover
    /// groups identify by their recorded name here — the sampling
    /// event's path as written (the parser's group naming).
    fn sample_cover_group_by_name(
        &mut self,
        spec: &str,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        let Some((type_name, group_name)) = spec.rsplit_once('.') else {
            return Err(self.error("E0402", "covers.sample_cg expects \"type.group\"", span));
        };
        let Some(entry_index) = self
            .coverage
            .iter()
            .position(|entry| entry.type_name == type_name)
        else {
            return Err(self.error(
                "E0401",
                format!("no cover group '{spec}': type '{type_name}' has no coverage"),
                span,
            ));
        };
        let group_indices: Vec<usize> = self.coverage[entry_index]
            .groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.name == group_name)
            .map(|(index, _)| index)
            .collect();
        if group_indices.is_empty() {
            return Err(self.error(
                "E0401",
                format!("no cover group named '{group_name}' on type '{type_name}'"),
                span,
            ));
        }
        let instance_indices: Vec<usize> = self
            .instances
            .iter()
            .enumerate()
            .filter(|(_, instance)| instance.type_name == type_name)
            .map(|(index, _)| index)
            .collect();
        let mut frame = Frame::default();
        for owner in instance_indices {
            for group_index in &group_indices {
                self.sample_group_on_instance(entry_index, *group_index, owner, &mut frame);
            }
        }
        Ok(())
    }

    /// The `files` singleton methods (30.5): `flush_log_file()` falls
    /// to the E0419 table; every other member is unsupported.
    fn call_files_method(
        &mut self,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        if let Some(diagnostic) = self.unimplemented_2019(name, &values, span) {
            return Err(diagnostic);
        }
        Err(self.error(
            "E0405",
            format!("method 'files.{name}' is not supported"),
            span,
        ))
    }

    // -----------------------------------------------------------------
    // The 32.2 instance iterator (T11.2) — the one reflection surface
    // implemented for real. `start(root)` / `start_all()` compute the
    // full depth-first walk over the instance field tree once; `next()`
    // moves along it and the `get_current_*` queries read the recorded
    // walk. Lists flatten in index order (nested lists recurse), an
    // already-visited object is not revisited, and only struct
    // instances reached through instance fields are visited (no port
    // traversal — the with_ports hook is a stub). Catalogue:
    // docs/REFLECTION_2019.md. Documented approximations: the walk is
    // a start-time snapshot; the descent is capped at
    // INSTANCE_WALK_MAX_DEPTH like the phase-method field tree; the
    // current path counts from the iteration root (the root's depth is
    // 0) and its string spells the root by its e_path when the root is
    // in the field tree under sys (`sys` for the run's root unit), by
    // its type name otherwise.
    // -----------------------------------------------------------------

    /// The `instance_iterator` methods (32.2).
    fn call_instance_iterator_method(
        &mut self,
        handle: Rc<RefCell<InstanceIteratorState>>,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        match (name, values.as_slice()) {
            ("start", [Value::Object(root)]) => {
                let root = *root;
                let root_label = self.iterator_root_label(root);
                let visits = self.walk_instance_tree(root);
                *handle.borrow_mut() = InstanceIteratorState {
                    visits,
                    root_label,
                    started: true,
                    cursor: Some(0),
                };
                Ok(Value::Void)
            }
            ("start", _) => Err(self.error(
                "E0402",
                "'start' expects the root instance to iterate from",
                span,
            )),
            ("start_all", []) => {
                // `start_all()` roots the walk at the run's root unit —
                // the whole field tree under sys.
                let sys_index = self.sys_instance();
                let Some(root) = sys_index else {
                    return Err(self.error(
                        "E0402",
                        "'start_all' has no root sys instance to iterate from",
                        span,
                    ));
                };
                let visits = self.walk_instance_tree(root);
                *handle.borrow_mut() = InstanceIteratorState {
                    visits,
                    root_label: "sys".to_owned(),
                    started: true,
                    cursor: Some(0),
                };
                Ok(Value::Void)
            }
            ("start_all", _) => Err(self.error(
                "E0402",
                format!("'start_all' expects no arguments, got {}", values.len()),
                span,
            )),
            ("finish", []) => {
                // `finish()` ends the iteration: the cursor is dropped,
                // the current-instance queries report from here on.
                let mut state = handle.borrow_mut();
                state.started = false;
                state.cursor = None;
                Ok(Value::Void)
            }
            ("finish", _) => Err(self.error(
                "E0402",
                format!("'finish' expects no arguments, got {}", values.len()),
                span,
            )),
            ("next", []) => {
                // Advance one visit; TRUE while the iterator points at
                // an instance afterwards. The root visited by `start()`
                // is current without a `next()` — the first `next()`
                // moves past it.
                let mut state = handle.borrow_mut();
                if !state.started {
                    return Err(self.error(
                        "E0402",
                        "the instance iterator has not been started — call start(root) or start_all() first (clause 32.2)",
                        span,
                    ));
                }
                let Some(cursor) = state.cursor.as_mut() else {
                    return Ok(Value::Bool(false));
                };
                *cursor += 1;
                if *cursor < state.visits.len() {
                    Ok(Value::Bool(true))
                } else {
                    state.cursor = None;
                    Ok(Value::Bool(false))
                }
            }
            ("next", _) => Err(self.error(
                "E0402",
                format!("'next' expects no arguments, got {}", values.len()),
                span,
            )),
            ("get_current_instance", []) => {
                let item = self.iterator_current_item(&handle, span)?;
                Ok(Value::Object(item.instance))
            }
            ("get_current_depth", []) => {
                let path = self.iterator_current_path(&handle, span)?;
                // The depth is the field-step count from the iteration
                // root: the root itself is depth 0, a list item shares
                // its field's depth.
                Ok(Value::Int((path.len() as i128).saturating_sub(1)))
            }
            ("get_current_path", []) => {
                let path = self.iterator_current_path(&handle, span)?;
                let items = path
                    .iter()
                    .map(|item| Value::InstancePathItem(Rc::new(item.clone())))
                    .collect();
                Ok(Value::List(ListValue {
                    items: Rc::new(RefCell::new(items)),
                    element: Some("instance_path_item".to_owned()),
                }))
            }
            ("get_current_path_item", []) => {
                let path = self.iterator_current_path(&handle, span)?;
                let item = path.last().expect("a current path is never empty");
                Ok(Value::InstancePathItem(Rc::new(item.clone())))
            }
            ("get_path_item_by_index", [Value::Int(index)]) => {
                let path = self.iterator_current_path(&handle, span)?;
                if *index < 0 || *index >= path.len() as i128 {
                    return Err(self.error(
                        "E0404",
                        format!(
                            "path item index {index} out of range (path length {})",
                            path.len()
                        ),
                        span,
                    ));
                }
                let item = &path[*index as usize];
                Ok(Value::InstancePathItem(Rc::new(item.clone())))
            }
            ("get_path_item_by_index", _) => Err(self.error(
                "E0402",
                "'get_path_item_by_index' expects the 0-based item index",
                span,
            )),
            ("get_current_path_string", []) => {
                let path = self.iterator_current_path(&handle, span)?;
                let state = handle.borrow();
                Ok(Value::Str(render_iterator_path(&path, &state.root_label)))
            }
            ("get_current_path_string", _) => Err(self.error(
                "E0402",
                format!(
                    "'get_current_path_string' expects no arguments, got {}",
                    values.len()
                ),
                span,
            )),
            // The 32.2 user-extension hooks: skip_field takes an
            // rf_struct_member (no reflection model), skip_instance and
            // with_ports are methods a user extends the iterator type
            // with — extending a built-in type is not supported in this
            // stage. Recognised, E0419.
            ("skip_field" | "skip_instance" | "with_ports", _) => Err(self.error(
                "E0419",
                format!("'{name}()' is recognised (clause 32.2) but not implemented in this stage"),
                span,
            )),
            _ => Err(self.error(
                "E0405",
                format!("method '{name}' is not supported on instance_iterator"),
                span,
            )),
        }
    }

    /// The run's root `sys` instance index, if the run has one.
    fn sys_instance(&self) -> Option<usize> {
        self.instances
            .iter()
            .position(|instance| instance.type_name == "sys")
    }

    /// The spelling the iterator's path strings use for the iteration
    /// root: its e_path when the root is in the field tree under sys
    /// (`sys` for the root unit itself), its type name otherwise (a
    /// struct created by `new` is outside the field tree).
    fn iterator_root_label(&self, root: usize) -> String {
        if self.sys_instance() == Some(root) {
            return "sys".to_owned();
        }
        if let Ok(Value::Str(path)) = self.instance_e_path(root, (origin(), origin())) {
            return path;
        }
        self.instances[root].type_name.clone()
    }

    /// The full depth-first visit order over `root`'s instance field
    /// tree: pre-order (an instance before its descendants), instance
    /// fields in declaration order, list items flattened in index
    /// order (nested lists recurse), an already-visited object
    /// skipped. Each visit records its full path from the root.
    fn walk_instance_tree(&self, root: usize) -> Vec<Rc<Vec<InstancePathItem>>> {
        let mut visits = Vec::new();
        let mut visited = BTreeSet::new();
        let root_path = Rc::new(vec![InstancePathItem {
            instance: root,
            field: None,
            list_indices: Vec::new(),
        }]);
        self.collect_instance_walk(root, root_path, &mut visits, &mut visited, 0);
        visits
    }

    fn collect_instance_walk(
        &self,
        index: usize,
        path: Rc<Vec<InstancePathItem>>,
        visits: &mut Vec<Rc<Vec<InstancePathItem>>>,
        visited: &mut BTreeSet<usize>,
        depth: u32,
    ) {
        if depth > INSTANCE_WALK_MAX_DEPTH || !visited.insert(index) {
            return;
        }
        visits.push(path.clone());
        // Snapshot the field values: the walk never holds a borrow (a
        // list's shared storage may be aliased elsewhere).
        let fields: Vec<(String, Value)> = self.instances[index]
            .fields
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        for (field_name, value) in fields {
            self.walk_field_value(&path, &field_name, &value, visits, visited, depth);
        }
    }

    /// Descend one field value of the walk: an instance field's object
    /// is one child step; a list contributes one child step per
    /// contained object, in index order (nested lists recurse, the
    /// index chain extending: `grid[0][2]`).
    fn walk_field_value(
        &self,
        path: &Rc<Vec<InstancePathItem>>,
        field_name: &str,
        value: &Value,
        visits: &mut Vec<Rc<Vec<InstancePathItem>>>,
        visited: &mut BTreeSet<usize>,
        depth: u32,
    ) {
        match value {
            Value::Object(child) => {
                let mut child_path = (**path).clone();
                child_path.push(InstancePathItem {
                    instance: *child,
                    field: Some(field_name.to_owned()),
                    list_indices: Vec::new(),
                });
                self.collect_instance_walk(*child, Rc::new(child_path), visits, visited, depth + 1);
            }
            Value::List(list) => {
                let items = list.items.borrow().clone();
                self.walk_list_items(path, field_name, &items, Vec::new(), visits, visited, depth);
            }
            _ => {}
        }
    }

    /// The list arm of [`walk_field_value`]: `chain` carries the index
    /// chain of the enclosing list items (empty for the field's own
    /// list). The chain is length-capped — a list stored inside itself
    /// cannot recurse forever.
    #[allow(clippy::too_many_arguments)]
    fn walk_list_items(
        &self,
        path: &Rc<Vec<InstancePathItem>>,
        field_name: &str,
        items: &[Value],
        chain: Vec<usize>,
        visits: &mut Vec<Rc<Vec<InstancePathItem>>>,
        visited: &mut BTreeSet<usize>,
        depth: u32,
    ) {
        for (position, item) in items.iter().enumerate() {
            let mut child_chain = chain.clone();
            child_chain.push(position);
            match item {
                Value::Object(child) => {
                    let mut child_path = (**path).clone();
                    child_path.push(InstancePathItem {
                        instance: *child,
                        field: Some(field_name.to_owned()),
                        list_indices: child_chain,
                    });
                    self.collect_instance_walk(
                        *child,
                        Rc::new(child_path),
                        visits,
                        visited,
                        depth + 1,
                    );
                }
                Value::List(nested) => {
                    if child_chain.len() >= INSTANCE_WALK_MAX_INDICES {
                        continue;
                    }
                    let nested_items = nested.items.borrow().clone();
                    self.walk_list_items(
                        path,
                        field_name,
                        &nested_items,
                        child_chain,
                        visits,
                        visited,
                        depth,
                    );
                }
                _ => {}
            }
        }
    }

    /// The current visit's path; the diagnostic names the iterator's
    /// state (not started / iteration over) when there is none.
    fn iterator_current_path(
        &self,
        handle: &Rc<RefCell<InstanceIteratorState>>,
        span: crate::ast::Span,
    ) -> Result<Vec<InstancePathItem>, Diagnostic> {
        let state = handle.borrow();
        if !state.started {
            return Err(self.error(
                "E0402",
                "the instance iterator has not been started — call start(root) or start_all() first (clause 32.2)",
                span,
            ));
        }
        let Some(cursor) = state.cursor else {
            return Err(self.error(
                "E0402",
                "the instance iterator's walk is over — no current instance remains",
                span,
            ));
        };
        Ok(state.visits[cursor].as_ref().clone())
    }

    /// Like [`iterator_current_path`], as the current path's last item.
    fn iterator_current_item(
        &self,
        handle: &Rc<RefCell<InstanceIteratorState>>,
        span: crate::ast::Span,
    ) -> Result<InstancePathItem, Diagnostic> {
        Ok(self
            .iterator_current_path(handle, span)?
            .pop()
            .expect("a recorded visit's path is never empty"))
    }

    /// The `instance_path_item` methods (32.2): the descriptors of one
    /// step of an iterator path. The items are handed out by the
    /// iterator; constructing one directly is the E0419 stub.
    fn call_instance_path_item_method(
        &self,
        item: Rc<InstancePathItem>,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        match (name, values.as_slice()) {
            ("get_instance", []) => Ok(Value::Object(item.instance)),
            ("get_instance", _) => Err(self.error(
                "E0402",
                format!("'get_instance' expects no arguments, got {}", values.len()),
                span,
            )),
            ("get_field", []) => match &item.field {
                // The root step has no field: NULL.
                Some(field) => Ok(Value::Str(field.clone())),
                None => Ok(Value::Void),
            },
            ("get_field", _) => Err(self.error(
                "E0402",
                format!("'get_field' expects no arguments, got {}", values.len()),
                span,
            )),
            ("get_list_index", []) => match item.list_indices.last() {
                Some(index) => Ok(Value::Int(*index as i128)),
                // Not a list step: NULL.
                None => Ok(Value::Void),
            },
            ("get_list_index", _) => Err(self.error(
                "E0402",
                format!(
                    "'get_list_index' expects no arguments, got {}",
                    values.len()
                ),
                span,
            )),
            ("get_list_indices", []) => {
                let indices = item
                    .list_indices
                    .iter()
                    .map(|index| Value::Int(*index as i128))
                    .collect();
                Ok(Value::List(ListValue {
                    items: Rc::new(RefCell::new(indices)),
                    element: Some("int".to_owned()),
                }))
            }
            ("get_list_indices", _) => Err(self.error(
                "E0402",
                format!(
                    "'get_list_indices' expects no arguments, got {}",
                    values.len()
                ),
                span,
            )),
            _ => Err(self.error(
                "E0405",
                format!("method '{name}' is not supported on instance_path_item"),
                span,
            )),
        }
    }

    // -----------------------------------------------------------------
    // The clause 33 resource-sharing structs: `semaphore`,
    // `rdv_semaphore` and `locker`. The blocking calls are TCM calls —
    // a successful one yields to the scheduler like any TCM (31.2.5's
    // non-preemptive model interrupts at TCM calls), a blocked one
    // parks the coroutine on the object's FIFO waiter queue until the
    // wake pass services it. `try_up()`/`try_down()` and the value
    // queries never block and never switch.
    // -----------------------------------------------------------------

    /// A statement-position clause 33 blocking call returns the turn's
    /// flow; any other spelling evaluates through the expression
    /// evaluator (`None` here).
    fn sync_statement_call(
        &mut self,
        expr: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Option<Flow>, Diagnostic> {
        let Expr::Call { callee, args, span } = expr else {
            return Ok(None);
        };
        if !args.is_empty() {
            return Ok(None);
        }
        let Expr::MemberAccess { object, name, .. } = callee.as_ref() else {
            return Ok(None);
        };
        if !matches!(name.as_str(), "up" | "down" | "lock" | "release") {
            return Ok(None);
        }
        let Value::Sync(sync) = self.eval(object, frame, me)? else {
            // Not a sync receiver: the generic call path produces its
            // own (correct) error or method resolution.
            return Ok(None);
        };
        let kind = sync.borrow().kind;
        let blocking = match (kind, name.as_str()) {
            (SyncKind::Locker, "lock" | "release") => true,
            (SyncKind::Semaphore | SyncKind::RdvSemaphore, "up" | "down") => true,
            // A blocking-spelled call on the wrong surface ("up" on a
            // locker, "release" on a semaphore) is the generic path's
            // no-such-method error.
            _ => false,
        };
        if !blocking {
            return Ok(None);
        }
        let outcome = match name.as_str() {
            "up" => self.sync_transfer(sync, SyncWaiter::Up, *span, frame, true)?,
            "down" => self.sync_transfer(sync, SyncWaiter::Down, *span, frame, true)?,
            "lock" => self.sync_lock_acquire(sync, *span, frame, true)?,
            _ => self.sync_lock_release(sync, *span, frame, true)?,
        };
        Ok(Some(match outcome {
            SyncOutcome::Done(_) => Flow::Normal,
            SyncOutcome::Yield => Flow::Suspend,
        }))
    }

    /// One clause 33 method call reached through the expression
    /// evaluator: the non-blocking surface plus the blocking calls run
    /// inline (an expression is an atomic fragment — a call that would
    /// block reports E0421 instead of suspending mid-expression).
    fn call_sync_method(
        &mut self,
        sync: Rc<RefCell<SyncState>>,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
        frame: &Frame,
    ) -> Result<Value, Diagnostic> {
        let kind = sync.borrow().kind;
        // Locker surface.
        if kind == SyncKind::Locker {
            match (name, values.as_slice()) {
                ("lock", []) => {
                    return self
                        .sync_lock_acquire(sync, span, frame, false)
                        .map(SyncOutcome::value);
                }
                ("lock", _) => {
                    return Err(self.error("E0402", "'lock()' expects no arguments", span));
                }
                ("release", []) => {
                    return self
                        .sync_lock_release(sync, span, frame, false)
                        .map(SyncOutcome::value);
                }
                ("release", _) => {
                    return Err(self.error("E0402", "'release()' expects no arguments", span));
                }
                _ => {}
            }
        }
        // Semaphore surfaces.
        match (name, values.as_slice()) {
            ("up", []) => self
                .sync_transfer(sync, SyncWaiter::Up, span, frame, false)
                .map(SyncOutcome::value),
            ("up", _) => Err(self.error("E0402", "'up()' expects no arguments", span)),
            ("down", []) => self
                .sync_transfer(sync, SyncWaiter::Down, span, frame, false)
                .map(SyncOutcome::value),
            ("down", _) => Err(self.error("E0402", "'down()' expects no arguments", span)),
            ("try_up", []) => self.sync_try(sync, SyncWaiter::Up),
            ("try_up", _) => Err(self.error("E0402", "'try_up()' expects no arguments", span)),
            ("try_down", []) => self.sync_try(sync, SyncWaiter::Down),
            ("try_down", _) => Err(self.error("E0402", "'try_down()' expects no arguments", span)),
            ("set_value", [Value::Int(new_value)]) => self.sync_set_value(sync, *new_value, span),
            ("set_value", _) => Err(self.error("E0402", "'set_value()' expects the value", span)),
            ("get_value", []) => Ok(Value::Int(sync.borrow().available)),
            ("get_value", _) => {
                Err(self.error("E0402", "'get_value()' expects no arguments", span))
            }
            ("set_max_value", [Value::Int(new_max)]) => {
                self.sync_set_max_value(sync, *new_max, span)
            }
            ("set_max_value", _) => {
                Err(self.error("E0402", "'set_max_value()' expects the maximum", span))
            }
            ("get_max_value", []) => Ok(Value::Int(sync.borrow().max_value)),
            ("get_max_value", _) => {
                Err(self.error("E0402", "'get_max_value()' expects no arguments", span))
            }
            _ => Err(self.error(
                "E0405",
                format!("'{}' has no method '{name}'", kind.name()),
                span,
            )),
        }
    }

    /// One `up()`/`down()` call: the transfer, or the park. On an
    /// `rdv_semaphore` both directions block until they coincide.
    /// `statement_position` marks the statement-level call, which may
    /// yield or park; an expression-position call runs inline.
    #[allow(clippy::too_many_arguments)]
    fn sync_transfer(
        &mut self,
        sync: Rc<RefCell<SyncState>>,
        direction: SyncWaiter,
        span: crate::ast::Span,
        frame: &Frame,
        statement_position: bool,
    ) -> Result<SyncOutcome, Diagnostic> {
        sync.borrow_mut().transfer_attempted = true;
        let kind = sync.borrow().kind;
        if kind == SyncKind::RdvSemaphore {
            return self.sync_rdv_transfer(sync, direction, span, frame, statement_position);
        }
        let satisfiable = {
            let state = sync.borrow();
            // FIFO: a request queues behind any pending request of the
            // same direction — blocked calls are serviced in request
            // order (31.2.1), and a fresh call does not steal ahead.
            let same_direction_pending =
                state.waiters.iter().any(|(_, waiter)| *waiter == direction);
            match direction {
                SyncWaiter::Up => !same_direction_pending && state.available < state.max_value,
                SyncWaiter::Down => !same_direction_pending && state.available > 0,
                SyncWaiter::Lock => false,
            }
        };
        if satisfiable {
            match direction {
                SyncWaiter::Up => sync.borrow_mut().available += 1,
                SyncWaiter::Down => sync.borrow_mut().available -= 1,
                SyncWaiter::Lock => {}
            }
            return self.yield_turn(span, frame, statement_position);
        }
        self.sync_block(sync, direction, span, frame, statement_position)
    }

    /// The `rdv_semaphore` transfer (31.2.1): `up()` and `down()` block
    /// until they coincide, and the paired `up()` thread always runs
    /// first, followed immediately by the `down()` thread. When a call
    /// of the other direction is already parked, the pairing wakes that
    /// parked half; the `up()` half runs this scheduling round and the
    /// `down()` half the next — the documented deterministic reading of
    /// the pairing order.
    fn sync_rdv_transfer(
        &mut self,
        sync: Rc<RefCell<SyncState>>,
        direction: SyncWaiter,
        span: crate::ast::Span,
        frame: &Frame,
        statement_position: bool,
    ) -> Result<SyncOutcome, Diagnostic> {
        let partner = if direction == SyncWaiter::Up {
            SyncWaiter::Down
        } else {
            SyncWaiter::Up
        };
        let paired_serial = {
            let mut state = sync.borrow_mut();
            state
                .waiters
                .iter()
                .position(|(_, waiter)| *waiter == partner)
                .and_then(|position| state.waiters.remove(position))
                .map(|(serial, _)| serial)
        };
        if let Some(serial) = paired_serial {
            // Wake the parked half. The pairing keeps the up() half
            // ahead: the parked up() runs this round, the parked down()
            // the next.
            for co in self.coroutines.iter_mut() {
                if let Some((other_sync, other_serial)) = &co.wake_sync
                    && Rc::ptr_eq(other_sync, &sync)
                    && *other_serial == serial
                {
                    co.wake_sync = None;
                    co.eligible_at_cycle = if partner == SyncWaiter::Up {
                        self.cycle
                    } else {
                        self.cycle + 1
                    };
                    co.suspended_temporal = false;
                    break;
                }
            }
            if direction == SyncWaiter::Up {
                return self.yield_turn(span, frame, statement_position);
            }
            // The down() half follows one round later.
            if !(statement_position && self.suspend_eligible()) {
                return Err(self.error(
                    "E0421",
                    "the blocking call suspends this coroutine until the resource is available — it needs a TCM context (clause 33)",
                    span,
                ));
            }
            self.suspend_here(frame, span, true, self.cycle + 1, false, None)?;
            Ok(SyncOutcome::Yield)
        } else {
            self.sync_block(sync, direction, span, frame, statement_position)
        }
    }

    /// A `try_up()`/`try_down()`: never blocks, never switches (31.2.2).
    /// The FIFO rule holds — a try fails while a pending call of its own
    /// direction waits.
    fn sync_try(
        &mut self,
        sync: Rc<RefCell<SyncState>>,
        direction: SyncWaiter,
    ) -> Result<Value, Diagnostic> {
        sync.borrow_mut().transfer_attempted = true;
        let kind = sync.borrow().kind;
        let mut state = sync.borrow_mut();
        let same_direction_pending = state.waiters.iter().any(|(_, waiter)| *waiter == direction);
        let changed = match kind {
            SyncKind::RdvSemaphore => false,
            SyncKind::Semaphore => match direction {
                SyncWaiter::Up => !same_direction_pending && state.available < state.max_value,
                SyncWaiter::Down => !same_direction_pending && state.available > 0,
                SyncWaiter::Lock => false,
            },
            SyncKind::Locker => false,
        };
        if changed {
            match direction {
                SyncWaiter::Up => state.available += 1,
                SyncWaiter::Down => state.available -= 1,
                SyncWaiter::Lock => {}
            }
        }
        Ok(Value::Bool(changed))
    }

    /// `locker.lock()`: acquire when free; otherwise park (31.2.5, FIFO
    /// order of grant).
    fn sync_lock_acquire(
        &mut self,
        sync: Rc<RefCell<SyncState>>,
        span: crate::ast::Span,
        frame: &Frame,
        statement_position: bool,
    ) -> Result<SyncOutcome, Diagnostic> {
        if sync.borrow().holder.is_none() {
            let holder = self.current_coroutine_id;
            sync.borrow_mut().holder = holder;
            return self.yield_turn(span, frame, statement_position);
        }
        self.sync_block(sync, SyncWaiter::Lock, span, frame, statement_position)
    }

    /// `locker.release()` (31.2.5): `lock()` and `release()` are issued
    /// by the same entity — releasing from another coroutine, or a
    /// locker nothing holds, is E0423.
    fn sync_lock_release(
        &mut self,
        sync: Rc<RefCell<SyncState>>,
        span: crate::ast::Span,
        frame: &Frame,
        statement_position: bool,
    ) -> Result<SyncOutcome, Diagnostic> {
        let holder = self.current_coroutine_id;
        if holder.is_some() && sync.borrow().holder == holder {
            sync.borrow_mut().holder = None;
            self.yield_turn(span, frame, statement_position)
        } else {
            Err(self.error(
                "E0423",
                "'release()' on a locker not held by the calling entity",
                span,
            ))
        }
    }

    /// Park the coroutine on the object's waiter queue: the wake pass
    /// services it when the condition holds. A call that would block
    /// outside a statement-position suspendable context is E0421.
    fn sync_block(
        &mut self,
        sync: Rc<RefCell<SyncState>>,
        direction: SyncWaiter,
        span: crate::ast::Span,
        frame: &Frame,
        statement_position: bool,
    ) -> Result<SyncOutcome, Diagnostic> {
        if !(statement_position && self.suspend_eligible()) {
            return Err(self.error(
                "E0421",
                "the blocking call suspends this coroutine until the resource is available — it needs a TCM context (clause 33)",
                span,
            ));
        }
        let serial = {
            let mut state = sync.borrow_mut();
            let serial = state.next_serial;
            state.next_serial += 1;
            state.waiters.push_back((serial, direction));
            serial
        };
        self.pending_wake_sync = Some((sync, serial));
        self.suspend_here(frame, span, true, self.cycle + 1, false, None)?;
        Ok(SyncOutcome::Yield)
    }

    /// The scheduler-side wake pass, run before every turn pick: service
    /// the oldest waiter whose condition holds, until a full sweep makes
    /// no progress. The queue front is the oldest request; the sweep
    /// walks the parked coroutines in request order, so blocked calls
    /// run in their blocked order (the FIFO reading of 31.2.1).
    fn service_sync_waiters(&mut self) {
        loop {
            let mut progressed = false;
            let mut waiters: Vec<(u64, usize, Rc<RefCell<SyncState>>)> = Vec::new();
            for (position, co) in self.coroutines.iter().enumerate() {
                if let Some((sync, serial)) = &co.wake_sync {
                    waiters.push((*serial, position, Rc::clone(sync)));
                }
            }
            waiters.sort_by_key(|(serial, _, _)| *serial);
            for (_, position, sync) in waiters {
                let Some(serial) = self.coroutines[position]
                    .wake_sync
                    .as_ref()
                    .map(|(_, serial)| *serial)
                else {
                    continue;
                };
                let kind = sync.borrow().kind;
                let Some((front_serial, front_waiter)) = sync.borrow().waiters.front().copied()
                else {
                    continue;
                };
                // FIFO: only the queue front is serviced this round.
                if front_serial != serial {
                    continue;
                }
                match kind {
                    SyncKind::Semaphore => {
                        let mut state = sync.borrow_mut();
                        let serviceable = (front_waiter == SyncWaiter::Up
                            && state.available < state.max_value)
                            || (front_waiter == SyncWaiter::Down && state.available > 0);
                        if serviceable {
                            match front_waiter {
                                SyncWaiter::Up => state.available += 1,
                                _ => state.available -= 1,
                            }
                            state.waiters.pop_front();
                            drop(state);
                            self.wake_sync_waiter(&sync, serial, self.cycle);
                            progressed = true;
                        }
                    }
                    SyncKind::Locker => {
                        let mut state = sync.borrow_mut();
                        if state.holder.is_none() {
                            state.holder = Some(self.coroutines[position].id);
                            state.waiters.pop_front();
                            drop(state);
                            self.wake_sync_waiter(&sync, serial, self.cycle);
                            progressed = true;
                        }
                    }
                    // The rendezvous: pair the front with the oldest
                    // opposite-direction waiter. The pairing keeps the
                    // up() half ahead: it runs this round, the down()
                    // half the next (see `sync_rdv_transfer`).
                    SyncKind::RdvSemaphore => {
                        let partner = if front_waiter == SyncWaiter::Up {
                            SyncWaiter::Down
                        } else {
                            SyncWaiter::Up
                        };
                        let partner_position = {
                            let state = sync.borrow();
                            state
                                .waiters
                                .iter()
                                .enumerate()
                                .skip(1)
                                .find(|(_, (_, waiter))| *waiter == partner)
                                .map(|(queue_position, _)| queue_position)
                        };
                        let Some(queue_position) = partner_position else {
                            continue;
                        };
                        let partner_serial = sync
                            .borrow_mut()
                            .waiters
                            .remove(queue_position)
                            .map(|(serial, _)| serial);
                        let Some(partner_serial) = partner_serial else {
                            continue;
                        };
                        sync.borrow_mut().waiters.pop_front();
                        // The up() half of the pairing runs this round,
                        // the down() half the next.
                        let (front_eligible, partner_eligible) = if front_waiter == SyncWaiter::Up {
                            (self.cycle, self.cycle + 1)
                        } else {
                            (self.cycle + 1, self.cycle)
                        };
                        self.wake_sync_waiter(&sync, serial, front_eligible);
                        self.wake_sync_waiter(&sync, partner_serial, partner_eligible);
                        progressed = true;
                    }
                }
            }
            if !progressed {
                break;
            }
        }
    }

    /// Wake one parked waiter of `sync` at the given eligibility.
    fn wake_sync_waiter(
        &mut self,
        sync: &Rc<RefCell<SyncState>>,
        serial: u64,
        eligible_at_cycle: u64,
    ) {
        for co in self.coroutines.iter_mut() {
            if let Some((other_sync, other_serial)) = &co.wake_sync
                && Rc::ptr_eq(other_sync, sync)
                && *other_serial == serial
            {
                co.wake_sync = None;
                co.eligible_at_cycle = eligible_at_cycle;
                co.suspended_temporal = false;
                break;
            }
        }
    }

    /// The scheduler-side yield of a successful blocking call: the turn
    /// ends at the call (31.2.5's non-preemptive model interrupts at
    /// TCM calls); the coroutine rejoins the round-robin this cycle,
    /// resuming after the statement. An expression-position (inline,
    /// atomic) call runs without the switch.
    fn yield_turn(
        &mut self,
        span: crate::ast::Span,
        frame: &Frame,
        statement_position: bool,
    ) -> Result<SyncOutcome, Diagnostic> {
        if statement_position && self.suspend_eligible() {
            self.suspend_here(frame, span, true, self.cycle, false, None)?;
            return Ok(SyncOutcome::Yield);
        }
        Ok(SyncOutcome::Done(Value::Void))
    }

    /// 31.2.3: `set_value()` — non-negative, within the maximum, and
    /// forbidden once `up()`/`down()` was called; an `rdv_semaphore`
    /// stays at 0.
    fn sync_set_value(
        &mut self,
        sync: Rc<RefCell<SyncState>>,
        new_value: i128,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let (kind, transfer_attempted, max_value) = {
            let state = sync.borrow();
            (state.kind, state.transfer_attempted, state.max_value)
        };
        if transfer_attempted {
            return Err(self.error(
                "E0422",
                "'set_value()' cannot be called after 'up()' or 'down()' was called (31.2.3)",
                span,
            ));
        }
        let valid = match kind {
            SyncKind::RdvSemaphore => new_value == 0,
            _ => new_value >= 0 && new_value <= max_value,
        };
        if !valid {
            return Err(self.error(
                "E0422",
                format!("'set_value({new_value})' is outside the semaphore's value range (31.2.3)"),
                span,
            ));
        }
        sync.borrow_mut().available = new_value;
        Ok(Value::Void)
    }

    /// 31.2.4: `set_max_value()` — positive, not smaller than the
    /// current value, forbidden once `up()`/`down()` was called; an
    /// `rdv_semaphore`'s maximum stays 0.
    fn sync_set_max_value(
        &mut self,
        sync: Rc<RefCell<SyncState>>,
        new_max: i128,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let (kind, transfer_attempted, available) = {
            let state = sync.borrow();
            (state.kind, state.transfer_attempted, state.available)
        };
        if transfer_attempted {
            return Err(self.error(
                "E0422",
                "'set_max_value()' cannot be called after 'up()' or 'down()' was called (31.2.4)",
                span,
            ));
        }
        let valid = match kind {
            SyncKind::RdvSemaphore => new_max == 0,
            _ => new_max > 0 && new_max >= available,
        };
        if !valid {
            return Err(self.error(
                "E0422",
                format!(
                    "'set_max_value({new_max})' is outside the semaphore's maximum range (31.2.4)"
                ),
                span,
            ));
        }
        sync.borrow_mut().max_value = new_max;
        Ok(Value::Void)
    }

    // The 32.3 procedural table API (T9.3): the `table_manager`
    // singleton, the methods on a `table` value and on a `table_row`
    // value. Rows are numbered from 0, columns from 1 (column 0 is the
    // row index). A table read from a file is cached per run and
    // read-only — modifying it is the copy-first runtime error.
    // -----------------------------------------------------------------

    /// The `table_manager` singleton methods (32.3).
    fn call_table_manager_method(
        &mut self,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        match (name, values.as_slice()) {
            ("create_table", [Value::List(list)]) => {
                let items = list.items.borrow().clone();
                let columns: Vec<String> =
                    items.iter().map(|item| self.format_value(item)).collect();
                let header = Rc::new(RefCell::new(crate::tables::TableRowValue {
                    cells: columns.clone(),
                    ..Default::default()
                }));
                Ok(Value::Table(Rc::new(RefCell::new(
                    crate::tables::TableValue {
                        columns,
                        header: Some(header),
                        ..Default::default()
                    },
                ))))
            }
            ("create_table", _) => Err(self.error(
                "E0402",
                "'table_manager.create_table' expects the column-name list",
                span,
            )),
            ("create_table_without_column_names", [Value::Int(size)]) => {
                if *size < 0 {
                    return Err(self.error(
                        "E0404",
                        format!("table size {size} is negative"),
                        span,
                    ));
                }
                Ok(Value::Table(Rc::new(RefCell::new(
                    crate::tables::TableValue {
                        columns: vec![String::new(); *size as usize],
                        ..Default::default()
                    },
                ))))
            }
            ("create_table_without_column_names", _) => Err(self.error(
                "E0402",
                "'table_manager.create_table_without_column_names' expects the column count",
                span,
            )),
            ("create_empty_table", []) => Ok(Value::Table(Rc::new(RefCell::new(
                crate::tables::TableValue::default(),
            )))),
            ("create_empty_table", _) => Err(self.error(
                "E0402",
                "'table_manager.create_empty_table' expects no arguments",
                span,
            )),
            ("create_row", [Value::Int(size)]) => {
                if *size < 0 {
                    return Err(self.error("E0404", format!("row size {size} is negative"), span));
                }
                Ok(Value::TableRow {
                    table: None,
                    row: Rc::new(RefCell::new(crate::tables::TableRowValue {
                        cells: vec![String::new(); *size as usize],
                        ..Default::default()
                    })),
                })
            }
            ("create_row", _) => Err(self.error(
                "E0402",
                "'table_manager.create_row' expects the cell count",
                span,
            )),
            ("create_row_with_data", [Value::List(list)]) => {
                let items = list.items.borrow().clone();
                Ok(Value::TableRow {
                    table: None,
                    row: Rc::new(RefCell::new(crate::tables::TableRowValue {
                        cells: items.iter().map(|item| self.format_value(item)).collect(),
                        ..Default::default()
                    })),
                })
            }
            ("create_row_with_data", _) => Err(self.error(
                "E0402",
                "'table_manager.create_row_with_data' expects the data list",
                span,
            )),
            ("read_table_from_csv", [Value::Str(file), Value::Str(tag)])
            | ("read_table_from_csv", [Value::Str(file), Value::Str(tag), Value::Bool(_)]) => {
                let error_if_none = match values.last() {
                    Some(Value::Bool(flag)) => *flag,
                    _ => true,
                };
                self.read_table_from_csv_cached(file, tag, error_if_none, span)
            }
            ("read_table_from_csv", _) => Err(self.error(
                "E0402",
                "'table_manager.read_table_from_csv' expects (file-name, tag [, error_if_none])",
                span,
            )),
            ("read_all_tables_from_csv", [Value::Str(file)]) => {
                let key = crate::tables::with_csv_extension(file);
                let tables = crate::tables::read_csv_tables(&key).map_err(|err| {
                    self.error("E0401", format!("read_all_tables_from_csv: {err}"), span)
                })?;
                let items: Vec<Value> = tables
                    .into_iter()
                    .map(|entry| Value::Table(Rc::new(RefCell::new(entry.table))))
                    .collect();
                Ok(Value::List(ListValue {
                    items: Rc::new(RefCell::new(items)),
                    element: Some("table".to_owned()),
                }))
            }
            ("read_all_tables_from_csv", _) => Err(self.error(
                "E0402",
                "'table_manager.read_all_tables_from_csv' expects the file name",
                span,
            )),
            // The excel readers are the honest 32.3 stubs: no Excel
            // reader exists in this stage.
            ("read_table_from_excel", _) | ("read_all_tables_from_excel", _) => Err(self.error(
                "E0419",
                format!("'{name}()' is recognised (clause 32.3) but not implemented in this stage"),
                span,
            )),
            ("is_empty_line", [Value::Str(line)]) | ("is_table_end", [Value::Str(line)]) => {
                Ok(Value::Bool(crate::tables::is_empty_line(line)))
            }
            ("is_empty_line", _) | ("is_table_end", _) => Err(self.error(
                "E0402",
                "'table_manager.{name}' expects the csv line string",
                span,
            )),
            ("skip_row", [Value::Str(line)]) => {
                // The tag-complement reading: the line's text after the
                // first comma (see the module-level design note).
                Ok(Value::Str(
                    line.split_once(',')
                        .map(|(_, rest)| rest.trim().to_owned())
                        .unwrap_or_default(),
                ))
            }
            ("skip_row", _) => Err(self.error(
                "E0402",
                "'table_manager.skip_row' expects the csv line string",
                span,
            )),
            ("get_table_tag_from_row", [Value::Str(line)]) => {
                Ok(Value::Str(crate::tables::table_tag_from_row(line)))
            }
            ("get_table_tag_from_row", _) => Err(self.error(
                "E0402",
                "'table_manager.get_table_tag_from_row' expects the csv line string",
                span,
            )),
            _ => Err(self.error(
                "E0405",
                format!("method 'table_manager.{name}' is not supported"),
                span,
            )),
        }
    }

    /// `table_manager.read_table_from_csv(file, tag [, error_if_none])`
    /// (32.3): the file-read table is cached per run and read-only —
    /// one shared value per (resolved path, tag). With
    /// `error_if_none` FALSE a missing table reads as NULL.
    fn read_table_from_csv_cached(
        &mut self,
        file: &str,
        tag: &str,
        error_if_none: bool,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let key = crate::tables::with_csv_extension(file);
        let cache_key = (key.clone(), tag.to_owned());
        if let Some(cached) = self.table_cache.get(&cache_key) {
            return Ok(Value::Table(cached.clone()));
        }
        let tables = crate::tables::read_csv_tables(&key)
            .map_err(|err| self.error("E0401", format!("read_table_from_csv: {err}"), span))?;
        let Some(entry) = tables.into_iter().find(|entry| entry.tag == tag) else {
            if error_if_none {
                return Err(self.error(
                    "E0401",
                    format!("read_table_from_csv: no table '{tag}' in '{key}'"),
                    span,
                ));
            }
            return Ok(Value::Void);
        };
        let handle = Rc::new(RefCell::new(entry.table));
        self.table_cache.insert(cache_key, handle.clone());
        Ok(Value::Table(handle))
    }

    /// The `table` value methods (32.3). Mutating methods report the
    /// copy-first error on a file-read (read-only) table.
    fn call_table_value_method(
        &mut self,
        table: Rc<RefCell<crate::tables::TableValue>>,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let read_only = table.borrow().read_only;
        let source = table
            .borrow()
            .file_name
            .clone()
            .unwrap_or_else(|| "the table".to_owned());
        let mutate = |name: &str| -> Result<(), Diagnostic> {
            if read_only {
                return Err(Diagnostic::new(
                    "E0402",
                    format!(
                        "'{name}' modifies a table read from '{source}', which is read-only; copy() it first (32.3)"
                    ),
                    span.0,
                    span.1,
                ));
            }
            Ok(())
        };
        match (name, values.as_slice()) {
            ("get_csv_file_name", []) => Ok(Value::Str(
                table.borrow().file_name.clone().unwrap_or_default(),
            )),
            ("get_excel_file_name", []) | ("get_excel_sheet_name", []) => {
                // No Excel source exists in this stage: the empty
                // answers (the csv readers never set one).
                Ok(Value::Str(String::new()))
            }
            ("get_columns_num", []) => Ok(Value::Int(table.borrow().columns.len() as i128)),
            ("get_column_name", [Value::Int(column)]) => {
                let index = column_index(table.borrow().columns.len(), *column, span)?;
                Ok(Value::Str(table.borrow().columns[index - 1].clone()))
            }
            ("set_column_name", [Value::Int(column), value]) => {
                mutate(name)?;
                let text = self.format_value(value);
                let mut borrowed = table.borrow_mut();
                let index = column_index(borrowed.columns.len(), *column, span)?;
                borrowed.columns[index - 1] = text.clone();
                if let Some(header) = &borrowed.header {
                    let mut row = header.borrow_mut();
                    while row.cells.len() < index {
                        row.cells.push(String::new());
                    }
                    row.cells[index - 1] = text;
                }
                drop(borrowed);
                Ok(Value::Void)
            }
            ("get_header_row", []) => match &table.borrow().header {
                Some(header) => Ok(Value::TableRow {
                    table: Some(table.clone()),
                    row: header.clone(),
                }),
                None => Ok(Value::Void),
            },
            ("replace_header_row", [Value::TableRow { row, .. }]) => {
                mutate(name)?;
                let cells = row.borrow().cells.clone();
                let mut borrowed = table.borrow_mut();
                borrowed.header = Some(row.clone());
                if !cells.is_empty() {
                    borrowed.columns = cells;
                }
                drop(borrowed);
                Ok(Value::Void)
            }
            ("get_rows_num", []) => Ok(Value::Int(table.borrow().rows.len() as i128)),
            ("get_row", [Value::Int(index)]) => {
                let rows_len = table.borrow().rows.len();
                if *index < 0 || *index >= rows_len as i128 {
                    return Err(self.error(
                        "E0404",
                        format!("table row index {index} out of range (size {rows_len})"),
                        span,
                    ));
                }
                let row = table.borrow().rows[*index as usize].clone();
                Ok(Value::TableRow {
                    table: Some(table.clone()),
                    row,
                })
            }
            ("get_column_index_by_name", [value]) => {
                let column = self.format_value(value);
                Ok(Value::Int(
                    table
                        .borrow()
                        .columns
                        .iter()
                        .position(|name| *name == column)
                        .map_or(0, |position| position as i128 + 1),
                ))
            }
            ("add_row", []) => {
                mutate(name)?;
                let width = table.borrow().columns.len();
                let row = Rc::new(RefCell::new(crate::tables::TableRowValue {
                    cells: vec![String::new(); width],
                    ..Default::default()
                }));
                table.borrow_mut().rows.push(row);
                Ok(Value::Void)
            }
            ("add_row", [Value::TableRow { row, .. }]) => {
                mutate(name)?;
                table.borrow_mut().rows.push(row.clone());
                Ok(Value::Void)
            }
            ("add_filled_row", [Value::List(list)]) => {
                mutate(name)?;
                let items = list.items.borrow().clone();
                let row = Rc::new(RefCell::new(crate::tables::TableRowValue {
                    cells: items.iter().map(|item| self.format_value(item)).collect(),
                    ..Default::default()
                }));
                table.borrow_mut().rows.push(row);
                Ok(Value::Void)
            }
            ("insert_row", [Value::Int(position)]) => {
                mutate(name)?;
                let width = table.borrow().columns.len();
                let row = Rc::new(RefCell::new(crate::tables::TableRowValue {
                    cells: vec![String::new(); width],
                    ..Default::default()
                }));
                insert_row_at(&mut table.borrow_mut(), *position, row, span)?;
                Ok(Value::Void)
            }
            ("insert_row", [Value::Int(position), Value::TableRow { row, .. }]) => {
                mutate(name)?;
                insert_row_at(&mut table.borrow_mut(), *position, row.clone(), span)?;
                Ok(Value::Void)
            }
            ("insert_filled_row", [Value::Int(position), Value::List(list)]) => {
                mutate(name)?;
                let items = list.items.borrow().clone();
                let row = Rc::new(RefCell::new(crate::tables::TableRowValue {
                    cells: items.iter().map(|item| self.format_value(item)).collect(),
                    ..Default::default()
                }));
                insert_row_at(&mut table.borrow_mut(), *position, row, span)?;
                Ok(Value::Void)
            }
            ("replace_row", [Value::Int(position), Value::TableRow { row, .. }]) => {
                mutate(name)?;
                let rows_len = table.borrow().rows.len();
                if *position < 0 || *position >= rows_len as i128 {
                    return Err(self.error(
                        "E0404",
                        format!("table row index {position} out of range (size {rows_len})"),
                        span,
                    ));
                }
                table.borrow_mut().rows[*position as usize] = row.clone();
                Ok(Value::Void)
            }
            ("delete_row", [Value::Int(position)]) => {
                mutate(name)?;
                let rows_len = table.borrow().rows.len();
                if *position < 0 || *position >= rows_len as i128 {
                    return Err(self.error(
                        "E0404",
                        format!("table row index {position} out of range (size {rows_len})"),
                        span,
                    ));
                }
                table.borrow_mut().rows.remove(*position as usize);
                Ok(Value::Void)
            }
            ("delete_column", [Value::Int(column)]) => {
                mutate(name)?;
                let index = column_index(table.borrow().columns.len(), *column, span)?;
                let mut borrowed = table.borrow_mut();
                borrowed.columns.remove(index - 1);
                for row in &borrowed.rows {
                    let mut row = row.borrow_mut();
                    if index - 1 < row.cells.len() {
                        row.cells.remove(index - 1);
                    }
                }
                if let Some(header) = &borrowed.header
                    && index - 1 < header.borrow().cells.len()
                {
                    header.borrow_mut().cells.remove(index - 1);
                }
                drop(borrowed);
                Ok(Value::Void)
            }
            ("copy", []) => Ok(Value::Table(Rc::new(RefCell::new(table.borrow().copied())))),
            ("copy_with_rows", [Value::List(list)]) => {
                // The copy restricted to the given rows (0-based row
                // indexes, or table_row handles selected by identity).
                let items = list.items.borrow().clone();
                let mut selection: Vec<Option<usize>> = Vec::with_capacity(items.len());
                for item in &items {
                    match item {
                        Value::Int(index) => {
                            if *index < 0 {
                                return Err(self.error(
                                    "E0404",
                                    format!("table row index {index} is negative"),
                                    span,
                                ));
                            }
                            selection.push(Some(*index as usize));
                        }
                        Value::TableRow { row, .. } => {
                            selection.push(table.borrow().row_index(row));
                        }
                        _ => {
                            return Err(self.error(
                                "E0402",
                                "'copy_with_rows' expects row indexes or table rows",
                                span,
                            ));
                        }
                    }
                }
                let borrowed = table.borrow();
                let mut copy = borrowed.copied();
                copy.rows.clear();
                for index in selection {
                    let Some(index) = index else {
                        return Err(self.error(
                            "E0404",
                            "copy_with_rows: one of the given rows is not in this table",
                            span,
                        ));
                    };
                    let rows_len = borrowed.rows.len();
                    if index >= rows_len {
                        return Err(self.error(
                            "E0404",
                            format!("table row index {index} out of range (size {rows_len})"),
                            span,
                        ));
                    }
                    let row = Rc::new(RefCell::new(borrowed.rows[index].borrow().clone()));
                    copy.rows.push(row);
                }
                drop(borrowed);
                Ok(Value::Table(Rc::new(RefCell::new(copy))))
            }
            _ => Err(self.error(
                "E0405",
                format!("method 'table.{name}' is not supported"),
                span,
            )),
        }
    }

    /// The `table_row` value methods (32.3). A row obtained from a
    /// read-only table reports the copy-first error on `set_data`; a
    /// standalone row (`create_row`/`create_row_with_data`) is writable.
    fn call_table_row_value_method(
        &mut self,
        table: Option<Rc<RefCell<crate::tables::TableValue>>>,
        row: Rc<RefCell<crate::tables::TableRowValue>>,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let read_only = table.as_ref().is_some_and(|t| t.borrow().read_only);
        let source = table
            .as_ref()
            .and_then(|t| t.borrow().file_name.clone())
            .unwrap_or_else(|| "the table".to_owned());
        match (name, values.as_slice()) {
            ("get_size", []) => Ok(Value::Int(row.borrow().cells.len() as i128)),
            ("get_data", [Value::Int(column)]) => {
                if *column == 0 {
                    // Column 0 is the row index (32.3): read through the
                    // owning table by row identity.
                    let Some(t) = &table else {
                        return Err(self.error(
                            "E0402",
                            "the row index (column 0) is only defined for a row of a table",
                            span,
                        ));
                    };
                    let index = t.borrow().row_index(&row);
                    return Ok(Value::Str(
                        index.map(|index| index.to_string()).unwrap_or_default(),
                    ));
                }
                let index = column_index(row.borrow().cells.len(), *column, span)?;
                Ok(Value::Str(row.borrow().cells[index - 1].clone()))
            }
            ("set_data", [Value::Int(column), value]) => {
                if read_only {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "'set_data' modifies a row of the table read from '{source}', which is read-only; copy() the table first (32.3)"
                        ),
                        span,
                    ));
                }
                if *column == 0 {
                    return Err(self.error(
                        "E0402",
                        "the row index (column 0) is not writable",
                        span,
                    ));
                }
                let text = self.format_value(value);
                let mut borrowed = row.borrow_mut();
                let index = column_index(borrowed.cells.len(), *column, span)?;
                while borrowed.cells.len() < index {
                    borrowed.cells.push(String::new());
                }
                borrowed.cells[index - 1] = text;
                drop(borrowed);
                Ok(Value::Void)
            }
            ("get_csv_line_num", []) => Ok(Value::Int(row.borrow().line.unwrap_or(0) as i128)),
            ("get_csv_file_name", []) => Ok(Value::Str(
                row.borrow().file_name.clone().unwrap_or_default(),
            )),
            ("get_excel_file_name", []) | ("get_excel_sheet_name", []) => {
                // No Excel source exists in this stage: the empty answers.
                Ok(Value::Str(String::new()))
            }
            ("copy_source_info_from", [Value::TableRow { row: other, .. }]) => {
                let other = other.borrow().clone();
                row.borrow_mut().copy_source_info_from(&other);
                Ok(Value::Void)
            }
            ("copy", []) => Ok(Value::TableRow {
                table: None,
                row: Rc::new(RefCell::new(row.borrow().clone())),
            }),
            _ => Err(self.error(
                "E0405",
                format!("method 'table_row.{name}' is not supported"),
                span,
            )),
        }
    }

    /// `deep_copy(<exp>)` (29.2.2): scalars, strings, enumerals and
    /// sets are values — returned as-is; a list deep copy mints a
    /// FRESH list object whose items are deep copies (`list_ptr==`
    /// FALSE against the original, 4.11.4); a struct deep copy is a
    /// new instance of the same type whose fields are deep copies,
    /// followed by the struct's own `copy()` blocks (29.2.2: copy() of
    /// every copied struct is called) when the type defines any.
    fn deep_copy_value(
        &mut self,
        value: &Value,
        depth: u32,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        if depth > 32 {
            return Err(self.error("E0406", "deep_copy nesting too deep", span));
        }
        match value {
            Value::Int(_)
            | Value::Real(_)
            | Value::Bool(_)
            | Value::Str(_)
            | Value::Enum { .. }
            | Value::Set(_)
            | Value::Void
            | Value::Port { .. } => Ok(value.clone()),
            // A clause 33 sync object deep copies as the same object:
            // the synchronization state is identity, not value.
            Value::Sync(_) => Ok(value.clone()),
            // A custom numeric value deep copies by value (T7.1): a
            // fresh implementing-struct instance through the same
            // copy rule a slot binding uses (`numeric_assign` when the
            // struct defines it, a per-field deep copy otherwise).
            Value::Custom { .. } => self.copy_custom_numeric(value, span),
            // A table deep copy (32.3's `copy`): a fresh writable table
            // with fresh row handles (see `TableValue::copied`).
            Value::Table(table) => Ok(Value::Table(Rc::new(RefCell::new(table.borrow().copied())))),
            Value::TableRow { row, .. } => Ok(Value::TableRow {
                table: None,
                row: Rc::new(RefCell::new(row.borrow().clone())),
            }),
            // An iterator copy (32.2, T11.2): an independent iterator
            // positioned identically — the same recorded walk, its own
            // cursor.
            Value::InstanceIterator(handle) => {
                let state = handle.borrow().clone();
                Ok(Value::InstanceIterator(Rc::new(RefCell::new(state))))
            }
            // A path item is an immutable snapshot: the copy shares it.
            Value::InstancePathItem(item) => Ok(Value::InstancePathItem(item.clone())),
            Value::List(list) => {
                let items = list.items.borrow().clone();
                let mut copies = Vec::with_capacity(items.len());
                for item in &items {
                    copies.push(self.deep_copy_value(item, depth + 1, span)?);
                }
                Ok(Value::List(ListValue {
                    items: Rc::new(RefCell::new(copies)),
                    element: list.element.clone(),
                }))
            }
            Value::Object(index) => {
                let index = *index;
                let type_name = self.instances[index].type_name.clone();
                let new_index = self.instantiate(&type_name);
                let fields: Vec<(String, Value)> = self.instances[index]
                    .fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect();
                for (field_name, field_value) in fields {
                    let copied = self.deep_copy_value(&field_value, depth + 1, span)?;
                    self.instances[new_index].set(&field_name, copied);
                }
                if resolve_method_blocks(self.types, &type_name, "copy", span).is_ok() {
                    self.call_method(new_index, "copy", Vec::new(), span)?;
                }
                Ok(Value::Object(new_index))
            }
        }
    }

    // ------------------------------------------------------------------
    // Custom numeric types (5.2.15, 5.8.4, clause 31 — T7.1/T7.3): the
    // by-value value model over the implementing struct's instances and
    // the operator dispatch onto its `numeric_i` methods.
    // ------------------------------------------------------------------

    /// Whether `name` declares a `numeric_type` (5.8.4): values of the
    /// name take their behavior from the implementing struct's
    /// `numeric_i` methods, not from a plain struct's.
    fn is_custom_numeric_type(&self, name: &str) -> bool {
        self.types
            .iter()
            .any(|t| t.name == name && t.kind == DeclarationKind::NumericType)
    }

    /// The implementing struct of custom numeric type `name` — the
    /// `numeric_type` declaration's underlying reference (5.8.4).
    fn numeric_impl_struct(&self, name: &str) -> Option<String> {
        let types: &[crate::elab::ElaboratedType] = self.types;
        let declared = types
            .iter()
            .find(|t| t.name == name && t.kind == DeclarationKind::NumericType)?;
        declared
            .underlying
            .as_ref()
            .and_then(|ty| ty.declared_name())
            .map(str::to_owned)
    }

    /// The `numeric_format_kind` enumerand for the default print radix:
    /// `dec` (T7.3 — `out`/`print`/`append` render a custom numeric
    /// value with `numeric_to_string(dec)`).
    fn numeric_radix_dec() -> Value {
        Value::Enum {
            type_name: "numeric_format_kind".to_owned(),
            name: "dec".to_owned(),
        }
    }

    /// Call one `numeric_i`-shaped method on the implementing-struct
    /// instance behind a custom numeric value (T7.3's dispatch). The
    /// instance is the `me` receiver; `other`/`res` arguments arrive as
    /// struct objects, so the method's writes (`res.raw = ...`) land in
    /// the shared instance. `Ok(None)` when the struct does not define
    /// the method — the operator sites turn that into `E0418`; the
    /// lifecycle sites accept the absence.
    fn call_numeric_method(
        &mut self,
        instance: usize,
        method: &str,
        args: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Option<Value>, Diagnostic> {
        let type_name = self.instances[instance].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        if resolve_method_blocks(types, &type_name, method, span).is_err() {
            return Ok(None);
        }
        let value = self.call_method(instance, method, args, span)?;
        Ok(Some(value))
    }

    /// A fresh custom-numeric value of `type_name` (T7.1's defaulting
    /// rule): a new instance of the implementing struct whose
    /// `set_initial_value()` defines the initial state — the method
    /// runs when the struct defines it; its absence leaves the fields
    /// at their own defaults.
    fn new_custom_numeric(&mut self, type_name: &str, span: crate::ast::Span) -> Value {
        let Some(impl_name) = self.numeric_impl_struct(type_name) else {
            return Value::Void;
        };
        let index = self.instantiate(&impl_name);
        let _ = self.call_numeric_method(index, "set_initial_value", Vec::new(), span);
        Value::Custom {
            type_name: type_name.to_owned(),
            instance: index,
        }
    }

    /// The fresh result instance of a custom-numeric operation (the
    /// `res` of the clause 31 `(other, res)` method shapes), or the
    /// honest error when the type's declaration is broken.
    fn custom_result_instance(
        &mut self,
        type_name: &str,
        span: crate::ast::Span,
    ) -> Result<usize, Diagnostic> {
        match self.new_custom_numeric(type_name, span) {
            Value::Custom { instance, .. } => Ok(instance),
            _ => Err(self.error(
                "E0402",
                format!("numeric_type '{type_name}' has no implementing struct"),
                span,
            )),
        }
    }

    /// The by-value copy of one custom numeric value (T7.1): a fresh
    /// implementing-struct instance, default-initialized
    /// (`set_initial_value()` when defined), then the struct's
    /// `numeric_assign(other)` — the clause 31 copy the struct authors
    /// when plain field-by-field copying is wrong. Without
    /// `numeric_assign` the fields deep copy one by one.
    fn copy_custom_numeric(
        &mut self,
        value: &Value,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let Value::Custom {
            type_name,
            instance,
        } = value
        else {
            return Ok(value.clone());
        };
        let (type_name, instance) = (type_name.clone(), *instance);
        let Some(impl_name) = self.numeric_impl_struct(&type_name) else {
            return Ok(Value::Custom {
                type_name,
                instance,
            });
        };
        let fresh = self.instantiate(&impl_name);
        let _ = self.call_numeric_method(fresh, "set_initial_value", Vec::new(), span);
        let assigned =
            self.call_numeric_method(fresh, "numeric_assign", vec![Value::Object(instance)], span)?;
        if assigned.is_none() {
            // No numeric_assign: the fields deep copy — the honest
            // by-value fallback, because a custom numeric's storage is
            // its struct instance's storage.
            let fields: Vec<(String, Value)> = self.instances[instance]
                .fields
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            for (name, field_value) in fields {
                let copied = self.deep_copy_value(&field_value, 0, span)?;
                self.instances[fresh].set(&name, copied);
            }
        }
        Ok(Value::Custom {
            type_name,
            instance: fresh,
        })
    }

    /// Build a plain scalar into a custom-numeric value of `type_name`
    /// (T7.3's implicit conversions): a fresh instance initialized by
    /// `set_initial_value()`, then `numeric_from_int(v)` or
    /// `numeric_from_real(v)` writes the scalar in. A struct without
    /// the conversion pair reports `E0418`.
    fn convert_scalar_to_custom(
        &mut self,
        value: &Value,
        type_name: &str,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let (method, arg) = match value {
            Value::Int(v) => ("numeric_from_int", Value::Int(*v)),
            Value::Real(v) => ("numeric_from_real", Value::Real(*v)),
            other => {
                return Err(self.error(
                    "E0402",
                    format!(
                        "cannot build a '{}' value from {} (a custom numeric value takes an int or real)",
                        type_name,
                        self.value_kind(other)
                    ),
                    span,
                ));
            }
        };
        let fresh = self.custom_result_instance(type_name, span)?;
        if self
            .call_numeric_method(fresh, method, vec![arg], span)?
            .is_none()
        {
            return Err(self.numeric_method_missing(type_name, "conversion", method, span));
        }
        Ok(Value::Custom {
            type_name: type_name.to_owned(),
            instance: fresh,
        })
    }

    /// The `E0418` diagnostic (T7.1): an operation or conversion the
    /// clause 31 dispatch needs has no definition in the numeric type's
    /// implementing struct.
    fn numeric_method_missing(
        &self,
        type_name: &str,
        what: &str,
        method: &str,
        span: crate::ast::Span,
    ) -> Diagnostic {
        self.error(
            "E0418",
            format!("{what} is not implemented by numeric type '{type_name}' ({method} missing)"),
            span,
        )
    }

    /// Bind `value` into a slot of declared type `ty` (T7.1's value
    /// semantics and implicit conversions):
    ///
    /// - a custom-numeric value copies wherever it is bound — a typed
    ///   or type-inferring slot, a field, a method parameter — so the
    ///   source and the bound value never share storage
    ///   (`numeric_assign` when the struct defines it, a deep field
    ///   copy otherwise);
    /// - a plain int/real builds into a custom-numeric slot through
    ///   the `numeric_from_int`/`numeric_from_real` pair
    ///   (`var a: fx_t = 3`);
    /// - a custom value of a *different* numeric type is the honest
    ///   `E0402` (an `as_a` conversion is the spelled route);
    /// - everything else passes through unchanged.
    fn bind_slot_value(
        &mut self,
        value: Value,
        ty: Option<&TypeExpr>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        if let Some(TypeExpr::Named { name, .. }) = ty
            && self.is_custom_numeric_type(name)
        {
            match &value {
                Value::Custom { type_name, .. } if type_name != name => {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "cannot assign a '{type_name}' value to a '{name}' slot (use as_a('{name}'))"
                        ),
                        span,
                    ));
                }
                Value::Custom { .. } => return self.copy_custom_numeric(&value, span),
                Value::Int(_) | Value::Real(_) => {
                    return self.convert_scalar_to_custom(&value, name, span);
                }
                _ => {}
            }
        }
        if matches!(value, Value::Custom { .. }) {
            return self.copy_custom_numeric(&value, span);
        }
        Ok(value)
    }

    /// The source symbol of a binary operator, for the `E0418` message.
    fn operator_symbol(op: BinaryOp) -> &'static str {
        match op {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::Eq | BinaryOp::IdentityEq => "==",
            BinaryOp::NotEq | BinaryOp::IdentityNe => "!=",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitOr => "|",
            BinaryOp::BitXor => "^",
            BinaryOp::ShiftLeft => "<<",
            BinaryOp::ShiftRight => ">>",
            _ => "operator",
        }
    }

    /// The `other` operand of a custom-numeric binary operation: a
    /// second custom value of the same type shares its instance; a
    /// plain scalar converts into the custom type first (T7.3 —
    /// `numeric_from_int`/`numeric_from_real`).
    fn custom_operand_instance(
        &mut self,
        other: Value,
        type_name: &str,
        op: BinaryOp,
        span: crate::ast::Span,
    ) -> Result<usize, Diagnostic> {
        match other {
            Value::Custom {
                type_name: other_type,
                instance,
            } if other_type == type_name => Ok(instance),
            Value::Custom {
                type_name: other_type,
                ..
            } => Err(self.error(
                "E0402",
                format!(
                    "operator '{}' between numeric types '{type_name}' and '{other_type}' is not supported (use as_a)",
                    Self::operator_symbol(op)
                ),
                span,
            )),
            scalar @ (Value::Int(_) | Value::Real(_)) => {
                match self.convert_scalar_to_custom(&scalar, type_name, span)? {
                    Value::Custom { instance, .. } => Ok(instance),
                    _ => unreachable!("convert_scalar_to_custom returns a custom value"),
                }
            }
            other => Err(self.error(
                "E0402",
                format!(
                    "invalid operand ({}) for '{}' on a custom numeric value",
                    self.value_kind(&other),
                    Self::operator_symbol(op)
                ),
                span,
            )),
        }
    }

    /// Operators and comparisons on custom numeric values (5.2.15, 5.7,
    /// clause 31 — T7.3): the arithmetic shapes call the struct's
    /// `numeric_add/subtract/multiply/divide/modulo(other, res)` with a
    /// fresh result instance; comparisons call `numeric_compare(other)`
    /// (the -1/0/1 result reads against 0); the bitwise shapes call
    /// `numeric_bit_and/or/xor(other, res)` and
    /// `numeric_shift_left/right(n, res)`. A plain scalar operand
    /// converts into the custom type first. A method the struct does
    /// not define is the runtime `E0418`.
    fn apply_custom_binary(
        &mut self,
        op: BinaryOp,
        lhs: Value,
        rhs: Value,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        use BinaryOp::*;
        let lhs_is_custom = matches!(lhs, Value::Custom { .. });
        let (type_name, instance, other_value) = if lhs_is_custom {
            let Value::Custom {
                type_name,
                instance,
            } = lhs
            else {
                unreachable!("lhs_is_custom checked above")
            };
            (type_name, instance, rhs)
        } else {
            let Value::Custom {
                type_name,
                instance,
            } = rhs
            else {
                unreachable!("apply_custom_binary dispatched on a custom operand")
            };
            (type_name, instance, lhs)
        };
        // Comparisons dispatch through numeric_compare(other): int.
        if let Lt | Le | Gt | Ge | Eq | NotEq = op {
            let other_instance = self.custom_operand_instance(other_value, &type_name, op, span)?;
            let ordering = self
                .call_numeric_method(
                    instance,
                    "numeric_compare",
                    vec![Value::Object(other_instance)],
                    span,
                )?
                .ok_or_else(|| {
                    self.numeric_method_missing(
                        &type_name,
                        &format!("operation '{}'", Self::operator_symbol(op)),
                        "numeric_compare",
                        span,
                    )
                })?;
            let Value::Int(ordering) = ordering else {
                return Err(self.error("E0402", "numeric_compare must return an int", span));
            };
            let result = match op {
                Lt => ordering < 0,
                Le => ordering <= 0,
                Gt => ordering > 0,
                Ge => ordering >= 0,
                Eq => ordering == 0,
                _ => ordering != 0,
            };
            return Ok(Value::Bool(result));
        }
        // The (other, res) shapes: a fresh instance carries the result.
        let method = match op {
            Add => "numeric_add",
            Sub => "numeric_subtract",
            Mul => "numeric_multiply",
            Div => "numeric_divide",
            Mod => "numeric_modulo",
            BitAnd => "numeric_bit_and",
            BitOr => "numeric_bit_or",
            BitXor => "numeric_bit_xor",
            ShiftLeft | ShiftRight => {
                return self.apply_custom_shift(op, instance, other_value, &type_name, span);
            }
            // Native string matching and the list-pointer identity
            // operators have no numeric_i method.
            _ => {
                return Err(self.error(
                    "E0402",
                    format!(
                        "operator '{}' is not supported on a custom numeric value",
                        Self::operator_symbol(op)
                    ),
                    span,
                ));
            }
        };
        let other_instance = self.custom_operand_instance(other_value, &type_name, op, span)?;
        let res = self.custom_result_instance(&type_name, span)?;
        if self
            .call_numeric_method(
                instance,
                method,
                vec![Value::Object(other_instance), Value::Object(res)],
                span,
            )?
            .is_none()
        {
            return Err(self.numeric_method_missing(
                &type_name,
                &format!("operation '{}'", Self::operator_symbol(op)),
                method,
                span,
            ));
        }
        Ok(Value::Custom {
            type_name,
            instance: res,
        })
    }

    /// The shift shapes: the right operand is the integer shift amount
    /// (`numeric_shift_left/right(n: int, res)` — clause 31.4), never
    /// converted to the custom type.
    fn apply_custom_shift(
        &mut self,
        op: BinaryOp,
        instance: usize,
        other_value: Value,
        type_name: &str,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let Value::Int(n) = other_value else {
            return Err(self.error("E0402", "shift amount must be an integer", span));
        };
        let method = if op == BinaryOp::ShiftLeft {
            "numeric_shift_left"
        } else {
            "numeric_shift_right"
        };
        let res = self.custom_result_instance(type_name, span)?;
        if self
            .call_numeric_method(
                instance,
                method,
                vec![Value::Int(n), Value::Object(res)],
                span,
            )?
            .is_none()
        {
            return Err(self.numeric_method_missing(
                type_name,
                &format!("operation '{}'", Self::operator_symbol(op)),
                method,
                span,
            ));
        }
        Ok(Value::Custom {
            type_name: type_name.to_owned(),
            instance: res,
        })
    }

    /// Unary `numeric_unary_minus/plus/bit_not(res)` on a custom
    /// numeric value (clause 31.4, T7.3): the method writes into a
    /// fresh result instance.
    fn custom_unary(
        &mut self,
        op: UnaryOp,
        type_name: &str,
        instance: usize,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let (method, symbol) = match op {
            UnaryOp::Negate => ("numeric_unary_minus", "-"),
            UnaryOp::Plus => ("numeric_unary_plus", "+"),
            _ => ("numeric_bit_not", "~"),
        };
        let res = self.custom_result_instance(type_name, span)?;
        if self
            .call_numeric_method(instance, method, vec![Value::Object(res)], span)?
            .is_none()
        {
            return Err(self.numeric_method_missing(
                type_name,
                &format!("operation unary '{symbol}'"),
                method,
                span,
            ));
        }
        Ok(Value::Custom {
            type_name: type_name.to_owned(),
            instance: res,
        })
    }

    /// `custom_value.as_a(target)` (5.10.2, T7.3): a scalar target
    /// calls the matching `numeric_to_*` conversion of the clause 31
    /// pair (`as_a(int)` is `numeric_to_int()`); another numeric type
    /// writes this value into a fresh instance of the target's struct
    /// via `numeric_to_numeric(other)`; the same type is a by-value
    /// copy. A struct without the conversion reports `E0418`.
    fn custom_numeric_as_a(
        &mut self,
        value: Value,
        ty: &TypeExpr,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let Value::Custom {
            type_name,
            instance,
        } = value
        else {
            unreachable!("custom_numeric_as_a dispatched on a custom value")
        };
        match ty {
            TypeExpr::Named { name, .. } if *name == type_name => self.copy_custom_numeric(
                &Value::Custom {
                    type_name,
                    instance,
                },
                span,
            ),
            TypeExpr::Named { name, .. } if self.is_custom_numeric_type(name) => {
                let target = self.custom_result_instance(name, span)?;
                if self
                    .call_numeric_method(
                        instance,
                        "numeric_to_numeric",
                        vec![Value::Object(target)],
                        span,
                    )?
                    .is_none()
                {
                    return Err(self.numeric_method_missing(
                        &type_name,
                        &format!("conversion to '{name}'"),
                        "numeric_to_numeric",
                        span,
                    ));
                }
                Ok(Value::Custom {
                    type_name: name.clone(),
                    instance: target,
                })
            }
            TypeExpr::Scalar { base, width, .. } => {
                let (method, wrap) = match base {
                    ScalarBase::Int if is_star_width(width) => ("numeric_to_unbounded", None),
                    ScalarBase::Int => ("numeric_to_int", Some((width_bits(width, 32), true))),
                    ScalarBase::Uint => ("numeric_to_uint", Some((width_bits(width, 32), false))),
                    ScalarBase::Longint => ("numeric_to_longint", Some((64, true))),
                    ScalarBase::Longuint => ("numeric_to_longuint", Some((64, false))),
                    ScalarBase::Real => ("numeric_to_real", None),
                    ScalarBase::String => ("numeric_to_string", None),
                    _ => {
                        return Err(self.error(
                            "E0402",
                            format!("cannot cast a '{}' value to {}", type_name, base.name()),
                            span,
                        ));
                    }
                };
                let converted = if method == "numeric_to_string" {
                    // The string conversion is numeric_to_string's
                    // radix query; the plain `as_a(string)` reading is
                    // the decimal radix.
                    self.call_numeric_method(
                        instance,
                        method,
                        vec![Self::numeric_radix_dec()],
                        span,
                    )?
                } else {
                    self.call_numeric_method(instance, method, Vec::new(), span)?
                };
                let Some(converted) = converted else {
                    return Err(self.numeric_method_missing(
                        &type_name,
                        &format!("conversion to '{}'", base.name()),
                        method,
                        span,
                    ));
                };
                match (converted, wrap) {
                    (Value::Int(v), Some((bits, signed))) => {
                        Ok(Value::Int(wrap_to_width(v, bits, signed)))
                    }
                    (value, _) => Ok(value),
                }
            }
            _ => Err(self.error(
                "E0402",
                format!("cannot cast a '{type_name}' value to this type"),
                span,
            )),
        }
    }

    /// Format one value for the message surfaces (`out`, `print`,
    /// `append`, `dut_error`, `message`): a custom numeric value
    /// renders through its struct's `numeric_to_string(dec)` (T7.3 —
    /// `dec` is the default radix); a struct without the method is the
    /// runtime `E0418`.
    fn format_value_checked(
        &mut self,
        value: &Value,
        span: crate::ast::Span,
    ) -> Result<String, Diagnostic> {
        if let Value::Custom {
            type_name,
            instance,
        } = value
        {
            let rendered = self
                .call_numeric_method(
                    *instance,
                    "numeric_to_string",
                    vec![Self::numeric_radix_dec()],
                    span,
                )?
                .ok_or_else(|| {
                    self.numeric_method_missing(type_name, "output", "numeric_to_string", span)
                })?;
            let Value::Str(text) = rendered else {
                return Err(self.error("E0402", "numeric_to_string must return a string", span));
            };
            return Ok(text);
        }
        Ok(self.format_value(value))
    }

    /// The checked form of the message paths' formatting loop.
    fn format_values_checked(
        &mut self,
        values: &[Value],
        span: crate::ast::Span,
    ) -> Result<String, Diagnostic> {
        let mut text = String::new();
        for value in values {
            text.push_str(&self.format_value_checked(value, span)?);
        }
        Ok(text)
    }

    /// Evaluate a per-item expression with a temporary `it` binding.
    /// Restore both the value and its type metadata, including on error.
    fn eval_list_item(
        &mut self,
        expr: &Expr,
        item: &Value,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Value, Diagnostic> {
        let previous = frame.locals.insert("it".to_owned(), item.clone());
        let previous_type = frame.local_types.remove("it");
        let result = self.eval(expr, frame, me);
        match previous {
            Some(value) => {
                frame.locals.insert("it".to_owned(), value);
            }
            None => {
                frame.locals.remove("it");
            }
        }
        if let Some(ty) = previous_type {
            frame.local_types.insert("it".to_owned(), ty);
        } else {
            frame.local_types.remove("it");
        }
        result
    }

    /// Keyed list extrema (27.4.15/16/18/19). Keys choose an item;
    /// unlike max_value/min_value the result is not the key itself.
    fn call_list_extreme(
        &mut self,
        list: &ListValue,
        name: &str,
        args: &[Expr],
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        if args.len() > 1 {
            return Err(self.error(
                "E0402",
                format!("'{name}' expects at most one item expression"),
                span,
            ));
        }
        let items = list.items.borrow().clone();
        let Some(expr) = args.first() else {
            return self.list_extreme(&items, name, span);
        };
        if items.is_empty() {
            return Err(self.error("E0402", format!("{name} of an empty list"), span));
        }
        let mut keys = Vec::with_capacity(items.len());
        for item in &items {
            let key = self.eval_list_item(expr, item, frame, me)?;
            // Validate singleton keys too; comparing only subsequent
            // keys would silently accept a single object or boolean.
            if compare_values(&key, &key).is_none() {
                return Err(self.error(
                    "E0402",
                    format!("{name}() key is not orderable"),
                    expr.span(),
                ));
            }
            keys.push(key);
        }
        let best = self.list_extreme_index(&keys, name, span)?;
        Ok(if name.ends_with("_index") {
            Value::Int(best as i128)
        } else {
            items[best].clone()
        })
    }

    /// Predicate list queries (27.4.4/10/29/30), with receiver snapshot.
    fn call_list_query(
        &mut self,
        list: &ListValue,
        name: &str,
        args: &[Expr],
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let [predicate] = args else {
            return Err(self.error(
                "E0402",
                format!("'{name}' expects one boolean item expression"),
                span,
            ));
        };
        let items = list.items.borrow().clone();
        let mut matches = Vec::new();
        let mut count = 0;
        for (index, item) in items.into_iter().enumerate() {
            let Value::Bool(selected) = self.eval_list_item(predicate, &item, frame, me)? else {
                return Err(self.error(
                    "E0402",
                    format!("{name}() expects a boolean item expression"),
                    predicate.span(),
                ));
            };
            if selected {
                if name == "has" {
                    return Ok(Value::Bool(true));
                }
                count += 1;
                match name {
                    "all" => matches.push(item),
                    "all_indices" => matches.push(Value::Int(index as i128)),
                    _ => {}
                }
            }
        }
        Ok(match name {
            "has" => Value::Bool(false),
            "count" => Value::Int(count),
            _ => Value::List(ListValue {
                items: Rc::new(RefCell::new(matches)),
                element: if name == "all_indices" {
                    Some("uint".to_owned())
                } else {
                    list.element.clone()
                },
            }),
        })
    }

    /// The 27.4.17/27.4.20/27.5.4-27.5.6 aggregate pseudo-methods with
    /// a per-item expression: `l.sum(it)`, `l.product(it * it)`,
    /// `l.average(it)`, `l.max_value(it)`, `l.min_value(it)` — and
    /// their no-argument forms over the items themselves. The per-item
    /// expression evaluates with `it` bound to the item. Numeric items
    /// only; the result carries the item expression's type (Int over
    /// integers, Real over reals; `average` of integers truncates —
    /// integer division keeps the int type). Empty list: `sum` is 0
    /// and `product` is 1 (the prose's stated empty results);
    /// `average` reports `E0402` — the standard's UNDEF result has no
    /// value-model representation here; `max_value`/`min_value`
    /// likewise report `E0402` (Table 54's element-type-keyed empty
    /// results are not modeled).
    fn call_list_aggregate(
        &mut self,
        items: &[Value],
        name: &str,
        args: &[Expr],
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        if args.len() > 1 {
            return Err(self.error(
                "E0402",
                format!("'{name}' expects at most one item expression"),
                span,
            ));
        }
        let mut int_result: i128 = match name {
            "product" => 1,
            _ => 0,
        };
        let mut real_result = 0.0f64;
        let mut is_real = false;
        let mut count = 0usize;
        // The extreme trackers (max_value/min_value) start at nothing.
        let mut extreme: Option<Value> = None;
        for item in items {
            let value = if args.is_empty() {
                item.clone()
            } else {
                self.eval_list_item(&args[0], item, frame, me)?
            };
            count += 1;
            if matches!(name, "max_value" | "min_value") {
                let takes_over = match &extreme {
                    None => true,
                    Some(best) => {
                        let ordering = compare_values(&value, best).ok_or_else(|| {
                            self.error("E0402", format!("{name}() items are not comparable"), span)
                        })?;
                        use std::cmp::Ordering::*;
                        if name == "max_value" {
                            ordering != Less
                        } else {
                            ordering != Greater
                        }
                    }
                };
                if takes_over {
                    extreme = Some(value);
                }
                continue;
            }
            match value {
                Value::Int(v) => {
                    if is_real {
                        return Err(self.error(
                            "E0402",
                            format!("{name}() items must all be of one numeric type (reals seen, integer found)"),
                            span,
                        ));
                    }
                    match name {
                        "sum" | "average" => int_result = int_result.wrapping_add(v),
                        "product" => int_result = int_result.wrapping_mul(v),
                        _ => {}
                    }
                }
                Value::Real(v) => {
                    if !is_real && count > 1 {
                        return Err(self.error(
                            "E0402",
                            format!("{name}() items must all be of one numeric type (integers seen, real found)"),
                            span,
                        ));
                    }
                    is_real = true;
                    match name {
                        "sum" | "average" => real_result += v,
                        "product" => real_result *= v,
                        _ => {}
                    }
                }
                _ => {
                    return Err(self.error(
                        "E0402",
                        format!("{name}() expects a numeric item expression"),
                        span,
                    ));
                }
            }
        }
        match name {
            "sum" => Ok(if is_real {
                Value::Real(real_result)
            } else {
                Value::Int(int_result)
            }),
            "product" => Ok(if is_real {
                Value::Real(real_result)
            } else {
                Value::Int(int_result)
            }),
            "average" => {
                if count == 0 {
                    return Err(self.error(
                        "E0402",
                        "average of an empty list (the standard's UNDEF result is not modeled)",
                        span,
                    ));
                }
                Ok(if is_real {
                    Value::Real(real_result / count as f64)
                } else {
                    Value::Int(int_result / count as i128)
                })
            }
            "max_value" | "min_value" => match extreme {
                Some(extreme) => Ok(extreme),
                None => Err(self.error(
                    "E0402",
                    format!("{name} of an empty list (the Table 54 empty results are not modeled)"),
                    span,
                )),
            },
            _ => Err(self.error("E0402", format!("unknown aggregate '{name}'"), span)),
        }
    }

    /// A set literal's value (5.2.12): the canonical interval set of
    /// its items. Enumeral items contribute their indices — the value
    /// model keeps intervals only (the `for each` path re-derives the
    /// enumerands from the literal's bounds, see
    /// [`Interp::set_literal_iteration_items`]).
    fn eval_set_literal(
        &mut self,
        items: &crate::ast::InSetExpr,
        span: crate::ast::Span,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Value, Diagnostic> {
        let mut intervals: Vec<(i128, i128)> = Vec::new();
        match items {
            crate::ast::InSetExpr::Items(exprs) => {
                for expr in exprs {
                    let value = self.eval(expr, frame, me)?;
                    let as_int = match &value {
                        Value::Int(v) => Some(*v),
                        Value::Bool(b) => Some(*b as i128),
                        Value::Enum { type_name, name } => self.enumeral_index(type_name, name),
                        _ => {
                            return Err(self.error(
                                "E0402",
                                "set literal items must be integers, booleans or enumerals",
                                span,
                            ));
                        }
                    };
                    match as_int {
                        Some(v) => intervals.push((v, v)),
                        None => {
                            return Err(self.error(
                                "E0402",
                                "set literal item is not an enumerand of a declared type",
                                span,
                            ));
                        }
                    }
                }
            }
            crate::ast::InSetExpr::Range(low, high) => {
                let low = self.set_literal_bound(low, frame, me, span)?;
                let high = self.set_literal_bound(high, frame, me, span)?;
                intervals.push((low, high));
            }
            // A general set expression (clause 4.11.6's set-exp) records
            // no literal bounds: it evaluates to a set value whose
            // canonical intervals are the result.
            crate::ast::InSetExpr::Expr(expr) => {
                let value = self.eval(expr, frame, me)?;
                let Value::Set(evaluated) = value else {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "set expression does not evaluate to a set, got {}",
                            self.value_kind(&value)
                        ),
                        span,
                    ));
                };
                intervals = evaluated;
            }
        }
        Ok(Value::Set(canonical_set(intervals)))
    }

    /// One set-literal bound: an integer, boolean or enumerand index.
    fn set_literal_bound(
        &mut self,
        expr: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<i128, Diagnostic> {
        let value = self.eval(expr, frame, me)?;
        match &value {
            Value::Int(v) => Ok(*v),
            Value::Bool(b) => Ok(*b as i128),
            Value::Enum { type_name, name } => {
                self.enumeral_index(type_name, name).ok_or_else(|| {
                    self.error(
                        "E0402",
                        format!("'{name}' is not an enumerand of '{type_name}'"),
                        span,
                    )
                })
            }
            _ => Err(self.error(
                "E0402",
                "set literal bounds must be integers, booleans or enumerals",
                span,
            )),
        }
    }

    /// The `for each in <set literal>` iteration items (20.2.5): the
    /// values in order, with enumeral bounds iterating the enumerands
    /// of their type (`for each (c) in [red..blue]` runs red, green,
    /// blue) and integer bounds the integers.
    fn set_literal_iteration_items(
        &mut self,
        items: &crate::ast::InSetExpr,
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<Vec<Value>, Diagnostic> {
        match items {
            crate::ast::InSetExpr::Items(exprs) => {
                let mut values = Vec::with_capacity(exprs.len());
                for expr in exprs {
                    values.push(self.eval(expr, frame, me)?);
                }
                Ok(values)
            }
            crate::ast::InSetExpr::Range(low, high) => {
                let low_value = self.eval(low, frame, me)?;
                let high_value = self.eval(high, frame, me)?;
                if let (
                    Value::Enum {
                        type_name: low_type,
                        name: low_name,
                    },
                    Value::Enum {
                        type_name: high_type,
                        name: high_name,
                    },
                ) = (&low_value, &high_value)
                {
                    if low_type == high_type
                        && let (Some(low_index), Some(high_index)) = (
                            self.enumeral_index(low_type, low_name),
                            self.enumeral_index(high_type, high_name),
                        )
                        && low_index <= high_index
                    {
                        let enumerals: Vec<String> = self
                            .enumeral_values(low_type)
                            .into_iter()
                            .skip(low_index as usize)
                            .take((high_index - low_index + 1) as usize)
                            .collect();
                        return Ok(enumerals
                            .into_iter()
                            .map(|name| Value::Enum {
                                type_name: low_type.clone(),
                                name,
                            })
                            .collect());
                    }
                    return Err(self.error(
                        "E0402",
                        "set range bounds must be enumerals of one type or integers",
                        span,
                    ));
                }
                let (Value::Int(low), Value::Int(high)) = (low_value, high_value) else {
                    return Err(self.error(
                        "E0402",
                        "set range bounds must be enumerals of one type or integers",
                        span,
                    ));
                };
                if low > high || high - low > 1_000_000 {
                    return Err(self.error(
                        "E0402",
                        "set range is empty or unreasonably large",
                        span,
                    ));
                }
                Ok((low..=high).map(Value::Int).collect())
            }
            // A general set expression: its set value's elements in order
            // (intervals expanded; the iteration cap of the range arm
            // does not apply — the set is already materialized).
            crate::ast::InSetExpr::Expr(expr) => {
                let value = self.eval(expr, frame, me)?;
                let Value::Set(intervals) = value else {
                    return Err(self.error(
                        "E0402",
                        "for-each expects a set expression to evaluate to a set",
                        span,
                    ));
                };
                Ok(intervals
                    .iter()
                    .flat_map(|(lo, hi)| (*lo..=*hi).map(Value::Int))
                    .collect())
            }
        }
    }

    /// The index of an enumerand within its declared type, by
    /// declaration order (the set model's enumerand numbering).
    fn enumeral_index(&self, type_name: &str, name: &str) -> Option<i128> {
        let types: &[crate::elab::ElaboratedType] = self.types;
        types
            .iter()
            .find(|t| t.name == type_name && t.kind == DeclarationKind::Type)?
            .enumerals
            .iter()
            .position(|enumeral| enumeral.name == name)
            .map(|position| position as i128)
    }

    /// The enumerand names of a declared type, in declaration order.
    fn enumeral_values(&self, type_name: &str) -> Vec<String> {
        let types: &[crate::elab::ElaboratedType] = self.types;
        types
            .iter()
            .find(|t| t.name == type_name && t.kind == DeclarationKind::Type)
            .map(|ty| ty.enumerals.iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default()
    }

    /// `outf("format %x", args)` — printf-style output; specs `%x`/`%X`
    /// (hex), `%d` (decimal) and `%s` (value as text) consume arguments in
    /// order; `%%` is a literal percent.
    fn call_outf(
        &mut self,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let Some((format, arguments)) = values.split_first() else {
            return Err(self.error(
                "E0402",
                "'outf' expects a format string and arguments",
                span,
            ));
        };
        let Value::Str(format) = format else {
            return Err(self.error("E0402", "'outf' expects a format string first", span));
        };
        let mut argument_index = 0usize;
        let mut rendered = String::new();
        let mut chars = format.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '%' {
                rendered.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => rendered.push('%'),
                Some(spec) => {
                    let Some(value) = arguments.get(argument_index) else {
                        return Err(self.error(
                            "E0402",
                            format!("'outf': no argument for '%{spec}'"),
                            span,
                        ));
                    };
                    argument_index += 1;
                    rendered.push_str(&match (spec, value) {
                        ('x', Value::Int(i)) => format!("{i:x}"),
                        ('X', Value::Int(i)) => format!("{i:X}"),
                        ('d', Value::Int(i)) => format!("{i}"),
                        // A custom numeric argument renders through
                        // numeric_to_string(dec) (T7.3).
                        (_, value) => self.format_value_checked(value, span)?,
                    });
                }
                None => rendered.push('%'),
            }
        }
        self.output.push_str(&rendered);
        Ok(Value::Void)
    }

    /// Resolve an `emit`/event target expression to its path key.
    fn event_path(&mut self, target: &Expr) -> Result<String, Diagnostic> {
        match target {
            Expr::Ident { name, .. } => Ok(name.clone()),
            Expr::MemberAccess { object, name, .. } => {
                let mut prefix = self.event_path(object)?;
                prefix.push('.');
                prefix.push_str(name);
                Ok(prefix)
            }
            // `Type::event` — a static event's log path is its
            // type-qualified spelling (`bus::reset`), the form TCM
            // sampling events (`f() @bus::reset`) park on.
            Expr::StaticAccess {
                type_name, member, ..
            } => Ok(format!("{type_name}::{member}")),
            _ => Err(self.error("E0402", "unsupported event target", target.span())),
        }
    }

    /// The temporal log has one state per cycle up to the current one;
    /// cycles where nothing was emitted carry empty states.
    fn pad_log(&mut self) {
        while self.temporal_log.len() <= self.cycle as usize {
            self.temporal_log.push(crate::temporal::State::default());
        }
    }

    /// Fire one event occurrence — the shared path of `emit` (11.2) and
    /// the simulator-side triggers: record it in the current cycle's
    /// state of the temporal log (clause 12), wake TCMs parked for the
    /// event (13.2.2's implicit entry sync — 17.1.2), execute the
    /// event's `on` action blocks immediately and in composition order
    /// (11.2 — an on action block runs before TCMs waiting for the same
    /// event), and sample the coverage groups (14.2).
    fn fire_event(
        &mut self,
        path: &str,
        owner: Option<usize>,
        span: crate::ast::Span,
        frame: &mut Frame,
    ) -> Result<(), Diagnostic> {
        self.emitted.insert(path.to_owned());
        self.pad_log();
        self.temporal_log[self.cycle as usize].record(path);
        self.wake_tcm_listeners(path);
        self.run_on_blocks(path, owner, span)?;
        self.sample_cover_groups(path, owner, frame);
        Ok(())
    }

    /// Wake every coroutine parked for `path`'s occurrence: a TCM
    /// declared `@path` implicitly synchronizes on its sampling event
    /// upon entry (17.1.2), so it commences when the event occurs
    /// (17.2.1). The woken TCM becomes eligible for the current cycle —
    /// it still only runs once the current turn yields, since a started
    /// TCM is scheduled only after its starter has been suspended
    /// (17.2.2).
    fn wake_tcm_listeners(&mut self, path: &str) {
        let cycle = self.cycle;
        for co in &mut self.coroutines {
            if co.wake_event.as_deref() == Some(path) {
                co.wake_event = None;
                co.eligible_at_cycle = cycle;
            }
        }
    }

    /// Whether `path` occurred in the current cycle's state of the
    /// temporal log (17.2.1's "already been emitted for the current
    /// tick").
    fn event_occurred_this_cycle(&self, path: &str) -> bool {
        self.temporal_log
            .get(self.cycle as usize)
            .is_some_and(|state| {
                state.occurs(&path.split('.').map(str::to_owned).collect::<Vec<_>>())
            })
    }

    /// The simulator-side sampling point (11.1.4.2): every virtual
    /// cycle, sample each `rise/fall/change(HDL expression) @sim` event
    /// trigger and each bound event port's HDL object, and fire the ones
    /// whose monitored value crossed their edge. A fire uses the same
    /// machinery as `emit` — the temporal log, the `on` blocks, coverage,
    /// and the wake of TCMs parked on the event.
    ///
    /// Approximations, documented: without an HDL source there is nothing
    /// to sample (the events never fire — HDL signals without a
    /// simulator); the first sample only establishes the baseline (no X
    /// initial state, so no edge on it); values are plain integers (the
    /// MVL transitions of 9.7.3 are not modeled — `any_change`
    /// degrades to `change`); sampling happens at the scheduler's cycle
    /// boundaries only (the inline spins of atomic contexts do not
    /// advance the HDL, the existing v1 limitation); a firing's on-block
    /// errors surface in the output like a TCM's, since no turn owns the
    /// firing.
    fn sample_sim_events(&mut self) {
        if self.hdl.is_none() {
            return;
        }
        let cycle = self.cycle;
        let triggers = self.sim_triggers.clone();
        for trigger in &triggers {
            // Every instance of the declaring type owns the event
            // (11.1.2), and each samples the monitored expression in its
            // own context (a port read resolves through that instance's
            // binding).
            let owners: Vec<usize> = self
                .instances
                .iter()
                .enumerate()
                .filter(|(_, instance)| instance.type_name == trigger.type_name)
                .map(|(index, _)| index)
                .collect();
            for owner in owners {
                let sampled = self
                    .eval(&trigger.condition, &mut Frame::default(), Some(owner))
                    .ok()
                    .and_then(|value| match value {
                        Value::Int(value) => Some(value),
                        Value::Bool(value) => Some(i128::from(value)),
                        _ => None,
                    });
                // An unsampleable expression never produces an edge.
                let Some(value) = sampled else {
                    continue;
                };
                let key = (owner, trigger.event_name.clone());
                let previous = self.sim_prev.insert(key, Some(value)).flatten();
                if edge_fires(&trigger.routine, previous, value)
                    && let Err(diagnostic) = self.fire_event(
                        &trigger.event_name,
                        Some(owner),
                        trigger.span,
                        &mut Frame::default(),
                    )
                {
                    self.output.push_str(&format!(
                        "@sim event {}: {}\n",
                        trigger.event_name,
                        diagnostic.render()
                    ));
                }
            }
        }
        // Bound event ports (9.5): an external event port's `hdl_path()`
        // attribute names the simulated object it is connected to, and
        // its `edge()` attribute (9.7.3, default `change`) specifies
        // the edge on which the port's event is generated. The port's
        // name already carries the `$` access operator (the lexer folds
        // `p$` into one identifier), which is the event path `emit p$`
        // and `on p$` use.
        let port_samples = self.collect_bound_event_ports();
        for (owner, port_name, path, edge) in port_samples {
            let value = match self.hdl.as_mut().unwrap().get(&path, cycle) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let key = (owner, port_name.clone());
            let previous = self.sim_prev.insert(key, Some(value)).flatten();
            if edge_fires(&edge, previous, value)
                && let Err(diagnostic) = self.fire_event(
                    &port_name,
                    Some(owner),
                    origin_span(),
                    &mut Frame::default(),
                )
            {
                self.output.push_str(&format!(
                    "event port {port_name}: {}\n",
                    diagnostic.render()
                ));
            }
        }
    }

    /// The (instance, port, bound HDL path, edge) tuples of every event
    /// port with an `hdl_path()` binding — the sampling worklist of
    /// [`Interp::sample_sim_events`]. The edge attribute comes from the
    /// type's `keep <port>.edge() == <value>` constraint (9.7.3) and
    /// defaults to `change`.
    fn collect_bound_event_ports(&self) -> Vec<(usize, String, String, String)> {
        let types: &[crate::elab::ElaboratedType] = self.types;
        let mut samples = Vec::new();
        for (index, instance) in self.instances.iter().enumerate() {
            let Some(declared) = types.iter().find(|t| t.name == instance.type_name) else {
                continue;
            };
            for (name, path) in &instance.ports {
                let Some(info) = declared.ports.iter().find(|p| &p.name == name) else {
                    continue;
                };
                if !info.is_event || path.is_empty() {
                    continue;
                }
                let edge = self
                    .port_edges
                    .get(&(instance.type_name.clone(), name.clone()))
                    .cloned()
                    .unwrap_or_else(|| "change".to_owned());
                samples.push((index, name.clone(), path.clone(), edge));
            }
        }
        samples
    }

    /// Record the child the walk descends into on the context stack's
    /// top entry (the enclosing block walk pushed the statement's
    /// level; the compound marks its descent so a suspension inside
    /// builds the matching resume path).
    fn set_ctx_child(&mut self, child: Child) {
        if let Some(level) = self.exec_context.last_mut() {
            level.child = Some(child);
        }
    }

    /// Sample the coverage groups (14.2) of the instance whose event
    /// fired: each item's recorded expression evaluates in the emitting
    /// frame, or the item's field is read from the owner, and the value
    /// lands in the item's buckets.
    fn sample_cover_groups(&mut self, path: &str, owner: Option<usize>, frame: &mut Frame) {
        let Some(owner) = owner else {
            return;
        };
        let type_name = self.instances[owner].type_name.clone();
        let Some(entry_index) = self
            .coverage
            .iter()
            .position(|entry| entry.type_name == type_name)
        else {
            return;
        };
        let group_indices: Vec<usize> = self.coverage[entry_index]
            .groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.event_path.as_deref() == Some(path))
            .map(|(index, _)| index)
            .collect();
        for group_index in group_indices {
            self.sample_group_on_instance(entry_index, group_index, owner, frame);
        }
    }

    /// One sampling event of one group on one instance: the group's
    /// sample count advances once, each item's recorded expression (or
    /// field) evaluates against the instance, the buckets record, and
    /// the cross items read the round's labels afterwards (14.4). The
    /// shared path of the emit-driven sampling (14.2) and the
    /// procedural `covers.sample_cg()` sampling (14.8.2.10).
    fn sample_group_on_instance(
        &mut self,
        entry_index: usize,
        group_index: usize,
        owner: usize,
        frame: &mut Frame,
    ) {
        let type_name = self.instances[owner].type_name.clone();
        // The items' recorded declarations (expression or field name),
        // looked up in the elaborated type's cover groups by name.
        let types: &[crate::elab::ElaboratedType] = self.types;
        let item_sources: Vec<(String, Option<Expr>)> = types
            .iter()
            .find(|t| t.name == type_name)
            .map(|ty| {
                ty.covers
                    .iter()
                    .flat_map(|decl| decl.items.iter())
                    .filter(|item| item.kind == crate::ast::CoverItemKind::Item)
                    .filter_map(|item| {
                        let name = item.names.first().map(|(name, _)| name.clone())?;
                        Some((name, item.expr.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.coverage[entry_index].groups[group_index].samples += 1;
        let item_names: Vec<String> = self.coverage[entry_index].groups[group_index]
            .items
            .iter()
            .map(|item| item.name.clone())
            .collect();
        for name in item_names {
            let Some((_, expr)) = item_sources
                .iter()
                .find(|(source_name, _)| *source_name == name)
            else {
                continue;
            };
            let value = match expr {
                Some(expr) => self.eval(expr, frame, Some(owner)).ok(),
                None => self.instances[owner].get(&name).cloned(),
            };
            if let Some(value) = value {
                self.coverage[entry_index].groups[group_index].sample(&name, &value);
            }
        }
        // The cross items record after all of their sources have
        // this round's labels (14.4).
        self.coverage[entry_index].groups[group_index].record_crosses();
    }

    /// Whether the running statement may suspend to the scheduler: a
    /// coroutine turn, not inside an atomic region (inline method
    /// calls, `compute`, `on` blocks, `check` else bodies — 13.2.2
    /// delimits atomic fragments at the synchronization actions, and
    /// these regions keep the conservative run-to-completion model of
    /// v1; `on` bodies may not contain time-consuming actions at all,
    /// 11.3). Every compound the walk descends into records its child
    /// on the context stack, so outside those regions the resume path
    /// reaches any statement.
    fn suspend_eligible(&self) -> bool {
        self.tracing && self.atomic_depth == 0
    }

    /// Raise a suspension for the scheduler (13.2.2): the turn's walk
    /// stack becomes the resume path. `skip_resume` resumes after this
    /// statement (a consumed numeric wait or bare sync — the first
    /// action following the synchronization action); otherwise the
    /// statement re-executes next turn (a temporal wait, re-evaluated
    /// against the grown log).
    fn suspend_here(
        &mut self,
        frame: &Frame,
        span: crate::ast::Span,
        skip_resume: bool,
        eligible_at_cycle: u64,
        temporal: bool,
        wait_start: Option<usize>,
    ) -> Result<Flow, Diagnostic> {
        let mut path = self.exec_context.clone();
        if skip_resume && let Some(last) = path.last_mut() {
            last.index += 1;
        }
        self.pending_suspension = Some(PendingSuspension {
            path,
            frame: frame.clone(),
            eligible_at_cycle,
            temporal,
            span,
            wait_start,
            wake_sync: self.pending_wake_sync.take(),
        });
        Ok(Flow::Suspend)
    }

    /// A temporal wait (13.2.4): in a coroutine turn, 13.2.2(b) —
    /// evaluate the TE once; on success resume immediately, otherwise
    /// suspend until the next sampling occurrence, when the statement
    /// re-executes against the grown log. Propositions and repeat
    /// bounds evaluate once per check, with current field values (the
    /// log keeps event occurrences, not value history). In atomic
    /// regions and non-scheduled contexts the wait spins inline,
    /// advancing one cycle at a time under the budget; a wait
    /// unsatisfied for TEMPORAL_WAIT_CYCLES cycles there reports
    /// `E0409` (the scheduler applies the same guard to suspends).
    fn temporal_wait(
        &mut self,
        te: &crate::ast::TemporalExpr,
        span: crate::ast::Span,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Flow, Diagnostic> {
        let start = self.resume_wait_start.take().unwrap_or(self.cycle as usize);
        self.pad_log();
        let (booleans, numerics) = self.evaluate_temporal_exprs(te, frame, me);
        if self.suspend_eligible() {
            let callbacks = temporal_callbacks(&booleans, &numerics);
            let slice = &self.temporal_log[start..];
            match crate::temporal::succeeds(te, slice, &callbacks) {
                Ok(true) => return Ok(Flow::Normal),
                Ok(false) => {
                    return self.suspend_here(
                        frame,
                        span,
                        false,
                        self.cycle + 1,
                        true,
                        Some(start),
                    );
                }
                Err(err) => {
                    return Err(self.error(
                        "E0402",
                        format!("temporal wait cannot be evaluated: {err:?}"),
                        span,
                    ));
                }
            }
        }
        let mut waited = 0u64;
        loop {
            let callbacks = temporal_callbacks(&booleans, &numerics);
            let slice = &self.temporal_log[start..];
            match crate::temporal::succeeds(te, slice, &callbacks) {
                Ok(true) => return Ok(Flow::Normal),
                Ok(false) => {}
                Err(err) => {
                    return Err(self.error(
                        "E0402",
                        format!("temporal wait cannot be evaluated: {err:?}"),
                        span,
                    ));
                }
            }
            if self.wait_budget == 0 || waited >= TEMPORAL_WAIT_CYCLES {
                return Err(self.error(
                    "E0409",
                    format!(
                        "cycle budget exceeded at cycle {} (temporal wait unsatisfied after {waited} cycles)",
                        self.cycle
                    ),
                    span,
                ));
            }
            self.wait_budget -= 1;
            waited += 1;
            self.cycle = self.cycle.saturating_add(1);
            self.pad_log();
        }
    }

    /// Build the set of all legal values of a type (5.10.5): scalars
    /// from their width and signedness, clause-5 `type` declarations
    /// from their underlying representation, enumeral types as
    /// `[0..count-1]`.
    fn set_of_type(&self, ty: &TypeExpr) -> Result<Value, Diagnostic> {
        let Some(range) = self.type_value_range(ty) else {
            return Err(self.error(
                "E0402",
                format!(
                    "set_of_values expects a numeric or enumeral type; '{}' has none",
                    ty.render()
                ),
                ty.span(),
            ));
        };
        Ok(Value::Set(vec![range]))
    }

    /// The inclusive value range of a type, when it has one.
    fn type_value_range(&self, ty: &TypeExpr) -> Option<(i128, i128)> {
        match ty {
            TypeExpr::Scalar { base, width, .. } => scalar_range(*base, width),
            TypeExpr::Named { name, .. } => {
                let types: &[crate::elab::ElaboratedType] = self.types;
                let declared = types.iter().find(|t| t.name == *name)?;
                if !declared.enumerals.is_empty() {
                    // An enumeral type's legal values are the
                    // enumerands' indices.
                    return Some((0, declared.enumerals.len() as i128 - 1));
                }
                declared
                    .underlying
                    .as_ref()
                    .and_then(|u| self.type_value_range(u))
            }
            TypeExpr::InlineEnum { names, .. } => {
                if names.is_empty() {
                    None
                } else {
                    Some((0, names.len() as i128 - 1))
                }
            }
            _ => None,
        }
    }

    /// The set pseudo-methods (28.5) on a set value: queries
    /// (`size`/`uint_size`/`min`/`uint_min`/`int_min`/`max`/`uint_max`/
    /// `int_max` and their `_is_*` guards), the algebra
    /// (`union`/`intersect`/`diff`), and `to_string`. `None` when the
    /// name is not a set pseudo-method (the caller reports its own
    /// error).
    fn call_set_pseudo_method(
        &mut self,
        intervals: &[(i128, i128)],
        name: &str,
        args: &[Value],
        span: crate::ast::Span,
    ) -> Option<Result<Value, Diagnostic>> {
        let set = Value::Set(intervals.to_vec());
        let result = match name {
            "size" | "uint_size" => {
                // 28.5.4/28.5.5: the number of elements in the set.
                let size: i128 = intervals.iter().map(|(lo, hi)| hi - lo + 1).sum();
                Ok(Value::Int(size))
            }
            "size_is_uint" => {
                let size: i128 = intervals.iter().map(|(lo, hi)| hi - lo + 1).sum();
                Ok(Value::Bool((0..=MAX_UINT_VALUE).contains(&size)))
            }
            "min" | "uint_min" | "int_min" => {
                // 28.5.7-28.5.10: the bottom boundary; empty is an
                // error (the prose's exception).
                match intervals.first() {
                    Some((lo, _)) => Ok(Value::Int(*lo)),
                    None => Err(self.error("E0402", "min of an empty set", span)),
                }
            }
            "max" | "uint_max" | "int_max" => match intervals.last() {
                Some((_, hi)) => Ok(Value::Int(*hi)),
                None => Err(self.error("E0402", "max of an empty set", span)),
            },
            "min_is_uint" => match intervals.first() {
                Some((lo, _)) => Ok(Value::Bool((0..=MAX_UINT_VALUE).contains(lo))),
                None => Err(self.error("E0402", "min of an empty set", span)),
            },
            "union" => {
                let Some(Value::Set(other)) = args.first() else {
                    return Some(Err(self.error(
                        "E0402",
                        "union expects a set argument",
                        span,
                    )));
                };
                Ok(union_sets(intervals, other))
            }
            "intersect" => {
                let Some(Value::Set(other)) = args.first() else {
                    return Some(Err(self.error(
                        "E0402",
                        "intersect expects a set argument",
                        span,
                    )));
                };
                Ok(intersect_sets(intervals, other))
            }
            "diff" => {
                let Some(Value::Set(other)) = args.first() else {
                    return Some(Err(self.error(
                        "E0402",
                        "diff expects a set argument",
                        span,
                    )));
                };
                Ok(diff_sets(intervals, other))
            }
            "to_string" => Ok(Value::Str(self.format_value(&set))),
            _ => return None,
        };
        Some(result)
    }

    /// The clause 27 list pseudo-methods, dispatched on a `Value::List`
    /// receiver. Pseudo-methods are macros, not struct methods (27.1) —
    /// they attach to any list expression. A modifying one mutates the
    /// receiver's shared list object in place through its handle
    /// ([`ListValue::items`]), so every alias of the object sees the
    /// change; `items` is a snapshot the caller took for the read-only
    /// arithmetic. `None` when the name is not one of this subset (the
    /// caller reports `E0405`).
    ///
    /// The methods taking a per-item expression (`apply`, `sort`, `all`,
    /// `has`, ...) cannot pass through this value-only dispatch — see the
    /// documented gaps in the module docs.
    fn call_list_pseudo_method(
        &mut self,
        list: &ListValue,
        items: &[Value],
        name: &str,
        args: &[Value],
        span: crate::ast::Span,
    ) -> Option<Result<Value, Diagnostic>> {
        let result = match (name, args) {
            // 27.4.22 size(): the number of items in the list.
            ("size", []) => Ok(Value::Int(items.len() as i128)),
            // 27.4.12 is_empty(): TRUE when the list has no items.
            ("is_empty", []) => Ok(Value::Bool(items.is_empty())),
            // 27.4.5 exists(): TRUE when an item with the given index
            // number exists in the list, FALSE when the index does not.
            ("exists", [Value::Int(index)]) => {
                Ok(Value::Bool(*index >= 0 && (*index as usize) < items.len()))
            }
            ("exists", _) => Err(self.error("E0402", "exists() expects an integer index", span)),
            // 27.4.3 copy(): a copy of the list — the list case of
            // exp.copy() (28.5.1). A fresh list object mints a new handle
            // (a `list_ptr==` mismatch, 4.11.4); struct items stay shared
            // by instance index.
            ("copy", []) => Ok(Value::List(ListValue {
                items: Rc::new(RefCell::new(items.to_vec())),
                element: list.element.clone(),
            })),
            // 27.4.21 reverse(): a new list of all the items in reverse
            // order (the receiver is unchanged).
            ("reverse", []) => {
                let mut reversed = items.to_vec();
                reversed.reverse();
                Ok(Value::List(ListValue {
                    items: Rc::new(RefCell::new(reversed)),
                    element: list.element.clone(),
                }))
            }
            // 27.4.26 top(): the last item, not removed; empty is an
            // error (the prose's exception).
            ("top", []) => match items.last() {
                Some(item) => Ok(item.clone()),
                None => Err(self.error("E0402", "top of an empty list", span)),
            },
            // 27.4.27 top0(): the first item, not removed; empty is an
            // error (the prose's exception; pairs with pop0 for queues).
            ("top0", []) => match items.first() {
                Some(item) => Ok(item.clone()),
                None => Err(self.error("E0402", "top0 of an empty list", span)),
            },
            // 27.3.10 pop(): removes and returns the last item; empty is
            // an error (the prose's exception).
            ("pop", []) => match items.split_last() {
                Some((last, rest)) => {
                    let popped = last.clone();
                    return Some(self.store_list(list, rest.to_vec(), popped));
                }
                None => Err(self.error("E0402", "pop of an empty list", span)),
            },
            // 27.3.11 pop0(): removes and returns the first item; empty
            // is an error (the prose's exception).
            ("pop0", []) => match items.split_first() {
                Some((first, rest)) => {
                    let popped = first.clone();
                    return Some(self.store_list(list, rest.to_vec(), popped));
                }
                None => Err(self.error("E0402", "pop0 of an empty list", span)),
            },
            // 27.3.1 add(item) / 27.3.12 push(item): the same function —
            // the item goes to the end of the list.
            // 27.3.2 add(list) / 27.3.14 push(list): the argument list's
            // items are appended. e declares no lists of lists (27.4.25),
            // so a list-valued argument is unambiguously the list form.
            ("push" | "add", [Value::List(added)]) => {
                // Read the argument's items before the mutating borrow:
                // the argument may be the receiver itself, and a
                // simultaneous mutable borrow of the same cell would
                // panic.
                let added_items = added.items.borrow().clone();
                list.items.borrow_mut().extend(added_items);
                Ok(Value::Void)
            }
            ("push" | "add", [item]) => {
                list.items.borrow_mut().push(item.clone());
                Ok(Value::Void)
            }
            // 27.3.3 add0(item) / 27.3.13 push0(item): the item becomes
            // the first position (index 0); later items reindex by +1.
            // 27.3.4 add0(list) / 27.3.15 push0(list): the argument list
            // is placed at the head (same list-form reading as above).
            ("push0" | "add0", [Value::List(added)]) => {
                let added_items = added.items.borrow().clone();
                let mut grown = added_items;
                grown.extend(items.iter().cloned());
                return Some(self.store_list(list, grown, Value::Void));
            }
            ("push0" | "add0", [item]) => {
                let mut grown = Vec::with_capacity(items.len() + 1);
                grown.push(item.clone());
                grown.extend(items.iter().cloned());
                return Some(self.store_list(list, grown, Value::Void));
            }
            ("push" | "add" | "push0" | "add0", _) => {
                Err(self.error("E0402", "push/add expects exactly one argument", span))
            }
            // 27.3.8 insert(index, item): the item goes at the index
            // location; index == size appends; a size smaller than index
            // is an error (the prose's exception; indexes count from 0,
            // 27.3.6).
            // 27.3.9 insert(index, list): all items of the argument list
            // go in starting at index (a list-valued second argument is
            // the list form; the prose's "positive integer" index is read
            // with the 0-based numbering of 27.3.6).
            ("insert", [Value::Int(index), Value::List(inserted)]) => {
                let at = match self.list_insert_position(items, *index, span) {
                    Ok(at) => at,
                    Err(diagnostic) => return Some(Err(diagnostic)),
                };
                let inserted_items = inserted.items.borrow().clone();
                let mut grown = Vec::with_capacity(items.len() + inserted_items.len());
                grown.extend_from_slice(&items[..at]);
                grown.extend(inserted_items);
                grown.extend_from_slice(&items[at..]);
                return Some(self.store_list(list, grown, Value::Void));
            }
            ("insert", [Value::Int(index), item]) => {
                let at = match self.list_insert_position(items, *index, span) {
                    Ok(at) => at,
                    Err(diagnostic) => return Some(Err(diagnostic)),
                };
                let mut grown = items.to_vec();
                grown.insert(at, item.clone());
                return Some(self.store_list(list, grown, Value::Void));
            }
            ("insert", _) => {
                Err(self.error("E0402", "insert expects an integer index and an item", span))
            }
            // 27.3.6 delete(index): removes item number index (indexes
            // start counting from 0) and renumbers sequentially; an index
            // that does not exist is an error (the prose's exception).
            ("delete", [Value::Int(index)]) => {
                let at = match self.list_index_position(items, *index, span) {
                    Ok(at) => at,
                    Err(diagnostic) => return Some(Err(diagnostic)),
                };
                let mut shrunk = items.to_vec();
                shrunk.remove(at);
                return Some(self.store_list(list, shrunk, Value::Void));
            }
            ("delete", _) => Err(self.error("E0402", "delete expects an integer index", span)),
            // 27.3.7 fast_delete(index): the last item's value moves into
            // the deleted slot, so every other item keeps its index; an
            // index that does not exist is an error (the prose's
            // exception).
            ("fast_delete", [Value::Int(index)]) => {
                let at = match self.list_index_position(items, *index, span) {
                    Ok(at) => at,
                    Err(diagnostic) => return Some(Err(diagnostic)),
                };
                let mut shrunk = items.to_vec();
                let last = shrunk.pop().expect("index in range implies a last item");
                if at < shrunk.len() {
                    shrunk[at] = last;
                }
                return Some(self.store_list(list, shrunk, Value::Void));
            }
            ("fast_delete", _) => {
                Err(self.error("E0402", "fast_delete expects an integer index", span))
            }
            // 27.3.5 clear(): deletes all items in the list.
            ("clear", []) => {
                return Some(self.store_list(list, Vec::new(), Value::Void));
            }
            // 27.5.1 and_all(): TRUE when every item is TRUE; empty is
            // TRUE (the prose's stated empty result). The exp-parameter
            // form is a documented gap; the no-argument form reads the
            // items themselves.
            ("and_all", []) => {
                let mut all = true;
                for item in items {
                    match item {
                        Value::Bool(value) => all &= *value,
                        _ => {
                            return Some(Err(self.error(
                                "E0402",
                                "and_all() expects a list of booleans",
                                span,
                            )));
                        }
                    }
                }
                Ok(Value::Bool(all))
            }
            // 27.5.2 or_all(): TRUE when any item is TRUE; empty is
            // FALSE (the prose's stated empty result).
            ("or_all", []) => {
                let mut any = false;
                for item in items {
                    match item {
                        Value::Bool(value) => any |= *value,
                        _ => {
                            return Some(Err(self.error(
                                "E0402",
                                "or_all() expects a list of booleans",
                                span,
                            )));
                        }
                    }
                }
                Ok(Value::Bool(any))
            }
            // 27.5.4 product(): the integer product over the items; 1
            // when empty (the prose's stated empty result).
            ("product", []) => {
                let mut product: i128 = 1;
                for item in items {
                    match item {
                        Value::Int(value) => product = product.wrapping_mul(*value),
                        _ => {
                            return Some(Err(self.error(
                                "E0402",
                                "product() expects a list of integers",
                                span,
                            )));
                        }
                    }
                }
                Ok(Value::Int(product))
            }
            // 27.5.5 sum(): the integer sum over the items; 0 when empty
            // (the prose's stated empty result).
            ("sum", []) => {
                let mut sum: i128 = 0;
                for item in items {
                    match item {
                        Value::Int(value) => sum = sum.wrapping_add(*value),
                        _ => {
                            return Some(Err(self.error(
                                "E0402",
                                "sum() expects a list of integers",
                                span,
                            )));
                        }
                    }
                }
                Ok(Value::Int(sum))
            }
            // 27.4.15 max() / 27.4.18 min(): the item with the largest /
            // smallest value — on ties the item latest in the list; empty
            // is an error (the prose's exception).
            // 27.4.16 max_index() / 27.4.19 min_index(): the same scan,
            // returning the winning index.
            ("max" | "min" | "max_index" | "min_index", []) => self.list_extreme(items, name, span),
            // 27.4.11 is_a_permutation(): TRUE when the argument holds
            // the same items with the same multiplicities (order
            // irrelevant); struct items compare by instance index — the
            // prose's "compares the addresses of the struct items".
            ("is_a_permutation", [Value::List(other)]) => {
                let other_items = other.items.borrow().clone();
                let mut remaining: Vec<&Value> = other_items.iter().collect();
                let mut permutation = true;
                for item in items {
                    match remaining
                        .iter()
                        .position(|candidate| values_equal(candidate, item))
                    {
                        Some(position) => {
                            remaining.remove(position);
                        }
                        None => {
                            permutation = false;
                            break;
                        }
                    }
                }
                Ok(Value::Bool(permutation && remaining.is_empty()))
            }
            ("is_a_permutation", _) => {
                Err(self.error("E0402", "is_a_permutation expects a list argument", span))
            }
            _ => return None,
        };
        Some(result)
    }

    /// Replace the shared list object's storage with `items` (the
    /// modifying pseudo-methods' in-place write: the object keeps its
    /// identity, so aliases see the change) and return the pseudo-method's
    /// own result.
    fn store_list(
        &mut self,
        list: &ListValue,
        items: Vec<Value>,
        result: Value,
    ) -> Result<Value, Diagnostic> {
        *list.items.borrow_mut() = items;
        Ok(result)
    }

    /// The valid index positions for the 27.3.6 `delete` / 27.3.7
    /// `fast_delete` family: an index that does not exist in the list is
    /// an error (the prose's exception; `E0404` matches the `[i]` index
    /// expression's out-of-range code).
    fn list_index_position(
        &self,
        items: &[Value],
        index: i128,
        span: crate::ast::Span,
    ) -> Result<usize, Diagnostic> {
        let len = items.len() as i128;
        if index < 0 || index >= len {
            Err(self.error(
                "E0404",
                format!("list index {index} out of range (size {len})"),
                span,
            ))
        } else {
            Ok(index as usize)
        }
    }

    /// The valid insertion positions for 27.3.8/27.3.9 `insert`: index ==
    /// size appends (the prose's stated case), a size smaller than index
    /// is an error (the prose's exception).
    fn list_insert_position(
        &self,
        items: &[Value],
        index: i128,
        span: crate::ast::Span,
    ) -> Result<usize, Diagnostic> {
        let len = items.len() as i128;
        if index < 0 || index > len {
            Err(self.error(
                "E0404",
                format!("insert index {index} out of range (size {len})"),
                span,
            ))
        } else {
            Ok(index as usize)
        }
    }

    /// 27.4.15/27.4.16/27.4.18/27.4.19: the extreme item or its index.
    /// On ties the item latest in the list wins (the prose's rule); empty
    /// is an error (the prose's exception, reported like the set
    /// pseudo-methods' empty-set errors), and items the relational
    /// operators do not order are a type error.
    fn list_extreme(
        &self,
        items: &[Value],
        name: &str,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let best = self.list_extreme_index(items, name, span)?;
        Ok(if name.ends_with("_index") {
            Value::Int(best as i128)
        } else {
            items[best].clone()
        })
    }

    fn list_extreme_index(
        &self,
        items: &[Value],
        name: &str,
        span: crate::ast::Span,
    ) -> Result<usize, Diagnostic> {
        if items.is_empty() {
            return Err(self.error("E0402", format!("{name} of an empty list"), span));
        }
        let want_max = name.starts_with("max");
        let mut best = 0usize;
        for (i, item) in items.iter().enumerate().skip(1) {
            let ordering = compare_values(item, &items[best]).ok_or_else(|| {
                self.error("E0402", format!("{name}() items are not comparable"), span)
            })?;
            use std::cmp::Ordering::*;
            // Equal takes over, so the latest of a tie wins.
            let takes_over = if want_max {
                ordering != Less
            } else {
                ordering != Greater
            };
            if takes_over {
                best = i;
            }
        }
        Ok(best)
    }

    /// Evaluate a temporal expression's propositions (`true(exp)` —
    /// `TRUE` or non-zero numeric, 12.1.1.1) and numeric repeat bounds
    /// once, with current field values. The results key the callback
    /// set the engine sees (by expression identity — the engine passes
    /// back the very references collected here); the interpreter keeps
    /// no per-cycle value history, so this is the honest approximation
    /// for propositions spanning several states.
    fn evaluate_temporal_exprs<'te>(
        &mut self,
        te: &'te crate::ast::TemporalExpr,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> TemporalValues<'te> {
        let mut propositions = Vec::new();
        let mut bounds = Vec::new();
        crate::temporal::collect_exprs(te, &mut propositions, &mut bounds);
        let booleans = propositions
            .into_iter()
            .map(|expr| {
                let value = match self.eval(expr, frame, me) {
                    Ok(Value::Bool(value)) => Some(value),
                    Ok(Value::Int(value)) => Some(value != 0),
                    _ => None,
                };
                (expr, value)
            })
            .collect();
        let numerics = bounds
            .into_iter()
            .map(|expr| {
                let value = match self.eval(expr, frame, me) {
                    Ok(Value::Int(value)) => Some(value),
                    _ => None,
                };
                (expr, value)
            })
            .collect();
        (booleans, numerics)
    }

    /// The instance whose `on` blocks an emitted event triggers: the
    /// enclosing instance for a bare event name (the event is local to
    /// its struct, clause 11.2), or the object the dotted path reads
    /// through. `None` when no instance resolves — the firing is still
    /// recorded, but no `on` blocks run.
    fn resolve_event_owner(
        &mut self,
        target: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Option<usize> {
        match target {
            Expr::Ident { .. } => me,
            Expr::MemberAccess { object, .. } => {
                let object_value = self.eval(object, frame, me).ok()?;
                match object_value {
                    Value::Object(index) => Some(index),
                    _ => None,
                }
            }
            // A static event has no instance owner.
            Expr::StaticAccess { .. } => None,
            _ => None,
        }
    }

    /// Execute the `on` action blocks (clause 11.2) registered for an
    /// emitted event path, in composition order — base types first, then
    /// extends as written. The event is local to the owner's struct, so
    /// the match is on the event name (last path segment) within the
    /// owner's type; each block runs with a fresh locals frame on the
    /// owning instance, and `return`/`break`/`continue` end that block
    /// only. The depth guard bounds `emit`-inside-`on` recursion.
    fn run_on_blocks(
        &mut self,
        path: &str,
        owner: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        let Some(owner) = owner else {
            return Ok(());
        };
        let event_name = path.rsplit('.').next().unwrap_or(path);
        let type_name = self.instances[owner].type_name.clone();
        let Some(ty) = self.types.iter().find(|t| t.name == type_name) else {
            return Ok(());
        };
        let blocks: Vec<crate::ast::OnDecl> = ty
            .ons
            .iter()
            .filter(|on| on.path.last().map(String::as_str) == Some(event_name))
            .cloned()
            .collect();
        if blocks.is_empty() {
            return Ok(());
        }
        if self.depth > 200 {
            return Err(self.error("E0406", "recursion too deep", span));
        }
        self.depth += 1;
        // `on` blocks are atomic regions: waits inside keep the inline
        // spin (the v1 limitation — a suspending on-block would strand
        // the emitter's continuation).
        self.atomic_depth += 1;
        for block in &blocks {
            let mut frame = Frame::default();
            match self.exec_block(&block.body, &mut frame, Some(owner))? {
                Flow::Normal | Flow::Break | Flow::Continue | Flow::Return(_) => {}
                Flow::Stop => {
                    self.stop_requested = true;
                }
                // Suspension is disabled in atomic regions.
                Flow::Suspend => {}
            }
        }
        self.atomic_depth -= 1;
        self.depth -= 1;
        Ok(())
    }

    /// Fire the static event `Type::event` (clause 11.1.4.1/11.2): one
    /// occurrence of the type itself, no instance. The occurrence is
    /// recorded in the temporal log under the type-qualified path
    /// (`bus::reset`) — the same log every event shares — TCMs parked
    /// on the path (`f() @bus::reset`) wake, and the static `on` blocks
    /// run: the declaring type's `static on <event>` blocks (composed
    /// per type, base-most first, then extends) and every
    /// type-qualified `on Type::event` block, all with no `me` (the
    /// same restrictions the static methods have, E0215/E0216 at the
    /// well-formedness pass). Coverage groups are sampled by path with
    /// no owner — an instance-dependent item evaluation has no `me` and
    /// keeps its previous behavior of not sampling.
    fn fire_static_event(
        &mut self,
        type_name: &str,
        event_name: &str,
        span: crate::ast::Span,
        frame: &mut Frame,
    ) -> Result<(), Diagnostic> {
        // The well-formedness pass validated the spelling; the runtime
        // check is the backstop for the documented gaps (statics inside
        // template bodies).
        let declared = self
            .static_events
            .get(type_name)
            .is_some_and(|events| events.iter().any(|event| event.name == event_name));
        if !declared {
            return Err(self.error(
                "E0401",
                format!("unknown static event '{type_name}::{event_name}'"),
                span,
            ));
        }
        let path = format!("{type_name}::{event_name}");
        self.emitted.insert(path.clone());
        self.pad_log();
        self.temporal_log[self.cycle as usize].record(&path);
        self.wake_tcm_listeners(&path);
        // The static on blocks: `static on <event>` of the declaring
        // type, plus every `on Type::event` block regardless of its
        // declaring type — both are static context (clause 11.2's
        // static forms), composed per type, not per instance.
        let mut blocks: Vec<(String, OnDecl)> = Vec::new();
        for (owner, ons) in self.static_ons {
            for on in ons {
                let matches_static = owner == type_name
                    && on.type_qualified.is_none()
                    && on.is_static
                    && on.path.last().map(String::as_str) == Some(event_name);
                let matches_qualified = on.type_qualified.as_deref() == Some(type_name)
                    && on.path.last().map(String::as_str) == Some(event_name);
                if matches_static || matches_qualified {
                    blocks.push((owner.clone(), on.clone()));
                }
            }
        }
        if !blocks.is_empty() {
            if self.depth > 200 {
                return Err(self.error("E0406", "recursion too deep", span));
            }
            self.depth += 1;
            // Static on blocks are atomic regions like instance ones.
            self.atomic_depth += 1;
            let previous_context = self.static_context.take();
            for (owner, block) in &blocks {
                // The block is static context of its declaring type: a
                // bare static-field reference inside resolves against
                // that type's store (no `me` exists here).
                self.static_context = Some(owner.clone());
                let mut on_frame = Frame::default();
                match self.exec_block(&block.body, &mut on_frame, None)? {
                    Flow::Normal | Flow::Break | Flow::Continue | Flow::Return(_) => {}
                    Flow::Stop => {
                        self.stop_requested = true;
                    }
                    Flow::Suspend => {}
                }
            }
            self.static_context = previous_context;
            self.atomic_depth -= 1;
            self.depth -= 1;
        }
        self.sample_cover_groups(&path, None, frame);
        Ok(())
    }

    /// The `hdl_*` routines: drive the external SystemVerilog simulator
    /// through [`crate::hdl`]. See examples/mixed_counter.e.
    fn call_hdl(
        &mut self,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        // Diagnostics are built inline (no closure holds a borrow of
        // `self` across the mutable session accesses).
        let not_loaded = || {
            Diagnostic::new(
                "E0408",
                "no simulator loaded — call hdl_load first",
                span.0,
                span.1,
            )
        };
        match name {
            "hdl_load" => {
                if values.len() != 2 {
                    return Err(self.error(
                        "E0402",
                        format!("'hdl_load' expects 2 arguments, got {}", values.len()),
                        span,
                    ));
                }
                let [file, top] = values.as_slice() else {
                    unreachable!("checked the count");
                };
                let Value::Str(file) = file else {
                    return Err(self.error("E0402", "hdl_load expects a design file string", span));
                };
                let Value::Str(top) = top else {
                    return Err(self.error(
                        "E0402",
                        "hdl_load expects a top module name string",
                        span,
                    ));
                };
                match crate::hdl::load(file, top) {
                    Ok(session) => {
                        self.hdl = Some(crate::hdl::SignalSource::Real(session));
                        Ok(Value::Void)
                    }
                    Err(msg) => Err(self.error("E0408", msg, span)),
                }
            }
            "hdl_set" => {
                if values.len() != 2 {
                    return Err(self.error(
                        "E0402",
                        format!("'hdl_set' expects 2 arguments, got {}", values.len()),
                        span,
                    ));
                }
                let [path, value] = values.as_slice() else {
                    unreachable!("checked the count");
                };
                let Value::Str(path) = path else {
                    return Err(self.error("E0402", "hdl_set expects a signal path string", span));
                };
                let Some(value) = as_i64(value) else {
                    return Err(self.error(
                        "E0402",
                        "hdl_set expects an integer (or boolean) value",
                        span,
                    ));
                };
                if self.hdl.is_none() {
                    return Err(not_loaded());
                }
                let cycle = self.cycle;
                match self.hdl.as_mut().unwrap().set(path, value, cycle) {
                    Ok(()) => Ok(Value::Void),
                    Err(msg) => Err(self.error("E0408", msg, span)),
                }
            }
            "hdl_get" => {
                if values.len() != 1 {
                    return Err(self.error(
                        "E0402",
                        format!("'hdl_get' expects 1 argument, got {}", values.len()),
                        span,
                    ));
                }
                let [path] = values.as_slice() else {
                    unreachable!("checked the count");
                };
                let Value::Str(path) = path else {
                    return Err(self.error("E0402", "hdl_get expects a signal path string", span));
                };
                if self.hdl.is_none() {
                    return Err(not_loaded());
                }
                let cycle = self.cycle;
                match self.hdl.as_mut().unwrap().get(path, cycle) {
                    Ok(value) => Ok(Value::Int(value)),
                    Err(msg) => Err(self.error("E0408", msg, span)),
                }
            }
            "hdl_run" => {
                if values.len() != 1 {
                    return Err(self.error(
                        "E0402",
                        format!("'hdl_run' expects 1 argument, got {}", values.len()),
                        span,
                    ));
                }
                let [delay] = values.as_slice() else {
                    unreachable!("checked the count");
                };
                let Some(delay) = as_i64(delay) else {
                    return Err(self.error("E0402", "hdl_run expects an integer delay", span));
                };
                if delay < 0 {
                    return Err(self.error("E0402", "hdl_run expects a non-negative delay", span));
                }
                if self.hdl.is_none() {
                    return Err(not_loaded());
                }
                match self.hdl.as_mut().unwrap().run(delay as u64) {
                    Ok(()) => Ok(Value::Void),
                    Err(msg) => Err(self.error("E0408", msg, span)),
                }
            }
            "hdl_finish" => {
                if !values.is_empty() {
                    return Err(self.error(
                        "E0402",
                        format!("'hdl_finish' expects no arguments, got {}", values.len()),
                        span,
                    ));
                }
                if let Some(mut session) = self.hdl.take() {
                    session.finish();
                }
                Ok(Value::Void)
            }
            _ => Err(self.error("E0405", format!("routine '{name}' is not supported"), span)),
        }
    }

    fn eval(
        &mut self,
        expr: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Value, Diagnostic> {
        match expr {
            Expr::IntLiteral { text, .. } => {
                parse_int_literal(text).map(Value::Int).ok_or_else(|| {
                    self.error(
                        "E0402",
                        format!("invalid integer literal '{text}'"),
                        expr.span(),
                    )
                })
            }
            Expr::RealLiteral { text, .. } => text
                .replace('_', "")
                .parse::<f64>()
                .map(Value::Real)
                .map_err(|_| {
                    self.error(
                        "E0402",
                        format!("invalid real literal '{text}'"),
                        expr.span(),
                    )
                }),
            Expr::BitLiteral { text, .. } => {
                if based_value_is_mvl(text) {
                    return Err(self.error(
                        "E0402",
                        format!("MVL literal '{text}' is not evaluated yet"),
                        expr.span(),
                    ));
                }
                parse_int_literal(text).map(Value::Int).ok_or_else(|| {
                    self.error(
                        "E0402",
                        format!("invalid based literal '{text}'"),
                        expr.span(),
                    )
                })
            }
            Expr::BoolLiteral { value, .. } => Ok(Value::Bool(*value)),
            Expr::StringLiteral { value, .. } => Ok(Value::Str(unescape(value))),
            // A template value formal substitutes to a constant at
            // instantiation (8.5.2); reaching the evaluator with one
            // means the substitution never bound it — no value.
            Expr::Formal { name, span } => Err(self.error(
                "E0402",
                format!("template value parameter <{name}> is not substituted"),
                *span,
            )),
            Expr::Ident { name, span } => {
                if name == "me" || name == "self" {
                    return match me {
                        Some(index) => Ok(Value::Object(index)),
                        None => Err(self.error("E0402", "no enclosing instance for 'me'", *span)),
                    };
                }
                // `port$` — simple_port value access through the bridge.
                if let Some(port) = name.strip_suffix('$') {
                    return self.read_port_value(port, me, *span);
                }
                // `define` constants.
                if let Some(value) = self.defines.get(name) {
                    return Ok(value.clone());
                }
                // The null reference.
                if name == "NULL" {
                    return Ok(Value::Void);
                }
                // Objection identifiers: values are irrelevant to the
                // inline subset.
                if name == "TEST_DONE" {
                    return Ok(Value::Int(0));
                }
                // The predefined numeric constants (clause 4.1's named
                // constants; the select-choice evaluator resolves them
                // too): MAX_UINT, MAX_INT, MIN_INT.
                if let Some(value) = predefined_constant(name) {
                    return Ok(Value::Int(value));
                }
                if let Some(value) = frame.locals.get(name) {
                    return Ok(value.clone());
                }
                if let Some(index) = me
                    && let Some(value) = self.instances[index].get(name)
                {
                    return Ok(value.clone());
                }
                // A static field named bare (T2.1): inside the type (or
                // its children — the shared occurrence), or inside a
                // static method of the type. The resolution walks the
                // instance's (or the static context's) base chain to the
                // declaring type's store entry.
                if let Some(owner) = self.bare_static_field_owner(me, name)
                    && let Some(value) =
                        self.statics.get(&owner).and_then(|fields| fields.get(name))
                {
                    return Ok(value.clone());
                }
                // A simple_port/event_port member read is the port
                // handle (clause 9): the 9.13 binding methods dispatch on
                // it; the port's *value* is the `name$` bridge access,
                // not this handle.
                if let Some(index) = me
                    && self.instance_has_port(index, name)
                {
                    return Ok(Value::Port {
                        owner: index,
                        name: name.clone(),
                    });
                }
                // The root unit instance is addressable by name from any
                // method (`for each in sys.ops`).
                if name == "sys" {
                    let sys_index = self
                        .instances
                        .iter()
                        .position(|instance| instance.type_name == "sys");
                    if let Some(index) = sys_index {
                        return Ok(Value::Object(index));
                    }
                }
                let types: &[crate::elab::ElaboratedType] = self.types;
                for t in types.iter().filter(|t| t.kind == DeclarationKind::Type) {
                    if t.enumerals.iter().any(|e| e.name == *name) {
                        return Ok(Value::Enum {
                            type_name: t.name.clone(),
                            name: name.clone(),
                        });
                    }
                }
                // Inline enumeral lists on fields: `[imm, reg]` etc.
                for t in types {
                    for field in &t.fields {
                        if let TypeExpr::InlineEnum { names, .. } = &field.ty
                            && names.iter().any(|n| n == name)
                        {
                            return Ok(Value::Enum {
                                type_name: inline_enum_key(names),
                                name: name.clone(),
                            });
                        }
                    }
                }
                // Message-verbosity levels — the fallback when nothing in
                // the program declares a same-named enumerand (a declared
                // `LOW`/`MEDIUM`/`HIGH` enumerand above shadows the
                // predefined message levels, as scoping requires).
                if matches!(name.as_str(), "LOW" | "MEDIUM" | "HIGH") {
                    let level = match name.as_str() {
                        "LOW" => 0,
                        "MEDIUM" => 1,
                        _ => 2,
                    };
                    return Ok(Value::Int(level));
                }
                // Clause 4.4.5.3 name resolution: for a path-less name,
                // the resolver checks the literal name `global` (which
                // denotes the global struct itself) BEFORE checking
                // whether the name is a member of `global`. The global
                // struct is not modelled in this interpreter — user
                // `extend global` is reserved for the implementation
                // (clause 4.4.2.2, rejected by the parser as E0116), and
                // the parser's file-level `define`/`import`
                // pseudo-declaration carries no runtime members — so
                // there is no global-struct value to produce and no
                // member-of-`global` namespace to fall back to. A bare
                // `global` therefore reaches the unknown-identifier
                // error below; the literal-`global`-first ordering is
                // recorded here for the stage that models the global
                // struct.
                Err(self.error("E0401", format!("unknown identifier '{name}'"), *span))
            }
            Expr::MemberAccess {
                object, name, span, ..
            } => {
                // `@T.member` — the annotation API's static methods on
                // the annotation type (clause 35.4): dispatched on the
                // name, with no receiver value behind `@T`. A call
                // through this shape dispatches in the `Expr::Call` arm
                // with its arguments; this path is the argument-less
                // member read.
                if let Expr::AnnotationTypeRef {
                    name: type_name, ..
                } = object.as_ref()
                {
                    return self.call_annotation_type_method(type_name, name, &[], *span);
                }
                let object_value = self.eval(object, frame, me)?;
                match object_value {
                    Value::Object(index) => {
                        // `unit.port$` — simple_port value access.
                        if let Some(port) = name.strip_suffix('$') {
                            let port = port.to_owned();
                            return self.read_port_value(&port, Some(index), *span);
                        }
                        if let Some(value) = self.instances[index].get(name) {
                            return Ok(value.clone());
                        }
                        // `unit.port` — the port handle (clause 9).
                        if self.instance_has_port(index, name) {
                            return Ok(Value::Port {
                                owner: index,
                                name: name.clone(),
                            });
                        }
                        Err(self.error("E0401", format!("unknown field '{name}'"), *span))
                    }
                    other => Err(self.error(
                        "E0402",
                        format!(
                            "cannot access member '{name}' of {}",
                            self.value_kind(&other)
                        ),
                        *span,
                    )),
                }
            }
            Expr::QualifiedField {
                base,
                qualifiers,
                field,
                field_span,
                span,
            } => {
                // 4.17.3: the field is present iff the instance currently
                // matches the qualifiers' subtype.
                let object_value = self.eval(base, frame, me)?;
                let Value::Object(index) = object_value else {
                    return Err(self.error(
                        "E0402",
                        "subtype-qualified access requires an object",
                        *span,
                    ));
                };
                self.read_qualified_field(index, qualifiers, field, *field_span, *span)
            }
            Expr::Call { callee, args, span } => {
                // The clause 29.13 rgf routines read their field argument
                // as a path (its instance and field name), not as a
                // value: the interception runs before the generic
                // argument evaluation.
                if let Expr::Ident { name, .. } = callee.as_ref()
                    && let Some(result) = self.call_rgf_routine(name, args, frame, me, *span)?
                {
                    return Ok(result);
                }
                // Key expressions need a per-item `it` binding, before
                // ordinary value-only argument evaluation. Preserve a
                // non-list receiver for same-named struct methods.
                let mut extreme_receiver = None;
                if let Expr::MemberAccess { object, name, .. } = callee.as_ref()
                    && matches!(name.as_str(), "has" | "count" | "all" | "all_indices")
                {
                    let receiver = self.eval(object, frame, me)?;
                    if let Value::List(list) = receiver {
                        return self.call_list_query(&list, name, args, frame, me, *span);
                    }
                    extreme_receiver = Some(receiver);
                }
                if let Expr::MemberAccess { object, name, .. } = callee.as_ref()
                    && matches!(name.as_str(), "min" | "max" | "min_index" | "max_index")
                {
                    let receiver = self.eval(object, frame, me)?;
                    if let Value::List(list) = receiver {
                        return self.call_list_extreme(&list, name, args, frame, me, *span);
                    }
                    extreme_receiver = Some(receiver);
                }
                // Numeric aggregates also evaluate per-item expressions;
                // their existing non-list fallback uses ordinary dispatch.
                if let Expr::MemberAccess { object, name, .. } = callee.as_ref()
                    && matches!(
                        name.as_str(),
                        "sum" | "product" | "average" | "max_value" | "min_value"
                    )
                    && let Value::List(list) = self.eval(object, frame, me)?
                {
                    let items = list.items.borrow().clone();
                    return self.call_list_aggregate(&items, name, args, frame, me, *span);
                }
                // A call argument's parameter type is the inference
                // context for an untyped `evaluate { ... }` in the
                // argument (4.17.7). Looked up only when an argument can
                // use it, and best-effort: a builtin or unknown callee
                // contributes no hints (the call itself reports its own
                // error).
                let hints = if args.iter().any(expr_has_untyped_evaluate) {
                    self.call_arg_hints(callee, args.len(), frame, me)
                } else {
                    Vec::new()
                };
                let mut values = Vec::with_capacity(args.len());
                for (position, arg) in args.iter().enumerate() {
                    let hint = hints.get(position).cloned().unwrap_or(None);
                    values.push(self.eval_with_hint(hint, arg, frame, me)?);
                }
                match callee.as_ref() {
                    Expr::Ident { name, .. } => {
                        if crate::builtins::is_builtin_routine(name) {
                            return self.call_builtin(name, values, *span, me);
                        }
                        let Some(index) = me else {
                            return Err(self.error(
                                "E0402",
                                format!("call to '{name}' without an enclosing instance"),
                                *span,
                            ));
                        };
                        self.call_method(index, name, values, *span)
                    }
                    Expr::MemberAccess { object, name, .. } => {
                        // Objection/other builtins called through an
                        // instance (`driver.raise_objection(...)`) route
                        // to the builtin table first.
                        if crate::builtins::is_builtin_routine(name) {
                            return self.call_builtin(name, values, *span, me);
                        }
                        // `@T.method(args)` — the clause 35.4
                        // annotation API on the annotation type:
                        // dispatched on the name, no receiver value.
                        if let Expr::AnnotationTypeRef {
                            name: type_name, ..
                        } = object.as_ref()
                        {
                            return self
                                .call_annotation_type_method(type_name, name, &values, *span);
                        }
                        // The predefined `covers` (14.9), `files` (30.5)
                        // and `table_manager` (32.3) singletons are
                        // method receivers only: their member calls
                        // dispatch here without an instance value behind
                        // the receiver. A local or field of the same
                        // name wins.
                        if let Expr::Ident {
                            name: singleton, ..
                        } = object.as_ref()
                            && (singleton == "covers"
                                || singleton == "files"
                                || singleton == "table_manager")
                            && !frame.locals.contains_key(singleton)
                            && !me
                                .is_some_and(|index| self.instances[index].get(singleton).is_some())
                        {
                            return match singleton.as_str() {
                                "covers" => self.call_covers_method(name, values, *span),
                                "files" => self.call_files_method(name, values, *span),
                                _ => self.call_table_manager_method(name, values, *span),
                            };
                        }
                        // The 27.4.17/27.4.20/27.5.4-27.5.6 aggregates
                        // dispatched before the argument evaluation
                        // above; every other member call evaluates its
                        // arguments to values first.
                        let object_value = match extreme_receiver {
                            Some(value) => value,
                            None => self.eval(object, frame, me)?,
                        };
                        match object_value {
                            Value::Object(index) => {
                                // Clause 6.13 dynamic dispatch: a call
                                // through an interface-typed expression
                                // invokes the object's actual type's
                                // method (call_method resolves on the
                                // instance). When the interface declares
                                // the method but the actual type does
                                // not provide it, the missing
                                // implementation is the error (E0414) —
                                // not the plain not-found one.
                                let interface_name = self
                                    .static_type_of(object, frame, me)
                                    .and_then(|declared| match declared {
                                        TypeExpr::Named { name: ty_name, .. }
                                            if self.is_interface(&ty_name) =>
                                        {
                                            Some(ty_name)
                                        }
                                        _ => None,
                                    });
                                if let Some(interface_name) = interface_name {
                                    let declares = self
                                        .types
                                        .iter()
                                        .find(|t| {
                                            t.name == interface_name
                                                && t.kind == DeclarationKind::Interface
                                        })
                                        .is_some_and(|t| t.methods.iter().any(|m| m.name == *name));
                                    if declares {
                                        let type_name = self.instances[index].type_name.clone();
                                        let types: &[crate::elab::ElaboratedType] = self.types;
                                        let provides =
                                            types.iter().find(|t| t.name == type_name).is_some_and(
                                                |t| t.methods.iter().any(|m| m.name == *name),
                                            );
                                        if !provides {
                                            return Err(self.error(
                                                "E0414",
                                                format!(
                                                    "struct '{type_name}' does not implement interface method '{name}'"
                                                ),
                                                *span,
                                            ));
                                        }
                                    }
                                }
                                self.call_method(index, name, values, *span)
                            }
                            Value::Port {
                                owner,
                                name: port_name,
                            } => self.call_port_method(owner, &port_name, name, values, *span),
                            Value::List(list) => {
                                // The modifying pseudo-methods mutate the
                                // shared storage through the handle; the
                                // dispatch reads a snapshot of the items.
                                let items = list.items.borrow().clone();
                                match self
                                    .call_list_pseudo_method(&list, &items, name, &values, *span)
                                {
                                    Some(result) => result,
                                    None => Err(self.error(
                                        "E0405",
                                        format!("list method '{name}' is not supported"),
                                        *span,
                                    )),
                                }
                            }
                            Value::Set(intervals) => {
                                match self.call_set_pseudo_method(&intervals, name, &values, *span)
                                {
                                    Some(result) => result,
                                    None => Err(self.error(
                                        "E0405",
                                        format!("set method '{name}' is not supported"),
                                        *span,
                                    )),
                                }
                            }
                            // The 32.3 table API (T9.3): methods on a
                            // table value and on a table_row value.
                            Value::Table(table) => {
                                self.call_table_value_method(table, name, values, *span)
                            }
                            Value::TableRow { table, row } => {
                                self.call_table_row_value_method(table, row, name, values, *span)
                            }
                            // The 32.2 instance iterator (T11.2) and
                            // the path items it hands out.
                            Value::InstanceIterator(handle) => {
                                self.call_instance_iterator_method(handle, name, values, *span)
                            }
                            Value::InstancePathItem(item) => {
                                self.call_instance_path_item_method(item, name, values, *span)
                            }
                            // The clause 33 resource-sharing structs:
                            // up()/down()/try_up()/try_down()/set_value()/
                            // get_value()/set_max_value()/get_max_value()
                            // on a semaphore or rdv_semaphore; lock()/
                            // release() on a locker.
                            Value::Sync(sync) => self.call_sync_method(sync, name, values, *span, frame),
                            // `to_string()` on scalars (the DAC macro
                            // shapes: `set.min().to_string()`).
                            value @ (Value::Int(_)
                            | Value::Bool(_)
                            | Value::Enum { .. }
                            | Value::Str(_))
                                if name == "to_string" && values.is_empty() =>
                            {
                                Ok(Value::Str(self.format_value(&value)))
                            }
                            // A custom numeric value has no directly
                            // callable methods (T7.1): operators,
                            // conversions and message rendering
                            // dispatch to its struct's numeric_i
                            // methods internally; struct-field access
                            // goes through as_a to the struct.
                            Value::Custom { type_name, .. } => Err(self.error(
                                "E0402",
                                format!(
                                    "cannot call method '{name}' on a '{type_name}' value (operators and as_a dispatch to the implementing struct)"
                                ),
                                *span,
                            )),
                            _ => Err(self.error(
                                "E0402",
                                format!("cannot call method '{name}' on this value"),
                                *span,
                            )),
                        }
                    }
                    // A method named through a subtype-qualified access
                    // (`p.red'f()`) resolves on the qualified base object
                    // — the field dispatch is the plain one (4.17.3
                    // qualifies field presence, not method lookup).
                    Expr::QualifiedField { base, field, .. } => {
                        let object_value = self.eval(base, frame, me)?;
                        let Value::Object(index) = object_value else {
                            return Err(self.error(
                                "E0402",
                                "subtype-qualified call requires an object",
                                *span,
                            ));
                        };
                        self.call_method(index, field, values, *span)
                    }
                    // `Type::method(args)` — the static method call
                    // (clause 17.2.2): dispatched without a receiver.
                    Expr::StaticAccess {
                        type_name, member, ..
                    } => self.call_static_method(type_name, member, values, *span),
                    _ => Err(self.error("E0402", "call target not supported", *span)),
                }
            }
            Expr::Index { base, index, span } => {
                let base_value = self.eval(base, frame, me)?;
                let index_value = self.eval(index, frame, me)?;
                let (Value::List(list), Value::Int(i)) = (&base_value, &index_value) else {
                    return Err(self.error(
                        "E0402",
                        "indexing requires a list and an integer",
                        *span,
                    ));
                };
                let items = list.items.borrow();
                let len = items.len() as i128;
                if *i < 0 || *i >= len {
                    return Err(self.error(
                        "E0404",
                        format!("list index {i} out of range (size {len})"),
                        *span,
                    ));
                }
                Ok(items[*i as usize].clone())
            }
            Expr::ListConcat { items, .. } => {
                // `{ e1; e2; ... }` (clause 4.6, Table 12): a fresh list;
                // a list element contributes its items, any other element
                // contributes itself. The creation site cannot know the
                // element type — a variable's declared type records it at
                // the `var` initialization.
                let mut values = Vec::new();
                for item in items {
                    match self.eval(item, frame, me)? {
                        Value::List(list) => {
                            values.extend(list.items.borrow().iter().cloned());
                        }
                        value => values.push(value),
                    }
                }
                Ok(Value::List(ListValue {
                    items: Rc::new(RefCell::new(values)),
                    element: None,
                }))
            }
            Expr::Binary { op, lhs, rhs, span } => match op {
                BinaryOp::LogicalAnd => {
                    let Value::Bool(l) = self.eval(lhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of && is not a boolean", *span));
                    };
                    if !l {
                        return Ok(Value::Bool(false));
                    }
                    let Value::Bool(r) = self.eval(rhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of && is not a boolean", *span));
                    };
                    Ok(Value::Bool(r))
                }
                BinaryOp::LogicalOr => {
                    let Value::Bool(l) = self.eval(lhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of || is not a boolean", *span));
                    };
                    if l {
                        return Ok(Value::Bool(true));
                    }
                    let Value::Bool(r) = self.eval(rhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of || is not a boolean", *span));
                    };
                    Ok(Value::Bool(r))
                }
                // `a => b` is `!a || b`, short-circuiting on a false lhs.
                BinaryOp::Imply => {
                    let Value::Bool(l) = self.eval(lhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of => is not a boolean", *span));
                    };
                    if !l {
                        return Ok(Value::Bool(true));
                    }
                    let Value::Bool(r) = self.eval(rhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of => is not a boolean", *span));
                    };
                    Ok(Value::Bool(r))
                }
                _ => {
                    // The 5.6.3 precision context reads the operands'
                    // *static* types before evaluation; the values then
                    // compute inside it (apply_binary).
                    let lhs_ty = self.static_type_of(lhs, frame, me);
                    let rhs_ty = self.static_type_of(rhs, frame, me);
                    let precision = precision_context(lhs_ty.as_ref(), rhs_ty.as_ref());
                    let l = self.eval(lhs, frame, me)?;
                    let r = self.eval(rhs, frame, me)?;
                    self.apply_binary(*op, l, r, *span, &precision)
                }
            },
            Expr::Unary {
                op,
                expr: inner,
                span,
            } => {
                let value = self.eval(inner, frame, me)?;
                match (op, value) {
                    (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (UnaryOp::Negate, Value::Int(i)) => Ok(Value::Int(-i)),
                    (UnaryOp::Negate, Value::Real(r)) => Ok(Value::Real(-r)),
                    (UnaryOp::Plus, value @ (Value::Int(_) | Value::Real(_))) => Ok(value),
                    (UnaryOp::BitNot, Value::Int(i)) => Ok(Value::Int(!i)),
                    // The clause 31 unary shapes on a custom numeric
                    // value (T7.3): numeric_unary_minus/plus/bit_not
                    // write into a fresh result instance. A logical
                    // `!` has no numeric_i method — the operand error.
                    (
                        unary @ (UnaryOp::Negate | UnaryOp::Plus | UnaryOp::BitNot),
                        Value::Custom { type_name, instance },
                    ) => self.custom_unary(*unary, &type_name, instance, *span),
                    _ => Err(self.error("E0402", "invalid operand for unary operator", *span)),
                }
            }
            Expr::Paren { expr, .. } => self.eval(expr, frame, me),
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
                span,
            } => {
                let Value::Bool(cond_value) = self.eval(cond, frame, me)? else {
                    return Err(self.error("E0402", "ternary condition is not a boolean", *span));
                };
                if cond_value {
                    self.eval(then_expr, frame, me)
                } else {
                    self.eval(else_expr, frame, me)
                }
            }
            Expr::CastAs { expr, ty, span } => {
                // Casts are value-preserving in this subset: integers,
                // enumerals and booleans flow through unchanged — an
                // integer cast to an enumeral type maps its value to
                // the enumerand at that index (5.10.2).
                let value = self.eval(expr, frame, me)?;
                // A custom numeric source converts through the clause
                // 31 pairs (T7.3) before the value-preserving fallback
                // below.
                if matches!(value, Value::Custom { .. }) {
                    return self.custom_numeric_as_a(value, ty, *span);
                }
                if let (Value::Int(index), TypeExpr::Named { name, .. }) = (&value, ty) {
                    let types: &[crate::elab::ElaboratedType] = self.types;
                    if let Some(declared) = types.iter().find(|t| t.name == *name)
                        && !declared.enumerals.is_empty()
                        && (0..declared.enumerals.len()).contains(&(*index as usize))
                    {
                        return Ok(Value::Enum {
                            type_name: name.clone(),
                            name: declared.enumerals[*index as usize].name.clone(),
                        });
                    }
                }
                if let (Value::Int(index), TypeExpr::InlineEnum { names, .. }) = (&value, ty)
                    && (0..names.len()).contains(&(*index as usize))
                {
                    return Ok(Value::Enum {
                        type_name: inline_enum_key(names),
                        name: names[*index as usize].clone(),
                    });
                }
                // Interface casts (5.10.2.4): no object is of an
                // interface type, so an `as_a` to an interface is a
                // checked reference cast — the same pointer when the
                // object's type implements the interface, NULL
                // otherwise. The same NULL-on-mismatch applies when an
                // INTERFACE-typed expression casts to a struct or
                // another interface: the pointer is kept when the object
                // satisfies the target, and NULL when it does not (a
                // subtype might implement the target later, so the cast
                // itself is legal). Struct-to-struct casts keep their
                // value-preserving behavior below.
                if let TypeExpr::Named { name, .. } = ty {
                    if self.is_interface(name) {
                        return Ok(match value {
                            Value::Object(index)
                                if self.implements_interface(
                                    &self.instances[index].type_name.clone(),
                                    name,
                                ) =>
                            {
                                Value::Object(index)
                            }
                            _ => Value::Void,
                        });
                    }
                    if let Value::Object(index) = value {
                        let operand_is_interface = self
                            .static_type_of(expr, frame, me)
                            .is_some_and(|operand| match &operand {
                                TypeExpr::Named {
                                    name: operand_name, ..
                                } => self.is_interface(operand_name),
                                _ => false,
                            });
                        if operand_is_interface {
                            return Ok(
                                if self.type_is_a(&self.instances[index].type_name.clone(), name) {
                                    Value::Object(index)
                                } else {
                                    Value::Void
                                },
                            );
                        }
                    }
                }
                Ok(value)
            }
            Expr::SetOfValues { ty, full, .. } => {
                // 5.10.5/5.10.6: the set of all legal (or possible)
                // values of the type. No range restrictions are
                // recorded on declared types yet, so both forms build
                // the same set (the honest reading of 5.10.6's "for
                // types that have no range restrictions...").
                let _ = full;
                self.set_of_type(ty)
            }
            Expr::BitSlice {
                base,
                high,
                low,
                span,
            } => {
                let Value::Int(value) = self.eval(base, frame, me)? else {
                    return Err(self.error("E0402", "bit slice expects an integer", *span));
                };
                let Value::Int(h) = self.eval(high, frame, me)? else {
                    return Err(self.error("E0402", "slice bound must be an integer", *span));
                };
                let Value::Int(l) = self.eval(low, frame, me)? else {
                    return Err(self.error("E0402", "slice bound must be an integer", *span));
                };
                if l < 0 || h < l || h >= 127 {
                    return Err(self.error("E0402", "bit slice range invalid", *span));
                }
                let width = (h - l + 1) as u32;
                let mask: i128 = if width >= 127 {
                    -1
                } else {
                    (1i128 << width) - 1
                };
                Ok(Value::Int((value >> l) & mask))
            }
            Expr::InSet { element, set, span } => {
                let value = self.eval(element, frame, me)?;
                let contained = match set {
                    crate::ast::InSetExpr::Items(items) => {
                        let mut found = false;
                        for item in items {
                            let item_value = self.eval(item, frame, me)?;
                            if matches!(
                                self.apply_binary(
                                    BinaryOp::Eq,
                                    value.clone(),
                                    item_value,
                                    *span,
                                    &Precision::unbounded(),
                                )?,
                                Value::Bool(true)
                            ) {
                                found = true;
                                break;
                            }
                        }
                        found
                    }
                    crate::ast::InSetExpr::Range(low, high) => {
                        let Value::Int(low_v) = self.eval(low, frame, me)? else {
                            return Err(self.error(
                                "E0402",
                                "range bound must be an integer",
                                *span,
                            ));
                        };
                        let Value::Int(high_v) = self.eval(high, frame, me)? else {
                            return Err(self.error(
                                "E0402",
                                "range bound must be an integer",
                                *span,
                            ));
                        };
                        let Value::Int(v) = &value else {
                            return Err(self.error(
                                "E0402",
                                "range membership expects an integer",
                                *span,
                            ));
                        };
                        low_v <= *v && *v <= high_v
                    }
                    // A general set expression (4.11.6's set-exp): the
                    // operand evaluates to a set — membership over its
                    // intervals — or to a list, whose items are compared
                    // item-by-item.
                    crate::ast::InSetExpr::Expr(set_expr) => {
                        let container = self.eval(set_expr, frame, me)?;
                        match container {
                            Value::Set(intervals) => match &value {
                                Value::Int(v) => {
                                    intervals.iter().any(|(lo, hi)| *lo <= *v && *v <= *hi)
                                }
                                Value::Bool(b) => {
                                    let v = *b as i128;
                                    intervals.iter().any(|(lo, hi)| *lo <= v && v <= *hi)
                                }
                                _ => {
                                    return Err(self.error(
                                        "E0402",
                                        "set membership expects an integer, boolean or enumerand",
                                        *span,
                                    ));
                                }
                            },
                            Value::List(list) => {
                                let items = list.items.borrow().clone();
                                let mut found = false;
                                for item in items {
                                    if matches!(
                                        self.apply_binary(
                                            BinaryOp::Eq,
                                            value.clone(),
                                            item,
                                            *span,
                                            &Precision::unbounded(),
                                        )?,
                                        Value::Bool(true)
                                    ) {
                                        found = true;
                                        break;
                                    }
                                }
                                found
                            }
                            other => {
                                return Err(self.error(
                                    "E0402",
                                    format!(
                                        "'in' expects a set or list, got {}",
                                        self.value_kind(&other)
                                    ),
                                    *span,
                                ));
                            }
                        }
                    }
                };
                Ok(Value::Bool(contained))
            }
            Expr::SetLiteral { items, span } => self.eval_set_literal(items, *span, frame, me),
            Expr::IsA {
                expr,
                ty,
                binding,
                negated,
                span,
            } => {
                let value = self.eval(expr, frame, me)?;
                let is_a = self.value_is_a(&value, ty, *span)?;
                // A true `is a T (v)` binds the matched value to `v` for
                // the surrounding statement's body — the `if` condition
                // evaluates before its block, so the binding is in place
                // when the branch runs.
                if is_a
                    && !*negated
                    && let Some(name) = binding
                {
                    frame.locals.insert(name.clone(), value);
                }
                Ok(Value::Bool(is_a != *negated))
            }
            Expr::InTable {
                tuple,
                rows,
                negated,
                span,
            } => {
                // 4.11.8: the tuple matches a row when every element
                // matches — a scalar row element by `==`, a set row
                // element by membership. The first matching row decides;
                // `not` negates the result.
                let mut values = Vec::with_capacity(tuple.len());
                for element in tuple {
                    values.push(self.eval(element, frame, me)?);
                }
                let mut matched = false;
                'rows: for row in rows {
                    for (element, value) in row.iter().zip(values.iter()) {
                        if !self.row_element_matches(element, value, frame, me, *span)? {
                            continue 'rows;
                        }
                    }
                    matched = true;
                    break;
                }
                Ok(Value::Bool(matched != *negated))
            }
            Expr::Evaluate {
                ty,
                var,
                body,
                span,
            } => {
                // 4.17.7: the actions run; the named variable's value
                // after the block is the expression's value. The variable
                // is visible only inside the block (it shadows an outer
                // name of the same spelling and is removed afterwards)
                // and starts at its type's default. `wait`/`sync`/`return`
                // are rejected at parse time (E0123).
                let result_ty = match ty {
                    Some(ty) => ty.clone(),
                    // Type inference v1: the declared type of the
                    // assignment/`var` target or of the call's parameter,
                    // threaded through `expected_type` by those eval
                    // sites. E0122 is raised here — where the inference
                    // contexts actually resolve — not at parse time; the
                    // parser accepts the untyped form because it cannot
                    // know the surrounding declaration's type.
                    None => match self.expected_type.clone() {
                        Some(ty) => ty,
                        None => {
                            return Err(self.error(
                                "E0122",
                                "evaluate: result type cannot be inferred; write `evaluate <type> { ... }`",
                                *span,
                            ));
                        }
                    },
                };
                let var_name = var.clone().unwrap_or_else(|| "value".to_owned());
                let saved_local = frame
                    .locals
                    .insert(var_name.clone(), self.default_for(&result_ty));
                let saved_type = frame.local_types.insert(var_name.clone(), result_ty);
                // The hint does not flow into the block: the body's own
                // evaluates infer from their own contexts.
                let saved_hint = self.expected_type.take();
                let flow = self.exec_block(body, frame, me);
                self.expected_type = saved_hint;
                match saved_type {
                    Some(ty) => {
                        frame.local_types.insert(var_name.clone(), ty);
                    }
                    None => {
                        frame.local_types.remove(&var_name);
                    }
                }
                let value = frame.locals.get(&var_name).cloned();
                match saved_local {
                    Some(value) => {
                        frame.locals.insert(var_name.clone(), value);
                    }
                    None => {
                        frame.locals.remove(&var_name);
                    }
                }
                flow?;
                Ok(value.unwrap_or(Value::Void))
            }
            Expr::Select { span, .. } => Err(self.error(
                "E0402",
                "select distribution expressions are not evaluated yet",
                *span,
            )),
            Expr::AnnotationTypeRef { name, span } => {
                // A bare `@name` has no value (clause 35.4): the
                // annotation type exists as the receiver of its API
                // methods, which the member-access arm dispatches
                // before evaluating its object.
                Err(self.error(
                    "E0402",
                    format!("annotation type '@{name}' has no value — call one of its methods (@{name}.get_all_annotations(), clause 35.4)"),
                    *span,
                ))
            }
            Expr::New { ty, inits, span } => {
                // `new [<type>] [with { .. }]` (clause 6.3): a typed
                // creation instantiates the named type — the
                // monomorphized template instance's canonical name for a
                // template reference — and runs the `with` assignments
                // against the fresh instance. An untyped `new` creates
                // with the enclosing `var x: t = new;`'s declared type
                // (clause 18) when one is evaluating; other untyped
                // spellings stay the explicit not-supported diagnostic.
                let new_name = match ty {
                    None => match self.new_type_hint.clone() {
                        Some(name) => name,
                        None => {
                            return Err(self.error(
                                "E0402",
                                "untyped 'new' instance creation is not supported yet",
                                *span,
                            ));
                        }
                    },
                    Some(new_ty) => match new_ty {
                        TypeExpr::Named { name, .. } => name.clone(),
                        TypeExpr::TemplateRef { name, args, .. } => {
                            crate::ast::template_instance_name(name, args)
                        }
                        _ => {
                            return Err(self.error(
                                "E0402",
                                "'new' supports named and template-instance types only",
                                new_ty.span(),
                            ));
                        }
                    },
                };
                if self.depth > 100 {
                    return Ok(Value::Void);
                }
                // The 32.2 instance iterator (T11.2): `new
                // instance_iterator` mints an unstarted iterator — the
                // one reflection surface constructed for real. `start()`
                // / `start_all()` root it afterwards.
                if new_name == "instance_iterator" {
                    return Ok(Value::InstanceIterator(Rc::new(RefCell::new(
                        InstanceIteratorState {
                            visits: Vec::new(),
                            root_label: String::new(),
                            started: false,
                            cursor: None,
                        },
                    ))));
                }
                // The clause 33 resource-sharing structs: `new
                // semaphore` / `new rdv_semaphore` / `new locker` mint
                // the sync object — a semaphore carries 0 available
                // resources under a MAX_INT maximum, an rdv_semaphore
                // stays at 0/0, a locker starts free. They have no
                // members to initialize.
                let sync_kind = match new_name.as_str() {
                    "semaphore" => Some(SyncKind::Semaphore),
                    "rdv_semaphore" => Some(SyncKind::RdvSemaphore),
                    "locker" => Some(SyncKind::Locker),
                    _ => None,
                };
                if let Some(kind) = sync_kind {
                    if !inits.is_empty() {
                        return Err(self.error(
                            "E0402",
                            format!("'{new_name}' has no members to initialize"),
                            *span,
                        ));
                    }
                    return Ok(Value::Sync(Rc::new(RefCell::new(SyncState::new(kind)))));
                }
                // The recognised clause 32.1/32.2 reflection types
                // (T11.1): the construction stub — recognised, with the
                // clause in the message, instead of an unknown-type
                // error. Only the base instance_iterator above is
                // constructed for real.
                if let Some(clause) = crate::builtins::reflection_type_clause(&new_name) {
                    return Err(self.error(
                        "E0419",
                        format!("'{new_name}' is recognised (clause {clause}) but not implemented in this stage"),
                        *span,
                    ));
                }
                // Clause 5.2.9: no object is *of* an interface type — an
                // interface-typed value points at an instance of a
                // struct that implements it, so `new <interface>` has no
                // instance to create.
                if self.is_interface(&new_name) {
                    return Err(self.error(
                        "E0402",
                        format!("cannot instantiate interface '{new_name}' (clause 5.2.9)"),
                        *span,
                    ));
                }
                // Clause 35.2: `new` is not allowed for an annotation
                // type — the annotation exists as metadata recorded on
                // declarations and members, not as an instance.
                if self.annotation_types.contains_key(&new_name) {
                    return Err(self.error(
                        "E0402",
                        format!(
                            "cannot instantiate annotation type '{new_name}' — 'new' is not allowed for annotations (clause 35.2)"
                        ),
                        *span,
                    ));
                }
                self.depth += 1;
                let index = self.instantiate(&new_name);
                let mut frame = Frame::default();
                let flow = self.exec_block(inits, &mut frame, Some(index))?;
                self.depth -= 1;
                // The `with` block is an assignment list; a `stop_run`
                // inside it unwinds the run like any other statement.
                if matches!(flow, Flow::Stop) {
                    self.stop_requested = true;
                }
                Ok(Value::Object(index))
            }
            Expr::HdlPath { path, span } => {
                if self.hdl.is_none() {
                    return Err(self.error(
                        "E0408",
                        "no simulator loaded — call hdl_load first",
                        *span,
                    ));
                }
                let cycle = self.cycle;
                match self.hdl.as_mut().unwrap().get(path, cycle) {
                    Ok(value) => Ok(Value::Int(value)),
                    Err(msg) => Err(self.error("E0408", msg, *span)),
                }
            }
            Expr::StaticAccess {
                type_name,
                member,
                span,
                ..
            } => {
                // `Type::field` (clause 4.17.4): the shared per-type
                // occurrence from the static store. The well-formedness
                // pass has validated that the member is a static member
                // declared in exactly `Type`; a missing store entry here
                // means the store was never initialized for it (a static
                // inside a template body — a documented gap) or the
                // member is a static method used as a value.
                self.read_static_field(type_name, member, *span)
            }
            // A 10.5.8 chosen_from constraint is not an evaluatable
            // expression: it is applied by `gen` when its target is
            // generated. Reaching evaluation means it fired outside that
            // protocol (an unsupported shape, or outside a keep).
            Expr::ChosenFrom { span, .. } => Err(self.error(
                "E0402",
                "chosen_from is only a keep-constraint form (10.5.8); it is applied when its target is generated",
                *span,
            )),
        }
    }

    /// The runtime half of `expr is [not] a <type>` (clause 4.17.1). Scalar
    /// kinds match by value shape (integers are one runtime value, so
    /// `is a int` and `is a uint` both accept it); named types match by
    /// declared name, walking `like` bases — subtype-aware dispatch is a
    /// later milestone.
    fn value_is_a(
        &self,
        value: &Value,
        ty: &TypeExpr,
        span: crate::ast::Span,
    ) -> Result<bool, Diagnostic> {
        match ty {
            TypeExpr::Scalar { base, .. } => Ok(matches!(
                (base, value),
                (
                    ScalarBase::Int
                        | ScalarBase::Uint
                        | ScalarBase::Longint
                        | ScalarBase::Longuint
                        | ScalarBase::Bit
                        | ScalarBase::Byte
                        | ScalarBase::Nibble
                        | ScalarBase::Time,
                    Value::Int(_)
                ) | (ScalarBase::Bool, Value::Bool(_))
                    | (ScalarBase::Real, Value::Real(_))
                    | (ScalarBase::String, Value::Str(_))
            )),
            TypeExpr::List { .. } => Ok(matches!(value, Value::List(_))),
            TypeExpr::Named { name, .. } => Ok(match value {
                Value::Object(index) => {
                    // Clause 5.7.1: `is a <interface>` holds for an
                    // instance whose type implements the interface
                    // (directly or through inheritance); for a struct
                    // type name the `like` walk decides.
                    if self.is_interface(name) {
                        self.implements_interface(&self.instances[*index].type_name.clone(), name)
                    } else {
                        self.type_is_a(&self.instances[*index].type_name, name)
                    }
                }
                Value::Enum { type_name, .. } => self.type_is_a(type_name, name),
                _ => false,
            }),
            TypeExpr::WhenQualified {
                value: modifier,
                base,
                ..
            } => {
                // The instance must carry the synthesized subtype view
                // (or a descendant of it — the view's base chain reaches
                // the base type, and a nested view's chain walks through
                // the outer view it was applied on top of).
                let Some(view_name) = when_view_name(modifier, base) else {
                    return Ok(false);
                };
                Ok(match value {
                    Value::Object(index) => {
                        self.type_is_a(&self.instances[*index].type_name, &view_name)
                    }
                    _ => false,
                })
            }
            // A template reference names a template instance; instances
            // come from monomorphization (a later milestone).
            TypeExpr::TemplateRef { name, args, .. } => Ok(match value {
                Value::Object(index) => self.type_is_a(
                    &self.instances[*index].type_name,
                    &crate::ast::template_instance_name(name, args),
                ),
                _ => false,
            }),
            // `typeof()`/`typeof_item()` placeholders resolve during
            // elaboration (5.9); an `is a` against an unresolved one has
            // no static type to check against.
            TypeExpr::TypeOf { span, .. } | TypeExpr::TypeOfItem { span, .. } => Err(self.error(
                "E0402",
                "'is a' against an unresolved typeof() type is not supported",
                *span,
            )),
            // A formal, a width-qualified formal or a value actual never
            // reaches a resolved `is a` check.
            TypeExpr::Formal { .. } | TypeExpr::FormalWidth { .. } | TypeExpr::Value { .. } => {
                Ok(false)
            }
            TypeExpr::InlineEnum { .. } => Err(self.error(
                "E0402",
                "'is a' with an inline enumeral type is not supported",
                span,
            )),
        }
    }

    /// Whether `name` is `target` or a `like` descendant of it.
    fn type_is_a(&self, name: &str, target: &str) -> bool {
        let mut current = name;
        for _ in 0..=self.types.len() {
            if current == target {
                return true;
            }
            let Some(declared) = self.types.iter().find(|t| t.name == current) else {
                return false;
            };
            match &declared.base {
                Some(base) => current = base.as_str(),
                None => return false,
            }
        }
        false
    }

    /// Whether `name` declares an interface (clause 6.13).
    fn is_interface(&self, name: &str) -> bool {
        self.types
            .iter()
            .any(|t| t.name == name && t.kind == DeclarationKind::Interface)
    }

    /// Whether interface `name` is `target` or inherits from it (clause
    /// 6.13: an interface includes all of its `like` bases' members).
    fn interface_is_a(&self, name: &str, target: &str) -> bool {
        let mut stack = vec![name.to_owned()];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(current) = stack.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if current == target {
                return true;
            }
            if let Some(declared) = self
                .types
                .iter()
                .find(|t| t.name == current && t.kind == DeclarationKind::Interface)
            {
                for base in &declared.interface_bases {
                    stack.push(base.clone());
                }
            }
        }
        false
    }

    /// Whether the struct type `type_name` implements interface `iface`
    /// (clause 6.3): its composed `implementing` set contains the
    /// interface, or an interface that inherits it. The composed set
    /// carries the like-chain bases' and the extend blocks' interfaces,
    /// so subtypes implement their base's interfaces.
    fn implements_interface(&self, type_name: &str, iface: &str) -> bool {
        let Some(declared) = self.types.iter().find(|t| t.name == type_name) else {
            return false;
        };
        declared
            .implements
            .iter()
            .any(|implemented| *implemented == iface || self.interface_is_a(implemented, iface))
    }

    /// Evaluate `expr` with `hint` as the declared type expected of it —
    /// the inference context for an untyped `evaluate { ... }` (4.17.7).
    /// The hint is set for the call and restored on both outcomes, so a
    /// swallowed error cannot leak it into a later evaluation.
    fn eval_with_hint(
        &mut self,
        hint: Option<TypeExpr>,
        expr: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Value, Diagnostic> {
        let saved = std::mem::replace(&mut self.expected_type, hint);
        let result = self.eval(expr, frame, me);
        self.expected_type = saved;
        result
    }

    /// The declared type of an expression, when statically known: a
    /// local's declared type, a field of the enclosing instance, or a
    /// field reached through names of known instance type. Used as the
    /// inference context for an untyped `evaluate { ... }` (4.17.7).
    fn static_type_of(&self, expr: &Expr, frame: &Frame, me: Option<usize>) -> Option<TypeExpr> {
        match expr {
            Expr::Ident { name, .. } => {
                if let Some(ty) = frame.local_types.get(name) {
                    return Some(ty.clone());
                }
                if let Some(index) = me {
                    let type_name = &self.instances[index].type_name;
                    let types: &[crate::elab::ElaboratedType] = self.types;
                    return types
                        .iter()
                        .find(|t| t.name == *type_name)
                        .and_then(|t| t.fields.iter().find(|f| f.name == *name))
                        .map(|f| f.ty.clone());
                }
                None
            }
            Expr::MemberAccess { object, name, .. } => {
                let base = self.static_type_of(object, frame, me)?;
                let type_name = match base {
                    TypeExpr::Named { name, .. } => name,
                    TypeExpr::WhenQualified { value, base, .. } => when_view_name(&value, &base)?,
                    _ => return None,
                };
                let types: &[crate::elab::ElaboratedType] = self.types;
                types
                    .iter()
                    .find(|t| t.name == type_name)
                    .and_then(|t| t.fields.iter().find(|f| f.name == *name))
                    .map(|f| f.ty.clone())
            }
            _ => None,
        }
    }

    /// The parameter types of a called method, one per argument position
    /// (`None` where the parameter is untyped, the callee is a builtin,
    /// or the callee cannot be resolved — the call itself reports its
    /// own error in those cases). The inference context for an untyped
    /// `evaluate { ... }` in an argument (4.17.7).
    fn call_arg_hints(
        &self,
        callee: &Expr,
        arg_count: usize,
        frame: &Frame,
        me: Option<usize>,
    ) -> Vec<Option<TypeExpr>> {
        let mut hints: Vec<Option<TypeExpr>> = vec![None; arg_count];
        // The type the callee is a method of, and the callee's name.
        let (type_name, method_name) = match callee {
            Expr::Ident { name, .. } => {
                if crate::builtins::is_builtin_routine(name) {
                    return hints;
                }
                let Some(index) = me else {
                    return hints;
                };
                (self.instances[index].type_name.clone(), name.clone())
            }
            Expr::MemberAccess { object, name, .. } => {
                let type_name = match self.static_type_of(object, frame, me) {
                    Some(TypeExpr::Named { name, .. }) => name,
                    Some(TypeExpr::WhenQualified { value, base, .. }) => {
                        match when_view_name(&value, &base) {
                            Some(view) => view,
                            None => return hints,
                        }
                    }
                    _ => return hints,
                };
                (type_name, name.clone())
            }
            _ => return hints,
        };
        let types: &[crate::elab::ElaboratedType] = self.types;
        let Some(ty) = types.iter().find(|t| t.name == type_name) else {
            return hints;
        };
        for method in &ty.methods {
            if method.name != *method_name {
                continue;
            }
            for (position, param) in method.params.iter().enumerate().take(arg_count) {
                if hints[position].is_none() {
                    hints[position] = param.ty.clone();
                }
            }
        }
        hints
    }

    /// Whether one `in_table` row element matches the tuple element's
    /// value (clause 4.11.8): a scalar element by `==`, a set element by
    /// membership. A tuple element and a row element of incompatible
    /// kinds (integer vs string, enumerals of different types, a range
    /// against a non-integer, ...) report `E0412`.
    fn row_element_matches(
        &mut self,
        element: &crate::ast::TableElem,
        value: &Value,
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<bool, Diagnostic> {
        match element {
            crate::ast::TableElem::Scalar(expr) => {
                let other = self.eval(expr, frame, me)?;
                if !table_values_compatible(value, &other) {
                    return Err(self.error("E0412", "in_table element type mismatch", span));
                }
                Ok(matches!(
                    self.apply_binary(
                        BinaryOp::Eq,
                        value.clone(),
                        other,
                        span,
                        &Precision::unbounded(),
                    )?,
                    Value::Bool(true)
                ))
            }
            crate::ast::TableElem::Set(crate::ast::InSetExpr::Items(items)) => {
                for item in items {
                    let item_value = self.eval(item, frame, me)?;
                    if !table_values_compatible(value, &item_value) {
                        return Err(self.error("E0412", "in_table element type mismatch", span));
                    }
                    if matches!(
                        self.apply_binary(
                            BinaryOp::Eq,
                            value.clone(),
                            item_value,
                            span,
                            &Precision::unbounded(),
                        )?,
                        Value::Bool(true)
                    ) {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            crate::ast::TableElem::Set(crate::ast::InSetExpr::Range(low, high)) => {
                let Value::Int(v) = value else {
                    return Err(self.error("E0412", "in_table element type mismatch", span));
                };
                let (Value::Int(low), Value::Int(high)) =
                    (self.eval(low, frame, me)?, self.eval(high, frame, me)?)
                else {
                    return Err(self.error("E0402", "range bound must be an integer", span));
                };
                Ok(low <= *v && *v <= high)
            }
            // A general set expression: evaluate it to a set and test the
            // membership over its intervals (the item-wise shape's set
            // counterpart).
            crate::ast::TableElem::Set(crate::ast::InSetExpr::Expr(set_expr)) => {
                let container = self.eval(set_expr, frame, me)?;
                let Value::Set(intervals) = container else {
                    return Err(self.error(
                        "E0402",
                        "in_table set element does not evaluate to a set",
                        span,
                    ));
                };
                let Value::Int(v) = value else {
                    return Err(self.error("E0412", "in_table element type mismatch", span));
                };
                Ok(intervals.iter().any(|(lo, hi)| lo <= v && v <= hi))
            }
            crate::ast::TableElem::TypeSet { ty, .. } => self.type_set_contains(ty, value, span),
        }
    }

    /// Whether `value` is a member of the value set of `ty`
    /// (5.10.5/5.10.6, as an `in_table` row element): integers and
    /// booleans by index over the type's range, enumerals by their
    /// enumerand index over their own type's set. Any other pairing is
    /// `E0412`.
    fn type_set_contains(
        &self,
        ty: &TypeExpr,
        value: &Value,
        span: crate::ast::Span,
    ) -> Result<bool, Diagnostic> {
        let Some(range) = self.type_value_range(ty) else {
            return Err(self.error(
                "E0402",
                format!(
                    "set_of_values expects a numeric or enumeral type; '{}' has none",
                    ty.render()
                ),
                ty.span(),
            ));
        };
        let index = match value {
            Value::Int(v) => {
                return Ok(range.0 <= *v && *v <= range.1);
            }
            Value::Bool(b) => i128::from(*b),
            Value::Enum { type_name, name } => {
                if type_set_key(ty).as_deref() != Some(type_name.as_str()) {
                    return Err(self.error("E0412", "in_table element type mismatch", span));
                }
                let types: &[crate::elab::ElaboratedType] = self.types;
                let Some(declared) = types.iter().find(|t| t.name == *type_name) else {
                    return Err(self.error("E0412", "in_table element type mismatch", span));
                };
                match declared.enumerals.iter().position(|e| e.name == *name) {
                    Some(index) => index as i128,
                    None => return Ok(false),
                }
            }
            _ => return Err(self.error("E0412", "in_table element type mismatch", span)),
        };
        Ok(range.0 <= index && index <= range.1)
    }

    fn apply_binary(
        &mut self,
        op: BinaryOp,
        lhs: Value,
        rhs: Value,
        span: crate::ast::Span,
        precision: &Precision,
    ) -> Result<Value, Diagnostic> {
        use BinaryOp::*;
        let err = |kind: &str| {
            self.error(
                "E0402",
                format!("invalid operand ({kind}) for this operator"),
                span,
            )
        };
        // A custom numeric operand dispatches to its struct's numeric_i
        // methods (5.2.15, clause 31 — T7.3); the 5.6.3 precision
        // context does not apply to method-dispatched operators.
        if matches!(lhs, Value::Custom { .. }) || matches!(rhs, Value::Custom { .. }) {
            return self.apply_custom_binary(op, lhs, rhs, span);
        }
        match op {
            Eq | NotEq | IdentityEq | IdentityNe => {
                let equal = match (&lhs, &rhs) {
                    // The 5.6.3 context casts both operands into its
                    // width and signedness before the comparison — a
                    // ≤32-bit/64-bit unsigned context reads a negative
                    // value as its two's-complement bit pattern.
                    (Value::Int(a), Value::Int(b)) => {
                        let (a, b) = if precision.bits().is_some() {
                            (precision.cast_operand(*a), precision.cast_operand(*b))
                        } else {
                            (*a, *b)
                        };
                        a == b
                    }
                    (Value::Real(a), Value::Real(b)) => a == b,
                    (Value::Int(a), Value::Real(b)) | (Value::Real(b), Value::Int(a)) => {
                        *a as f64 == *b
                    }
                    (Value::Bool(a), Value::Bool(b)) => a == b,
                    (Value::Str(a), Value::Str(b)) => a == b,
                    (
                        Value::Enum {
                            type_name: ta,
                            name: na,
                        },
                        Value::Enum {
                            type_name: tb,
                            name: nb,
                        },
                    ) => ta == tb && na == nb,
                    (Value::Object(a), Value::Object(b)) => a == b,
                    // NULL comparisons (5.4.4.4): two null references are
                    // equal; an object never equals NULL. The regular
                    // value-vs-NULL comparison stays the type error it
                    // was.
                    (Value::Void, Value::Void) => true,
                    (Value::Object(_), Value::Void) | (Value::Void, Value::Object(_)) => false,
                    // The regular `==` on lists is item-by-item (4.11.4's
                    // contrast with `list_ptr==`).
                    (Value::List(a), Value::List(b)) => lists_equal(a, b, 0),
                    // Set equality (5.2.12): the same canonical
                    // intervals — whatever produced each set (a literal,
                    // the set algebra, or the clause 29.13 rgf_get_range
                    // query).
                    (Value::Set(a), Value::Set(b)) => a == b,
                    _ => return Err(err("comparison")),
                };
                // `===`/`!==` are treated as `==`/`!=` in this subset: value
                // identity for references is not distinguished yet.
                Ok(Value::Bool(match op {
                    Eq | IdentityEq => equal,
                    _ => !equal,
                }))
            }
            // `list_ptr==` / `!list_ptr=` (clause 4.11.4): object identity
            // of the two list operands. Both must be lists of the same
            // type — the declared element types, where both creation
            // sites recorded them, must match, and either operand that is
            // not a list reports `E0411`.
            ListPtrEq | ListPtrNe => {
                let (Value::List(a), Value::List(b)) = (&lhs, &rhs) else {
                    return Err(self.error(
                        "E0411",
                        "list_ptr== operands must be lists of the same type",
                        span,
                    ));
                };
                if let (Some(ta), Some(tb)) = (&a.element, &b.element)
                    && ta != tb
                {
                    return Err(self.error(
                        "E0411",
                        "list_ptr== operands must be lists of the same type",
                        span,
                    ));
                }
                let same = Rc::ptr_eq(&a.items, &b.items);
                Ok(Value::Bool(if op == ListPtrEq { same } else { !same }))
            }
            Lt | Le | Gt | Ge => {
                use std::cmp::Ordering::*;
                let ordering = match (&lhs, &rhs) {
                    // The context cast applies to comparisons as well
                    // (5.6.3): rule (b)'s unsigned 64-bit context makes
                    // `longint -1 < longuint 1` FALSE.
                    (Value::Int(a), Value::Int(b)) => {
                        let (a, b) = if precision.bits().is_some() {
                            (precision.cast_operand(*a), precision.cast_operand(*b))
                        } else {
                            (*a, *b)
                        };
                        a.cmp(&b)
                    }
                    (Value::Real(a), Value::Real(b)) => {
                        a.partial_cmp(b).ok_or_else(|| err("comparison"))?
                    }
                    (Value::Int(a), Value::Real(b)) => (*a as f64)
                        .partial_cmp(b)
                        .ok_or_else(|| err("comparison"))?,
                    (Value::Real(a), Value::Int(b)) => a
                        .partial_cmp(&(*b as f64))
                        .ok_or_else(|| err("comparison"))?,
                    (Value::Str(a), Value::Str(b)) => a.cmp(b),
                    _ => return Err(err("comparison")),
                };
                Ok(Value::Bool(match op {
                    Lt => ordering == Less,
                    Le => ordering != Greater,
                    Gt => ordering == Greater,
                    Ge => ordering != Less,
                    _ => unreachable!(),
                }))
            }
            Add | Sub | Mul => match (&lhs, &rhs) {
                (Value::Int(a), Value::Int(b)) => {
                    // A ≤32-bit/64-bit context computes and truncates at
                    // its width (5.6.3's rules (a)/(b)); unbounded keeps
                    // the full i128 result.
                    let (a, b) = if precision.bits().is_some() {
                        (precision.cast_operand(*a), precision.cast_operand(*b))
                    } else {
                        (*a, *b)
                    };
                    let value = match op {
                        Add => a.wrapping_add(b),
                        Sub => a.wrapping_sub(b),
                        Mul => a.wrapping_mul(b),
                        _ => unreachable!(),
                    };
                    Ok(Value::Int(precision.cast_operand(value)))
                }
                _ => {
                    let Some(a) = as_f64(&lhs) else {
                        return Err(err("arithmetic"));
                    };
                    let Some(b) = as_f64(&rhs) else {
                        return Err(err("arithmetic"));
                    };
                    let value = match op {
                        Add => a + b,
                        Sub => a - b,
                        Mul => a * b,
                        _ => unreachable!(),
                    };
                    Ok(Value::Real(value))
                }
            },
            Div | Mod => match (&lhs, &rhs) {
                (Value::Int(a), Value::Int(b)) => {
                    // The context cast lands before the operation, so a
                    // wrapped-around unsigned dividend divides as its
                    // bit pattern (5.6.3's "operands are cast").
                    let (a, b) = if precision.bits().is_some() {
                        (precision.cast_operand(*a), precision.cast_operand(*b))
                    } else {
                        (*a, *b)
                    };
                    if b == 0 {
                        return Err(self.error("E0403", "division by zero", span));
                    }
                    let value = if op == Div { a / b } else { a % b };
                    Ok(Value::Int(precision.cast_operand(value)))
                }
                _ => {
                    let Some(a) = as_f64(&lhs) else {
                        return Err(err("arithmetic"));
                    };
                    let Some(b) = as_f64(&rhs) else {
                        return Err(err("arithmetic"));
                    };
                    Ok(Value::Real(if op == Div { a / b } else { a % b }))
                }
            },
            BitAnd | BitOr | BitXor | ShiftLeft | ShiftRight => {
                let (Value::Int(a), Value::Int(b)) = (&lhs, &rhs) else {
                    return Err(err("bitwise"));
                };
                if *b < 0 || *b >= 128 {
                    return Err(self.error("E0402", "shift amount out of range", span));
                }
                let (a, b) = if precision.bits().is_some() {
                    (precision.cast_operand(*a), *b)
                } else {
                    (*a, *b)
                };
                let value = match op {
                    BitAnd => a & b,
                    BitOr => a | b,
                    BitXor => a ^ b,
                    ShiftLeft => a.wrapping_shl(b as u32),
                    ShiftRight => a.wrapping_shr(b as u32),
                    _ => unreachable!(),
                };
                Ok(Value::Int(precision.cast_operand(value)))
            }
            Match | NotMatch => {
                let (Value::Str(text), Value::Str(pattern)) = (&lhs, &rhs) else {
                    return Err(err("string match"));
                };
                // `/.../` selects AWK-style regular expression matching
                // (clause 4.12.3) — not implemented.
                if pattern.len() >= 2 && pattern.starts_with('/') && pattern.ends_with('/') {
                    return Err(self.error(
                        "E0410",
                        "AWK-style regular expression string matching is not supported",
                        span,
                    ));
                }
                // Native matching (clause 4.12.2) is case insensitive and
                // always matches the full string. Only a literal pattern —
                // letters, digits, underscore and space — is evaluated;
                // the meta-characters of the native pattern language are
                // not implemented.
                if !pattern
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ' ')
                {
                    return Err(self.error(
                        "E0410",
                        "native string matching with meta-characters is not supported",
                        span,
                    ));
                }
                let matched = text.to_lowercase() == pattern.to_lowercase();
                Ok(Value::Bool(if op == Match { matched } else { !matched }))
            }
            LogicalAnd | LogicalOr | Imply => unreachable!("handled with short-circuiting"),
        }
    }

    /// Read a subtype-qualified field (4.17.3): the accessed field must
    /// be provided by a when-subtype view of the instance's type whose
    /// when-of set contains every qualifier word, and the instance must
    /// currently match that subtype — otherwise `E0413` ("the field is
    /// not present").
    fn read_qualified_field(
        &mut self,
        index: usize,
        qualifiers: &[WhenQualifier],
        field: &str,
        field_span: crate::ast::Span,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let Some(view) = self.find_qualified_field_view(index, qualifiers, field) else {
            return Err(self.error(
                "E0401",
                format!(
                    "unknown field '{field}': no when-subtype view of the instance's type provides it under these qualifiers"
                ),
                span,
            ));
        };
        if !self.instance_matches_view(index, view) {
            return Err(self.error(
                "E0413",
                format!(
                    "field '{}' is not present: instance is not of subtype '{}'",
                    field, view.name
                ),
                span,
            ));
        }
        if self.instances[index].get(field).is_none() {
            self.materialize_qualified_field(index, view, field, field_span)?;
        }
        Ok(self.instances[index]
            .get(field)
            .cloned()
            .unwrap_or(Value::Void))
    }

    /// The elaborated subtype view a subtype-qualified access (4.17.3)
    /// resolves against: a view derived from the instance's root base
    /// type whose when-of set contains every qualifier word and whose
    /// fields include the accessed one. A qualifier word matches a
    /// (field id, value) pair by value (`red` for `red'color`) or, for a
    /// Boolean determinant's short form, by field id (`good` for
    /// `TRUE'good`); the most specific matching view (the most pairs)
    /// wins.
    fn find_qualified_field_view(
        &self,
        index: usize,
        qualifiers: &[WhenQualifier],
        field: &str,
    ) -> Option<&'a crate::elab::ElaboratedType> {
        let words: Vec<&str> = qualifiers.iter().map(|q| q.value.as_str()).collect();
        let mut best: Option<&'a crate::elab::ElaboratedType> = None;
        for view in self.types {
            if view.when_of.is_empty() {
                continue;
            }
            if !view.fields.iter().any(|f| f.name == field) {
                continue;
            }
            if !view_contains_words(view, &words) {
                continue;
            }
            if !self.instance_in_view_family(index, view) {
                continue;
            }
            if best.is_none_or(|best| view.when_of.len() > best.when_of.len()) {
                best = Some(view);
            }
        }
        best
    }

    /// Whether the instance belongs to the subtype family a view was
    /// derived from: its type is the view itself, the family's root base
    /// type, a `like` descendant of the root, or a when-qualified
    /// spelling over the root (`good packet` — a spelling whose view was
    /// not synthesized keeps the root's name as its last word).
    fn instance_in_view_family(&self, index: usize, view: &crate::elab::ElaboratedType) -> bool {
        let type_name = self.instances[index].type_name.as_str();
        if type_name == view.name {
            return true;
        }
        let Some(root) = self.when_root_base(view) else {
            return false;
        };
        type_name == root
            || self.type_is_a(type_name, &root)
            || type_name.rsplit(' ').next() == Some(root.as_str())
    }

    /// The unmarked base type a when view was derived from (the same
    /// walk the elaborator's view synthesis uses).
    fn when_root_base(&self, view: &crate::elab::ElaboratedType) -> Option<String> {
        let mut current = view;
        for _ in 0..=self.types.len() {
            if current.when_of.is_empty() {
                return Some(current.name.clone());
            }
            let base_name = current.base.as_deref()?;
            current = self.types.iter().find(|t| t.name == base_name)?;
        }
        None
    }

    /// Whether the instance currently satisfies a view's subtype
    /// (4.17.3): every determinant pair whose field id names a field of
    /// the instance holds on its value. Pairs whose id names no field
    /// (the corpus's space form keeps the struct name as the pair's id)
    /// cannot be verified and are skipped.
    fn instance_matches_view(&self, index: usize, view: &crate::elab::ElaboratedType) -> bool {
        view.when_of.iter().all(|(id, value)| {
            match self.instances[index].get(id) {
                Some(field_value) => value_holds(field_value, value),
                // No such field on the instance: the pair is
                // unverifiable here, not violated.
                None => true,
            }
        })
    }

    /// A subtype-qualified access whose subtype matched but whose field
    /// is not on the instance (the instance was created from a less
    /// specific view or from the base type): the field's presence
    /// follows from the subtype match (4.17.3 — the field is there iff
    /// the instance is of the subtype), so it materializes from the
    /// view's declaration and generates under the view's hard `keep`
    /// constraints. A constraint that cannot be evaluated against the
    /// instance (it names a field the instance does not carry) is
    /// unverifiable and does not gate the materialization.
    fn materialize_qualified_field(
        &mut self,
        index: usize,
        view: &crate::elab::ElaboratedType,
        field: &str,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        let Some(declaration) = view.fields.iter().find(|f| f.name == field) else {
            return Err(self.error("E0401", format!("unknown field '{field}'"), span));
        };
        let ty = declaration.ty.clone();
        let default = self.default_for(&ty);
        self.instances[index]
            .fields
            .push((field.to_owned(), default));
        let constraints: Vec<Expr> = view
            .constraints
            .iter()
            .filter(|constraint| !constraint.is_bind)
            .map(|constraint| constraint.expr.clone())
            .collect();
        // The same domain narrowing gen uses: a `keep x == 5` shapes the
        // sampling domain before the rejection loop.
        let domains = self.constraint_domains(&constraints, &[(field.to_owned(), ty.clone())]);
        for _ in 0..1000 {
            let value = match domains.get(field) {
                Some(domain) => self.sample_int_in(&ty, *domain),
                None => self.sample_type(&ty)?,
            };
            self.instances[index].set(field, value);
            let mut pass = true;
            for constraint in &constraints {
                // The same shapes gen skips (HDL bindings, soft and
                // select constraints) do not gate a materialization.
                if expr_calls_any(
                    constraint,
                    &["hdl_path", "delayed", "soft", "select", "edge"],
                ) {
                    continue;
                }
                match self.eval(constraint, &mut Frame::default(), Some(index)) {
                    Ok(Value::Bool(true)) => {}
                    Ok(Value::Bool(false)) => {
                        pass = false;
                        break;
                    }
                    Ok(_) => {
                        return Err(self.error(
                            "E0402",
                            "constraint did not evaluate to a boolean",
                            span,
                        ));
                    }
                    // Unverifiable here (a missing sibling field): skip.
                    Err(_) => {}
                }
            }
            if pass {
                return Ok(());
            }
        }
        Err(self.error(
            "E0407",
            format!("no solution found for the constraints of field '{field}' after 1000 attempts"),
            span,
        ))
    }

    fn read_slot(
        &mut self,
        target: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Value, Diagnostic> {
        match target {
            Expr::Ident { name, .. } => {
                if let Some(port) = name.strip_suffix('$') {
                    let port = port.to_owned();
                    return self.read_port_value(&port, me, target.span());
                }
                if let Some(value) = frame.locals.get(name) {
                    return Ok(value.clone());
                }
                if let Some(index) = me
                    && let Some(value) = self.instances[index].get(name)
                {
                    return Ok(value.clone());
                }
                // A static field named bare (T2.1) — the read side of
                // the bare-name resolution (see [`Interp::eval`]'s
                // `Expr::Ident` arm).
                if let Some(owner) = self.bare_static_field_owner(me, name)
                    && let Some(value) =
                        self.statics.get(&owner).and_then(|fields| fields.get(name))
                {
                    return Ok(value.clone());
                }
                // The root unit instance is addressable by name from any
                // method (`for each in sys.ops`).
                if name == "sys"
                    && let Some(index) = self
                        .instances
                        .iter()
                        .position(|instance| instance.type_name == "sys")
                {
                    return Ok(Value::Object(index));
                }
                Err(self.error(
                    "E0401",
                    format!("unknown identifier '{name}'"),
                    target.span(),
                ))
            }
            Expr::MemberAccess { object, name, .. } => {
                let object_value = self.eval(object, frame, me)?;
                let Value::Object(index) = object_value else {
                    return Err(self.error(
                        "E0402",
                        "member access requires an object",
                        target.span(),
                    ));
                };
                self.instances[index].get(name).cloned().ok_or_else(|| {
                    self.error("E0401", format!("unknown field '{name}'"), target.span())
                })
            }
            Expr::QualifiedField {
                base,
                qualifiers,
                field,
                field_span,
                span,
            } => {
                let object_value = self.eval(base, frame, me)?;
                let Value::Object(index) = object_value else {
                    return Err(self.error(
                        "E0402",
                        "subtype-qualified access requires an object",
                        target.span(),
                    ));
                };
                self.read_qualified_field(index, qualifiers, field, *field_span, *span)
            }
            _ => Err(self.error("E0402", "unsupported assignment target", target.span())),
        }
    }

    fn write_slot(
        &mut self,
        target: &Expr,
        value: Value,
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        match target {
            Expr::Ident { name, .. } => {
                if let Some(port) = name.strip_suffix('$') {
                    let port = port.to_owned();
                    return self.write_port_value(&port, value, me, span);
                }
                if frame.locals.contains_key(name) {
                    let declared = frame.local_types.get(name).cloned();
                    let value = self.bind_slot_value(value, declared.as_ref(), span)?;
                    frame.locals.insert(name.clone(), value);
                    return Ok(());
                }
                if let Some(index) = me {
                    // The field's declared type binds the value (T7.1's
                    // value semantics: a custom numeric copies, a
                    // scalar converts through numeric_from_*).
                    let declared = self.declared_type_of(name, false, frame, index);
                    let value = self.bind_slot_value(value, declared.as_ref(), span)?;
                    if self.instances[index].set(name, value.clone()) {
                        return Ok(());
                    }
                    // A static field named bare (T2.1): the write lands
                    // in the declaring type's store entry — the same
                    // resolution the read side uses.
                    if let Some(owner) = self.bare_static_field_owner(me, name) {
                        self.statics
                            .entry(owner)
                            .or_default()
                            .insert(name.clone(), value);
                        return Ok(());
                    }
                    return Err(self.error("E0401", format!("unknown field '{name}'"), span));
                }
                Err(self.error("E0401", format!("unknown identifier '{name}'"), span))
            }
            Expr::MemberAccess { object, name, .. } => {
                // `unit.port$ = value` — simple_port write.
                if let Some(port) = name.strip_suffix('$') {
                    let port = port.to_owned();
                    let object_value = self.eval(object, frame, me)?;
                    let Value::Object(index) = object_value else {
                        return Err(self.error(
                            "E0402",
                            "port assignment requires an object",
                            span,
                        ));
                    };
                    return self.write_port_value(&port, value, Some(index), span);
                }
                let object_value = self.eval(object, frame, me)?;
                let Value::Object(index) = object_value else {
                    return Err(self.error("E0402", "member assignment requires an object", span));
                };
                let owner_type = self.instances[index].type_name.clone();
                let declared: Option<TypeExpr> = self
                    .types
                    .iter()
                    .find(|t| t.name == owner_type)
                    .and_then(|t| t.fields.iter().find(|f| f.name == *name))
                    .map(|f| f.ty.clone());
                let value = self.bind_slot_value(value, declared.as_ref(), span)?;
                if self.instances[index].set(name, value) {
                    Ok(())
                } else {
                    Err(self.error("E0401", format!("unknown field '{name}'"), span))
                }
            }
            Expr::QualifiedField {
                base,
                qualifiers,
                field,
                field_span,
                ..
            } => {
                // The same presence rule as the read (4.17.3): the write
                // lands only if the instance currently matches the
                // qualifiers' subtype.
                let object_value = self.eval(base, frame, me)?;
                let Value::Object(index) = object_value else {
                    return Err(self.error(
                        "E0402",
                        "subtype-qualified assignment requires an object",
                        span,
                    ));
                };
                let Some(view) = self.find_qualified_field_view(index, qualifiers, field) else {
                    return Err(self.error(
                        "E0401",
                        format!(
                            "unknown field '{field}': no when-subtype view of the instance's type provides it under these qualifiers"
                        ),
                        span,
                    ));
                };
                if !self.instance_matches_view(index, view) {
                    return Err(self.error(
                        "E0413",
                        format!(
                            "field '{}' is not present: instance is not of subtype '{}'",
                            field, view.name
                        ),
                        span,
                    ));
                }
                if self.instances[index].get(field).is_none() {
                    let field_span = *field_span;
                    self.materialize_qualified_field(index, view, field, field_span)?;
                }
                let declared: Option<TypeExpr> = view
                    .fields
                    .iter()
                    .find(|f| f.name == *field)
                    .map(|f| f.ty.clone());
                let value = self.bind_slot_value(value, declared.as_ref(), span)?;
                self.instances[index].set(field, value);
                Ok(())
            }
            Expr::StaticAccess {
                type_name, member, ..
            } => {
                // `Type::field = value` (clause 4.17.4): the write lands
                // in the declaring type's store entry — the one shared
                // occurrence, visible to every instance. The member's
                // declared type still binds the value (T7.1).
                let declared: Option<TypeExpr> = self
                    .types
                    .iter()
                    .find(|t| t.name == *type_name)
                    .and_then(|t| t.fields.iter().find(|f| f.name == *member))
                    .map(|f| f.ty.clone());
                let value = self.bind_slot_value(value, declared.as_ref(), span)?;
                if let Some(fields) = self.statics.get_mut(type_name) {
                    fields.insert(member.clone(), value);
                    Ok(())
                } else {
                    Err(self.error(
                        "E0401",
                        format!("unknown static member '{type_name}::{member}'"),
                        span,
                    ))
                }
            }
            _ => Err(self.error("E0402", "unsupported assignment target", span)),
        }
    }

    /// `port$` read through the simulator bridge.
    fn read_port_value(
        &mut self,
        port: &str,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let Some(index) = me else {
            return Err(self.error("E0402", "port access requires an enclosing instance", span));
        };
        let Some(path) = self.instances[index].port_path(port).map(str::to_owned) else {
            return Err(self.error("E0401", format!("unknown port '{port}'"), span));
        };
        if self.hdl.is_none() {
            return Err(self.error("E0408", "no simulator loaded — call hdl_load first", span));
        }
        let cycle = self.cycle;
        match self.hdl.as_mut().unwrap().get(&path, cycle) {
            Ok(value) => Ok(Value::Int(value)),
            Err(msg) => Err(self.error("E0408", msg, span)),
        }
    }

    /// `port$ = value` write through the simulator bridge.
    fn write_port_value(
        &mut self,
        port: &str,
        value: Value,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        let Some(index) = me else {
            return Err(self.error("E0402", "port access requires an enclosing instance", span));
        };
        let Some(path) = self.instances[index].port_path(port).map(str::to_owned) else {
            return Err(self.error("E0401", format!("unknown port '{port}'"), span));
        };
        let Some(int_value) = as_i64(&value) else {
            return Err(self.error(
                "E0402",
                "port assignment expects an integer (or boolean) value",
                span,
            ));
        };
        if self.hdl.is_none() {
            return Err(self.error("E0408", "no simulator loaded — call hdl_load first", span));
        }
        let cycle = self.cycle;
        match self.hdl.as_mut().unwrap().set(&path, int_value, cycle) {
            Ok(()) => Ok(()),
            Err(msg) => Err(self.error("E0408", msg, span)),
        }
    }

    fn format_value(&self, value: &Value) -> String {
        self.format_value_bounded(value, 0)
    }

    /// Depth-bounded rendering: `MAX_VALUE_DEPTH` is far above any legal
    /// nesting, so hitting it means a value cycle (a list stored inside
    /// itself) — rendered as a placeholder instead of overflowing the
    /// stack.
    fn format_value_bounded(&self, value: &Value, depth: usize) -> String {
        if depth > MAX_VALUE_DEPTH {
            return "...".to_owned();
        }
        match value {
            Value::Int(i) => i.to_string(),
            Value::Real(f) => {
                if f.is_finite() && f.fract() == 0.0 {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
            Value::Str(s) => s.clone(),
            Value::Enum { name, .. } => name.clone(),
            Value::List(list) => {
                let items = list.items.borrow();
                let parts: Vec<String> = items
                    .iter()
                    .map(|item| self.format_value_bounded(item, depth + 1))
                    .collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Set(intervals) => render_set(intervals),
            // A custom numeric value renders through its struct's
            // `numeric_to_string(dec)` on the checked message paths
            // (`out`, `print`, `append` — `format_values_checked`).
            // This infallible renderer is the fallback for contexts
            // that cannot report an E0418 (nested struct-field
            // rendering): it prints the implementing-struct instance,
            // the same shape an `as_a` back to the struct shows.
            Value::Custom { instance, .. } => {
                let instance = &self.instances[*instance];
                let parts: Vec<String> = instance
                    .fields
                    .iter()
                    .map(|(name, value)| {
                        format!("{name}: {}", self.format_value_bounded(value, depth + 1))
                    })
                    .collect();
                format!("{}{{{}}}", instance.type_name, parts.join(", "))
            }
            Value::Object(index) => {
                let instance = &self.instances[*index];
                let parts: Vec<String> = instance
                    .fields
                    .iter()
                    .map(|(name, value)| {
                        format!("{name}: {}", self.format_value_bounded(value, depth + 1))
                    })
                    .collect();
                format!("{}{{{}}}", instance.type_name, parts.join(", "))
            }
            // A port handle renders by its member name (clause 9); the
            // port's value is the `name$` bridge access, not this.
            Value::Port { name, .. } => format!("port {name}"),
            // A table (32.3) renders by its shape; the rows read through
            // `get_row(i).get_data(c)`.
            Value::Table(table) => {
                let table = table.borrow();
                format!(
                    "table(columns: {}, rows: {})",
                    table.columns.len(),
                    table.rows.len()
                )
            }
            Value::TableRow { row, .. } => {
                let row = row.borrow();
                let cells: Vec<String> = row.cells.clone();
                format!("[{}]", cells.join(", "))
            }
            // An instance iterator (32.2, T11.2) renders by its walk
            // state; a path item by its field and index chain.
            Value::InstanceIterator(handle) => {
                let state = handle.borrow();
                if !state.started {
                    "instance_iterator(not started)".to_owned()
                } else {
                    match state.cursor {
                        Some(cursor) => format!(
                            "instance_iterator({} of {})",
                            cursor + 1,
                            state.visits.len()
                        ),
                        None => "instance_iterator(finished)".to_owned(),
                    }
                }
            }
            Value::InstancePathItem(item) => {
                let mut rendered = item.field.clone().unwrap_or_else(|| "<root>".to_owned());
                for index in &item.list_indices {
                    rendered.push_str(&format!("[{index}]"));
                }
                format!("instance_path_item({rendered})")
            }
            // A clause 33 sync object renders by its struct name — the
            // state is identity, not value.
            Value::Sync(sync) => sync.borrow().kind.name().to_owned(),
            // Clause 5.4.4.4: `Value::Void` doubles as the null
            // reference — an interface-typed variable with no instance
            // prints NULL (T3.5). A struct-typed interface value prints
            // the underlying instance, as any object does above.
            Value::Void => "NULL".to_owned(),
        }
    }

    fn value_kind(&self, value: &Value) -> &'static str {
        match value {
            Value::Int(_) | Value::Real(_) => "a number",
            Value::Bool(_) => "a boolean",
            Value::Str(_) => "a string",
            Value::Enum { .. } => "an enumeral",
            Value::List(_) => "a list",
            Value::Set(_) => "a set",
            Value::Object(_) => "an object",
            Value::Port { .. } => "a port",
            Value::Table(_) => "a table",
            Value::TableRow { .. } => "a table row",
            Value::Custom { .. } => "a custom numeric value",
            Value::InstanceIterator(_) => "an instance iterator",
            Value::InstancePathItem(_) => "an instance path item",
            Value::Sync(_) => "a resource-sharing object",
            Value::Void => "void",
        }
    }

    fn error(
        &self,
        code: &'static str,
        message: impl Into<String>,
        span: crate::ast::Span,
    ) -> Diagnostic {
        Diagnostic::new(code, message, span.0, span.1)
    }
}

/// The `get_current_path_string()` rendering (32.2, T11.2): the root's
/// label, then `.field` per step, each list step's index chain
/// appended (`sys.a`, `sys.lst[0]`, `sys.grid[0][2]`) — the `e_path`
/// shapes. The root step itself renders as the label alone.
fn render_iterator_path(path: &[InstancePathItem], root_label: &str) -> String {
    let mut rendered = String::new();
    for (position, item) in path.iter().enumerate() {
        if position == 0 {
            rendered.push_str(root_label);
            continue;
        }
        rendered.push('.');
        rendered.push_str(item.field.as_deref().unwrap_or(""));
        for index in &item.list_indices {
            rendered.push_str(&format!("[{index}]"));
        }
    }
    rendered
}

/// Bits of a width specification: the literal value, or the given default
/// when the width is absent or not a literal. Capped at 64.
/// A 1-based table column index against a width (the number of columns
/// or cells): validated per 32.3's numbering — columns start at 1,
/// column 0 is the row-index pseudo-column and is never a cell here.
fn column_index(width: usize, column: i128, span: crate::ast::Span) -> Result<usize, Diagnostic> {
    if column < 1 || column > width as i128 {
        return Err(Diagnostic::new(
            "E0404",
            format!("table column index {column} out of range (1..{width})"),
            span.0,
            span.1,
        ));
    }
    Ok(column as usize)
}

/// Insert a row handle at the 0-based position (shifting the rest down;
/// `position == rows.len()` appends).
fn insert_row_at(
    table: &mut crate::tables::TableValue,
    position: i128,
    row: Rc<RefCell<crate::tables::TableRowValue>>,
    span: crate::ast::Span,
) -> Result<(), Diagnostic> {
    let rows_len = table.rows.len();
    if position < 0 || position > rows_len as i128 {
        return Err(Diagnostic::new(
            "E0404",
            format!("table row insert position {position} out of range (0..={rows_len})"),
            span.0,
            span.1,
        ));
    }
    table.rows.insert(position as usize, row);
    Ok(())
}

fn width_bits(width: &Option<Box<Expr>>, default_bits: u32) -> u32 {
    match width.as_deref() {
        Some(Expr::IntLiteral { text, .. }) => parse_int_literal(text)
            .map(|v| v.clamp(1, 64) as u32)
            .unwrap_or(default_bits),
        _ => default_bits,
    }
}

/// Narrow a field's sampling domain to the intersection with one more
/// constraint shape's bounds. Unbounded so far means the full integer
/// range.
fn intersect_domain(
    domains: &mut BTreeMap<String, (i128, i128)>,
    name: &str,
    bounds: (i128, i128),
) {
    let entry = domains
        .entry(name.to_owned())
        .or_insert((i128::MIN, i128::MAX));
    entry.0 = entry.0.max(bounds.0);
    entry.1 = entry.1.min(bounds.1);
}

/// Integer view of a value for the `hdl_*` routines: integers pass
/// through, booleans become 0/1 (handy for clock and reset pins).
/// Stable type key for an inline enumeral list.
fn inline_enum_key(names: &[String]) -> String {
    format!("[{}]", names.join(","))
}

/// The source-like rendering of one recorded attachment, as the clause
/// 35.4 `get_all_annotations()` list element: `@name` or
/// `@name(attr = value, ...)` with each attribute value compactly
/// rendered as written (a string literal keeps its quotes). This is
/// the annotation's documented value representation — a string, not a
/// reflective object.
fn render_attached_annotation(annotation: &crate::elab::AttachedAnnotation) -> String {
    if annotation.attrs.is_empty() {
        return format!("@{}", annotation.type_name);
    }
    let attrs = annotation
        .attrs
        .iter()
        .map(|(attr, value)| format!("{attr} = {}", crate::ast::render_expr_compact(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("@{}({})", annotation.type_name, attrs)
}

fn as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Int(i) => i64::try_from(*i).ok(),
        Value::Bool(b) => Some(i64::from(*b)),
        _ => None,
    }
}

/// The synthesized when-subtype view's name for a when-qualified type
/// (6.7): the qualifying values join space-separated in written order
/// before the base type's name — `LO HI pkt` for the chained spelling
/// `LO HI pkt`, matching the elaborator's synthesized
/// [`crate::elab::ElaboratedType`] entry. `None` when the chain does
/// not bottom out in a named base type.
pub(crate) fn when_view_name(value: &str, base: &TypeExpr) -> Option<String> {
    let mut name = String::from(value);
    let mut base = base;
    loop {
        match base {
            TypeExpr::WhenQualified {
                value: inner_value,
                base: inner_base,
                ..
            } => {
                name.push(' ');
                name.push_str(inner_value);
                base = inner_base;
            }
            TypeExpr::Named {
                name: base_name, ..
            } => {
                name.push(' ');
                name.push_str(base_name);
                return Some(name);
            }
            _ => return None,
        }
    }
}

/// The qualifier words of a when-qualified type spelling, in written
/// order (`["LO", "HI"]` for `LO HI pkt`).
pub(crate) fn when_spelling_words(value: &str, base: &TypeExpr) -> Vec<String> {
    let mut words = vec![value.to_owned()];
    let mut base = base;
    while let TypeExpr::WhenQualified {
        value: inner_value,
        base: inner_base,
        ..
    } = base
    {
        words.push(inner_value.clone());
        base = inner_base;
    }
    words
}

/// The named base type a when-qualified spelling bottoms out in.
fn when_view_base(base: &TypeExpr) -> Option<String> {
    match base {
        TypeExpr::Named { name, .. } => Some(name.clone()),
        TypeExpr::WhenQualified { base, .. } => when_view_base(base),
        _ => None,
    }
}

/// Whether a view's when-of set contains every qualifier word (4.17.3's
/// legality reading): each word denotes a (field id, value) determinant
/// of the view — by value (`red` for `red'color`, `TRUE` for
/// `TRUE'good`) or by field id (a determinant's full or short spelling,
/// `color` for `red'color`, `good` for `TRUE'good`). Several words may
/// denote the same determinant (the full `TRUE'good` spelling of the
/// short `good`), and qualifiers already implied by the static type are
/// simply absent — so the words only need to cover a subset of the
/// view's determinants; the subtype check below verifies the rest.
fn view_contains_words(view: &crate::elab::ElaboratedType, words: &[&str]) -> bool {
    words.iter().all(|word| {
        view.when_of
            .iter()
            .any(|(id, value)| value == word || id == word)
    })
}

/// Whether a determinant value word holds on a field value: an
/// enumerand by name, a Boolean by its TRUE/FALSE word, a number by its
/// decimal text.
fn value_holds(value: &Value, word: &str) -> bool {
    match value {
        Value::Bool(b) => word.eq_ignore_ascii_case("true") == *b,
        Value::Enum { name, .. } => name == word,
        Value::Int(i) => word.parse::<i128>().map(|w| w == *i).unwrap_or(false),
        _ => false,
    }
}

/// The MAX_UINT predefined constant (the 28.5 guards' bound).
const MAX_UINT_VALUE: i128 = (1i128 << 32) - 1;

/// The predefined named numeric constants resolvable as ordinary
/// expression identifiers: MAX_UINT (2^32-1), MAX_INT (2^31-1) and
/// MIN_INT (-2^31). The select-choice evaluator resolves the same names
/// over its recorded tokens.
fn predefined_constant(name: &str) -> Option<i128> {
    match name {
        "MAX_UINT" => Some((1i128 << 32) - 1),
        "MAX_INT" => Some((1i128 << 31) - 1),
        "MIN_INT" => Some(-(1i128 << 31)),
        _ => None,
    }
}

/// The inclusive value range of a scalar type from its base and width.
fn scalar_range(base: ScalarBase, width: &Option<Box<Expr>>) -> Option<(i128, i128)> {
    match base {
        ScalarBase::Uint => {
            let bits = width_bits(width, 32);
            Some((0, (1i128 << bits) - 1))
        }
        ScalarBase::Int => {
            let bits = width_bits(width, 32);
            Some((-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1))
        }
        // The long bases are 64-bit wide (Table 17, clause 5.2.3.1).
        ScalarBase::Longuint => Some((0, (1i128 << 64) - 1)),
        ScalarBase::Longint => Some((-(1i128 << 63), (1i128 << 63) - 1)),
        ScalarBase::Bit => Some((0, 1)),
        ScalarBase::Byte => Some((0, 255)),
        ScalarBase::Nibble => Some((0, 15)),
        ScalarBase::Bool => Some((0, 1)),
        ScalarBase::Real | ScalarBase::String | ScalarBase::Time => None,
    }
}

/// The arithmetic precision context of one binary operator or comparison
/// (clause 5.6.3): the rule reads the operands' *static* types and picks
/// the width and signedness the operation computes at.
///
/// The consequence the plan's test exercises: `longint -1 < longuint 1`
/// is FALSE — rule (b)'s context is unsigned because one operand is
/// `longuint`, so the -1 casts to MAX_LONGUINT — while
/// `int(bits:64) -1 < uint(bits:64) 1` is TRUE, because rule (c)
/// computes unbounded and -1 stays -1. Before this context existed the
/// interpreter computed every integer operation on untruncated i128
/// values; the unbounded mode keeps exactly that behavior, so only the
/// ≤32-bit and 64-bit modes change results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Precision {
    /// (a) every operand type is at most 32 bits: compute in 32 bits,
    /// unsigned when any operand is unsigned.
    Bits32 { unsigned: bool },
    /// (b) some operand is `longint`/`longuint` and every other operand
    /// is a ≤32-bit scalar or a long type: compute in 64 bits, unsigned
    /// when any operand is `longuint`.
    Bits64 { unsigned: bool },
    /// (c) some operand other than the long types is wider than 32 bits
    /// (`int(bits:64)`, `uint(bits:64)`, `int(bits:*)`): unbounded
    /// precision, operands sign/zero-extended — the i128 path,
    /// untruncated.
    Unbounded,
    /// (d) some operand is `real`: double precision (the f64 path).
    Real,
}

impl Precision {
    /// The context for sites with no expression-level type information
    /// (set membership checks, constraint atoms): the unbounded mode —
    /// the behavior those sites always had.
    fn unbounded() -> Self {
        Precision::Unbounded
    }

    /// The width this context computes at (`None` for the unbounded and
    /// real modes, where values are not wrapped).
    fn bits(self) -> Option<u32> {
        match self {
            Precision::Bits32 { .. } => Some(32),
            Precision::Bits64 { .. } => Some(64),
            Precision::Unbounded | Precision::Real => None,
        }
    }

    fn unsigned(self) -> bool {
        match self {
            Precision::Bits32 { unsigned } | Precision::Bits64 { unsigned } => unsigned,
            Precision::Unbounded | Precision::Real => false,
        }
    }

    /// Cast one operand into the context: truncate to the width and
    /// interpret it signed or unsigned (5.6.3's "operands are cast").
    /// The unbounded and real modes keep the value as is.
    fn cast_operand(self, value: i128) -> i128 {
        match self.bits() {
            None => value,
            Some(bits) => wrap_to_width(value, bits, self.unsigned()),
        }
    }
}

/// One operand's numeric classification for the 5.6.3 context rules:
/// its bit width and signedness, or a non-integer shape.
#[derive(Debug, Clone, Copy)]
enum OperandClass {
    Real,
    /// A Table 17 long type: 64 bits, signed or unsigned.
    Long {
        signed: bool,
    },
    /// An `int(bits: *)`-style unbounded-width integer (5.2.3.1).
    Unbounded,
    /// Any other scalar integer type with its width and signedness. The
    /// predefined widths are Table 17's: `bit` 1 unsigned, `nibble` 4
    /// unsigned, `byte` 8 signed, `time` 32 unsigned, `int`/`uint` 32.
    Int {
        bits: u32,
        signed: bool,
    },
    /// No static type (a literal, a call result, an operator result):
    /// the rule's literal shapes are 32-bit signed decimal, so an
    /// unknown operand never widens or unsigns the context — it takes
    /// the context the other operand (or the fallback) selects.
    Unknown,
}

fn operand_class(ty: Option<&TypeExpr>) -> OperandClass {
    let Some(TypeExpr::Scalar { base, width, .. }) = ty else {
        return OperandClass::Unknown;
    };
    match base {
        ScalarBase::Real => OperandClass::Real,
        ScalarBase::Longint => OperandClass::Long { signed: true },
        ScalarBase::Longuint => OperandClass::Long { signed: false },
        // The star width (the `int(bits: *)` marker the parser records)
        // names the unbounded-width integer: rule (c)'s wide shape.
        _ if is_star_width(width) => OperandClass::Unbounded,
        ScalarBase::Int | ScalarBase::Uint => OperandClass::Int {
            bits: width_bits(width, 32),
            signed: *base == ScalarBase::Int,
        },
        ScalarBase::Bit => OperandClass::Int {
            bits: 1,
            signed: false,
        },
        ScalarBase::Byte => OperandClass::Int {
            bits: 8,
            signed: true,
        },
        ScalarBase::Nibble => OperandClass::Int {
            bits: 4,
            signed: false,
        },
        ScalarBase::Time => OperandClass::Int {
            bits: 32,
            signed: false,
        },
        // A `bool` or `string` operand is not an arithmetic context
        // member; the evaluator reports its own operand error for those.
        ScalarBase::Bool | ScalarBase::String => OperandClass::Unknown,
    }
}

/// The parser's `int(bits: *)` marker width.
fn is_star_width(width: &Option<Box<Expr>>) -> bool {
    matches!(
        width.as_deref(),
        Some(Expr::Ident { name, .. }) if name == "*"
    )
}

/// The 5.6.3 context type set from the operands' static types — the
/// four rules (a)–(d), tried in order.
fn precision_context(lhs: Option<&TypeExpr>, rhs: Option<&TypeExpr>) -> Precision {
    use OperandClass::*;
    let (l, r) = (operand_class(lhs), operand_class(rhs));
    // No operand carries a static type (two literals, call results,
    // member chains the type walk cannot resolve): no context type set
    // exists — the unbounded path the interpreter ran before 5.6.3's
    // sizing. A typed operand next to an unknown one keeps its own
    // context: the unknown side is the literal shape the rules read.
    if matches!(l, Unknown) && matches!(r, Unknown) {
        return Precision::Unbounded;
    }
    // (d) any real operand: double precision.
    if matches!(l, Real) || matches!(r, Real) {
        return Precision::Real;
    }
    // (b) any longint/longuint, the rest ≤ 32 bits or long: 64 bits,
    // unsigned if any longuint.
    if matches!(l, Long { .. }) || matches!(r, Long { .. }) {
        let unsigned = matches!(l, Long { signed: false }) || matches!(r, Long { signed: false });
        return Precision::Bits64 { unsigned };
    }
    // (c) any other type wider than 32 bits (`int(bits:64)`,
    // `uint(bits:64)`, `int(bits:*)`): unbounded.
    let wide =
        |c: &OperandClass| matches!(c, Unbounded) || matches!(c, Int { bits, .. } if *bits > 32);
    if wide(&l) || wide(&r) {
        return Precision::Unbounded;
    }
    // (a) every type is ≤ 32 bits: 32-bit compute, unsigned if any
    // operand is unsigned.
    let unsigned = matches!(l, Int { signed: false, .. }) || matches!(r, Int { signed: false, .. });
    Precision::Bits32 { unsigned }
}

/// Truncate `value` to `bits`, interpreted signed or unsigned — the
/// wrap-around the 32- and 64-bit contexts of rules (a) and (b) compute
/// at (clause 5.6.3).
fn wrap_to_width(value: i128, bits: u32, unsigned: bool) -> i128 {
    if bits == 0 || bits >= 127 {
        return value;
    }
    let modulus = 1i128 << bits;
    let wrapped = value.rem_euclid(modulus);
    if unsigned {
        wrapped
    } else {
        let half = 1i128 << (bits - 1);
        if wrapped >= half {
            wrapped - modulus
        } else {
            wrapped
        }
    }
}

/// Intersection of two canonical interval sets (sorted, disjoint): a
/// two-pointer sweep; the result is canonical again.
fn intersect_interval_sets(a: &[(i128, i128)], b: &[(i128, i128)]) -> Vec<(i128, i128)> {
    let mut result = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < a.len() && j < b.len() {
        let low = a[i].0.max(b[j].0);
        let high = a[i].1.min(b[j].1);
        if low <= high {
            result.push((low, high));
        }
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    result
}

/// Union of two canonical interval sets: concatenation plus the
/// canonicalization merge.
fn union_interval_sets(a: &[(i128, i128)], b: &[(i128, i128)]) -> Vec<(i128, i128)> {
    let mut all = a.to_vec();
    all.extend_from_slice(b);
    canonical_set(all)
}

/// The complement of a canonical interval set within `domain` (the
/// field type's full range): the gaps between the set's intervals.
fn complement_interval_set(set: &[(i128, i128)], domain: (i128, i128)) -> Vec<(i128, i128)> {
    let mut result = Vec::new();
    let mut cursor = domain.0;
    for &(low, high) in set {
        if high < cursor || low > domain.1 {
            continue;
        }
        if low > cursor {
            result.push((cursor, low.saturating_sub(1)));
        }
        match high.checked_add(1) {
            Some(next) if next <= domain.1 => cursor = next.max(cursor),
            _ => return result,
        }
    }
    if cursor <= domain.1 {
        result.push((cursor, domain.1));
    }
    result
}

/// The rgf fields a constraint expression references (10.3.13's split):
/// bare identifiers naming an rgf field of the generated type, and
/// `me`/`self`-rooted paths ending at one. Deeper member paths belong
/// to nested instances and are not this generation scope's rgf
/// references. The walk covers the shapes a constraint can combine;
/// nodes it does not model (a `select` distribution, `evaluate`, `new`,
/// table tests) do not descend — a reference hidden inside one is
/// caught by the shape check's atom rules instead.
fn rgf_field_refs(expr: &Expr, rgf: &BTreeSet<String>) -> Vec<String> {
    match expr {
        Expr::Ident { name, .. } => {
            if rgf.contains(name) {
                vec![name.clone()]
            } else {
                Vec::new()
            }
        }
        Expr::MemberAccess { object, name, .. } => {
            let mut refs = rgf_field_refs(object, rgf);
            if matches!(
                object.as_ref(),
                Expr::Ident { name: root, .. } if root == "me" || root == "self"
            ) && rgf.contains(name)
            {
                refs.push(name.clone());
            }
            refs
        }
        Expr::Paren { expr, .. } => rgf_field_refs(expr, rgf),
        Expr::Unary { expr, .. } => rgf_field_refs(expr, rgf),
        Expr::Binary { lhs, rhs, .. } => {
            let mut refs = rgf_field_refs(lhs, rgf);
            refs.extend(rgf_field_refs(rhs, rgf));
            refs
        }
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            let mut refs = rgf_field_refs(cond, rgf);
            refs.extend(rgf_field_refs(then_expr, rgf));
            refs.extend(rgf_field_refs(else_expr, rgf));
            refs
        }
        Expr::InSet { element, set, .. } => {
            let mut refs = rgf_field_refs(element, rgf);
            refs.extend(rgf_inset_refs(set, rgf));
            refs
        }
        Expr::Call { callee, args, .. } => {
            let mut refs = rgf_field_refs(callee, rgf);
            for arg in args {
                refs.extend(rgf_field_refs(arg, rgf));
            }
            refs
        }
        Expr::Index { base, index, .. } => {
            let mut refs = rgf_field_refs(base, rgf);
            refs.extend(rgf_field_refs(index, rgf));
            refs
        }
        Expr::CastAs { expr, .. } => rgf_field_refs(expr, rgf),
        Expr::BitSlice {
            base, high, low, ..
        } => {
            let mut refs = rgf_field_refs(base, rgf);
            refs.extend(rgf_field_refs(high, rgf));
            refs.extend(rgf_field_refs(low, rgf));
            refs
        }
        Expr::ListConcat { items, .. } => {
            let mut refs = Vec::new();
            for item in items {
                refs.extend(rgf_field_refs(item, rgf));
            }
            refs
        }
        Expr::SetLiteral { items, .. } => rgf_inset_refs(items, rgf),
        _ => Vec::new(),
    }
}

/// The rgf references of an `in` right side (bounds, items or a general
/// set expression).
fn rgf_inset_refs(set: &crate::ast::InSetExpr, rgf: &BTreeSet<String>) -> Vec<String> {
    match set {
        crate::ast::InSetExpr::Items(items) => {
            let mut refs = Vec::new();
            for item in items {
                refs.extend(rgf_field_refs(item, rgf));
            }
            refs
        }
        crate::ast::InSetExpr::Range(low, high) => {
            let mut refs = rgf_field_refs(low, rgf);
            refs.extend(rgf_field_refs(high, rgf));
            refs
        }
        crate::ast::InSetExpr::Expr(expr) => rgf_field_refs(expr, rgf),
    }
}

/// Whether the expression is a bare reference to rgf field `field`: the
/// identifier itself (parentheses unwrapped) or a `me`/`self`-rooted
/// path ending at the field.
fn rgf_bare_ref(expr: &Expr, field: &str) -> bool {
    match expr {
        Expr::Ident { name, .. } => name == field,
        Expr::Paren { expr, .. } => rgf_bare_ref(expr, field),
        Expr::MemberAccess { object, name, .. } => {
            matches!(
                object.as_ref(),
                Expr::Ident { name: root, .. } if root == "me" || root == "self"
            ) && name == field
        }
        _ => false,
    }
}

/// The `(element, set)` of an `in` membership, parentheses unwrapped —
/// the `not in` spelling wraps the membership in a unary `not`.
fn rgf_inset_parts(expr: &Expr) -> Option<(&Expr, &crate::ast::InSetExpr)> {
    match expr {
        Expr::InSet { element, set, .. } => Some((element, set)),
        Expr::Paren { expr, .. } => rgf_inset_parts(expr),
        _ => None,
    }
}

/// Whether the non-rgf operand side of a supported atom is evaluable to
/// a constant: no rgf references, no `select` distribution (10.3.12.3's
/// sampler does not reduce a set) and no generative bitwise operators.
fn rgf_exp_side_ok(expr: &Expr, rgf: &BTreeSet<String>) -> bool {
    rgf_field_refs(expr, rgf).is_empty() && !rgf_exp_generative(expr)
}

/// Whether the expression contains a `select` distribution or a
/// generative bitwise operator anywhere in its expression tree.
fn rgf_exp_generative(expr: &Expr) -> bool {
    match expr {
        Expr::Select { .. } => true,
        Expr::Unary {
            op: UnaryOp::BitNot,
            ..
        } => true,
        Expr::Binary {
            op:
                BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight,
            ..
        } => true,
        Expr::Paren { expr, .. } => rgf_exp_generative(expr),
        Expr::Unary { expr, .. } => rgf_exp_generative(expr),
        Expr::Binary { lhs, rhs, .. } => rgf_exp_generative(lhs) || rgf_exp_generative(rhs),
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rgf_exp_generative(cond)
                || rgf_exp_generative(then_expr)
                || rgf_exp_generative(else_expr)
        }
        Expr::InSet { element, set, .. } => {
            rgf_exp_generative(element)
                || match set {
                    crate::ast::InSetExpr::Items(items) => items.iter().any(rgf_exp_generative),
                    crate::ast::InSetExpr::Range(low, high) => {
                        rgf_exp_generative(low) || rgf_exp_generative(high)
                    }
                    crate::ast::InSetExpr::Expr(expr) => rgf_exp_generative(expr),
                }
        }
        Expr::Call { callee, args, .. } => {
            rgf_exp_generative(callee) || args.iter().any(rgf_exp_generative)
        }
        Expr::Index { base, index, .. } => rgf_exp_generative(base) || rgf_exp_generative(index),
        Expr::CastAs { expr, .. } => rgf_exp_generative(expr),
        Expr::BitSlice {
            base, high, low, ..
        } => rgf_exp_generative(base) || rgf_exp_generative(high) || rgf_exp_generative(low),
        Expr::ListConcat { items, .. } => items.iter().any(rgf_exp_generative),
        Expr::SetLiteral { items, .. } => match items {
            crate::ast::InSetExpr::Items(items) => items.iter().any(rgf_exp_generative),
            crate::ast::InSetExpr::Range(low, high) => {
                rgf_exp_generative(low) || rgf_exp_generative(high)
            }
            crate::ast::InSetExpr::Expr(expr) => rgf_exp_generative(expr),
        },
        _ => false,
    }
}

/// The first involved rgf field in declaration order — the field an
/// E0317 names.
fn rgf_first_in(order: &[String], refs: &[String]) -> String {
    order
        .iter()
        .find(|name| refs.contains(name))
        .cloned()
        .unwrap_or_else(|| refs.first().cloned().unwrap_or_default())
}

/// The supported reduced-set constraint shapes (10.3.13), checked before
/// generation: `keep [soft] [bool-exp bool-op] rgf-exp rel-op exp
/// [bool-op bool-exp]` with `bool-op ∈ {&&, ||, =>}` and `rel-op ∈
/// {==, !=, in, not in, >, >=, <, <=}`, where the rgf side is a bare
/// field reference, the other side contains no rgf references, no
/// `select` and no generative bitwise operators. At most one rgf atom
/// appears in the whole constraint: two atoms naming different fields
/// would couple the fields' sets, which the per-field pass cannot
/// split (`a > 3 && b < 10` is E0317; two atoms for the same field
/// intersect or union soundly and stay supported). `Err` carries the
/// first involved rgf field in declaration order, for the E0317
/// message.
fn rgf_constraint_shape(
    constraint: &Expr,
    rgf: &BTreeSet<String>,
    order: &[String],
) -> Result<(), String> {
    rgf_shape_node(constraint, rgf, order).map(|_| ())
}

/// The field whose single rgf atom the (already shape-checked)
/// constraint carries, or `None` when it is rgf-free.
fn rgf_constraint_atom_field(
    constraint: &Expr,
    rgf: &BTreeSet<String>,
    order: &[String],
) -> Result<Option<String>, String> {
    rgf_shape_node(constraint, rgf, order)
}

/// One shape-walk outcome: rgf-free (`Ok(None)`), the one field the
/// walk's rgf atom constrains (`Ok(Some(field))`), or unsupported
/// (`Err(field)`).
fn rgf_shape_node(
    expr: &Expr,
    rgf: &BTreeSet<String>,
    order: &[String],
) -> Result<Option<String>, String> {
    match expr {
        Expr::Paren { expr, .. } => rgf_shape_node(expr, rgf, order),
        Expr::Binary {
            op: BinaryOp::LogicalAnd | BinaryOp::LogicalOr,
            lhs,
            rhs,
            ..
        } => {
            let left = rgf_shape_node(lhs, rgf, order)?;
            let right = rgf_shape_node(rhs, rgf, order)?;
            match (left, right) {
                (None, right) => Ok(right),
                (left, None) => Ok(left),
                (Some(left), Some(right)) => {
                    if left == right {
                        Ok(Some(left))
                    } else {
                        Err(rgf_first_in(order, &[left, right]))
                    }
                }
            }
        }
        // The guard of an implication is a bool-exp: rgf-free.
        Expr::Binary {
            op: BinaryOp::Imply,
            lhs,
            rhs,
            ..
        } => {
            let guard_refs = rgf_field_refs(lhs, rgf);
            if !guard_refs.is_empty() {
                return Err(rgf_first_in(order, &guard_refs));
            }
            rgf_shape_node(rhs, rgf, order)
        }
        Expr::Binary {
            op:
                BinaryOp::Eq
                | BinaryOp::NotEq
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge,
            lhs,
            rhs,
            ..
        } => {
            let left_refs = rgf_field_refs(lhs, rgf);
            let right_refs = rgf_field_refs(rhs, rgf);
            match (left_refs.is_empty(), right_refs.is_empty()) {
                (true, true) => Ok(None),
                (false, true) => rgf_atom_side(lhs, &left_refs, rhs, rgf, order),
                (true, false) => rgf_atom_side(rhs, &right_refs, lhs, rgf, order),
                (false, false) => {
                    let mut refs = left_refs;
                    refs.extend(right_refs);
                    Err(rgf_first_in(order, &refs))
                }
            }
        }
        // The `not in` spelling (and `!(f in set)`, parentheses
        // unwrapped): the same atom rules as `in`.
        Expr::Unary {
            op: UnaryOp::Not,
            expr: inner,
            ..
        } => {
            if let Some((element, set)) = rgf_inset_parts(inner) {
                let element_refs = rgf_field_refs(element, rgf);
                let set_refs = rgf_inset_refs(set, rgf);
                match element_refs.len() {
                    0 => {
                        if set_refs.is_empty() && !rgf_exp_generative_expr(set) {
                            Ok(None)
                        } else {
                            Err(rgf_first_in(order, &set_refs))
                        }
                    }
                    1 if rgf_bare_ref(element, &element_refs[0]) => {
                        if set_refs.is_empty() && !rgf_exp_generative_expr(set) {
                            Ok(Some(element_refs[0].clone()))
                        } else {
                            let mut refs = element_refs;
                            refs.extend(set_refs);
                            Err(rgf_first_in(order, &refs))
                        }
                    }
                    _ => Err(rgf_first_in(order, &element_refs)),
                }
            } else {
                let refs = rgf_field_refs(inner, rgf);
                if refs.is_empty() {
                    Ok(None)
                } else {
                    Err(rgf_first_in(order, &refs))
                }
            }
        }
        Expr::InSet { element, set, .. } => {
            let element_refs = rgf_field_refs(element, rgf);
            let set_refs = rgf_inset_refs(set, rgf);
            match element_refs.len() {
                0 => {
                    if set_refs.is_empty() && !rgf_exp_generative_expr(set) {
                        Ok(None)
                    } else {
                        Err(rgf_first_in(order, &set_refs))
                    }
                }
                1 if rgf_bare_ref(element, &element_refs[0]) => {
                    if set_refs.is_empty() && !rgf_exp_generative_expr(set) {
                        Ok(Some(element_refs[0].clone()))
                    } else {
                        let mut refs = element_refs;
                        refs.extend(set_refs);
                        Err(rgf_first_in(order, &refs))
                    }
                }
                _ => Err(rgf_first_in(order, &element_refs)),
            }
        }
        _ => {
            let refs = rgf_field_refs(expr, rgf);
            if refs.is_empty() {
                Ok(None)
            } else {
                Err(rgf_first_in(order, &refs))
            }
        }
    }
}

/// The generative check over an `in` right side (bounds, items or a
/// general set expression).
fn rgf_exp_generative_expr(set: &crate::ast::InSetExpr) -> bool {
    match set {
        crate::ast::InSetExpr::Items(items) => items.iter().any(rgf_exp_generative),
        crate::ast::InSetExpr::Range(low, high) => {
            rgf_exp_generative(low) || rgf_exp_generative(high)
        }
        crate::ast::InSetExpr::Expr(expr) => rgf_exp_generative(expr),
    }
}

/// One comparison atom of the shape walk: the rgf side must be a single
/// bare reference and the other side constant-evaluable.
fn rgf_atom_side(
    rgf_side: &Expr,
    refs: &[String],
    exp_side: &Expr,
    rgf: &BTreeSet<String>,
    order: &[String],
) -> Result<Option<String>, String> {
    if refs.len() == 1 && rgf_bare_ref(rgf_side, &refs[0]) && rgf_exp_side_ok(exp_side, rgf) {
        Ok(Some(refs[0].clone()))
    } else {
        Err(rgf_first_in(order, refs))
    }
}

/// Canonicalize intervals (5.2.12): sort, merge overlapping and
/// neighboring ones, drop empties.
fn canonical_set(mut intervals: Vec<(i128, i128)>) -> Vec<(i128, i128)> {
    intervals.retain(|(lo, hi)| lo <= hi);
    intervals.sort();
    let mut merged: Vec<(i128, i128)> = Vec::new();
    for (lo, hi) in intervals {
        match merged.last_mut() {
            Some((_, last_hi)) if lo <= last_hi.saturating_add(1) => {
                if hi > *last_hi {
                    *last_hi = hi;
                }
            }
            _ => merged.push((lo, hi)),
        }
    }
    merged
}

fn union_sets(a: &[(i128, i128)], b: &[(i128, i128)]) -> Value {
    let mut all = a.to_vec();
    all.extend_from_slice(b);
    Value::Set(canonical_set(all))
}

fn intersect_sets(a: &[(i128, i128)], b: &[(i128, i128)]) -> Value {
    let mut result = Vec::new();
    for (alo, ahi) in a {
        for (blo, bhi) in b {
            let lo = (*alo).max(*blo);
            let hi = (*ahi).min(*bhi);
            if lo <= hi {
                result.push((lo, hi));
            }
        }
    }
    Value::Set(canonical_set(result))
}

fn diff_sets(a: &[(i128, i128)], b: &[(i128, i128)]) -> Value {
    // Remove b's intervals from a's one at a time.
    let mut current = a.to_vec();
    for (blo, bhi) in b {
        let mut next = Vec::new();
        for (lo, hi) in current {
            if *bhi < lo || *blo > hi {
                next.push((lo, hi));
                continue;
            }
            if *blo > lo {
                next.push((lo, blo - 1));
            }
            if *bhi < hi {
                next.push((bhi + 1, hi));
            }
        }
        current = next;
    }
    Value::Set(canonical_set(current))
}

/// Render a set in canonical form (5.2.12): `[1..5,10]` — single-value
/// intervals show the value alone.
fn render_set(intervals: &[(i128, i128)]) -> String {
    let parts: Vec<String> = intervals
        .iter()
        .map(|(lo, hi)| {
            if lo == hi {
                format!("{lo}")
            } else {
                format!("{lo}..{hi}")
            }
        })
        .collect();
    format!("[{}]", parts.join(","))
}

/// Rebase an enclosing instance's constraints onto a generated
/// object's own field names (10.3.6-10.3.7's connected field sets, v1):
/// `p.v == select { .. }` gating `gen p` becomes `v == select { .. }`.
/// The dotted path an expression spells, when it is one: a bare
/// identifier or a chain of member accesses rooted at one. Anything else
/// (indexing, calls, literals, …) is not a simple path — `None`.
fn expr_path_segments(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::Ident { name, .. } => Some(vec![name.clone()]),
        Expr::MemberAccess { object, name, .. } => {
            let mut segments = expr_path_segments(object)?;
            segments.push(name.clone());
            Some(segments)
        }
        _ => None,
    }
}

/// The path a constraint spells, with a leading `me`/`self` segment
/// stripped (constraint paths may be written me-relative).
fn constraint_path_segments(expr: &Expr) -> Option<Vec<String>> {
    let mut segments = expr_path_segments(expr)?;
    if segments
        .first()
        .is_some_and(|name| name == "me" || name == "self")
    {
        segments.remove(0);
    }
    Some(segments)
}

/// Whether the expression contains a member path whose segments extend
/// `prefix` (a chosen_from target path, `me`/`self`-normalized): the
/// constraint then constrains a path through the target, so it gates the
/// 10.5.8 element choice.
fn expr_extends_path(expr: &Expr, prefix: &[String]) -> bool {
    if let Some(segments) = constraint_path_segments(expr)
        && segments.len() >= prefix.len()
        && segments[..prefix.len()] == *prefix
    {
        return true;
    }
    match expr {
        Expr::MemberAccess { object, .. } => expr_extends_path(object, prefix),
        Expr::Call { callee, args, .. } => {
            expr_extends_path(callee, prefix)
                || args.iter().any(|arg| expr_extends_path(arg, prefix))
        }
        Expr::Index { base, index, .. } => {
            expr_extends_path(base, prefix) || expr_extends_path(index, prefix)
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_extends_path(lhs, prefix) || expr_extends_path(rhs, prefix)
        }
        Expr::Unary { expr: inner, .. } | Expr::Paren { expr: inner, .. } => {
            expr_extends_path(inner, prefix)
        }
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_extends_path(cond, prefix)
                || expr_extends_path(then_expr, prefix)
                || expr_extends_path(else_expr, prefix)
        }
        Expr::BitSlice {
            base, high, low, ..
        } => {
            expr_extends_path(base, prefix)
                || expr_extends_path(high, prefix)
                || expr_extends_path(low, prefix)
        }
        Expr::CastAs { expr: inner, .. } => expr_extends_path(inner, prefix),
        Expr::InSet { element, set, .. } => {
            let set_has = match set {
                crate::ast::InSetExpr::Items(items) => {
                    items.iter().any(|item| expr_extends_path(item, prefix))
                }
                crate::ast::InSetExpr::Range(low, high) => {
                    expr_extends_path(low, prefix) || expr_extends_path(high, prefix)
                }
                crate::ast::InSetExpr::Expr(expr) => expr_extends_path(expr, prefix),
            };
            expr_extends_path(element, prefix) || set_has
        }
        Expr::IsA { expr: inner, .. } => expr_extends_path(inner, prefix),
        Expr::QualifiedField { base, .. } => expr_extends_path(base, prefix),
        Expr::ListConcat { items, .. } => items.iter().any(|item| expr_extends_path(item, prefix)),
        _ => false,
    }
}

/// Whether the constraint compares the chosen_from target as a whole
/// object — either operand of an equality, identity or list-identity
/// comparison, or the tested expression of an `is a` — a
/// pointer-equality or like-narrowing constraint on the target, which
/// the 10.5.8 protocol of this stage does not support (E0402). Returns
/// the unsupported family's name for the diagnostic; `None` when the
/// constraint does not touch the target whole (paths *through* it,
/// `p.x == q.y`, are ordinary constraints).
fn constraint_compares_target_whole(expr: &Expr, target: &[String]) -> Option<&'static str> {
    let whole_path =
        |expr: &Expr| constraint_path_segments(expr).is_some_and(|segments| segments == target);
    match expr {
        Expr::Binary { op, lhs, rhs, .. } => {
            if matches!(
                op,
                BinaryOp::Eq
                    | BinaryOp::NotEq
                    | BinaryOp::IdentityEq
                    | BinaryOp::IdentityNe
                    | BinaryOp::ListPtrEq
                    | BinaryOp::ListPtrNe
            ) && (whole_path(lhs) || whole_path(rhs))
            {
                return Some("pointer-equality");
            }
            constraint_compares_target_whole(lhs, target)
                .or_else(|| constraint_compares_target_whole(rhs, target))
        }
        Expr::IsA { expr: inner, .. } => {
            if whole_path(inner) {
                return Some("type-narrowing");
            }
            constraint_compares_target_whole(inner, target)
        }
        Expr::Unary { expr: inner, .. } | Expr::Paren { expr: inner, .. } => {
            constraint_compares_target_whole(inner, target)
        }
        _ => None,
    }
}

/// Render an expression for a diagnostic message: the common
/// constraint shapes verbatim, a placeholder for anything else.
fn expr_text(expr: &Expr) -> String {
    match expr {
        Expr::Ident { name, .. } => name.clone(),
        Expr::MemberAccess { object, name, .. } => format!("{}.{}", expr_text(object), name),
        Expr::Call { callee, args, .. } => format!(
            "{}({})",
            expr_text(callee),
            args.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
        Expr::Paren { expr: inner, .. } => format!("({})", expr_text(inner)),
        Expr::Index { base, index, .. } => format!("{}[{}]", expr_text(base), expr_text(index)),
        Expr::IntLiteral { text, .. }
        | Expr::RealLiteral { text, .. }
        | Expr::BitLiteral { text, .. } => text.clone(),
        Expr::BoolLiteral { value, .. } => {
            if *value {
                "TRUE".to_owned()
            } else {
                "FALSE".to_owned()
            }
        }
        Expr::StringLiteral { value, .. } => format!("\"{value}\""),
        _ => "list".to_owned(),
    }
}

/// A constraint rebases only when every path it references is rooted
/// at the generated object (depth two — `prefix.field`); anything else
/// (other roots, deeper paths, whole-object equalities) drops, because
/// it cannot evaluate against the generated instance's frame. Full
/// CFS scoping is a later milestone.
fn rebase_constraints(constraints: &[Expr], prefix: &str) -> Vec<Expr> {
    constraints
        .iter()
        .filter_map(|constraint| rebase_expr(constraint, prefix))
        .collect()
}

fn rebase_expr(expr: &Expr, prefix: &str) -> Option<Expr> {
    match expr {
        Expr::Binary { op, lhs, rhs, span } => Some(Expr::Binary {
            op: *op,
            lhs: Box::new(rebase_expr(lhs, prefix)?),
            rhs: Box::new(rebase_expr(rhs, prefix)?),
            span: *span,
        }),
        Expr::Unary { op, expr, span } => Some(Expr::Unary {
            op: *op,
            expr: Box::new(rebase_expr(expr, prefix)?),
            span: *span,
        }),
        Expr::Paren { expr, span } => Some(Expr::Paren {
            expr: Box::new(rebase_expr(expr, prefix)?),
            span: *span,
        }),
        // `prefix.field` — depth two: the rebase target.
        Expr::MemberAccess {
            object,
            name,
            name_span,
            span,
        } => {
            let Expr::Ident { name: root, .. } = object.as_ref() else {
                return None;
            };
            if root == prefix {
                let _ = name_span;
                Some(Expr::Ident {
                    name: name.clone(),
                    span: *span,
                })
            } else {
                None
            }
        }
        // The select's weights are constants; the choice tokens stay as
        // written (the sampler evaluates them against the generated
        // object's frame).
        Expr::Select { arms, span } => Some(Expr::Select {
            arms: arms.clone(),
            span: *span,
        }),
        // A bare identifier is either the whole generated object (an
        // object equality — needs full CFS) or an unrelated name; both
        // drop.
        _ => None,
    }
}

/// The natural bounds of an integer scalar type (from `sample_int_in`'s
/// width table); `None` for non-integer scalars and compound types.
fn natural_range(ty: &TypeExpr) -> Option<(i128, i128)> {
    let TypeExpr::Scalar { base, width, .. } = ty else {
        return None;
    };
    match base {
        ScalarBase::Uint => {
            let bits = width_bits(width, 32);
            Some((0i128, (1i128 << bits) - 1))
        }
        ScalarBase::Int => {
            let bits = width_bits(width, 32);
            Some((-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1))
        }
        // The long bases are 64-bit wide (Table 17, clause 5.2.3.1).
        ScalarBase::Longuint => Some((0, (1i128 << 64) - 1)),
        ScalarBase::Longint => Some((-(1i128 << 63), (1i128 << 63) - 1)),
        ScalarBase::Bit => Some((0, 1)),
        ScalarBase::Byte => Some((0, 255)),
        ScalarBase::Nibble => Some((0, 15)),
        ScalarBase::Bool | ScalarBase::Real | ScalarBase::String | ScalarBase::Time => None,
    }
}

/// The `field == select { .. }` leaves of a constraint conjunction, as
/// (field name, arms) pairs — either operand may carry the select.
fn select_leaves(expr: &Expr) -> Vec<(String, &[crate::ast::SelectArm])> {
    match expr {
        Expr::Binary {
            op: BinaryOp::LogicalAnd,
            lhs,
            rhs,
            ..
        } => {
            let mut leaves = select_leaves(lhs);
            leaves.extend(select_leaves(rhs));
            leaves
        }
        Expr::Paren { expr: inner, .. } => select_leaves(inner),
        Expr::Binary {
            op: BinaryOp::Eq,
            lhs,
            rhs,
            ..
        } => {
            for (side, other) in [(lhs, rhs), (rhs, lhs)] {
                if let (Expr::Ident { name, .. }, Expr::Select { arms, .. }) =
                    (side.as_ref(), other.as_ref())
                {
                    return vec![(name.clone(), arms)];
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// The choice keyword of a select arm's recorded tokens and, for
/// `normal(center, stddev)`, its two evaluated arguments.
fn select_choice(tokens: &[Token]) -> Option<(&'static str, Option<(i128, i128)>)> {
    let word = match &tokens.first()?.kind {
        TokenKind::Identifier(word) | TokenKind::Keyword(word) => word.as_str(),
        _ => return None,
    };
    // The literals keep the return kind 'static (no borrow of tokens).
    let kind: &'static str = match word {
        "min" => "min",
        "max" => "max",
        "others" => "others",
        "edges" => "edges",
        "normal" => "normal",
        _ => return None,
    };
    if kind != "normal" {
        return Some((kind, None));
    }
    // `normal(center, stddev)` — two comma-separated constant integer
    // expressions over the recorded tokens.
    if !token_kind_is_punct(tokens.get(1), "(") || !token_kind_is_punct(tokens.last(), ")") {
        return None;
    }
    let mut parts: Vec<&[Token]> = Vec::new();
    let mut start = 2;
    let mut depth = 0usize;
    for (index, token) in tokens[2..tokens.len() - 1].iter().enumerate() {
        match &token.kind {
            TokenKind::Punctuation(p) if p == "(" => depth += 1,
            TokenKind::Punctuation(p) if p == ")" => depth = depth.saturating_sub(1),
            TokenKind::Punctuation(p) if p == "," && depth == 0 => {
                parts.push(&tokens[start..2 + index]);
                start = 3 + index;
            }
            _ => {}
        }
    }
    parts.push(&tokens[start..tokens.len() - 1]);
    if parts.len() != 2 {
        return None;
    }
    let center = eval_choice_int(parts[0])?;
    let stddev = eval_choice_int(parts[1])?;
    Some((kind, Some((center, stddev))))
}

/// Evaluate a constant integer expression over the recorded choice
/// tokens: decimal/based literals, the `MAX_UINT`/`MAX_INT` predefined
/// constants, `+`/`-`/`*`/`/` with precedence, and parentheses. `None`
/// on anything else (the arm is then skipped, honestly).
/// Evaluate a constant integer expression over recorded tokens
/// (literals, the predefined `MAX_UINT`/`MAX_INT`, `+`/`-`/`*`/`/`
/// with precedence, parentheses); `None` on anything else.
pub(crate) fn eval_choice_int(tokens: &[Token]) -> Option<i128> {
    if tokens.is_empty() {
        return None;
    }
    // Split the top-level additive terms.
    let mut terms: Vec<(bool, &[Token])> = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut negate = false;
    for (index, token) in tokens.iter().enumerate() {
        match &token.kind {
            TokenKind::Punctuation(p) if p == "(" => depth += 1,
            TokenKind::Punctuation(p) if p == ")" => depth = depth.saturating_sub(1),
            TokenKind::Punctuation(p) if p == "+" && depth == 0 => {
                terms.push((negate, &tokens[start..index]));
                negate = false;
                start = index + 1;
            }
            TokenKind::Punctuation(p) if p == "-" && depth == 0 => {
                terms.push((negate, &tokens[start..index]));
                negate = true;
                start = index + 1;
            }
            _ => {}
        }
    }
    terms.push((negate, &tokens[start..]));
    let mut total: Option<i128> = None;
    for (negative, term) in terms {
        if term.is_empty() {
            return None;
        }
        let value = eval_choice_product(term)?;
        total = Some(match total {
            None => {
                if negative {
                    -value
                } else {
                    value
                }
            }
            Some(current) => {
                if negative {
                    current - value
                } else {
                    current + value
                }
            }
        });
    }
    total
}

/// The multiplicative level of the choice-expression evaluator.
fn eval_choice_product(tokens: &[Token]) -> Option<i128> {
    let mut total: Option<i128> = None;
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut operation = "*";
    for (index, token) in tokens.iter().enumerate() {
        match &token.kind {
            TokenKind::Punctuation(p) if p == "(" => depth += 1,
            TokenKind::Punctuation(p) if p == ")" => depth = depth.saturating_sub(1),
            TokenKind::Punctuation(p) if (p == "*" || p == "/") && depth == 0 => {
                let value = eval_choice_atom(&tokens[start..index])?;
                total = Some(apply_choice_op(total, operation, value)?);
                operation = p.as_str();
                start = index + 1;
            }
            _ => {}
        }
    }
    let value = eval_choice_atom(&tokens[start..])?;
    apply_choice_op(total, operation, value)
}

fn apply_choice_op(total: Option<i128>, operation: &str, value: i128) -> Option<i128> {
    match (total, operation) {
        (None, "*") => Some(value),
        (None, "/") => Some(value),
        (Some(current), "*") => current.checked_mul(value),
        (Some(current), "/") => {
            if value == 0 {
                None
            } else {
                Some(current / value)
            }
        }
        _ => None,
    }
}

fn eval_choice_atom(tokens: &[Token]) -> Option<i128> {
    let first = tokens.first()?;
    match &first.kind {
        TokenKind::Number(text) => {
            if tokens.len() != 1 {
                return None;
            }
            crate::interp::parse_int_literal(text)
        }
        TokenKind::Identifier(word) | TokenKind::Keyword(word) => {
            if tokens.len() != 1 {
                return None;
            }
            match word.as_str() {
                "MAX_UINT" => Some((1i128 << 32) - 1),
                "MAX_INT" => Some((1i128 << 31) - 1),
                _ => None,
            }
        }
        TokenKind::Punctuation(p) if p == "(" => {
            if tokens.len() < 3 || !token_kind_is_punct(tokens.last(), ")") {
                return None;
            }
            eval_choice_int(&tokens[1..tokens.len() - 1])
        }
        TokenKind::Punctuation(p) if p == "-" => {
            if tokens.len() < 2 {
                return None;
            }
            Some(0i128.wrapping_sub(eval_choice_int(&tokens[1..])?))
        }
        _ => None,
    }
}

fn token_kind_is_punct(token: Option<&Token>, symbol: &str) -> bool {
    token.is_some_and(|token| matches!(&token.kind, TokenKind::Punctuation(p) if p == symbol))
}

/// Whether one monitored sample crosses the edge. `rise`/`fall`/`change`
/// follow the 9.7.3 reading — "equivalent to the behavior of @sim
/// TEs", transitions detected between defined values only. There is no
/// X/Z value model here, so `any_change` degrades to `change`, and the
/// first sample (no previous value) only establishes the baseline.
fn edge_fires(edge: &str, previous: Option<i128>, value: i128) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    match edge {
        "rise" => previous != 1 && value == 1,
        "fall" => previous != 0 && value == 0,
        // `change` and (without MVL values) `any_change`.
        _ => previous != value,
    }
}

/// The sampling events that occur every virtual cycle in this model:
/// `sys.any` — in stand-alone operation the only event that occurs
/// automatically, the stand-alone clock (11.1.4.1.1); `sys.new_time` —
/// emitted on every sys.any in stand-alone operation (11.1.4.1.7); and
/// `sim` — the simulation tick the virtual cycles stand in for
/// (11.1.4.2). A TCM sampled by one of these keeps the every-cycle
/// implicit sync; any other declared sampling event gates the entry on
/// the event's occurrence.
fn is_every_cycle_event(path: &str) -> bool {
    matches!(
        path,
        "sys.any" | "any" | "sys.new_time" | "new_time" | "sim"
    )
}

/// The `keep <port>.edge() == <value>` attribute values (9.7.3) per
/// (type name, port name), read structurally from the composed
/// constraints (either orientation, call on either side). Values outside
/// the modeled subset (`change`/`rise`/`fall`/`any_change` — the MVL
/// transitions need an MVL value model) fall back to the `change`
/// default at the sampling point.
fn collect_port_edge_attributes(elaborated: &Elaborated) -> BTreeMap<(String, String), String> {
    let mut edges = BTreeMap::new();
    for ty in &elaborated.types {
        for constraint in &ty.constraints {
            let Expr::Binary {
                op: BinaryOp::Eq,
                lhs,
                rhs,
                ..
            } = &constraint.expr
            else {
                continue;
            };
            for (call, value) in [(lhs.as_ref(), rhs.as_ref()), (rhs.as_ref(), lhs.as_ref())] {
                let Expr::Call { callee, args, .. } = call else {
                    continue;
                };
                if !args.is_empty() {
                    continue;
                }
                let Expr::MemberAccess {
                    object,
                    name: method,
                    ..
                } = callee.as_ref()
                else {
                    continue;
                };
                if method != "edge" {
                    continue;
                }
                let (Expr::Ident { name: port, .. }, Expr::Ident { name: edge, .. }) =
                    (object.as_ref(), value)
                else {
                    continue;
                };
                edges.insert((ty.name.clone(), port.clone()), edge.clone());
            }
        }
    }
    edges
}

/// Whether the expression calls any of the named routines anywhere in its
/// tree (including as a member call like `x.hdl_path()`).
pub(crate) fn expr_calls_any(expr: &Expr, names: &[&str]) -> bool {
    match expr {
        Expr::Call { callee, args, .. } => {
            let callee_name = match callee.as_ref() {
                Expr::Ident { name, .. } => Some(name.as_str()),
                Expr::MemberAccess { name, .. } => Some(name.as_str()),
                _ => None,
            };
            callee_name.is_some_and(|name| names.contains(&name))
                || args.iter().any(|arg| expr_calls_any(arg, names))
        }
        Expr::MemberAccess { object, .. } => expr_calls_any(object, names),
        // A `select { .. }` distribution marks its whole constraint as a
        // distribution — the rejection predicate treats it as a no-op.
        Expr::Select { .. } => true,
        Expr::Unary { expr: inner, .. }
        | Expr::Paren { expr: inner, .. }
        | Expr::CastAs { expr: inner, .. } => expr_calls_any(inner, names),
        Expr::Binary { lhs, rhs, .. } => expr_calls_any(lhs, names) || expr_calls_any(rhs, names),
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_calls_any(cond, names)
                || expr_calls_any(then_expr, names)
                || expr_calls_any(else_expr, names)
        }
        Expr::BitSlice {
            base, high, low, ..
        } => {
            expr_calls_any(base, names) || expr_calls_any(high, names) || expr_calls_any(low, names)
        }
        Expr::Index { base, index, .. } => {
            expr_calls_any(base, names) || expr_calls_any(index, names)
        }
        Expr::InSet { element, set, .. } => {
            let set_has = match set {
                crate::ast::InSetExpr::Items(items) => {
                    items.iter().any(|item| expr_calls_any(item, names))
                }
                crate::ast::InSetExpr::Range(low, high) => {
                    expr_calls_any(low, names) || expr_calls_any(high, names)
                }
                crate::ast::InSetExpr::Expr(expr) => expr_calls_any(expr, names),
            };
            expr_calls_any(element, names) || set_has
        }
        Expr::ChosenFrom { target, list, .. } => {
            expr_calls_any(target, names) || expr_calls_any(list, names)
        }
        _ => false,
    }
}

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Int(i) => Some(*i as f64),
        Value::Real(r) => Some(*r),
        _ => None,
    }
}

/// Whether an `in_table` tuple element and a row element may be compared
/// at all (clause 4.11.8): same-kind scalars, the mixed integer/real
/// pair (as `==` accepts), objects by identity. Enumerals only compare
/// within their own type; anything else is the E0412 mismatch.
fn table_values_compatible(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(_), Value::Int(_))
        | (Value::Int(_), Value::Real(_))
        | (Value::Real(_), Value::Int(_))
        | (Value::Real(_), Value::Real(_))
        | (Value::Bool(_), Value::Bool(_))
        | (Value::Str(_), Value::Str(_))
        | (Value::Object(_), Value::Object(_)) => true,
        (
            Value::Enum {
                type_name: type_a, ..
            },
            Value::Enum {
                type_name: type_b, ..
            },
        ) => type_a == type_b,
        _ => false,
    }
}

/// The canonical type name a `set_of_values`-style type argument denotes,
/// for comparison with an enumerand value's type: the declared name, or
/// the inline enum's synthetic key for an inline enumeral list.
fn type_set_key(ty: &TypeExpr) -> Option<String> {
    match ty {
        TypeExpr::Named { name, .. } => Some(name.clone()),
        TypeExpr::InlineEnum { names, .. } => Some(inline_enum_key(names)),
        _ => None,
    }
}

/// Whether the expression anywhere contains an `evaluate { ... }` block
/// without an explicit type — the only case a call argument needs its
/// parameter's declared type as the inference context (4.17.7).
fn expr_has_untyped_evaluate(expr: &Expr) -> bool {
    match expr {
        Expr::Evaluate { ty, .. } => ty.is_none(),
        Expr::Unary { expr, .. } | Expr::Paren { expr, .. } | Expr::CastAs { expr, .. } => {
            expr_has_untyped_evaluate(expr)
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_has_untyped_evaluate(lhs) || expr_has_untyped_evaluate(rhs)
        }
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_untyped_evaluate(cond)
                || expr_has_untyped_evaluate(then_expr)
                || expr_has_untyped_evaluate(else_expr)
        }
        Expr::Index { base, index, .. } => {
            expr_has_untyped_evaluate(base) || expr_has_untyped_evaluate(index)
        }
        Expr::BitSlice {
            base, high, low, ..
        } => {
            expr_has_untyped_evaluate(base)
                || expr_has_untyped_evaluate(high)
                || expr_has_untyped_evaluate(low)
        }
        Expr::MemberAccess { object, .. } => expr_has_untyped_evaluate(object),
        Expr::Call { callee, args, .. } => {
            expr_has_untyped_evaluate(callee) || args.iter().any(expr_has_untyped_evaluate)
        }
        Expr::InSet { element, set, .. } => {
            let set_has = match set {
                crate::ast::InSetExpr::Items(items) => items.iter().any(expr_has_untyped_evaluate),
                crate::ast::InSetExpr::Range(low, high) => {
                    expr_has_untyped_evaluate(low) || expr_has_untyped_evaluate(high)
                }
                crate::ast::InSetExpr::Expr(expr) => expr_has_untyped_evaluate(expr),
            };
            expr_has_untyped_evaluate(element) || set_has
        }
        Expr::InTable { tuple, rows, .. } => {
            tuple.iter().any(expr_has_untyped_evaluate)
                || rows.iter().any(|row| {
                    row.iter().any(|element| match element {
                        crate::ast::TableElem::Scalar(expr) => expr_has_untyped_evaluate(expr),
                        crate::ast::TableElem::Set(crate::ast::InSetExpr::Items(items)) => {
                            items.iter().any(expr_has_untyped_evaluate)
                        }
                        crate::ast::TableElem::Set(crate::ast::InSetExpr::Range(low, high)) => {
                            expr_has_untyped_evaluate(low) || expr_has_untyped_evaluate(high)
                        }
                        crate::ast::TableElem::Set(crate::ast::InSetExpr::Expr(expr)) => {
                            expr_has_untyped_evaluate(expr)
                        }
                        crate::ast::TableElem::TypeSet { .. } => false,
                    })
                })
        }
        _ => false,
    }
}

/// The ordering used by the list `min`/`max` family (27.4.15-27.4.19):
/// the operand rules of the relational operators (the `Lt`..`Ge` arms of
/// `apply_binary`). `None` when the pair is not comparable.
fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
        (Value::Real(x), Value::Real(y)) => x.partial_cmp(y),
        (Value::Int(x), Value::Real(y)) => (*x as f64).partial_cmp(y),
        (Value::Real(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)),
        (Value::Str(x), Value::Str(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

/// The equality used by `is_a_permutation()` (27.4.11): the operand rules
/// of `==` (the `Eq` arm of `apply_binary`), with struct items compared by
/// instance index — the prose's "compares the addresses of the struct
/// items". Pairs `==` does not accept compare unequal.
fn values_equal(a: &Value, b: &Value) -> bool {
    values_equal_bounded(a, b, 0)
}

/// The recursion cap of the value-level comparisons and formatting. Far
/// above any legal nesting (e declares no lists of lists), it exists so a
/// list that was illegally stored inside itself — the runtime performs no
/// element-type check on item stores — reports unequal or renders a
/// placeholder instead of overflowing the stack.
const MAX_VALUE_DEPTH: usize = 64;

fn values_equal_bounded(a: &Value, b: &Value, depth: usize) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Real(x), Value::Real(y)) => x == y,
        (Value::Int(x), Value::Real(y)) | (Value::Real(y), Value::Int(x)) => *x as f64 == *y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (
            Value::Enum {
                type_name: type_x,
                name: name_x,
            },
            Value::Enum {
                type_name: type_y,
                name: name_y,
            },
        ) => type_x == type_y && name_x == name_y,
        (Value::Object(x), Value::Object(y)) => x == y,
        (Value::List(x), Value::List(y)) => lists_equal(x, y, depth),
        // Set equality (5.2.12): same canonical intervals — the sets
        // denote the same value collection regardless of how each was
        // built (a literal, a set algebra result, or a clause 29.13
        // rgf_get_range query).
        (Value::Set(x), Value::Set(y)) => x == y,
        _ => false,
    }
}

/// Item-wise list equality (the regular `==`, 4.11.4's contrast with
/// `list_ptr==`): same length, items equal pairwise.
fn lists_equal(a: &ListValue, b: &ListValue, depth: usize) -> bool {
    if depth > MAX_VALUE_DEPTH {
        return false;
    }
    let a_items = a.items.borrow();
    let b_items = b.items.borrow();
    a_items.len() == b_items.len()
        && a_items
            .iter()
            .zip(b_items.iter())
            .all(|(x, y)| values_equal_bounded(x, y, depth + 1))
}

/// The element type of a list type expression (identity for non-lists).
fn list_element_type(ty: &TypeExpr) -> TypeExpr {
    match ty {
        TypeExpr::List { element, .. } => (**element).clone(),
        other => other.clone(),
    }
}

/// The bit width of a scalar type declaration (`uint(bits: 3)` → 3);
/// unsized scalars count as one bit for packing — except the long bases,
/// whose default packing width is 64 bits (Table 17, clause 5.2.3.1).
fn field_width_bits(ty: &TypeExpr) -> Option<i128> {
    let TypeExpr::Scalar { base, width, .. } = ty else {
        return None;
    };
    let Some(width) = width else {
        return match base {
            ScalarBase::Longint | ScalarBase::Longuint => Some(64),
            _ => None,
        };
    };
    match width.as_ref() {
        Expr::IntLiteral { text, .. } => parse_int_literal(text),
        _ => None,
    }
}

/// Whether the value digits of a sized or unsized based literal include an
/// MVL letter (clause 4.2.5.4). The base character itself (`'h`, `'x`) is
/// not part of the value. In an ambiguous context those letters make the
/// literal MVL rather than a number; hex `x` is therefore not read as 15.
pub(crate) fn based_value_is_mvl(text: &str) -> bool {
    let Some(quote) = text.find('\'') else {
        return false;
    };
    text[quote + 1..].chars().skip(1).any(|c| {
        matches!(
            c,
            'x' | 'X' | 'z' | 'Z' | 'u' | 'U' | 'l' | 'L' | 'h' | 'H' | 'w' | 'W' | 'n' | 'N'
        )
    })
}

/// Parse an integer literal: decimal (optional `K`/`M` multiplier),
/// `0c"..."` character, `0x`/`0b`/`0o` prefixed, or a sized/unsized based
/// literal (`8'hFF`, `'hFF`). Underscores are ignored. The size prefix is
/// ignored (widths are not truncated yet). MVL literals return [`None`].
pub(crate) fn parse_int_literal(text: &str) -> Option<i128> {
    if text.len() >= 2
        && text.as_bytes()[0] == b'0'
        && text.as_bytes()[1].eq_ignore_ascii_case(&b'c')
    {
        return parse_character_literal(text);
    }
    if based_value_is_mvl(text) {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    let cleaned: String = lower.chars().filter(|c| *c != '_').collect();
    for (prefix, radix) in [("0x", 16u32), ("0b", 2), ("0o", 8)] {
        if let Some(digits) = cleaned.strip_prefix(prefix) {
            return i128::from_str_radix(digits, radix).ok();
        }
    }
    if let Some(quote_pos) = cleaned.find('\'') {
        let mut rest = cleaned[quote_pos + 1..].chars();
        let base = rest.next()?;
        let digits: &str = rest.as_str();
        let radix = match base {
            'b' => 2u32,
            'o' => 8,
            'd' => 10,
            'x' | 'h' | 'f' => 16,
            _ => return None,
        };
        return i128::from_str_radix(digits, radix).ok();
    }
    let (body, factor) = decimal_multiplier(&cleaned);
    let value = body.parse::<i128>().ok()?;
    value.checked_mul(factor)
}

/// `K`/`k` is ×1024 and `M`/`m` is ×1024×1024, on a decimal integer only.
fn decimal_multiplier(text: &str) -> (&str, i128) {
    let Some(last) = text.chars().last() else {
        return (text, 1);
    };
    let factor = match last {
        'k' => 1024,
        'm' => 1024 * 1024,
        _ => return (text, 1),
    };
    let body = &text[..text.len() - last.len_utf8()];
    if body.is_empty() || !body.bytes().all(|byte| byte.is_ascii_digit()) {
        return (text, 1);
    }
    (body, factor)
}

/// `0c"..."` / `0C"..."` evaluates to the character's code point.
fn parse_character_literal(text: &str) -> Option<i128> {
    let rest = if let Some(rest) = text.strip_prefix("0c") {
        rest
    } else {
        text.strip_prefix("0C")?
    };
    let inner = rest.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = inner.chars();
    let value = match chars.next()? {
        '\\' => match chars.next()? {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '\\' => '\\',
            '"' => '"',
            '0' => '\0',
            _ => return None,
        },
        other => other,
    };
    if chars.next().is_some() {
        return None;
    }
    Some(u32::from(value) as i128)
}

/// Interpret the escape sequences the lexer preserved as raw text.
pub(crate) fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('0') => out.push('\0'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl<'a> Interp<'a> {
    /// One scheduler turn (13.2.2): execute the coroutine atomically
    /// from its resume point until it completes, suspends at a
    /// synchronization action, or errors.
    fn run_coroutine_turn(&mut self, co: &mut Coroutine<'a>) -> TurnOutcome {
        // The kind's payload is copied out so the turn's &mut self
        // calls never hold a borrow of `co`.
        enum Plan<'a> {
            Main {
                blocks: Vec<&'a [Stmt]>,
                next: usize,
            },
            Tcm {
                instance: usize,
                name: String,
                args: Vec<Value>,
                next: usize,
            },
        }
        let plan = match &co.kind {
            CoroutineKind::Main { blocks, next_block } => Plan::Main {
                blocks: blocks.clone(),
                next: *next_block,
            },
            CoroutineKind::Tcm {
                instance,
                method,
                args,
                next_block,
            } => Plan::Tcm {
                instance: *instance,
                name: method.clone(),
                args: args.clone(),
                next: *next_block,
            },
        };
        match plan {
            Plan::Main { blocks, mut next } => {
                let block_count = blocks.len();
                while next < block_count {
                    let body = blocks[next];
                    match self.run_body(body, co) {
                        Ok(Flow::Normal | Flow::Break | Flow::Continue | Flow::Return(_)) => {
                            next += 1;
                            co.path.clear();
                            co.frame = Frame::default();
                        }
                        Ok(Flow::Stop) => {
                            self.stop_requested = true;
                            if let CoroutineKind::Main { next_block, .. } = &mut co.kind {
                                *next_block = next;
                            }
                            return TurnOutcome::Complete;
                        }
                        Ok(Flow::Suspend) => {
                            if let CoroutineKind::Main { next_block, .. } = &mut co.kind {
                                *next_block = next;
                            }
                            return self.take_suspension(co);
                        }
                        Err(diagnostic) => return TurnOutcome::Error(diagnostic),
                    }
                }
                TurnOutcome::Complete
            }
            Plan::Tcm {
                instance,
                name,
                args,
                mut next,
            } => {
                let types: &[crate::elab::ElaboratedType] = self.types;
                let type_name = self.instances[instance].type_name.clone();
                let Ok(blocks) = resolve_method_blocks(types, &type_name, &name, co.span) else {
                    return TurnOutcome::Error(Diagnostic::new(
                        "E0401",
                        format!("method '{name}' not found for the started TCM"),
                        co.span.0,
                        co.span.1,
                    ));
                };
                let block_count = blocks.len();
                while next < block_count {
                    let block = blocks[next];
                    if co.path.is_empty() {
                        // A fresh `is also` block binds its parameters
                        // from the start-time arguments.
                        co.frame = Frame::default();
                        for (param, arg) in block.params.iter().zip(args.iter()) {
                            if let Some(ty) = &param.ty {
                                co.frame.local_types.insert(param.name.clone(), ty.clone());
                            }
                            co.frame.locals.insert(param.name.clone(), arg.clone());
                        }
                    }
                    match self.run_body(&block.body, co) {
                        Ok(Flow::Normal | Flow::Break | Flow::Continue | Flow::Return(_)) => {
                            next += 1;
                            co.path.clear();
                        }
                        Ok(Flow::Stop) => {
                            self.stop_requested = true;
                            break;
                        }
                        Ok(Flow::Suspend) => {
                            if let CoroutineKind::Tcm { next_block, .. } = &mut co.kind {
                                *next_block = next;
                            }
                            return self.take_suspension(co);
                        }
                        Err(diagnostic) => return TurnOutcome::Error(diagnostic),
                    }
                }
                if let CoroutineKind::Tcm { next_block, .. } = &mut co.kind {
                    *next_block = next;
                }
                TurnOutcome::Complete
            }
        }
    }

    /// Execute one body, resuming through the statement path when
    /// suspended.
    fn run_body(&mut self, body: &'a [Stmt], co: &mut Coroutine<'a>) -> Result<Flow, Diagnostic> {
        if co.path.is_empty() {
            self.exec_block(body, &mut co.frame, co.me)
        } else {
            self.exec_block_resumable(body, &mut co.frame, co.me, &co.path)
        }
    }

    /// Collect the suspension the turn raised.
    fn take_suspension(&mut self, co: &mut Coroutine<'a>) -> TurnOutcome {
        match self.pending_suspension.take() {
            Some(suspension) => {
                co.path = suspension.path;
                co.frame = suspension.frame;
                co.eligible_at_cycle = suspension.eligible_at_cycle;
                co.suspended_temporal = suspension.temporal;
                co.span = suspension.span;
                co.wait_start = suspension.wait_start;
                co.wake_sync = suspension.wake_sync;
                TurnOutcome::Suspended
            }
            None => TurnOutcome::Error(self.error(
                "E0402",
                "internal: suspension without state",
                origin_span(),
            )),
        }
    }
}
