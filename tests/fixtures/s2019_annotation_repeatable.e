<'
// IEEE 1647-2019 clause 35.3: two attachments of one non-repeatable
// annotation type on one entity (E0139). The declaration and the
// attachments share one file here, so the same-module rule of clause
// 35.2 also fires (E0321, once per type at its first use) — the
// composed two-module shape (declare in ann.e, attach from the
// importer) lives in the tests/parser.rs annotation tests.
annotation @reviewed { by: string; };
struct packet_s { @reviewed(by = "a") @reviewed(by = "b") x: uint; };
'>
