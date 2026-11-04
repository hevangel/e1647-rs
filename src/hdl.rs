//! The simulator seam for mixed e + HDL testcases: one [`Simulator`]
//! trait, a small explicit backend registry, and the signal sources the
//! interpreter reads from.
//!
//! Two backends are registered:
//!
//! - [`XezimSimulator`] (native targets only) — the binding this crate
//!   started with: it spawns `xezim`, the external SystemVerilog
//!   simulator, with the VPI control bridge (`c/xezim_bridge.c`, see the
//!   compose `mixed` service) and drives it over stdin/stdout with a
//!   one-response-per-command line protocol:
//!
//!   ```text
//!   SET <path> <value>   -> OK
//!   GET <path>           -> VAL <value>
//!   RUN <delay>          -> OK, READY
//!   FINISH               -> BYE
//!   ```
//!
//!   Lines the simulator prints on its own are ignored by the reader —
//!   only the response prefixes are matched.
//!
//! - [`NullSimulator`] — the no-op backend for where no simulator
//!   exists. Every operation reports the honest "no simulator" error
//!   (the interpreter surfaces backend errors as `E0408`), and "loading"
//!   it fails up front, so an `hdl_load` resolved to it never pretends
//!   to succeed. Under `target_arch = "wasm32"` a wasm sandbox has no
//!   child processes, so the xezim backend is not compiled at all and
//!   the null backend is the only registered one. It is also the tests'
//!   double for the trait dispatch.
//!
//! A third backend (VHPI, say) plugs in beside them: implement
//! [`Simulator`], give it a registry name, and add one constructor arm
//! to [`load_backend`] — the interpreter and the signal sources do not
//! change. There is no VHPI today, and no configuration knob selects a
//! backend yet: `hdl_load` resolves through [`load`] (the default
//! backend), while [`load_backend`] is the explicit form tests and
//! future configuration can name a backend through.
//!
//! Values can also come from an injected per-cycle history
//! ([`SignalSource::Injected`]) without any backend: the interpreter's
//! virtual cycles are the sampled timeline there. Tests and offline runs
//! use that seam because the `xezim` binary is only provided inside the
//! container.
//!
//! Security posture (the spawning backend): the spawned program is the
//! fixed name `xezim` resolved through `PATH` (the container provides
//! it); every argument (bridge module, design file, top name) is
//! validated against a strict character allowlist before the argv-list
//! spawn — there is no shell and no string concatenation into a command
//! line.

use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{BufRead, BufReader, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// One HDL simulator backend: how a loaded session drives signal reads
/// and writes, advances time and ends the run.
///
/// [`Simulator::load`] constructs the session — backends that need a
/// process spawn it there — and the object methods run against the
/// loaded session, which is why `load` is sized-only while the rest
/// dispatch through `dyn Simulator`.
pub trait Simulator {
    /// Load the design: construct the session for `design_file` with
    /// top module `top`, ready for commands. Fails honestly when the
    /// backend has nothing to load.
    fn load(design_file: &str, top: &str) -> Result<Self, String>
    where
        Self: Sized;

    /// The backend's registry name — the value of the backend's `NAME`
    /// const (the key [`load_backend`] matches on).
    fn name(&self) -> &'static str;

    /// Assign an integer value to a signal.
    fn set(&mut self, path: &str, value: i64) -> Result<(), String>;

    /// Read an integer value from a signal.
    fn get(&mut self, path: &str) -> Result<i128, String>;

    /// Advance simulation time by the design's time units.
    fn run(&mut self, delay: u64) -> Result<(), String>;

    /// End the simulation. A backend with nothing to end does nothing.
    fn finish(&mut self);
}

#[cfg(not(target_arch = "wasm32"))]
/// Validate a path or identifier passed as a simulator argument: letters,
/// digits and `/_ . -` only, non-empty, and no leading `-` (no option
/// confusion).
fn valid_location(value: &str, what: &str) -> Result<(), String> {
    let ok = !value.is_empty()
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.' | '-'));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "invalid {what} '{value}': allowed are letters, digits and / _ . -"
        ))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct HdlSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[cfg(not(target_arch = "wasm32"))]
/// Read response lines until one starts with one of the protocol
/// prefixes; simulator-own output in between is skipped. Returns the
/// matching line.
fn next_protocol_line(
    stdout: &mut BufReader<ChildStdout>,
    prefixes: &[&str],
) -> Result<String, String> {
    loop {
        let mut line = String::new();
        match stdout.read_line(&mut line) {
            Ok(0) => {
                return Err("the simulator closed its output before answering (did \
                     the design elaborate?)"
                    .to_owned());
            }
            Ok(_) => {
                let trimmed = line.trim_end().to_owned();
                if prefixes
                    .iter()
                    .any(|prefix| trimmed == *prefix || trimmed.starts_with(prefix))
                {
                    return Ok(trimmed);
                }
            }
            Err(err) => return Err(format!("could not read from the simulator: {err}")),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl HdlSession {
    /// Spawn the simulator with the VPI bridge and wait for it to finish
    /// elaboration (the first `READY`). The bridge module location can be
    /// overridden with `E1647_XEZIM_BRIDGE` (container default
    /// `/opt/xezim/bridge.so`).
    pub fn load(design_file: &str, top: &str) -> Result<Self, String> {
        let bridge = std::env::var("E1647_XEZIM_BRIDGE")
            .unwrap_or_else(|_| "/opt/xezim/bridge.so".to_owned());
        valid_location(&bridge, "VPI bridge (E1647_XEZIM_BRIDGE)")?;
        valid_location(design_file, "design file")?;
        valid_location(top, "top module name")?;
        let mut child = Command::new("xezim")
            .arg("--simulate")
            .arg("--vpi-lib")
            .arg(&bridge)
            .arg("-s")
            .arg(top)
            .arg(design_file)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|err| {
                "could not spawn 'xezim' (is it on PATH? the container \
                 provides it): "
                    .to_owned()
                    + &err.to_string()
            })?;
        let stdin = child.stdin.take().ok_or("no stdin for the simulator")?;
        let stdout = child.stdout.take().ok_or("no stdout for the simulator")?;
        let mut session = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        };
        session.wait_ready()?;
        Ok(session)
    }

    fn wait_ready(&mut self) -> Result<(), String> {
        next_protocol_line(&mut self.stdout, &["READY"]).map(|_| ())
    }

    fn send(&mut self, command: &str) -> Result<(), String> {
        self.stdin
            .write_all(command.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|err| format!("could not write to the simulator: {err}"))
    }

    fn expect(&mut self, prefixes: &[&str]) -> Result<String, String> {
        next_protocol_line(&mut self.stdout, prefixes)
    }

    /// Assign an integer value to a signal.
    pub fn set(&mut self, path: &str, value: i64) -> Result<(), String> {
        self.send(&format!("SET {path} {value}"))?;
        match self.expect(&["OK", "ERR"])?.as_str() {
            "OK" => Ok(()),
            err => Err(err.to_owned()),
        }
    }

    /// Read an integer value from a signal.
    pub fn get(&mut self, path: &str) -> Result<i128, String> {
        self.send(&format!("GET {path}"))?;
        match self.expect(&["VAL", "ERR"])?.strip_prefix("VAL ") {
            Some(text) => text
                .parse::<i128>()
                .map_err(|err| format!("unreadable value '{text}': {err}")),
            None => Err("signal read failed".to_owned()),
        }
    }

    /// Advance simulation time by the design's time units.
    pub fn run(&mut self, delay: u64) -> Result<(), String> {
        self.send(&format!("RUN {delay}"))?;
        self.expect(&["OK", "ERR"])?;
        self.wait_ready()
    }

    /// End the simulation and reap the child process.
    pub fn finish(&mut self) {
        if self.send("FINISH").is_ok() {
            let _ = self.expect(&["BYE"]);
        }
        let _ = self.child.wait();
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for HdlSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// The xezim VPI-bridge backend: this crate's original binding, the
/// [`Simulator`] implementation over [`HdlSession`]. Every operation
/// delegates 1:1 to the session — the trait adds the registry name,
/// nothing else. Native targets only: a wasm32 sandbox has no child
/// processes, so this backend is not compiled there.
pub struct XezimSimulator {
    session: HdlSession,
}

#[cfg(not(target_arch = "wasm32"))]
impl XezimSimulator {
    /// The backend's registry name (the key [`load_backend`] matches).
    pub const NAME: &'static str = "xezim";
}

#[cfg(not(target_arch = "wasm32"))]
impl Simulator for XezimSimulator {
    fn load(design_file: &str, top: &str) -> Result<Self, String> {
        Ok(Self {
            session: HdlSession::load(design_file, top)?,
        })
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn set(&mut self, path: &str, value: i64) -> Result<(), String> {
        self.session.set(path, value)
    }

    fn get(&mut self, path: &str) -> Result<i128, String> {
        self.session.get(path)
    }

    fn run(&mut self, delay: u64) -> Result<(), String> {
        self.session.run(delay)
    }

    fn finish(&mut self) {
        self.session.finish();
    }
}

/// The no-op backend for where no simulator exists: the only registered
/// backend under `target_arch = "wasm32"` (a wasm sandbox has no child
/// processes, so [`XezimSimulator`] is not compiled there) and the
/// tests' double for the [`Simulator`] dispatch.
///
/// Every operation reports the honest "no simulator" error — the
/// interpreter surfaces backend errors as `E0408` — and [`Simulator::load`]
/// fails up front, so an `hdl_load` resolved to this backend never
/// pretends to succeed. `finish` has nothing to end and does nothing.
pub struct NullSimulator;

impl NullSimulator {
    /// The backend's registry name (the key [`load_backend`] matches).
    pub const NAME: &'static str = "null";

    /// The honest reason every drive fails: there is no simulator behind
    /// this backend, so there is no `<what>` to act on.
    fn unavailable(what: &str) -> String {
        format!("no simulator available: the null backend has no {what}")
    }
}

impl Simulator for NullSimulator {
    fn load(_design_file: &str, _top: &str) -> Result<Self, String> {
        Err(Self::unavailable("design to load"))
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn set(&mut self, _path: &str, _value: i64) -> Result<(), String> {
        Err(Self::unavailable("signal to write"))
    }

    fn get(&mut self, _path: &str) -> Result<i128, String> {
        Err(Self::unavailable("signal to read"))
    }

    fn run(&mut self, _delay: u64) -> Result<(), String> {
        Err(Self::unavailable("simulation to advance"))
    }

    fn finish(&mut self) {}
}

/// The backends registered for `hdl_load` to resolve through, in
/// registration order. The xezim bridge is native-only (see
/// [`XezimSimulator`]); the null backend is always present as the honest
/// stand-in where no real backend exists. A third binding adds itself
/// here and to [`load_backend`] — this list and that match are the whole
/// registration.
pub fn registered_backends() -> Vec<&'static str> {
    #[cfg(not(target_arch = "wasm32"))]
    let native = [XezimSimulator::NAME];
    #[cfg(target_arch = "wasm32")]
    let native: [&'static str; 0] = [];
    let mut backends = Vec::from(native);
    backends.push(NullSimulator::NAME);
    backends
}

/// The backend [`load`] resolves through when the source just asks for a
/// simulator: the xezim VPI bridge on native targets; under `wasm32` the
/// null backend is the only registered one, so loads there fail with its
/// honest no-simulator error.
pub fn default_backend() -> &'static str {
    #[cfg(not(target_arch = "wasm32"))]
    let backend = XezimSimulator::NAME;
    #[cfg(target_arch = "wasm32")]
    let backend = NullSimulator::NAME;
    backend
}

/// Construct the named backend for `design_file` with top module `top`.
/// This is the seam's registration point: a third binding (VHPI, say)
/// plugs in with its own [`Simulator`] impl and one more arm here.
/// Unknown names answer with the registered list.
pub fn load_backend(
    backend: &str,
    design_file: &str,
    top: &str,
) -> Result<Box<dyn Simulator>, String> {
    match backend {
        #[cfg(not(target_arch = "wasm32"))]
        XezimSimulator::NAME => Ok(Box::new(XezimSimulator::load(design_file, top)?)),
        NullSimulator::NAME => Ok(Box::new(NullSimulator::load(design_file, top)?)),
        other => Err(format!(
            "unknown simulator backend '{other}' (registered: {})",
            registered_backends().join(", ")
        )),
    }
}

/// Load a simulator session through the default backend — the
/// constructor the interpreter's `hdl_load` resolves through (the xezim
/// VPI bridge on native targets; see [`default_backend`]).
pub fn load(design_file: &str, top: &str) -> Result<Box<dyn Simulator>, String> {
    load_backend(default_backend(), design_file, top)
}

/// Where signal values come from: a live external simulator session
/// (whatever [`Simulator`] backend the registry constructed for
/// `hdl_load`), or an injected per-cycle signal history.
///
/// The injected form is the offline/test seam: the `xezim` binary is only
/// provided inside the container (`compose mixed` service), so interpreter
/// tests and offline runs feed the `hdl_*` routines and the `@sim`
/// sampling (11.1.4.2) from a predetermined per-virtual-cycle history
/// instead. An injected source never advances real HDL time — the
/// interpreter's virtual cycles ARE the sampled timeline.
pub enum SignalSource {
    /// A live simulator session driven through the [`Simulator`] seam.
    Real(Box<dyn Simulator>),
    /// A predetermined signal history (see [`InjectedSignals`]).
    Injected(InjectedSignals),
}

impl SignalSource {
    /// Read a signal's value as of the given virtual cycle. The cycle is
    /// the interpreter's sampling clock; a real session ignores it (the
    /// backend reads the current simulated value).
    pub fn get(&mut self, path: &str, cycle: u64) -> Result<i128, String> {
        match self {
            SignalSource::Real(session) => session.get(path),
            SignalSource::Injected(signals) => signals.get(path, cycle).ok_or_else(|| {
                format!(
                    "no value injected for '{path}' at cycle {cycle} (an unsampleable \
                     signal never produces an edge)"
                )
            }),
        }
    }

    /// Assign a value to a signal. An injected source records the value
    /// from `cycle` on (until a later record overrides it).
    pub fn set(&mut self, path: &str, value: i64, cycle: u64) -> Result<(), String> {
        match self {
            SignalSource::Real(session) => session.set(path, value),
            SignalSource::Injected(signals) => {
                signals.record(path, cycle, i128::from(value));
                Ok(())
            }
        }
    }

    /// Advance HDL simulation time. An injected source has none — its
    /// timeline is the interpreter's virtual cycles — so this is a no-op.
    pub fn run(&mut self, delay: u64) -> Result<(), String> {
        match self {
            SignalSource::Real(session) => session.run(delay),
            SignalSource::Injected(_) => Ok(()),
        }
    }

    /// End the session. An injected source has nothing to end.
    pub fn finish(&mut self) {
        if let SignalSource::Real(session) = self {
            session.finish();
        }
    }
}

/// A predetermined per-virtual-cycle signal history for the injected
/// [`SignalSource`]: per path, a sparse, non-decreasing sequence of
/// (cycle, value) records. A read returns the value of the latest record
/// at or before the queried cycle; before the first record the signal has
/// no value (reads fail, so no edge can be detected off it — the honest
/// counterpart of an uninitialized signal).
#[derive(Debug, Clone, Default)]
pub struct InjectedSignals {
    records: BTreeMap<String, Vec<(u64, i128)>>,
}

impl InjectedSignals {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `value` for `path` from virtual `cycle` on. Records for the
    /// same path stay sorted by cycle; recording at an already-recorded
    /// cycle overrides that record.
    pub fn at(&mut self, path: &str, cycle: u64, value: i128) -> &mut Self {
        self.record(path, cycle, value);
        self
    }

    fn record(&mut self, path: &str, cycle: u64, value: i128) {
        let entries = self.records.entry(path.to_owned()).or_default();
        match entries.binary_search_by_key(&cycle, |(recorded, _)| *recorded) {
            Ok(index) => entries[index].1 = value,
            Err(index) => entries.insert(index, (cycle, value)),
        }
    }

    fn get(&self, path: &str, cycle: u64) -> Option<i128> {
        let entries = self.records.get(path)?;
        let index = match entries.binary_search_by_key(&cycle, |(recorded, _)| *recorded) {
            Ok(index) => index,
            // Before the first record the signal has no value yet.
            Err(0) => return None,
            Err(insert_at) => insert_at - 1,
        };
        Some(entries[index].1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The null backend is the trait-dispatch double: every drive
    /// operation reports the honest no-simulator error (the interpreter
    /// surfaces backend errors as `E0408`) under its registry name, and
    /// finishing is a no-op.
    #[test]
    fn null_simulator_reports_no_simulator_from_every_operation() {
        let mut simulator = NullSimulator;
        assert_eq!(simulator.name(), "null");
        let set_error = simulator.set("top.count", 1).unwrap_err();
        assert!(
            set_error.starts_with("no simulator available"),
            "error: {set_error}"
        );
        let get_error = simulator.get("top.count").unwrap_err();
        assert!(
            get_error.starts_with("no simulator available"),
            "error: {get_error}"
        );
        let run_error = simulator.run(10).unwrap_err();
        assert!(
            run_error.starts_with("no simulator available"),
            "error: {run_error}"
        );
        simulator.finish();
    }

    /// "Loading" the null backend fails up front: there is no simulator
    /// behind it, so an `hdl_load` resolved to it never pretends to
    /// succeed.
    #[test]
    fn null_simulator_cannot_be_loaded() {
        let Err(error) = NullSimulator::load("counter.sv", "top") else {
            panic!("the null backend must not load");
        };
        assert!(
            error.starts_with("no simulator available"),
            "error: {error}"
        );
    }

    /// The registry answers the null backend with the same up-front
    /// failure (on native and wasm32 builds alike — no simulator binary
    /// involved).
    #[test]
    fn registry_answers_the_null_backend_up_front() {
        let Err(error) = load_backend(NullSimulator::NAME, "counter.sv", "top") else {
            panic!("the null backend must not load through the registry");
        };
        assert!(
            error.starts_with("no simulator available"),
            "error: {error}"
        );
    }

    /// Unknown names answer with the registered list — `vhpi` names the
    /// well-known future third backend that is deliberately not
    /// registered.
    #[test]
    fn registry_answers_unknown_backend_names() {
        let Err(error) = load_backend("vhpi", "counter.sv", "top") else {
            panic!("'vhpi' must not resolve to a backend");
        };
        assert!(
            error.contains("unknown simulator backend 'vhpi'"),
            "error: {error}"
        );
        assert!(
            registered_backends().contains(&NullSimulator::NAME),
            "registered: {:?}",
            registered_backends()
        );
    }

    /// The dispatch the interpreter relies on: a `SignalSource::Real`
    /// holding a `Box<dyn Simulator>` surfaces the backend's errors
    /// verbatim, whatever backend is in the box (the null one here — no
    /// simulator binary needed).
    #[test]
    fn signal_source_real_dispatches_to_the_loaded_backend() {
        let mut source = SignalSource::Real(Box::new(NullSimulator));
        let get_error = source.get("top.count", 0).unwrap_err();
        assert!(
            get_error.starts_with("no simulator available"),
            "error: {get_error}"
        );
        let set_error = source.set("top.count", 1, 0).unwrap_err();
        assert!(
            set_error.starts_with("no simulator available"),
            "error: {set_error}"
        );
        let run_error = source.run(10).unwrap_err();
        assert!(
            run_error.starts_with("no simulator available"),
            "error: {run_error}"
        );
        source.finish();
    }

    /// The default backend is one of the registered ones: the xezim
    /// bridge on native targets, the null backend under wasm32 (where
    /// the xezim constructor is not compiled).
    #[test]
    fn default_backend_is_registered() {
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(default_backend(), XezimSimulator::NAME);
        #[cfg(target_arch = "wasm32")]
        assert_eq!(default_backend(), NullSimulator::NAME);
        assert!(registered_backends().contains(&default_backend()));
    }

    /// The xezim backend keeps its registry name and stays registered on
    /// native targets (spawning needs the container, so only the name
    /// and the registration are asserted here — the protocol itself is
    /// exercised by the compose `mixed` service).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn xezim_backend_is_registered_under_its_name() {
        assert_eq!(XezimSimulator::NAME, "xezim");
        assert!(registered_backends().contains(&"xezim"));
    }
}
