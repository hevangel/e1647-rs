//! Bridge to an external SystemVerilog simulator for mixed e + HDL
//! testcases.
//!
//! The simulator (xezim) is the master: it is spawned as a child process
//! with the VPI control bridge (`c/xezim_bridge.c`, see the compose
//! `mixed` service) and this side drives it over stdin/stdout with a
//! one-response-per-command line protocol:
//!
//! ```text
//! SET <path> <value>   -> OK
//! GET <path>           -> VAL <value>
//! RUN <delay>          -> OK, READY
//! FINISH               -> BYE
//! ```
//!
//! Lines the simulator prints on its own are ignored by the reader —
//! only the response prefixes are matched.
//!
//! Security posture: the spawned program is the fixed name `xezim`
//! resolved through `PATH` (the container provides it); every argument
//! (bridge module, design file, top name) is validated against a strict
//! character allowlist before the argv-list spawn — there is no shell
//! and no string concatenation into a command line.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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

pub struct HdlSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

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

impl Drop for HdlSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
