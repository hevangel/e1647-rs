//! Single-source browser pipeline. This is an experimental API, not an
//! IEEE conformance claim. Imports are rejected rather than silently
//! ignored. The shipped client runs the WASM ABI in a disposable worker
//! with a deadline and a build-time memory maximum; native library calls
//! to [`evaluate`] do not acquire those isolation limits.

use crate::{ast::Member, coverage::TypeCoverage, diagnostic::Diagnostic};

pub const API_VERSION: u32 = 1;
pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
pub const MAX_RESULT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Parse,
    Check,
    Run,
}

impl Operation {
    fn name(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::Check => "check",
            Self::Run => "run",
        }
    }
}

#[derive(Debug)]
pub struct Response {
    pub operation: Operation,
    pub seed: u64,
    pub diagnostics: Vec<Diagnostic>,
    pub output: String,
    pub coverage: Vec<TypeCoverage>,
    pub dut_errors: usize,
    pub error: Option<(&'static str, &'static str)>,
}

impl Response {
    pub fn ok(&self) -> bool {
        self.error.is_none() && self.diagnostics.is_empty() && self.dut_errors == 0
    }

    /// JSON v1 uses decimal strings for the seed, counters and bucket bounds so
    /// JavaScript never rounds a 64/128-bit integer through Number.
    pub fn to_json(&self) -> String {
        let diagnostics = self.diagnostics.iter().map(|diagnostic| format!(
            "{{\"code\":{},\"message\":{},\"start\":{{\"byte\":{},\"line\":{},\"column\":{}}},\"end\":{{\"byte\":{},\"line\":{},\"column\":{}}}}}",
            json_string(diagnostic.code), json_string(&diagnostic.message),
            diagnostic.start.byte, diagnostic.start.line, diagnostic.start.column,
            diagnostic.end.byte, diagnostic.end.line, diagnostic.end.column,
        )).collect::<Vec<_>>().join(",");
        let coverage = self.coverage.iter().flat_map(|ty| ty.groups.iter().map(|group| {
            let items = group.items.iter().map(|item| {
                let buckets = item.buckets.iter().map(|bucket| {
                    let (lo, hi) = bucket.bounds_text();
                    let upper_bound = if bucket.real_bounds().is_some() {
                        format!(",\"upperInclusive\":{}", bucket.upper_inclusive())
                    } else {
                        String::new()
                    };
                    format!(
                        "{{\"lo\":{},\"hi\":{},\"label\":{},\"hits\":{},\"goal\":{},\"excluded\":{}{upper_bound}}}",
                        json_string(&lo), json_string(&hi), json_string(&bucket.label),
                        json_string(&bucket.hits.to_string()), json_string(&bucket.goal.to_string()), bucket.excluded,
                    )
                }).collect::<Vec<_>>().join(",");
                format!("{{\"name\":{},\"samples\":{},\"ungradeableHits\":{},\"buckets\":[{}]}}",
                    json_string(&item.name), json_string(&item.samples.to_string()), json_string(&item.ungradeable_hits.to_string()), buckets)
            }).collect::<Vec<_>>().join(",");
            let derived = group.derived.iter().map(|item| {
                let kind = match item.kind {
                    crate::ast::CoverItemKind::Cross => "cross",
                    crate::ast::CoverItemKind::Transition => "transition",
                    crate::ast::CoverItemKind::Item => unreachable!("derived coverage kind"),
                };
                let sources = item.sources.iter().map(|source| json_string(source)).collect::<Vec<_>>().join(",");
                // Keep an array: distinct bucket identities can have the same
                // rendered label, and a label-keyed object would lose data.
                let combinations = item.combos.iter().map(|(label, hits)| format!(
                    "{{\"label\":{},\"hits\":{}}}", json_string(label), json_string(&hits.to_string()),
                )).collect::<Vec<_>>().join(",");
                let total = item.total_combos.map_or("null".to_owned(), |total| json_string(&total.to_string()));
                format!("{{\"name\":{},\"kind\":{},\"sources\":[{}],\"samples\":{},\"goal\":{},\"weight\":{},\"totalCombos\":{},\"grade\":{},\"combinations\":[{}]}}",
                    json_string(&item.name), json_string(kind), sources, json_string(&item.samples.to_string()),
                    json_string(&item.goal.to_string()), json_string(&item.weight.to_string()), total, json_grade(item.grade()), combinations)
            }).collect::<Vec<_>>().join(",");
            format!("{{\"type\":{},\"name\":{},\"unit\":{},\"samples\":{},\"grade\":{},\"items\":[{}],\"derived\":[{}]}}",
                json_string(&ty.type_name), json_string(&group.name),
                group.unit_instance.as_deref().map_or("null".to_owned(), json_string), json_string(&group.samples.to_string()), json_grade(group.grade()), items, derived)
        })).collect::<Vec<_>>().join(",");
        let error = self.error.map_or("null".to_owned(), |(code, message)| {
            format!(
                "{{\"code\":{},\"message\":{}}}",
                json_string(code),
                json_string(message)
            )
        });
        let json = format!(
            "{{\"version\":{API_VERSION},\"operation\":{},\"seed\":{},\"ok\":{},\"output\":{},\"diagnostics\":[{}],\"dutErrors\":{},\"coverage\":[{}],\"error\":{}}}",
            json_string(self.operation.name()),
            json_string(&self.seed.to_string()),
            self.ok(),
            json_string(&self.output),
            diagnostics,
            self.dut_errors,
            coverage,
            error
        );
        if json.len() > MAX_RESULT_BYTES {
            api_error_json(
                "result_limit",
                "The result exceeds the browser response limit",
            )
        } else {
            json
        }
    }
}

fn json_grade(grade: Option<f64>) -> String {
    grade
        .filter(|value| value.is_finite())
        .map_or("null".to_owned(), |value| value.to_string())
}

fn json_string(value: &str) -> String {
    use std::fmt::Write;
    let mut json = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            character if character <= '\u{1f}' => {
                write!(json, "\\u{:04x}", character as u32).expect("String writes cannot fail");
            }
            character => json.push(character),
        }
    }
    json.push('"');
    json
}

fn api_error_json(code: &str, message: &str) -> String {
    format!(
        "{{\"version\":{API_VERSION},\"ok\":false,\"output\":\"\",\"diagnostics\":[],\"coverage\":[],\"dutErrors\":0,\"error\":{{\"code\":{},\"message\":{}}}}}",
        json_string(code),
        json_string(message)
    )
}

fn contains_import(members: &[Member]) -> bool {
    members.iter().any(|member| match member {
        Member::Import { .. } => true,
        Member::When(when) => contains_import(&when.members),
        _ => false,
    })
}

/// Parse/check/run one UTF-8 source with the same staged front-end and
/// interpreter as the native library. No multi-file loader is invoked.
pub fn evaluate(source: &str, operation: Operation, seed: u64) -> Response {
    let mut response = Response {
        operation,
        seed,
        diagnostics: Vec::new(),
        output: String::new(),
        coverage: Vec::new(),
        dut_errors: 0,
        error: None,
    };
    if source.len() > MAX_SOURCE_BYTES {
        response.error = Some(("input_limit", "The source exceeds the browser input limit"));
        return response;
    }
    let parsed = crate::parser::parse_unchecked(source);
    response.diagnostics = parsed.diagnostics;
    if !response.diagnostics.is_empty() {
        return response;
    }
    if parsed
        .declarations
        .iter()
        .any(|declaration| contains_import(&declaration.members))
    {
        response.error = Some((
            "unsupported_import",
            "The browser API accepts one source; imports are unavailable",
        ));
        return response;
    }
    response
        .diagnostics
        .extend(crate::check::check_declarations(&parsed.declarations));
    if !response.diagnostics.is_empty() || operation == Operation::Parse {
        return response;
    }
    let elaborated = crate::elab::elaborate(&parsed.declarations);
    response
        .diagnostics
        .extend(elaborated.diagnostics.iter().cloned());
    response
        .diagnostics
        .extend(crate::typing::check_types(&elaborated));
    if !response.diagnostics.is_empty() || operation == Operation::Check {
        return response;
    }
    let result = crate::interp::run(&elaborated, seed);
    response.output = result.output;
    response.diagnostics = result.diagnostics;
    response.coverage = result.coverage;
    response.dut_errors = result.dut_errors;
    response
}

#[cfg(target_arch = "wasm32")]
mod abi {
    use super::*;
    use std::{cell::RefCell, collections::BTreeMap};

    const MAX_HANDLES: usize = 8;
    struct Input {
        bytes: Box<[u8]>,
        length: u32,
    }
    #[derive(Default)]
    struct Buffers {
        inputs: BTreeMap<u32, Input>,
        results: BTreeMap<u32, Box<[u8]>>,
        next: u32,
    }
    thread_local! { static BUFFERS: RefCell<Buffers> = RefCell::new(Buffers::default()); }

    #[unsafe(no_mangle)]
    pub extern "C" fn e1647_api_version() -> u32 {
        API_VERSION
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn e1647_source_limit() -> u32 {
        MAX_SOURCE_BYTES as u32
    }

    /// Only pointers to owned registered buffers are accepted by execute.
    /// The ABI never dereferences a pointer supplied by its caller.
    #[unsafe(no_mangle)]
    pub extern "C" fn e1647_alloc(length: u32) -> u32 {
        BUFFERS.with_borrow_mut(|buffers| {
            if length as usize > MAX_SOURCE_BYTES || buffers.inputs.len() >= MAX_HANDLES {
                return 0;
            }
            let mut bytes = vec![0; (length as usize).max(1)].into_boxed_slice();
            let pointer = bytes.as_mut_ptr() as u32;
            buffers.inputs.insert(pointer, Input { bytes, length });
            pointer
        })
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn e1647_free(pointer: u32) -> u32 {
        BUFFERS.with_borrow_mut(|buffers| u32::from(buffers.inputs.remove(&pointer).is_some()))
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn e1647_execute(
        pointer: u32,
        length: u32,
        operation: u32,
        seed_lo: u32,
        seed_hi: u32,
    ) -> u32 {
        BUFFERS.with_borrow_mut(|buffers| {
            if buffers.results.len() >= MAX_HANDLES {
                return 0;
            }
            let result = match buffers
                .inputs
                .get(&pointer)
                .filter(|input| input.length == length)
            {
                None => api_error_json(
                    "invalid_buffer",
                    "The input buffer or length is not registered",
                ),
                Some(input) => match std::str::from_utf8(&input.bytes[..length as usize]) {
                    Err(_) => api_error_json("invalid_utf8", "The source is not valid UTF-8"),
                    Ok(source) => match operation {
                        0..=2 => evaluate(
                            source,
                            [Operation::Parse, Operation::Check, Operation::Run]
                                [operation as usize],
                            (u64::from(seed_hi) << 32) | u64::from(seed_lo),
                        )
                        .to_json(),
                        _ => api_error_json(
                            "invalid_operation",
                            "The operation must be parse, check or run",
                        ),
                    },
                },
            };
            let handle = loop {
                buffers.next = buffers.next.wrapping_add(1).max(1);
                if !buffers.results.contains_key(&buffers.next) {
                    break buffers.next;
                }
            };
            buffers
                .results
                .insert(handle, result.into_bytes().into_boxed_slice());
            handle
        })
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn e1647_result_ptr(handle: u32) -> u32 {
        BUFFERS.with_borrow(|buffers| {
            buffers
                .results
                .get(&handle)
                .map_or(0, |bytes| bytes.as_ptr() as u32)
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn e1647_result_len(handle: u32) -> u32 {
        BUFFERS.with_borrow(|buffers| {
            buffers
                .results
                .get(&handle)
                .map_or(0, |bytes| bytes.len() as u32)
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn e1647_result_free(handle: u32) -> u32 {
        BUFFERS.with_borrow_mut(|buffers| u32::from(buffers.results.remove(&handle).is_some()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_strings_preserve_unicode_and_escape_control_characters() {
        assert_eq!(
            json_string("λ\t\n\r\u{1}\\\""),
            "\"λ\\t\\n\\r\\u0001\\\\\\\"\""
        );
    }
    #[test]
    fn imports_and_large_inputs_fail_explicitly() {
        assert_eq!(
            evaluate("import missing;", Operation::Run, 1)
                .error
                .unwrap()
                .0,
            "unsupported_import"
        );
        assert_eq!(
            evaluate(&" ".repeat(MAX_SOURCE_BYTES + 1), Operation::Parse, 1)
                .error
                .unwrap()
                .0,
            "input_limit"
        );
    }
    #[test]
    fn type_errors_prevent_execution_and_dut_failures_are_not_success() {
        let response = evaluate(
            "extend sys { run() is also { var x: int = \"bad\"; out(\"must not run\"); }; };",
            Operation::Run,
            1,
        );
        assert!(
            response
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "E0220")
        );
        assert!(response.output.is_empty());
        let response = evaluate(
            "extend sys { run() is also { check that FALSE; }; };",
            Operation::Run,
            1,
        );
        assert_eq!(response.dut_errors, 1);
        assert!(!response.ok());
    }
    #[test]
    fn oversized_results_are_replaced_with_a_bounded_error() {
        let mut response = evaluate("", Operation::Parse, 1);
        response.output = "x".repeat(MAX_RESULT_BYTES);
        let json = response.to_json();
        assert!(json.contains("result_limit"));
        assert!(json.len() < 512);
    }
}
