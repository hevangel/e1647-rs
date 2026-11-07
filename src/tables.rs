//! The clause 26 table pre-pass (26.2 `table … with { … }` expansion,
//! 26.3 `table from operator(...)`) and the table value model shared
//! with the interpreter's procedural API (32.3).
//!
//! ## Where the pass runs
//!
//! A token-level pre-pass like [`crate::macros`] and [`crate::preproc`],
//! between them: the clause 21 `#define` replacement runs first (a
//! table's row data may use defined names), the clause 15 macro
//! expansion runs last (a table body may invoke macros). The pipeline
//! order lives in `src/parser.rs` (`parse`/`parse_unchecked`); each
//! file is expanded independently, like the other pre-passes.
//!
//! ## The expansion (26.2)
//!
//! `table [count] table-data with { body-code }` is a preprocessor
//! construct usable wherever a code block can appear. The pass replaces
//! the whole construct (its trailing `;` consumed with it) by one `{ … }`
//! brace group containing `body-code` once per data row, with each cell
//! reference substituted: `<#n>` (column n, from 1), `<#name>` (a header
//! name), `<#0>`/`<#row_index>` (the row index, 0-based over the data
//! rows). In nested tables `<##n>`/`<##name>` refer to the enclosing
//! table's row, `<###…>` two levels up, and so on — the number of `#`
//! marks selects the table that many levels up the nesting, where the
//! innermost table's own row is one `#`. The expansion is emitted as an
//! ordinary brace group, which every code-block position already parses
//! (file-level and struct-member groups splice transparently — clause
//! 4.3.2/4.3.3; the parser's statement reader splices action-position
//! groups the same way).
//!
//! `table` is contextual: the pass only triggers on an identifier
//! `table` followed by `count`, `{`, `[` or `from`, and — because a
//! variable could legitimately be named `table` and indexed — the brace
//! and bracket forms additionally require a depth-0 `with` after the
//! balanced data group before the pass treats them as a construct. The
//! `from` form requires the operator-call shape (`table from name(`)
//! and a depth-0 `with`; a `from` invocation without a body's `with` is
//! a malformed construct and reports `E0108`.
//!
//! `table-data` is one of:
//! 1. inline rows `{ row; row; … }` — cells separated by `|`, rows by
//!    `;` (a cell may contain `;`/`|` inside parentheses/brackets); the
//!    first row is a header when every one of its cells is `#name`;
//! 2. an inline constant set literal `[0,4..7]` — one column, one row
//!    per value, ranges enumerated;
//! 3. `from table-operator(params) [using option, …]` (26.3).
//!
//! With `count`, the data reduces to a single-cell table whose value is
//! the data-row count (header rows excluded; operator options applied).
//!
//! ## Table operators (26.3)
//!
//! Only the two predefined operators are expandable in this stage:
//! `csv_to_table(file-name, tag)` (a tiny CSV reader honouring the
//! 32.3.3.10–32.3.3.13 defaults — an empty line is only
//! whitespace/commas and ends a table, a tag line is the text until the
//! first comma, the first row after the tag is the column-name header)
//! and `excel_to_table(file-name, sheet, tag)`, which reports the
//! honest `E0419` stub (no Excel reader). A user-defined operator —
//! 26.3's final `table_manager` method returning a table type — reports
//! `E0136`: the pre-pass has no type information and cannot execute a
//! method body at parse time, so no user-defined operator is
//! expandable, whether or not its declaration would satisfy 26.3. The
//! `using` options (`filter = bool-exp`, `unique = string-exp`) are
//! evaluated per row at expansion time by a restricted expression
//! evaluator whose only row-aware call is `table_column("name-or-index")`
//! (column 0 is the row index); an option the evaluator cannot read
//! reports `E0136` and the construct is dropped.
//!
//! ## Cell references and diagnostics
//!
//! A reference is the token shape `<` `#`ⁿ column `>` (column: a number,
//! an identifier, or the word `row_index`) — whitespace between the
//! tokens is invisible, as everywhere in the token stream. Inside a
//! body it substitutes from the selected nesting level's row
//! (`E0134` "unknown table column" when the level does not exist, the
//! column is out of range, or no header carries the name). Outside any
//! table body — token shape or the same shape inside a string literal —
//! it reports `E0135` "table cell reference outside a table body".
//! `E0133` fires when a data row's cell count differs from the header's.
//!
//! ## Documented v1 approximations
//!
//! - The expansion is one brace group with the body repeated per row in
//!   the same scope — a `var` declared in the body is declared once per
//!   row there, so the second row trips the duplicate-local check. This
//!   is the literal 26.2 expansion shape (the body text is repeated);
//!   declare such variables outside the table.
//! - Constructs inside `define … as { … }` macro bodies are skipped
//!   verbatim (the macro pass owns those spans; they never expand).
//! - A cell renders to text with single spaces between its tokens when
//!   substituted into a string literal; a cell containing `"` would
//!   break the surrounding string literal and is not diagnosed.
//! - A `from`-table's cells re-tokenize for code-position splicing (the
//!   csv cell text `8` splices as the numeral 8); a cell whose text does
//!   not tokenize splices as one quoted string literal.
//! - `csv_to_table` re-reads the file at every expansion (parse-time
//!   file IO, path relative to the process working directory); the
//!   runtime `read_table_from_csv` caches per run instead (32.3's
//!   read-cache). Quoted CSV fields, escapes and separators other than
//!   `,` are not read.
//! - `skip_row(line)` returns the line's text after the first comma —
//!   the 32.3 prose available to this repository does not state the
//!   return value; the tag-complement reading is the useful one.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::diagnostic::Diagnostic;
use crate::lexer::{Position, Token, TokenKind};

// ---------------------------------------------------------------------------
// The value model (32.3): string cells, column names, csv source info.
// ---------------------------------------------------------------------------

/// One row of a [`TableValue`]: column 1..=n cells plus the csv source
/// info the readers record. Rows live behind shared handles — cloning a
/// row value aliases the same row (so `get_row(i).set_data(...)` mutates
/// the table's row), and `copy()` mints fresh handles.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableRowValue {
    /// Column 1..=n cell values. Column 0 (the row index) is not stored.
    pub cells: Vec<String>,
    pub file_name: Option<String>,
    pub line: Option<usize>,
    pub sheet_name: Option<String>,
}

impl TableRowValue {
    /// The row's source fields copied from `other`
    /// (`copy_source_info_from`, 32.3).
    pub fn copy_source_info_from(&mut self, other: &TableRowValue) {
        self.file_name = other.file_name.clone();
        self.line = other.line;
        self.sheet_name = other.sheet_name.clone();
    }
}

/// A table (32.3): named columns (column 1..=n; the names may be empty
/// for `create_table_without_column_names`), an optional header row,
/// and the data rows. A table read from a file is `read_only` — the
/// mutating methods report the copy-first error instead of touching it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableValue {
    pub columns: Vec<String>,
    pub header: Option<Rc<RefCell<TableRowValue>>>,
    pub rows: Vec<Rc<RefCell<TableRowValue>>>,
    /// The csv file the table was read from; a file-read table is
    /// cached and read-only (32.3).
    pub file_name: Option<String>,
    pub sheet_name: Option<String>,
    pub read_only: bool,
}

impl TableValue {
    /// A deep, writable copy (`copy`, 32.3): fresh row handles with the
    /// same cell text; the copy is writable even when the original was
    /// read-only (copy first, then modify).
    pub fn copied(&self) -> Self {
        Self {
            columns: self.columns.clone(),
            header: self
                .header
                .as_ref()
                .map(|row| Rc::new(RefCell::new(row.borrow().clone()))),
            rows: self
                .rows
                .iter()
                .map(|row| Rc::new(RefCell::new(row.borrow().clone())))
                .collect(),
            file_name: self.file_name.clone(),
            sheet_name: self.sheet_name.clone(),
            read_only: false,
        }
    }

    /// The 0-based index of a row handle in this table, by identity —
    /// the row-index pseudo-column (column 0) reads it.
    pub fn row_index(&self, row: &Rc<RefCell<TableRowValue>>) -> Option<usize> {
        self.rows
            .iter()
            .position(|candidate| Rc::ptr_eq(candidate, row))
    }
}

// ---------------------------------------------------------------------------
// The csv reader (32.3.3.10–32.3.3.13 defaults, as the migration plan
// summarises them).
// ---------------------------------------------------------------------------

/// Whether a csv line is empty in the table-reader sense: only
/// whitespace and commas (32.3.3 — such a line ends a table).
pub fn is_empty_line(line: &str) -> bool {
    line.chars().all(|c| c.is_whitespace() || c == ',')
}

/// The tag of a csv line: the text until the first comma, trimmed (the
/// tag line is "text until the first comma").
pub fn table_tag_from_row(line: &str) -> String {
    let tag = line.split(',').next().unwrap_or("");
    tag.trim().to_owned()
}

/// One csv line split into cells, each trimmed (the reader's cell
/// separator is `,`; quoted fields are not read).
fn csv_cells(line: &str) -> Vec<String> {
    line.split(',').map(|cell| cell.trim().to_owned()).collect()
}

/// One table read from a csv file: its tag and the table (the first row
/// after the tag line is the column-name header; data rows carry their
/// 1-based line numbers in the file).
#[derive(Debug, Clone)]
pub struct CsvTable {
    pub tag: String,
    pub table: TableValue,
}

/// The csv path with the default extension applied (`.csv` appended
/// when the path has none) — the read-cache's key and the reader's
/// resolution agree through this helper.
pub fn with_csv_extension(path: &str) -> String {
    if std::path::Path::new(path).extension().is_none() {
        format!("{path}.csv")
    } else {
        path.to_owned()
    }
}

/// Read a csv file into its tables. The default extension `.csv` is
/// appended when the path has none. Each non-empty line after a tag
/// line is a row of that tag's table; an empty line (whitespace/commas
/// only) ends the table. `Err` carries the reader's complaint.
///
/// The open table's accumulated rows: (cells, 1-based line number).
type CsvRows = Vec<(Vec<String>, usize)>;

pub fn read_csv_tables(path: &str) -> Result<Vec<CsvTable>, String> {
    let resolved = with_csv_extension(path);
    let text = std::fs::read_to_string(&resolved)
        .map_err(|err| format!("cannot read '{resolved}': {err}"))?;
    let mut tables: Vec<CsvTable> = Vec::new();
    // The open table's tag and the rows so far.
    let mut open: Option<(String, CsvRows)> = None;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if is_empty_line(line) {
            if let Some((tag, rows)) = open.take() {
                tables.push(build_csv_table(&resolved, &tag, rows));
            }
            continue;
        }
        match &mut open {
            None => {
                // A tag line opens the next table; the text after the
                // first comma belongs to the tag line and is ignored.
                open = Some((table_tag_from_row(line), Vec::new()));
            }
            Some((_, rows)) => {
                rows.push((csv_cells(line), index + 1));
            }
        }
    }
    if let Some((tag, rows)) = open.take() {
        tables.push(build_csv_table(&resolved, &tag, rows));
    }
    Ok(tables)
}

/// Assemble one csv table: the first row is the header (column names),
/// the rest are data rows with their line numbers recorded.
fn build_csv_table(file_name: &str, tag: &str, rows: CsvRows) -> CsvTable {
    let mut table = TableValue {
        file_name: Some(file_name.to_owned()),
        read_only: true,
        ..TableValue::default()
    };
    if let Some((header_cells, header_line)) = rows.first() {
        table.columns = header_cells.clone();
        table.header = Some(Rc::new(RefCell::new(TableRowValue {
            cells: header_cells.clone(),
            file_name: Some(file_name.to_owned()),
            line: Some(*header_line),
            sheet_name: None,
        })));
    }
    for (cells, line) in rows.iter().skip(1) {
        table.rows.push(Rc::new(RefCell::new(TableRowValue {
            cells: cells.clone(),
            file_name: Some(file_name.to_owned()),
            line: Some(*line),
            sheet_name: None,
        })));
    }
    CsvTable {
        tag: tag.to_owned(),
        table,
    }
}

// ---------------------------------------------------------------------------
// The pre-pass.
// ---------------------------------------------------------------------------

/// One enclosing table's current row during expansion: the row index
/// (for `<#0>`/`<#row_index>`), the cells (column 1..=n) and the header
/// names (name → 1-based column).
struct RowContext {
    index: usize,
    cells: Vec<Vec<Token>>,
    names: BTreeMap<String, usize>,
}

/// The whole table pre-pass: expand every `table … with { … }` construct
/// in the token stream, reporting the clause 26 diagnostics. Runs after
/// the `#define` replacement and before the macro expansion.
pub fn expand_tables(tokens: Vec<Token>, diagnostics: &mut Vec<Diagnostic>) -> Vec<Token> {
    let mut output = Vec::with_capacity(tokens.len());
    walk(&tokens, &mut Vec::new(), &mut output, diagnostics);
    output
}

/// Walk `tokens`, expanding constructs and substituting references.
/// `stack` carries the enclosing tables' current rows (innermost last).
fn walk(
    tokens: &[Token],
    stack: &mut Vec<RowContext>,
    output: &mut Vec<Token>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut index = 0usize;
    while index < tokens.len() {
        // A `define` definition is copied verbatim: the macro pass owns
        // its spans, and its pattern/body text must not be read as
        // ordinary code here.
        if matches!(&tokens[index].kind, TokenKind::Keyword(word) if word == "define")
            && let Some(end) = crate::macros::definition_end_at(tokens, index)
        {
            output.extend(tokens[index..end].iter().cloned());
            index = end;
            continue;
        }
        match construct_at(tokens, index) {
            Some(ConstructShape::MalformedFrom) => {
                diagnostics.push(Diagnostic::new(
                    "E0108",
                    "expected 'with' before the table body (26.2)",
                    tokens[index].start,
                    tokens[index + 1].end,
                ));
                if let Some(end) = malformed_from_end(tokens, index) {
                    index = end;
                    continue;
                }
            }
            Some(shape) => {
                if let Some(end) =
                    expand_construct(tokens, index, shape, stack, output, diagnostics)
                {
                    index = end;
                    continue;
                }
                // The expansion declined (its diagnostics are already
                // recorded); drop the construct's tokens so the broken
                // shape does not leak into the parse as garbage.
                if let Some(end) = construct_drop_end(tokens, index, shape) {
                    index = end;
                    continue;
                }
            }
            None => {}
        }
        if let Some((reference_end, depth, column_index)) = reference_at(tokens, index) {
            if stack.is_empty() {
                // A well-formed reference outside any table body (26.2):
                // reported and left as written — the parser's own error
                // for the leftover text follows.
                diagnostics.push(Diagnostic::new(
                    "E0135",
                    "table cell reference outside a table body (26.2)",
                    tokens[index].start,
                    tokens[reference_end - 1].end,
                ));
                output.extend(tokens[index..reference_end].iter().cloned());
                index = reference_end;
                continue;
            }
            match resolve_reference(tokens, column_index, stack, depth) {
                Some(replacement) => {
                    output.extend(replacement);
                    index = reference_end;
                    continue;
                }
                None => {
                    diagnostics.push(Diagnostic::new(
                        "E0134",
                        format!(
                            "unknown table column '<{}{}>' (26.2)",
                            "#".repeat(depth),
                            reference_column_text(&tokens[column_index..reference_end - 1])
                        ),
                        tokens[index].start,
                        tokens[reference_end - 1].end,
                    ));
                    output.extend(tokens[index..reference_end].iter().cloned());
                    index = reference_end;
                    continue;
                }
            }
        }
        if let TokenKind::StringLiteral(value) = &tokens[index].kind {
            output.push(substitute_string_refs(
                &tokens[index],
                value,
                stack,
                diagnostics,
            ));
            index += 1;
            continue;
        }
        output.push(tokens[index].clone());
        index += 1;
    }
}

/// The text of a reference's column token (for the diagnostic message).
fn reference_column_text(tokens: &[Token]) -> String {
    match &tokens[0].kind {
        TokenKind::Punctuation(symbol) => symbol.clone(),
        TokenKind::Identifier(word) | TokenKind::Keyword(word) => word.clone(),
        TokenKind::Number(text) | TokenKind::StringLiteral(text) => text.clone(),
    }
}

/// A well-formed cell reference at `index`: `<`, one or more `#`, one
/// column token (a number or an identifier), `>`. Returns the end index
/// (past the `>`), the `#` count and the column token's index.
fn reference_at(tokens: &[Token], index: usize) -> Option<(usize, usize, usize)> {
    if !punct_at(tokens, index, "<") {
        return None;
    }
    let mut cursor = index + 1;
    let mut depth = 0usize;
    while punct_at(tokens, cursor, "#") {
        depth += 1;
        cursor += 1;
    }
    if depth == 0 {
        return None;
    }
    match &tokens.get(cursor)?.kind {
        TokenKind::Number(_) | TokenKind::Identifier(_) => {}
        _ => return None,
    }
    cursor += 1;
    if !punct_at(tokens, cursor, ">") {
        return None;
    }
    Some((cursor + 1, depth, cursor - 1))
}

/// Resolve a reference against the row stack: `depth` `#`s select the
/// table that many levels up (1 = innermost). Returns the replacement
/// tokens — the cell's own tokens, or a synthesized number for the row
/// index — or `None` when the level, column or name does not exist.
fn resolve_reference(
    tokens: &[Token],
    column_index: usize,
    stack: &[RowContext],
    depth: usize,
) -> Option<Vec<Token>> {
    if depth > stack.len() {
        return None;
    }
    let row = &stack[stack.len() - depth];
    let column = &tokens[column_index];
    match &column.kind {
        TokenKind::Number(text) => {
            let number = crate::interp::parse_int_literal(text)?;
            if number == 0 {
                // `<#0>` — the row index as a synthesized numeral.
                return Some(vec![Token {
                    kind: TokenKind::Number(row.index.to_string()),
                    start: column.start,
                    end: column.end,
                }]);
            }
            let index = usize::try_from(number).ok()?.checked_sub(1)?;
            Some(row.cells.get(index)?.clone())
        }
        TokenKind::Identifier(name) if name == "row_index" => Some(vec![Token {
            kind: TokenKind::Number(row.index.to_string()),
            start: column.start,
            end: column.end,
        }]),
        TokenKind::Identifier(name) => {
            let position = *row.names.get(name)?;
            Some(row.cells.get(position - 1)?.clone())
        }
        _ => None,
    }
}

/// Substitute the reference-shaped text inside one string literal.
/// Resolved references become the cell's rendered text; unresolvable
/// well-formed references report `E0134` (inside a body) or `E0135`
/// (outside any) and stay as written.
fn substitute_string_refs(
    token: &Token,
    value: &str,
    stack: &[RowContext],
    diagnostics: &mut Vec<Diagnostic>,
) -> Token {
    let characters: Vec<char> = value.chars().collect();
    let mut result = String::with_capacity(value.len());
    let mut index = 0usize;
    while index < characters.len() {
        if characters[index] != '<' {
            result.push(characters[index]);
            index += 1;
            continue;
        }
        // `<` `#`ⁿ column `>` in text.
        let mut cursor = index + 1;
        let mut depth = 0usize;
        while cursor < characters.len() && characters[cursor] == '#' {
            depth += 1;
            cursor += 1;
        }
        if depth == 0 {
            result.push(characters[index]);
            index += 1;
            continue;
        }
        let column_start = cursor;
        while cursor < characters.len()
            && (characters[cursor].is_ascii_alphanumeric() || characters[cursor] == '_')
        {
            cursor += 1;
        }
        if cursor == column_start || cursor >= characters.len() || characters[cursor] != '>' {
            result.push(characters[index]);
            index += 1;
            continue;
        }
        let column_text: String = characters[column_start..cursor].iter().collect();
        let end = cursor + 1;
        match render_string_reference(stack, depth, &column_text) {
            Some(text) => result.push_str(&text),
            None => {
                let (code, message) = if stack.is_empty() {
                    (
                        "E0135",
                        "table cell reference outside a table body (26.2)".to_owned(),
                    )
                } else {
                    (
                        "E0134",
                        format!(
                            "unknown table column '<{}{}>' (26.2)",
                            "#".repeat(depth),
                            column_text
                        ),
                    )
                };
                diagnostics.push(Diagnostic::new(code, message, token.start, token.end));
                result.push('<');
                index += 1;
                continue;
            }
        }
        index = end;
    }
    Token {
        kind: TokenKind::StringLiteral(result),
        start: token.start,
        end: token.end,
    }
}

/// The rendered text of a string-position reference.
fn render_string_reference(
    stack: &[RowContext],
    depth: usize,
    column_text: &str,
) -> Option<String> {
    if depth > stack.len() {
        return None;
    }
    let row = &stack[stack.len() - depth];
    if column_text == "0" || column_text == "row_index" {
        return Some(row.index.to_string());
    }
    if let Ok(number) = column_text.parse::<usize>() {
        return row
            .cells
            .get(number.checked_sub(1)?)
            .map(|cell| render_cell_text(cell));
    }
    let column = *row.names.get(column_text)?;
    Some(render_cell_text(&row.cells[column - 1]))
}

/// A cell's tokens rendered as text (for string substitution): string
/// literals render their unescaped value, everything else its raw text,
/// joined with single spaces.
fn render_cell_text(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| match &token.kind {
            TokenKind::StringLiteral(value) => crate::interp::unescape(value),
            TokenKind::Punctuation(symbol) => symbol.clone(),
            TokenKind::Identifier(word) | TokenKind::Keyword(word) => word.clone(),
            TokenKind::Number(text) => text.clone(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_punct(token: &Token, symbol: &str) -> bool {
    matches!(&token.kind, TokenKind::Punctuation(p) if p == symbol)
}

fn is_word(token: &Token, word: &str) -> bool {
    matches!(&token.kind, TokenKind::Identifier(w) if w == word)
}

/// Whether the token at `index` is the punctuation `symbol` (`false`
/// past the end).
fn punct_at(tokens: &[Token], index: usize, symbol: &str) -> bool {
    tokens
        .get(index)
        .is_some_and(|token| is_punct(token, symbol))
}

/// Whether the token at `index` is the identifier `word` (`false` past
/// the end).
fn word_at(tokens: &[Token], index: usize, word: &str) -> bool {
    tokens.get(index).is_some_and(|token| is_word(token, word))
}

/// Whether the token at `index` is the keyword `word` — `with`, `from`
/// and `using` are Table 8 keywords and lex as keywords, unlike the
/// contextual `table`/`count` (`false` past the end).
fn keyword_at(tokens: &[Token], index: usize, word: &str) -> bool {
    tokens
        .get(index)
        .is_some_and(|token| matches!(&token.kind, TokenKind::Keyword(w) if w == word))
}

/// The construct shapes the pass recognizes at `tokens[index]`
/// (identifier `table` followed by one of the four data triggers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConstructShape {
    Rows,
    CountRows,
    Set,
    CountSet,
    From,
    CountFrom,
    /// `table from name(…)` without a body's `with` — recognized to be
    /// diagnosed and dropped (`table from` has no other reading).
    MalformedFrom,
}

/// Whether a table construct starts at `index`, and of which shape. The
/// brace/bracket forms must be followed by a depth-0 `with` after their
/// balanced data group — a variable named `table` being indexed or
/// brace-grouped stays ordinary code. The `from` form requires the
/// operator-call shape `table from name(…)`.
fn construct_at(tokens: &[Token], index: usize) -> Option<ConstructShape> {
    if !word_at(tokens, index, "table") {
        return None;
    }
    let next = tokens.get(index + 1)?;
    let count = is_word(next, "count");
    let data_index = index + 1 + usize::from(count);
    let data = tokens.get(data_index)?;
    let shape = if is_punct(data, "{") {
        match balanced_group_end(tokens, data_index, "{", "}") {
            Some(end) if keyword_at(tokens, end + 1, "with") => ConstructShape::Rows,
            _ => return None,
        }
    } else if is_punct(data, "[") {
        match balanced_group_end(tokens, data_index, "[", "]") {
            Some(end) if keyword_at(tokens, end + 1, "with") => ConstructShape::Set,
            _ => return None,
        }
    } else if matches!(&data.kind, TokenKind::Keyword(w) if w == "from") {
        let operator = tokens.get(data_index + 1)?;
        if !matches!(&operator.kind, TokenKind::Identifier(_))
            || !punct_at(tokens, data_index + 2, "(")
        {
            return None;
        }
        if depth_zero_with_after_from(tokens, data_index) {
            ConstructShape::From
        } else {
            ConstructShape::MalformedFrom
        }
    } else {
        return None;
    };
    Some(match (count, shape) {
        (true, ConstructShape::Rows) => ConstructShape::CountRows,
        (true, ConstructShape::Set) => ConstructShape::CountSet,
        (true, ConstructShape::From) => ConstructShape::CountFrom,
        (_, shape) => shape,
    })
}

/// The index of the token closing the balanced group opened at `open`
/// (`open` holds the opening symbol). Brackets nest; string literals
/// are single tokens. `None` when the group never closes.
fn balanced_group_end(
    tokens: &[Token],
    open: usize,
    open_symbol: &str,
    close_symbol: &str,
) -> Option<usize> {
    if !punct_at(tokens, open, open_symbol) {
        return None;
    }
    let mut depth = 0usize;
    let mut cursor = open;
    while cursor < tokens.len() {
        if is_punct(&tokens[cursor], open_symbol) {
            depth += 1;
        } else if is_punct(&tokens[cursor], close_symbol) {
            depth -= 1;
            if depth == 0 {
                return Some(cursor);
            }
        }
        cursor += 1;
    }
    None
}

/// Whether a depth-0 `with` keyword follows the `from` at `from_index`
/// (the operator invocation's options may carry brackets, so bracket
/// depth separates options from the body; a depth-0 `;` ends the scan).
fn depth_zero_with_after_from(tokens: &[Token], from_index: usize) -> bool {
    let mut depth = 0i32;
    for token in &tokens[from_index..] {
        if is_punct(token, "(") || is_punct(token, "[") {
            depth += 1;
        } else if is_punct(token, ")") || is_punct(token, "]") {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        } else if depth == 0 {
            if matches!(&token.kind, TokenKind::Keyword(word) if word == "with") {
                return true;
            }
            if is_punct(token, ";") {
                return false;
            }
        }
    }
    false
}

/// The end of a malformed `from` construct (no `with` found): the pass
/// drops up to and including the statement's terminating `;` when one
/// follows the invocation's closing parenthesis, else just the
/// invocation.
fn malformed_from_end(tokens: &[Token], index: usize) -> Option<usize> {
    let mut cursor = index + 1;
    if word_at(tokens, cursor, "count") {
        cursor += 1;
    }
    if !keyword_at(tokens, cursor, "from") {
        return None;
    }
    cursor += 1;
    if !matches!(&tokens.get(cursor)?.kind, TokenKind::Identifier(_)) {
        return None;
    }
    cursor += 1;
    if !punct_at(tokens, cursor, "(") {
        return None;
    }
    let close = balanced_group_end(tokens, cursor, "(", ")")?;
    let mut end = close + 1;
    if punct_at(tokens, end, ";") {
        end += 1;
    }
    Some(end)
}

/// The end of a construct whose expansion declined (diagnostics already
/// recorded): drop through the body's closing brace and the trailing
/// `;` when present.
fn construct_drop_end(tokens: &[Token], index: usize, shape: ConstructShape) -> Option<usize> {
    if shape == ConstructShape::MalformedFrom {
        return malformed_from_end(tokens, index);
    }
    let mut cursor = index + 1;
    if word_at(tokens, cursor, "count") {
        cursor += 1;
    }
    let end = if shape == ConstructShape::From || shape == ConstructShape::CountFrom {
        // `from` name ( args ) [using …] with { body } [;] — the data
        // group is the operator's `(`…`)`, the options run to the
        // depth-0 `with`, then the body braces.
        if !keyword_at(tokens, cursor, "from") {
            return None;
        }
        cursor += 1;
        if !matches!(&tokens.get(cursor)?.kind, TokenKind::Identifier(_)) {
            return None;
        }
        cursor += 1;
        if !punct_at(tokens, cursor, "(") {
            return None;
        }
        let args_close = balanced_group_end(tokens, cursor, "(", ")")?;
        cursor = args_close + 1;
        while cursor < tokens.len() && !keyword_at(tokens, cursor, "with") {
            cursor += 1;
        }
        let body_open = cursor + 1;
        let body_close = balanced_group_end(tokens, body_open, "{", "}")?;
        body_close + 1
    } else {
        // The brace/bracket data forms: the balanced data group, the
        // `with`, the body braces.
        let open_symbol = match &tokens.get(cursor)?.kind {
            TokenKind::Punctuation(p) if p == "{" || p == "[" => p.clone(),
            _ => return None,
        };
        let close_symbol = if open_symbol == "{" { "}" } else { "]" };
        let close = balanced_group_end(tokens, cursor, &open_symbol, close_symbol)?;
        if !keyword_at(tokens, close + 1, "with") {
            return None;
        }
        let body_open = close + 2;
        let body_close = balanced_group_end(tokens, body_open, "{", "}")?;
        body_close + 1
    };
    if punct_at(tokens, end, ";") {
        return Some(end + 1);
    }
    Some(end)
}

/// One parsed `table-data`: the rows (cells as token vectors) and the
/// header names when the data carried a `#name` header row (position i
/// = column i+1).
struct TableRows {
    names: Option<Vec<String>>,
    rows: Vec<Vec<Vec<Token>>>,
}

/// Expand the construct starting at `index` (shape from
/// [`construct_at`]). Emits the expansion into `output` and returns the
/// index after the construct. `None` means the expansion declined (its
/// diagnostics are already recorded) and the caller drops the tokens.
fn expand_construct(
    tokens: &[Token],
    index: usize,
    shape: ConstructShape,
    stack: &mut Vec<RowContext>,
    output: &mut Vec<Token>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<usize> {
    let start = tokens[index].start;
    let mut cursor = index + 1;
    let count = match shape {
        ConstructShape::CountRows | ConstructShape::CountSet | ConstructShape::CountFrom => {
            cursor += 1; // 'count'
            true
        }
        _ => false,
    };
    let data = match shape {
        ConstructShape::Rows | ConstructShape::CountRows => {
            let (rows, end) = parse_inline_rows(tokens, cursor, diagnostics)?;
            cursor = end;
            rows
        }
        ConstructShape::Set | ConstructShape::CountSet => {
            let (rows, end) = parse_set_items(tokens, cursor, diagnostics)?;
            cursor = end;
            rows
        }
        ConstructShape::From | ConstructShape::CountFrom => {
            let (rows, end) = expand_from_data(tokens, cursor, diagnostics)?;
            cursor = end;
            rows
        }
        ConstructShape::MalformedFrom => return None,
    };
    // `with { body }` — the shape check in [`construct_at`] guarantees
    // the `with`; the body braces are balanced.
    if !keyword_at(tokens, cursor, "with") {
        diagnostics.push(Diagnostic::new(
            "E0108",
            "expected 'with' before the table body (26.2)",
            tokens[cursor - 1].start,
            tokens[cursor - 1].end,
        ));
        return None;
    }
    cursor += 1;
    let open = cursor;
    let close = balanced_group_end(tokens, open, "{", "}")?;
    let body = &tokens[open + 1..close];
    cursor = close + 1;
    // The construct's trailing `;` is consumed with it: every block
    // context writes `table … with { … };` and the body statements carry
    // their own `;`.
    if punct_at(tokens, cursor, ";") {
        cursor += 1;
    }
    // `count` reduces the data to a single-cell table whose value is the
    // row count (26.2): the count's cell is a synthesized numeral.
    let (rows, names) = if count {
        let count_token = Token {
            kind: TokenKind::Number(data.rows.len().to_string()),
            start,
            end: tokens[close].end,
        };
        (vec![vec![vec![count_token]]], None::<Vec<String>>)
    } else {
        (data.rows, data.names)
    };
    // The expansion: one brace group, the body once per data row (26.2).
    output.push(Token {
        kind: TokenKind::Punctuation("{".to_owned()),
        start,
        end: tokens[index].end,
    });
    let name_positions: Option<BTreeMap<String, usize>> = names.map(|names| {
        names
            .iter()
            .enumerate()
            .filter(|(_, name)| !name.is_empty())
            .map(|(position, name)| (name.clone(), position + 1))
            .collect()
    });
    for (row_index, cells) in rows.iter().enumerate() {
        stack.push(RowContext {
            index: row_index,
            cells: cells.clone(),
            names: name_positions.clone().unwrap_or_default(),
        });
        walk(body, stack, output, diagnostics);
        stack.pop();
    }
    output.push(Token {
        kind: TokenKind::Punctuation("}".to_owned()),
        start: tokens[close].start,
        end: tokens[close].end,
    });
    Some(cursor)
}

/// Parse inline rows `{ row; row; … }` at `open` (the `{`). Cells are
/// separated by `|`, rows by `;`, both at bracket depth 0; the matching
/// `}` ends the data. A header row — every cell `#name` — splits off
/// with a cell-count check against the data rows (`E0133`). Returns the
/// rows and the index after the closing `}`.
fn parse_inline_rows(
    tokens: &[Token],
    open: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(TableRows, usize)> {
    let close = balanced_group_end(tokens, open, "{", "}")?;
    let mut rows: Vec<Vec<Vec<Token>>> = Vec::new();
    let mut row: Vec<Vec<Token>> = Vec::new();
    let mut cell: Vec<Token> = Vec::new();
    for token in &tokens[open + 1..close] {
        match &token.kind {
            TokenKind::Punctuation(p) if (p == "|" || p == ";") && is_cell_split(&cell) => {
                if p == "|" {
                    row.push(std::mem::take(&mut cell));
                } else {
                    row.push(std::mem::take(&mut cell));
                    rows.push(std::mem::take(&mut row));
                }
            }
            _ => cell.push(token.clone()),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        if !cell.is_empty() {
            row.push(cell);
        }
        rows.push(row);
    }
    // Header detection: the first row, every cell `#name`.
    let mut names: Option<Vec<String>> = None;
    if let Some(first) = rows.first()
        && !first.is_empty()
        && first.iter().all(|cell| is_header_cell(cell))
    {
        names = Some(
            first
                .iter()
                .map(|cell| match &cell[1].kind {
                    TokenKind::Identifier(word) => word.clone(),
                    _ => String::new(),
                })
                .collect(),
        );
        rows.remove(0);
    }
    if let Some(header_names) = &names {
        for (position, data_row) in rows.iter().enumerate() {
            if data_row.len() != header_names.len() {
                let (start, end) =
                    row_span(data_row).unwrap_or((tokens[open].start, tokens[open].end));
                diagnostics.push(Diagnostic::new(
                    "E0133",
                    format!(
                        "table row {} has {} cells, header has {} (26.2)",
                        position + 1,
                        data_row.len(),
                        header_names.len()
                    ),
                    start,
                    end,
                ));
            }
        }
    }
    Some((TableRows { names, rows }, close + 1))
}

/// The span of one row: its first cell's first token to its last cell's
/// last token. `None` when the row has no tokens.
fn row_span(row: &[Vec<Token>]) -> Option<(Position, Position)> {
    let first = row.first()?.first()?;
    let last = row.last()?.last()?;
    Some((first.start, last.end))
}

/// Whether the cell in progress is at bracket depth 0 — a `;`/`|`
/// outside any parentheses/brackets/braces splits cells and rows (26.2:
/// arbitrary text unless inside parentheses/brackets; braces tracked
/// for the list-concat spellings). Unbalanced brackets inside a cell
/// shift the count — a documented approximation.
fn is_cell_split(cell: &[Token]) -> bool {
    let mut depth = 0i32;
    for token in cell {
        match &token.kind {
            TokenKind::Punctuation(p) if p == "(" || p == "[" || p == "{" => depth += 1,
            TokenKind::Punctuation(p) if p == ")" || p == "]" || p == "}" => depth -= 1,
            _ => {}
        }
    }
    depth == 0
}

/// Whether one cell is the header shape `#name` (a `#` token followed by
/// an identifier, nothing else).
fn is_header_cell(cell: &[Token]) -> bool {
    cell.len() == 2 && is_punct(&cell[0], "#") && matches!(&cell[1].kind, TokenKind::Identifier(_))
}

/// Parse a set literal `[0,4..7]` at `open` (the `[`) into one-column
/// rows, one row per value, ranges enumerated. Only integer constants
/// are readable here; anything else reports `E0136`. Returns the rows
/// and the index after the `]`.
fn parse_set_items(
    tokens: &[Token],
    open: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(TableRows, usize)> {
    let close = balanced_group_end(tokens, open, "[", "]")?;
    let mut rows: Vec<Vec<Vec<Token>>> = Vec::new();
    let mut item: Vec<Token> = Vec::new();
    for token in &tokens[open + 1..close] {
        if is_punct(token, ",") {
            flush_set_item(&item, &mut rows, diagnostics);
            item.clear();
            continue;
        }
        item.push(token.clone());
    }
    flush_set_item(&item, &mut rows, diagnostics);
    Some((TableRows { names: None, rows }, close + 1))
}

/// Enumerate one set-literal item into one-column rows: an integer
/// constant or `lo..hi` (both inclusive).
fn flush_set_item(
    item: &[Token],
    rows: &mut Vec<Vec<Vec<Token>>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if item.is_empty() {
        return;
    }
    let span = (item[0].start, item[item.len() - 1].end);
    let values = enumerate_int_item(item);
    let Some(values) = values else {
        diagnostics.push(Diagnostic::new(
            "E0136",
            "invalid table data: a set-literal table's items must be integer constants or lo..hi ranges (26.2)",
            span.0,
            span.1,
        ));
        return;
    };
    for value in values {
        rows.push(vec![vec![Token {
            kind: TokenKind::Number(value.to_string()),
            start: span.0,
            end: span.1,
        }]]);
    }
}

/// Enumerate one set-literal item: an integer constant or `lo..hi`. The
/// range length is bounded (a runaway range would explode the stream).
fn enumerate_int_item(item: &[Token]) -> Option<Vec<i128>> {
    if item.len() == 1
        && let TokenKind::Number(text) = &item[0].kind
    {
        return crate::interp::parse_int_literal(text).map(|value| vec![value]);
    }
    if item.len() == 3
        && let (TokenKind::Number(lo), TokenKind::Punctuation(p), TokenKind::Number(hi)) =
            (&item[0].kind, &item[1].kind, &item[2].kind)
        && p == ".."
        && let (Some(lo), Some(hi)) = (
            crate::interp::parse_int_literal(lo),
            crate::interp::parse_int_literal(hi),
        )
        && hi >= lo
        && hi - lo <= 100_000
    {
        return Some((lo..=hi).collect());
    }
    None
}

/// The `from` data (26.3): resolve the operator and produce the rows.
/// Returns the rows (cells re-tokenized for code-position splicing) and
/// the index of the `with` keyword.
fn expand_from_data(
    tokens: &[Token],
    from_index: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(TableRows, usize)> {
    // `from` name ( args ) [using option = expr, …]
    let mut cursor = from_index + 1; // past `from`
    let TokenKind::Identifier(operator) = &tokens.get(cursor)?.kind else {
        return None;
    };
    let operator = operator.clone();
    cursor += 1;
    if !punct_at(tokens, cursor, "(") {
        return None;
    }
    let args_close = balanced_group_end(tokens, cursor, "(", ")")?;
    let args = &tokens[cursor + 1..args_close];
    cursor = args_close + 1;
    // Options: `using name = expr, …` — an option's expression runs to
    // a depth-0 `,` or the `with` keyword.
    let mut options: Vec<(String, Vec<Token>)> = Vec::new();
    if keyword_at(tokens, cursor, "using") {
        cursor += 1;
        loop {
            let TokenKind::Identifier(option) = &tokens.get(cursor)?.kind else {
                drop_diagnose(
                    diagnostics,
                    "E0136",
                    format!(
                        "table operator '{operator}': expected an option name after 'using' (26.3)"
                    ),
                    tokens,
                    from_index,
                    args_close,
                );
                return None;
            };
            let option = option.clone();
            cursor += 1;
            if !punct_at(tokens, cursor, "=") {
                drop_diagnose(
                    diagnostics,
                    "E0136",
                    format!(
                        "table operator '{operator}': expected '=' after the option '{option}' (26.3)"
                    ),
                    tokens,
                    from_index,
                    args_close,
                );
                return None;
            }
            cursor += 1;
            let start = cursor;
            let mut depth = 0i32;
            loop {
                let token = tokens.get(cursor)?;
                if is_punct(token, "(") || is_punct(token, "[") {
                    depth += 1;
                } else if is_punct(token, ")") || is_punct(token, "]") {
                    depth -= 1;
                } else if depth == 0
                    && (is_punct(token, ",")
                        || matches!(&token.kind, TokenKind::Keyword(word) if word == "with"))
                {
                    break;
                }
                cursor += 1;
            }
            options.push((option, tokens[start..cursor].to_vec()));
            if punct_at(tokens, cursor, ",") {
                cursor += 1;
                continue;
            }
            break;
        }
    }
    // The two predefined operators (26.3); anything else is a
    // user-defined operator, and no user-defined operator is expandable
    // in this stage (the pre-pass has no type information and cannot run
    // a method body) — E0136.
    let span = (tokens[from_index].start, tokens[args_close].end);
    let table = match operator.as_str() {
        "csv_to_table" => {
            let (file_name, tag) =
                operator_string_args(args, 2, "csv_to_table", span, diagnostics)?;
            let read = read_csv_tables(&file_name);
            let Ok(tables) = read else {
                drop_diagnose(
                    diagnostics,
                    "E0136",
                    format!(
                        "table operator 'csv_to_table' failed: {} (26.3)",
                        read.err().unwrap_or_default()
                    ),
                    tokens,
                    from_index,
                    args_close,
                );
                return None;
            };
            match tables.into_iter().find(|entry| entry.tag == tag) {
                Some(entry) => entry.table,
                None => {
                    drop_diagnose(
                        diagnostics,
                        "E0136",
                        format!(
                            "table operator 'csv_to_table' failed: no table '{tag}' in '{file_name}' (26.3)"
                        ),
                        tokens,
                        from_index,
                        args_close,
                    );
                    return None;
                }
            }
        }
        "excel_to_table" => {
            // The honest stub: no Excel reader exists in this stage.
            diagnostics.push(Diagnostic::new(
                "E0419",
                "'excel_to_table()' is recognised (clause 26.3) but not implemented in this stage",
                span.0,
                span.1,
            ));
            return None;
        }
        _ => {
            diagnostics.push(Diagnostic::new(
                "E0136",
                format!(
                    "table operator '{operator}' must be a final method of table_manager returning a table type (26.3); only the predefined csv_to_table/excel_to_table are expandable in this stage"
                ),
                span.0,
                span.1,
            ));
            return None;
        }
    };
    // Options apply at expansion time, per row: filter drops rows
    // evaluating FALSE, unique keeps the first row per distinct value
    // (applied after filter). Both may read the row through
    // `table_column("name-or-index")`.
    let filter = options.iter().find(|(name, _)| name == "filter");
    let unique = options.iter().find(|(name, _)| name == "unique");
    for (name, _) in &options {
        if name != "filter" && name != "unique" {
            drop_diagnose(
                diagnostics,
                "E0136",
                format!("table operator '{operator}': unknown 'using' option '{name}' (26.3)"),
                tokens,
                from_index,
                args_close,
            );
            return None;
        }
    }
    // The csv header supplies the column names for `<#name>` refs.
    let names = Some(table.columns.clone());
    let column_names: BTreeMap<String, usize> = table
        .columns
        .iter()
        .enumerate()
        .filter(|(_, name)| !name.is_empty())
        .map(|(position, name)| (name.clone(), position + 1))
        .collect();
    let mut rows: Vec<Vec<Vec<Token>>> = Vec::new();
    let mut seen_unique: Vec<String> = Vec::new();
    for (index, row) in table.rows.iter().enumerate() {
        let row = row.borrow();
        let cells: Vec<Vec<Token>> = row
            .cells
            .iter()
            .map(|cell| cell_tokens(cell, span))
            .collect();
        let context = RowContext {
            index,
            cells,
            names: column_names.clone(),
        };
        if let Some((_, filter)) = filter {
            match eval_option_expr(filter, &context) {
                Ok(OptionValue::Bool(true)) => {}
                Ok(OptionValue::Bool(false)) => continue,
                Ok(_) => {
                    drop_diagnose(
                        diagnostics,
                        "E0136",
                        "table 'using' option 'filter' must be a boolean expression (26.3)"
                            .to_owned(),
                        tokens,
                        from_index,
                        args_close,
                    );
                    return None;
                }
                Err(_) => {
                    drop_diagnose(
                        diagnostics,
                        "E0136",
                        "table 'using' option 'filter' is not evaluable in this stage (26.3): supported are string/integer literals, comparisons, and/or/not, and table_column(\"name-or-index\")".to_owned(),
                        tokens,
                        from_index,
                        args_close,
                    );
                    return None;
                }
            }
        }
        if let Some((_, unique)) = unique {
            match eval_option_expr(unique, &context) {
                Ok(OptionValue::Str(text)) => {
                    if seen_unique.contains(&text) {
                        continue;
                    }
                    seen_unique.push(text);
                }
                Ok(_) => {
                    drop_diagnose(
                        diagnostics,
                        "E0136",
                        "table 'using' option 'unique' must be a string expression (26.3)"
                            .to_owned(),
                        tokens,
                        from_index,
                        args_close,
                    );
                    return None;
                }
                Err(_) => {
                    drop_diagnose(
                        diagnostics,
                        "E0136",
                        "table 'using' option 'unique' is not evaluable in this stage (26.3): supported are string/integer literals, comparisons, and/or/not, and table_column(\"name-or-index\")".to_owned(),
                        tokens,
                        from_index,
                        args_close,
                    );
                    return None;
                }
            }
        }
        rows.push(context.cells);
    }
    Some((TableRows { names, rows }, cursor))
}

/// Push one diagnostic over the operator invocation's span (the helper
/// exists so every decline path records its cause before dropping).
fn drop_diagnose(
    diagnostics: &mut Vec<Diagnostic>,
    code: &'static str,
    message: String,
    tokens: &[Token],
    from_index: usize,
    args_close: usize,
) {
    diagnostics.push(Diagnostic::new(
        code,
        message,
        tokens[from_index].start,
        tokens[args_close].end,
    ));
}

/// A csv cell's tokens for code-position splicing: the cell text
/// re-tokenized (the text `8` splices as the numeral 8); a cell whose
/// text does not tokenize splices as one quoted string literal.
fn cell_tokens(cell: &str, span: (Position, Position)) -> Vec<Token> {
    match crate::lexer::tokenize(cell) {
        Ok(tokens) if !tokens.is_empty() => tokens
            .into_iter()
            .map(|mut token| {
                token.start = span.0;
                token.end = span.1;
                token
            })
            .collect(),
        _ => vec![Token {
            kind: TokenKind::StringLiteral(cell.to_owned()),
            start: span.0,
            end: span.1,
        }],
    }
}

/// The two `csv_to_table` arguments as strings: string literals only.
fn operator_string_args(
    args: &[Token],
    expected: usize,
    operator: &str,
    span: (Position, Position),
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(String, String)> {
    let mut parts: Vec<Vec<Token>> = Vec::new();
    let mut current: Vec<Token> = Vec::new();
    let mut depth = 0i32;
    for token in args {
        if is_punct(token, "(") || is_punct(token, "[") {
            depth += 1;
        } else if is_punct(token, ")") || is_punct(token, "]") {
            depth -= 1;
        } else if depth == 0 && is_punct(token, ",") {
            parts.push(std::mem::take(&mut current));
            continue;
        }
        current.push(token.clone());
    }
    if !current.is_empty() {
        parts.push(current);
    }
    if parts.len() != expected {
        diagnostics.push(Diagnostic::new(
            "E0136",
            format!(
                "table operator '{operator}' expects {expected} argument(s), got {} (26.3)",
                parts.len()
            ),
            span.0,
            span.1,
        ));
        return None;
    }
    let mut values = Vec::new();
    for part in &parts {
        match part.as_slice() {
            [
                Token {
                    kind: TokenKind::StringLiteral(value),
                    ..
                },
            ] => values.push(crate::interp::unescape(value)),
            _ => {
                diagnostics.push(Diagnostic::new(
                    "E0136",
                    format!("table operator '{operator}' expects string-literal arguments (26.3)"),
                    span.0,
                    span.1,
                ));
                return None;
            }
        }
    }
    let mut iter = values.into_iter();
    Some((iter.next()?, iter.next()?))
}

// ---------------------------------------------------------------------------
// The `using` option evaluator: a restricted constant-expression reader
// whose only row-aware call is table_column("name-or-index").
// ---------------------------------------------------------------------------

/// One evaluated option value.
#[derive(Debug, Clone, PartialEq)]
enum OptionValue {
    Str(String),
    Int(i128),
    Bool(bool),
}

/// One comparison operator of the option evaluator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Evaluate one `using` option expression for one row.
fn eval_option_expr(tokens: &[Token], context: &RowContext) -> Result<OptionValue, ()> {
    let mut evaluator = OptionEvaluator {
        tokens,
        cursor: 0,
        context,
    };
    let value = evaluator.parse_or()?;
    if evaluator.cursor != tokens.len() {
        return Err(());
    }
    Ok(value)
}

/// The restricted option-expression evaluator: or → and → not →
/// comparison → add → primary.
struct OptionEvaluator<'a> {
    tokens: &'a [Token],
    cursor: usize,
    context: &'a RowContext,
}

impl<'a> OptionEvaluator<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    fn take_punct(&mut self, symbol: &str) -> bool {
        if self.peek().is_some_and(|token| is_punct(token, symbol)) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn take_keyword(&mut self, word: &str) -> bool {
        if self
            .peek()
            .is_some_and(|token| matches!(&token.kind, TokenKind::Keyword(w) if w == word))
        {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn parse_or(&mut self) -> Result<OptionValue, ()> {
        let mut left = self.parse_and()?;
        while self.take_keyword("or") {
            let right = self.parse_and()?;
            left = match (left, right) {
                (OptionValue::Bool(a), OptionValue::Bool(b)) => OptionValue::Bool(a || b),
                _ => return Err(()),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<OptionValue, ()> {
        let mut left = self.parse_not()?;
        while self.take_keyword("and") {
            let right = self.parse_not()?;
            left = match (left, right) {
                (OptionValue::Bool(a), OptionValue::Bool(b)) => OptionValue::Bool(a && b),
                _ => return Err(()),
            };
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<OptionValue, ()> {
        if self.take_keyword("not") || self.take_punct("!") {
            let value = self.parse_not()?;
            return match value {
                OptionValue::Bool(value) => Ok(OptionValue::Bool(!value)),
                _ => Err(()),
            };
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<OptionValue, ()> {
        let left = self.parse_add()?;
        // Longest match first: `<=` before `<`.
        for (symbol, op) in [
            ("==", CompareOp::Eq),
            ("!=", CompareOp::Ne),
            ("<=", CompareOp::Le),
            (">=", CompareOp::Ge),
            ("<", CompareOp::Lt),
            (">", CompareOp::Gt),
        ] {
            if self.take_punct(symbol) {
                let right = self.parse_add()?;
                return compare(left, right, op);
            }
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<OptionValue, ()> {
        let mut left = self.parse_primary()?;
        while self.take_punct("+") {
            let right = self.parse_primary()?;
            left = match (left, right) {
                (OptionValue::Str(a), OptionValue::Str(b)) => OptionValue::Str(a + &b),
                (OptionValue::Int(a), OptionValue::Int(b)) => OptionValue::Int(a + b),
                _ => return Err(()),
            };
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<OptionValue, ()> {
        let Some(token) = self.peek() else {
            return Err(());
        };
        match &token.kind {
            TokenKind::StringLiteral(value) => {
                let value = crate::interp::unescape(value);
                self.cursor += 1;
                Ok(OptionValue::Str(value))
            }
            TokenKind::Number(text) => {
                let value = crate::interp::parse_int_literal(text).ok_or(())?;
                self.cursor += 1;
                Ok(OptionValue::Int(value))
            }
            TokenKind::Keyword(word) if word == "TRUE" => {
                self.cursor += 1;
                Ok(OptionValue::Bool(true))
            }
            TokenKind::Keyword(word) if word == "FALSE" => {
                self.cursor += 1;
                Ok(OptionValue::Bool(false))
            }
            TokenKind::Identifier(name) if name == "table_column" => {
                self.cursor += 1;
                if !self.take_punct("(") {
                    return Err(());
                }
                let column = match self.peek() {
                    Some(Token {
                        kind: TokenKind::StringLiteral(value),
                        ..
                    }) => {
                        let value = crate::interp::unescape(value);
                        self.cursor += 1;
                        Column::Name(value)
                    }
                    Some(Token {
                        kind: TokenKind::Number(text),
                        ..
                    }) => {
                        let value = crate::interp::parse_int_literal(text).ok_or(())?;
                        self.cursor += 1;
                        Column::Index(value)
                    }
                    _ => return Err(()),
                };
                if !self.take_punct(")") {
                    return Err(());
                }
                self.table_column(column)
            }
            TokenKind::Punctuation(p) if p == "(" => {
                self.cursor += 1;
                let value = self.parse_or()?;
                if !self.take_punct(")") {
                    return Err(());
                }
                Ok(value)
            }
            _ => Err(()),
        }
    }

    /// `table_column("name-or-index")`: the current row's cell. Column 0
    /// is the row index; a name must be in the header.
    fn table_column(&self, column: Column) -> Result<OptionValue, ()> {
        match column {
            Column::Index(0) => Ok(OptionValue::Str(self.context.index.to_string())),
            Column::Index(index) => {
                let position = usize::try_from(index)
                    .map_err(|_| ())?
                    .checked_sub(1)
                    .ok_or(())?;
                let cell = self.context.cells.get(position).ok_or(())?;
                Ok(OptionValue::Str(render_cell_text(cell)))
            }
            Column::Name(name) => {
                let position = *self.context.names.get(&name).ok_or(())?;
                let cell = self.context.cells.get(position - 1).ok_or(())?;
                Ok(OptionValue::Str(render_cell_text(cell)))
            }
        }
    }
}

enum Column {
    Name(String),
    Index(i128),
}

fn compare(left: OptionValue, right: OptionValue, op: CompareOp) -> Result<OptionValue, ()> {
    let ordering = match (&left, &right) {
        (OptionValue::Str(a), OptionValue::Str(b)) => a.cmp(b),
        (OptionValue::Int(a), OptionValue::Int(b)) => a.cmp(b),
        _ => return Err(()),
    };
    let result = match op {
        CompareOp::Eq => ordering.is_eq(),
        CompareOp::Ne => ordering.is_ne(),
        CompareOp::Lt => ordering.is_lt(),
        CompareOp::Le => ordering.is_le(),
        CompareOp::Gt => ordering.is_gt(),
        CompareOp::Ge => ordering.is_ge(),
    };
    Ok(OptionValue::Bool(result))
}
