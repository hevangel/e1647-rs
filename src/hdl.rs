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
//!   Ordinary log lines are ignored; reserved reply tokens match exactly.
//!   Startup and complete commands have bounded deadlines. See
//!   `docs/HDL_QUALIFICATION.md` for the concrete profile and limits.
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
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::process::{Child, ChildStdin, Command, Stdio};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{self, Receiver};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};

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

    /// Checked cleanup when a backend can report a failed handshake/exit.
    /// Existing backends retain their cleanup behavior by default.
    fn finish_checked(&mut self) -> Result<(), String> {
        self.finish();
        Ok(())
    }
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
const MAX_RESPONSE_LINE: usize = 16 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[cfg(not(target_arch = "wasm32"))]
pub struct HdlSession {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: Option<Receiver<Result<String, String>>>,
    timeout: Duration,
    failed: Option<String>,
    finished: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn session_timeout(setting: Option<&str>) -> Result<Duration, String> {
    let millis = match setting {
        None => 30_000,
        Some(text) => text
            .parse::<u64>()
            .ok()
            .filter(|n| (1..=300_000).contains(n))
            .ok_or("E1647_XEZIM_TIMEOUT_MS must be an integer from 1 to 300000")?,
    };
    Ok(Duration::from_millis(millis))
}

#[cfg(not(target_arch = "wasm32"))]
fn wave_enabled(setting: Option<&str>) -> Result<bool, String> {
    match setting {
        None | Some("0") => Ok(false),
        Some("1") => Ok(true),
        _ => Err("E1647_XEZIM_WAVE must be 0 or 1".to_owned()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn valid_signal_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.len() > 511 || !path.bytes().all(|c| c.is_ascii_graphic()) {
        Err(
            "invalid signal path: expected 1..511 printable ASCII bytes without whitespace"
                .to_owned(),
        )
    } else {
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl HdlSession {
    /// Load the native xezim VPI session. A complete startup/command handshake
    /// has a 30-second deadline, configurable with E1647_XEZIM_TIMEOUT_MS
    /// (1..300000). Source and bridge paths use an argv list without a shell.
    pub fn load(design_file: &str, top: &str) -> Result<Self, String> {
        let bridge = std::env::var("E1647_XEZIM_BRIDGE")
            .unwrap_or_else(|_| "/opt/xezim/bridge.so".to_owned());
        valid_location(&bridge, "VPI bridge (E1647_XEZIM_BRIDGE)")?;
        valid_location(design_file, "design file")?;
        valid_location(top, "top module name")?;
        let setting =
            std::env::var("E1647_XEZIM_TIMEOUT_MS")
                .map(Some)
                .or_else(|error| match error {
                    std::env::VarError::NotPresent => Ok(None),
                    _ => Err("E1647_XEZIM_TIMEOUT_MS is not valid Unicode".to_owned()),
                })?;
        let timeout = session_timeout(setting.as_deref())?;
        let wave = std::env::var("E1647_XEZIM_WAVE")
            .map(Some)
            .or_else(|error| match error {
                std::env::VarError::NotPresent => Ok(None),
                _ => Err("E1647_XEZIM_WAVE is not valid Unicode".to_owned()),
            })?;
        let mut command = Command::new("xezim");
        command.args(["--simulate", "--vpi-lib", &bridge, "-s", top, design_file]);
        if wave_enabled(wave.as_deref())? {
            command.arg("--wave");
        }
        Self::spawn(command, timeout)
    }

    fn spawn(mut command: Command, timeout: Duration) -> Result<Self, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                format!("could not spawn the simulator (is xezim on PATH?): {error}")
            })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let mut session = Self {
            child,
            stdin,
            responses: None,
            timeout,
            failed: None,
            finished: false,
        };
        let Some(stdout) = stdout else {
            return Err("no stdout for the simulator".to_owned());
        };
        if session.stdin.is_none() {
            return Err("no stdin for the simulator".to_owned());
        }
        // A bounded queue prevents an eager simulator from retaining unbounded
        // log output. Dropping the receiver releases a blocked sender; killing
        // the owned child closes its output. Descendant process trees are not
        // managed by this session.
        let (sender, receiver) = mpsc::sync_channel(8);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                let result = match reader
                    .by_ref()
                    .take((MAX_RESPONSE_LINE + 1) as u64)
                    .read_until(b'\n', &mut bytes)
                {
                    Ok(0) => Err("the simulator closed its output before answering".to_owned()),
                    Ok(_) if bytes.len() > MAX_RESPONSE_LINE => {
                        Err("simulator output line exceeds 16384 bytes".to_owned())
                    }
                    Ok(_) => String::from_utf8(bytes)
                        .map(|line| line.trim_end_matches(['\r', '\n']).to_owned())
                        .map_err(|_| "simulator output is not UTF-8".to_owned()),
                    Err(error) => Err(format!("could not read from the simulator: {error}")),
                };
                let terminal = result.is_err();
                if sender.send(result).is_err() || terminal {
                    break;
                }
            }
        });
        session.responses = Some(receiver);
        let deadline = Instant::now() + timeout;
        let result = session.expect(&["READY"], deadline, &mut 0).map(|_| ());
        session.complete_exchange(result)?;
        Ok(session)
    }

    fn available(&self) -> Result<(), String> {
        if let Some(error) = &self.failed {
            Err(format!("simulator session is closed after: {error}"))
        } else if self.finished {
            Err("simulator session is finished".to_owned())
        } else {
            Ok(())
        }
    }

    fn terminate(&mut self) {
        self.stdin.take();
        self.responses.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn complete_exchange<T>(&mut self, result: Result<T, String>) -> Result<T, String> {
        if let Err(error) = &result {
            self.failed = Some(error.clone());
            self.terminate();
        }
        result
    }

    fn send(&mut self, command: &str) -> Result<(), String> {
        self.available()?;
        self.stdin
            .as_mut()
            .ok_or("simulator input is closed")?
            .write_all(format!("{command}\n").as_bytes())
            .and_then(|_| self.stdin.as_mut().unwrap().flush())
            .map_err(|error| format!("could not write to the simulator: {error}"))
    }

    fn expect(
        &self,
        expected: &[&str],
        deadline: Instant,
        bytes: &mut usize,
    ) -> Result<String, String> {
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("timed out waiting for the simulator response")?;
            let line = self
                .responses
                .as_ref()
                .ok_or("simulator output is closed")?
                .recv_timeout(remaining)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => {
                        "timed out waiting for the simulator response"
                    }
                    mpsc::RecvTimeoutError::Disconnected => "simulator output reader stopped",
                })??;
            *bytes += line.len() + 1;
            if *bytes > MAX_RESPONSE_BYTES {
                return Err("simulator response exceeds 1048576 bytes".to_owned());
            }
            let token = line
                .split_once(' ')
                .map_or(line.as_str(), |(token, _)| token);
            if !matches!(token, "READY" | "OK" | "VAL" | "ERR" | "BYE") {
                continue;
            }
            if token == "ERR" {
                return Err(line);
            }
            if !expected.contains(&token)
                || (matches!(token, "READY" | "OK" | "BYE") && line != token)
            {
                return Err(format!(
                    "unexpected simulator response '{line}', expected {}",
                    expected.join(" or ")
                ));
            }
            return Ok(line);
        }
    }

    /// Assign a signed 32-bit VPI integer. Wider transfers are not supported.
    pub fn set(&mut self, path: &str, value: i64) -> Result<(), String> {
        self.available()?;
        valid_signal_path(path)?;
        i32::try_from(value).map_err(|_| "HDL bridge writes require a signed 32-bit value")?;
        let deadline = Instant::now() + self.timeout;
        let result = (|| {
            self.send(&format!("SET {path} {value}"))?;
            self.expect(&["OK"], deadline, &mut 0).map(|_| ())
        })();
        self.complete_exchange(result)
    }

    /// Read a signed 32-bit VPI integer; the bridge rejects wider/MVL signals.
    pub fn get(&mut self, path: &str) -> Result<i128, String> {
        self.available()?;
        valid_signal_path(path)?;
        let deadline = Instant::now() + self.timeout;
        let result = (|| {
            self.send(&format!("GET {path}"))?;
            let line = self.expect(&["VAL"], deadline, &mut 0)?;
            let text = line
                .strip_prefix("VAL ")
                .ok_or("malformed simulator value response")?;
            text.parse::<i32>()
                .map(i128::from)
                .map_err(|_| format!("unreadable signed 32-bit value '{text}'"))
        })();
        self.complete_exchange(result)
    }

    /// Advance time. ERR terminates the exchange immediately; OK and READY
    /// share one deadline, rather than waiting forever after a rejected RUN.
    pub fn run(&mut self, delay: u64) -> Result<(), String> {
        self.available()?;
        let deadline = Instant::now() + self.timeout;
        let result = (|| {
            self.send(&format!("RUN {delay}"))?;
            let mut bytes = 0;
            self.expect(&["OK"], deadline, &mut bytes)?;
            self.expect(&["READY"], deadline, &mut bytes).map(|_| ())
        })();
        self.complete_exchange(result)
    }

    /// Request a clean finish, wait for BYE and a successful process exit,
    /// then reap. Missing replies/exits are bounded by the same deadline.
    pub fn finish_checked(&mut self) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        self.available()?;
        let deadline = Instant::now() + self.timeout;
        let result = (|| {
            self.send("FINISH")?;
            self.expect(&["BYE"], deadline, &mut 0)?;
            self.stdin.take();
            loop {
                if let Some(status) = self
                    .child
                    .try_wait()
                    .map_err(|e| format!("could not reap the simulator: {e}"))?
                {
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(format!("simulator exited with {status}"))
                    };
                }
                if Instant::now() >= deadline {
                    return Err("timed out waiting for simulator exit".to_owned());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })();
        let result = self.complete_exchange(result);
        if result.is_ok() {
            self.finished = true;
            self.responses.take();
        }
        result
    }

    /// Compatibility cleanup entry point. Use finish_checked to observe errors.
    pub fn finish(&mut self) {
        let _ = self.finish_checked();
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for HdlSession {
    fn drop(&mut self) {
        self.terminate();
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

    fn finish_checked(&mut self) -> Result<(), String> {
        self.session.finish_checked()
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

    /// End the session and observe backend handshake/exit errors.
    pub fn finish_checked(&mut self) -> Result<(), String> {
        match self {
            SignalSource::Real(session) => session.finish_checked(),
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

    // These owned subprocesses test transport mechanics, not HDL semantics.
    // The child is this same Rust test binary, selected with a per-process
    // environment variable; no PATH mutation, shell or external simulator.
    #[cfg(not(target_arch = "wasm32"))]
    fn fixture_command(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--quiet",
                "--exact",
                "hdl::tests::owned_protocol_fixture",
                "--nocapture",
            ])
            .env("E1647_OWNED_HDL_FIXTURE", mode);
        command
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn fixture_session(mode: &str) -> HdlSession {
        HdlSession::spawn(fixture_command(mode), Duration::from_secs(3)).unwrap()
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn owned_protocol_fixture() {
        let Ok(mode) = std::env::var("E1647_OWNED_HDL_FIXTURE") else {
            return;
        };
        if mode == "startup-timeout" {
            std::thread::sleep(Duration::from_secs(3));
            return;
        }
        if mode == "startup-eof" {
            return;
        }
        if mode == "startup-error" {
            println!("ERR owned startup failure");
            return;
        }
        if mode == "startup-malformed" {
            println!("READY extra");
            return;
        }
        println!("READYING is ordinary simulator output");
        println!("READY");
        std::io::stdout().flush().unwrap();
        for line in std::io::stdin().lock().lines() {
            let line = line.unwrap();
            if line.starts_with("RUN ") {
                match mode.as_str() {
                    "run-error" => println!("ERR owned callback failure"),
                    "run-timeout" => println!("OK"),
                    "run-order" => println!("READY"),
                    "run-malformed" => println!("OK extra"),
                    "run-deadline" => {
                        std::thread::sleep(Duration::from_millis(80));
                        println!("OK");
                        std::io::stdout().flush().unwrap();
                        std::thread::sleep(Duration::from_millis(80));
                        println!("READY");
                    }
                    _ => println!("OK\nREADY"),
                }
            } else if line.starts_with("SET ") {
                println!("OK");
            } else if line.starts_with("GET ") {
                match mode.as_str() {
                    "get-error" => println!("ERR owned missing signal"),
                    "get-value" => println!("VAL not-an-integer"),
                    "get-overflow" => println!("VAL 2147483648"),
                    "get-order" => println!("READY"),
                    "get-eof" => return,
                    "get-long" => println!("{}", "x".repeat(MAX_RESPONSE_LINE + 1)),
                    "get-noise" => {
                        for _ in 0..140 {
                            println!("{}", "x".repeat(8192));
                        }
                    }
                    "get-utf8" => {
                        std::io::stdout().write_all(&[255, b'\n']).unwrap();
                    }
                    _ => println!("VALUE is ordinary simulator output\nVAL -123"),
                }
            } else if line == "FINISH" {
                match mode.as_str() {
                    "finish-timeout" => {
                        std::thread::sleep(Duration::from_secs(3));
                        return;
                    }
                    "finish-exit-timeout" => {
                        println!("BYE");
                        std::io::stdout().flush().unwrap();
                        std::thread::sleep(Duration::from_secs(3));
                        return;
                    }
                    "finish-failed" => {
                        println!("BYE");
                        std::io::stdout().flush().unwrap();
                        std::process::exit(7);
                    }
                    "finish-error" => {
                        println!("ERR owned finish failure");
                        return;
                    }
                    _ => {
                        println!("BYE");
                        return;
                    }
                }
            }
            std::io::stdout().flush().unwrap();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn owned_session_handshakes_and_validation() {
        let mut session = fixture_session("normal");
        session.set("top.values[0]", i64::from(i32::MIN)).unwrap();
        assert!(
            session
                .set("top.x", i64::from(i32::MAX) + 1)
                .unwrap_err()
                .contains("32-bit")
        );
        for path in ["", "top.x\nFINISH", "top .x", &"x".repeat(512)] {
            assert!(
                session
                    .get(path)
                    .unwrap_err()
                    .contains("invalid signal path")
            );
        }
        assert_eq!(session.get("top.values[0]").unwrap(), -123);
        session.run(u64::MAX).unwrap();
        session.finish_checked().unwrap();
        session.finish_checked().unwrap();
        assert!(session.child.try_wait().unwrap().unwrap().success());
        assert!(session.get("top.x").unwrap_err().contains("finished"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn owned_run_error_does_not_wait_for_ready() {
        let mut session = fixture_session("run-error");
        let error = session.run(1).unwrap_err();
        assert_eq!(error, "ERR owned callback failure");
        assert!(session.child.try_wait().unwrap().is_some());
        assert!(session.get("top.x").unwrap_err().contains(&error));
        assert!(session.finish_checked().unwrap_err().contains(&error));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn owned_session_startup_failures_are_bounded() {
        for (mode, message) in [
            ("startup-timeout", "timed out"),
            ("startup-eof", "closed its output"),
            ("startup-error", "ERR owned startup failure"),
            ("startup-malformed", "unexpected simulator response"),
        ] {
            let start = Instant::now();
            let error = HdlSession::spawn(fixture_command(mode), Duration::from_millis(500))
                .err()
                .unwrap();
            assert!(error.contains(message), "{mode}: {error}");
            assert!(start.elapsed() < Duration::from_secs(5), "{mode}");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn owned_response_failures_reap_the_child() {
        for (mode, message) in [
            ("get-error", "ERR owned missing signal"),
            ("get-value", "unreadable"),
            ("get-overflow", "32-bit"),
            ("get-order", "unexpected"),
            ("get-eof", "closed its output"),
            ("get-long", "line exceeds"),
            ("get-noise", "response exceeds"),
            ("get-utf8", "not UTF-8"),
            ("run-order", "unexpected"),
            ("run-malformed", "unexpected"),
            ("run-timeout", "timed out"),
            ("run-deadline", "timed out"),
        ] {
            let mut session = fixture_session(mode);
            session.timeout = Duration::from_millis(120);
            let start = Instant::now();
            let error = if mode.starts_with("run") {
                session.run(1).unwrap_err()
            } else {
                session.get("top.x").unwrap_err()
            };
            assert!(error.contains(message), "{mode}: {error}");
            assert!(session.child.try_wait().unwrap().is_some(), "{mode}");
            assert!(start.elapsed() < Duration::from_secs(5), "{mode}");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn owned_finish_failures_are_bounded_and_reaped() {
        for (mode, message) in [
            ("finish-timeout", "timed out"),
            ("finish-exit-timeout", "timed out"),
            ("finish-failed", "exited with"),
            ("finish-error", "ERR owned finish failure"),
        ] {
            let mut session = fixture_session(mode);
            session.timeout = Duration::from_millis(120);
            let error = session.finish_checked().unwrap_err();
            assert!(error.contains(message), "{mode}: {error}");
            assert!(session.child.try_wait().unwrap().is_some(), "{mode}");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn session_timeout_settings_are_explicit() {
        assert_eq!(session_timeout(None).unwrap(), Duration::from_secs(30));
        assert_eq!(
            session_timeout(Some("1")).unwrap(),
            Duration::from_millis(1)
        );
        assert_eq!(
            session_timeout(Some("300000")).unwrap(),
            Duration::from_secs(300)
        );
        for setting in ["0", "300001", "no", "-1", ""] {
            assert!(session_timeout(Some(setting)).is_err());
        }
        assert!(!wave_enabled(None).unwrap());
        assert!(!wave_enabled(Some("0")).unwrap());
        assert!(wave_enabled(Some("1")).unwrap());
        assert!(wave_enabled(Some("yes")).is_err());
    }

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
