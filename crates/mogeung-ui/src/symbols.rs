//! Editor navigation: symbol outlines, fold ranges, occurrence highlighting
//! and word-under-cursor. Roadmap `R-B28`.
//!
//! All of it is pure — file body in, positions out — so everything here is
//! testable without a window.
//!
//! **These are line scanners, not parsers.** The spec (0018) deliberately
//! chose line-based scanning over adding a tree-sitter dependency: outline
//! and occurrences are *syntactic and honest about it* (pillar K — this is a
//! viewer, not an IDE). The scanners recognise declaration-shaped lines per
//! language family and will miss things (declarations split across lines,
//! braces inside strings) and occasionally over-match. That is the accepted
//! trade for zero dependencies and code one can read in a sitting; if it ever
//! needs to be *correct* rather than *helpful*, replace it wholesale with a
//! real parser rather than patching this toward accuracy.
//!
//! Everything is bounded, so pathological input (a 10 MB single line, a
//! machine-generated staircase file) clips instead of hanging:
//! - per-line scanning looks at the first [`MAX_LINE_BYTES`] bytes of a line
//! - outlines stop at [`MAX_SYMBOLS`] symbols, names at [`MAX_NAME`] chars
//! - [`fold_ranges`] returns at most [`MAX_FOLDS`] ranges
//! - [`occurrences`] returns at most [`MAX_OCCURRENCES`] hits
//! - [`word_at`] considers the first [`MAX_WORDAT_BYTES`] bytes of the line
//!   and refuses "identifiers" longer than [`MAX_WORD`] chars
//!
//! Languages are identified the way `explorer::language_of` does it: `lang`
//! is a file extension ("rs", "py", "ts", "md") or a bare filename
//! ("Makefile"). Unknown languages get an *empty* outline, never a guess.

// The pane integration for R-B28 lands separately; until it does, nothing
// calls into this module. Remove this when it is wired up.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

/// Longest prefix of a line the declaration scanners look at. A minified
/// bundle or a 10 MB single-line JSON blob is not going to have a symbol
/// worth outlining past this point.
const MAX_LINE_BYTES: usize = 4096;
/// Hard cap on outline length.
const MAX_SYMBOLS: usize = 2000;
/// Hard cap on a symbol name's length in chars.
const MAX_NAME: usize = 256;
/// Hard cap on fold ranges.
const MAX_FOLDS: usize = 512;
/// Hard cap on occurrence hits.
const MAX_OCCURRENCES: usize = 2000;
/// Longest identifier `word_at` will return.
const MAX_WORD: usize = 512;
/// How far into a single line `word_at` will look for the click column.
const MAX_WORDAT_BYTES: usize = 1 << 20;

/// What kind of thing an outline entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Type,
    Const,
    Module,
    Heading,
    Other,
}

/// One outline entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// 1-based line number.
    pub line: usize,
    pub name: String,
    pub kind: SymbolKind,
    /// Nesting depth for indenting the outline. 0 is top level, except in
    /// Markdown where it is the heading level (`#` = 1).
    pub depth: u8,
}

/// Scan a file body for its outline. `lang` is what `explorer::language_of`
/// produced. Unknown languages return an empty outline — never a guess.
pub fn outline(body: &str, lang: &str) -> Vec<Symbol> {
    match lang {
        "rs" => outline_rust(body),
        "py" | "pyi" => outline_python(body),
        "js" | "jsx" | "mjs" | "cjs" => outline_js(body, false),
        "ts" | "tsx" | "mts" | "cts" => outline_js(body, true),
        "go" => outline_go(body),
        "java" | "kt" | "kts" => outline_jvm(body),
        "md" | "markdown" => outline_markdown(body),
        _ => Vec::new(),
    }
}

/// Foldable regions as 1-based *inclusive* line ranges: brace-balanced blocks
/// starting at outline symbols for the brace languages, indentation blocks for
/// Python and YAML, heading-to-end-of-section for Markdown. At most
/// [`MAX_FOLDS`] ranges. Brace counting ignores strings and comments — a
/// brace in a string literal will skew a fold, which a fold aid survives.
pub fn fold_ranges(body: &str, lang: &str) -> Vec<(usize, usize)> {
    match lang {
        "rs" | "go" | "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" | "java"
        | "kt" | "kts" => brace_folds(body, lang),
        "py" | "pyi" | "yml" | "yaml" => indent_folds(body),
        "md" | "markdown" => heading_folds(body),
        _ => Vec::new(),
    }
}

/// Whole-word occurrences of `word` as (1-based line, 0-based char column).
/// Word boundaries are non-alphanumeric-non-underscore, so `read` never
/// matches inside `thread`. Empty and single-char words return nothing (all
/// noise, no signal), as does a "word" containing boundary characters. At
/// most [`MAX_OCCURRENCES`] hits.
pub fn occurrences(body: &str, word: &str) -> Vec<(usize, usize)> {
    let wchars = word.chars().count();
    if wchars < 2 || !word.chars().all(is_word) {
        return Vec::new();
    }
    let wbytes = word.len();
    let first = word.chars().next().unwrap();
    let mut out = Vec::new();
    'lines: for (li, line) in body.lines().enumerate() {
        let mut i = 0; // byte offset — always on a char boundary
        let mut col = 0; // char column
        let mut prev_word = false;
        while i < line.len() {
            let c = line[i..].chars().next().unwrap();
            if !prev_word && c == first && line[i..].starts_with(word) {
                let after_ok = line[i + wbytes..].chars().next().map_or(true, |a| !is_word(a));
                if after_ok {
                    out.push((li + 1, col));
                    if out.len() >= MAX_OCCURRENCES {
                        break 'lines;
                    }
                    i += wbytes;
                    col += wchars;
                    prev_word = true; // the word ends in a word char
                    continue;
                }
            }
            prev_word = is_word(c);
            i += c.len_utf8();
            col += 1;
        }
    }
    out
}

/// The identifier under a column (0-based, in chars), for click-to-highlight.
/// A click one column past a word still counts as on it, which is where a
/// click after the last character actually lands. Returns `None` past the end
/// of the line, on whitespace or punctuation with no word to the left, and
/// for "identifiers" longer than [`MAX_WORD`] chars (nothing real is).
pub fn word_at(line: &str, col_chars: usize) -> Option<String> {
    let line = clip_to(line, MAX_WORDAT_BYTES);
    let chars: Vec<char> = line.chars().collect();
    let mut i = col_chars;
    if i >= chars.len() || !is_word(chars[i]) {
        // Fall back to the char just left of the click.
        i = col_chars.checked_sub(1)?;
        if i >= chars.len() || !is_word(chars[i]) {
            return None;
        }
    }
    let mut start = i;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
        if i - start > MAX_WORD {
            return None;
        }
    }
    let mut end = i + 1;
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
        if end - start > MAX_WORD {
            return None;
        }
    }
    Some(chars[start..end].iter().collect())
}

// ---------------------------------------------------------------- shared

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// First `max` bytes of `s`, backed off to a char boundary.
fn clip_to(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn clip(line: &str) -> &str {
    clip_to(line, MAX_LINE_BYTES)
}

/// Leading whitespace width; a tab counts as 4.
fn indent_of(line: &str) -> usize {
    let mut n = 0;
    for c in line.chars() {
        match c {
            ' ' => n += 1,
            '\t' => n += 4,
            _ => break,
        }
    }
    n
}

/// The leading identifier of `s`, capped at [`MAX_NAME`].
fn ident_of(s: &str) -> String {
    s.chars().take_while(|&c| is_word(c)).take(MAX_NAME).collect()
}

/// `s` minus a leading keyword, when the keyword is followed by whitespace.
fn strip_kw<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    if rest.starts_with(char::is_whitespace) {
        Some(rest.trim_start())
    } else {
        None
    }
}

/// Strip any of `mods` (each including its trailing space) repeatedly.
fn strip_mods<'a>(mut s: &'a str, mods: &[&str]) -> &'a str {
    loop {
        let mut stripped = false;
        for m in mods {
            if let Some(r) = s.strip_prefix(m) {
                s = r.trim_start();
                stripped = true;
                break;
            }
        }
        if !stripped {
            return s;
        }
    }
}

// ---------------------------------------------------------------- Rust

fn outline_rust(body: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    // Open containers as (indent, contributes-methods). impl and trait
    // blocks make their fns Methods; mod blocks just add depth.
    let mut stack: Vec<(usize, bool)> = Vec::new();
    for (li, raw) in body.lines().enumerate() {
        if out.len() >= MAX_SYMBOLS {
            break;
        }
        let line = clip(raw);
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('#') {
            continue;
        }
        let indent = indent_of(line);
        if t.starts_with('}') {
            while stack.last().is_some_and(|&(ci, _)| indent <= ci) {
                stack.pop();
            }
            continue;
        }
        let s = strip_rust_mods(t);
        let depth = stack.len().min(u8::MAX as usize) as u8;
        let in_impl = stack.last().is_some_and(|&(_, m)| m);
        let line_no = li + 1;

        if let Some(r) = strip_kw(s, "fn") {
            let kind = if in_impl { SymbolKind::Method } else { SymbolKind::Function };
            push_sym(&mut out, line_no, ident_of(r), kind, depth);
        } else if let Some(r) = strip_kw(s, "struct").or_else(|| strip_kw(s, "enum")).or_else(|| strip_kw(s, "union")) {
            push_sym(&mut out, line_no, ident_of(r), SymbolKind::Type, depth);
        } else if let Some(r) = strip_kw(s, "trait") {
            push_sym(&mut out, line_no, ident_of(r), SymbolKind::Type, depth);
            if t.ends_with('{') {
                stack.push((indent, true));
            }
        } else if let Some(r) = s.strip_prefix("impl").filter(|r| r.starts_with(char::is_whitespace) || r.starts_with('<')) {
            push_sym(&mut out, line_no, impl_name(r), SymbolKind::Type, depth);
            if t.ends_with('{') {
                stack.push((indent, true));
            }
        } else if let Some(r) = strip_kw(s, "mod") {
            push_sym(&mut out, line_no, ident_of(r), SymbolKind::Module, depth);
            if t.ends_with('{') {
                stack.push((indent, false));
            }
        } else if let Some(r) = strip_kw(s, "const") {
            // `const fn` was consumed by the modifier strip; this is a value.
            push_sym(&mut out, line_no, ident_of(r), SymbolKind::Const, depth);
        } else if let Some(r) = strip_kw(s, "static") {
            let r = strip_mods(r, &["mut "]);
            push_sym(&mut out, line_no, ident_of(r), SymbolKind::Const, depth);
        }
    }
    out
}

/// Strip Rust visibility and qualifier prefixes off a declaration line.
fn strip_rust_mods(mut s: &str) -> &str {
    loop {
        if let Some(r) = s.strip_prefix("pub") {
            let r = if let Some(inner) = r.strip_prefix('(') {
                match inner.find(')') {
                    Some(p) => &inner[p + 1..],
                    None => return s,
                }
            } else {
                r
            };
            if r.starts_with(char::is_whitespace) {
                s = r.trim_start();
                continue;
            }
        }
        if let Some(r) = s.strip_prefix("const ") {
            // Only a qualifier when a callable follows; `const NAME` stays.
            let r = r.trim_start();
            if r.starts_with("fn ") || r.starts_with("unsafe ") || r.starts_with("extern ") {
                s = r;
                continue;
            }
        }
        if let Some(r) = s.strip_prefix("extern ") {
            // Skip an optional ABI string: extern "C" fn …
            let r = r.trim_start();
            let r = match r.strip_prefix('"').and_then(|q| q.find('"').map(|p| &q[p + 1..])) {
                Some(rest) => rest.trim_start(),
                None => r,
            };
            s = r;
            continue;
        }
        let next = strip_mods(s, &["unsafe ", "async ", "default "]);
        if next.len() == s.len() {
            return s;
        }
        s = next;
    }
}

/// Name for an `impl` line: generics skipped, cut at `{` or `where`.
/// `impl<T> Display for Foo<T> {` → "Display for Foo<T>".
fn impl_name(rest: &str) -> String {
    let mut s = rest.trim_start();
    if s.starts_with('<') {
        let mut depth = 0i32;
        let mut end = s.len();
        for (i, c) in s.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        s = s[end..].trim_start();
    }
    let cut = s.find(" where").or_else(|| s.find('{')).unwrap_or(s.len());
    s[..cut].trim().chars().take(MAX_NAME).collect()
}

// ---------------------------------------------------------------- Python

fn outline_python(body: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    // Open scopes as (indent, is-class); popped by dedent, not braces.
    let mut stack: Vec<(usize, bool)> = Vec::new();
    for (li, raw) in body.lines().enumerate() {
        if out.len() >= MAX_SYMBOLS {
            break;
        }
        let line = clip(raw);
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let indent = indent_of(line);
        while stack.last().is_some_and(|&(ci, _)| indent <= ci) {
            stack.pop();
        }
        let s = strip_mods(t, &["async "]);
        let depth = stack.len().min(u8::MAX as usize) as u8;
        if let Some(r) = strip_kw(s, "def") {
            let in_class = stack.last().is_some_and(|&(_, c)| c);
            let kind = if in_class { SymbolKind::Method } else { SymbolKind::Function };
            push_sym(&mut out, li + 1, ident_of(r), kind, depth);
            stack.push((indent, false));
        } else if let Some(r) = strip_kw(s, "class") {
            push_sym(&mut out, li + 1, ident_of(r), SymbolKind::Type, depth);
            stack.push((indent, true));
        }
    }
    out
}

// ---------------------------------------------------------------- JS / TS

/// Words that look like `name(` but open control flow, not a method.
const CONTROL_WORDS: &[&str] = &[
    "if", "for", "while", "switch", "catch", "else", "do", "try", "return", "new", "function",
    "await", "typeof", "delete", "void", "in", "of", "with", "super", "this", "throw", "yield",
    "case", "synchronized", "assert",
];

fn outline_js(body: &str, ts: bool) -> Vec<Symbol> {
    let mut out = Vec::new();
    let mut class_stack: Vec<usize> = Vec::new();
    for (li, raw) in body.lines().enumerate() {
        if out.len() >= MAX_SYMBOLS {
            break;
        }
        let line = clip(raw);
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") {
            continue;
        }
        let indent = indent_of(line);
        if t.starts_with('}') {
            while class_stack.last().is_some_and(|&ci| indent <= ci) {
                class_stack.pop();
            }
            continue;
        }
        let s = strip_mods(t, &["export default ", "export ", "declare ", "abstract "]);
        let depth = class_stack.len().min(u8::MAX as usize) as u8;
        let line_no = li + 1;

        if let Some(r) = strip_mods_once(s, &["async "]).strip_prefix("function") {
            // function foo(  /  function* foo(
            let r = r.strip_prefix('*').unwrap_or(r).trim_start();
            let name = ident_of(r);
            if !name.is_empty() {
                push_sym(&mut out, line_no, name, SymbolKind::Function, depth);
            }
            continue;
        }
        if let Some(r) = strip_kw(s, "class") {
            push_sym(&mut out, line_no, ident_of(r), SymbolKind::Type, depth);
            class_stack.push(indent);
            continue;
        }
        if ts {
            if let Some(r) = strip_kw(s, "interface").or_else(|| strip_kw(s, "enum")) {
                push_sym(&mut out, line_no, ident_of(r), SymbolKind::Type, depth);
                continue;
            }
            if let Some(r) = strip_kw(s, "type").filter(|r| r.contains('=')) {
                push_sym(&mut out, line_no, ident_of(r), SymbolKind::Type, depth);
                continue;
            }
            if let Some(r) = strip_kw(s, "namespace") {
                push_sym(&mut out, line_no, ident_of(r), SymbolKind::Module, depth);
                continue;
            }
        }
        // `const x = (…) =>` — top level only, per spec: no indent, no class.
        if indent == 0 && class_stack.is_empty() {
            if let Some(r) = strip_kw(s, "const")
                .or_else(|| strip_kw(s, "let"))
                .or_else(|| strip_kw(s, "var"))
            {
                if s.contains("=>") || s.contains("= function") {
                    let name = ident_of(r);
                    if !name.is_empty() {
                        push_sym(&mut out, line_no, name, SymbolKind::Function, depth);
                    }
                }
                continue;
            }
        }
        // Method-looking line inside a class body: `[static|async|get|set] name(…) {`
        if !class_stack.is_empty() && t.ends_with('{') {
            let r = strip_mods(
                s,
                &["static ", "async ", "get ", "set ", "public ", "private ", "protected ",
                  "readonly ", "override ", "* ", "*"],
            );
            let name = ident_of(r);
            if !name.is_empty()
                && !CONTROL_WORDS.contains(&name.as_str())
                && r[name.len()..].trim_start().starts_with('(')
            {
                push_sym(&mut out, line_no, name, SymbolKind::Method, depth);
            }
        }
    }
    out
}

/// Strip each of `mods` at most once, in order.
fn strip_mods_once<'a>(mut s: &'a str, mods: &[&str]) -> &'a str {
    for m in mods {
        if let Some(r) = s.strip_prefix(m) {
            s = r.trim_start();
        }
    }
    s
}

// ---------------------------------------------------------------- Go

fn outline_go(body: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    for (li, raw) in body.lines().enumerate() {
        if out.len() >= MAX_SYMBOLS {
            break;
        }
        // Go declarations live at column 0; everything indented is a body.
        if raw.starts_with(char::is_whitespace) {
            continue;
        }
        let t = clip(raw).trim_end();
        let line_no = li + 1;
        if let Some(r) = strip_kw(t, "func") {
            if let Some(recv) = r.strip_prefix('(') {
                // Method: skip the `(recv T)` receiver, take the name after.
                if let Some(p) = recv.find(')') {
                    let name = ident_of(recv[p + 1..].trim_start());
                    push_sym(&mut out, line_no, name, SymbolKind::Method, 0);
                }
            } else {
                push_sym(&mut out, line_no, ident_of(r), SymbolKind::Function, 0);
            }
        } else if let Some(r) = strip_kw(t, "type") {
            // `type (` grouped blocks yield an empty name and are skipped.
            push_sym(&mut out, line_no, ident_of(r), SymbolKind::Type, 0);
        } else if let Some(r) = strip_kw(t, "const") {
            push_sym(&mut out, line_no, ident_of(r), SymbolKind::Const, 0);
        } else if let Some(r) = strip_kw(t, "var") {
            push_sym(&mut out, line_no, ident_of(r), SymbolKind::Other, 0);
        }
    }
    out
}

// ---------------------------------------------------------------- Java / Kotlin

fn outline_jvm(body: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    let mut class_stack: Vec<usize> = Vec::new();
    for (li, raw) in body.lines().enumerate() {
        if out.len() >= MAX_SYMBOLS {
            break;
        }
        let line = clip(raw);
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") || t.starts_with('@') {
            continue;
        }
        let indent = indent_of(line);
        if t.starts_with('}') {
            while class_stack.last().is_some_and(|&ci| indent <= ci) {
                class_stack.pop();
            }
            continue;
        }
        let s = strip_mods(
            t,
            &["public ", "private ", "protected ", "static ", "final ", "abstract ", "sealed ",
              "open ", "data ", "internal ", "override ", "synchronized ", "native ", "inline ",
              "suspend ", "operator ", "default "],
        );
        let depth = class_stack.len().min(u8::MAX as usize) as u8;
        let line_no = li + 1;

        if let Some(r) = strip_kw(s, "class")
            .or_else(|| strip_kw(s, "interface"))
            .or_else(|| strip_kw(s, "enum"))
            .or_else(|| strip_kw(s, "record"))
            .or_else(|| strip_kw(s, "object"))
        {
            push_sym(&mut out, line_no, ident_of(r), SymbolKind::Type, depth);
            if !t.ends_with(';') {
                class_stack.push(indent);
            }
            continue;
        }
        if let Some(r) = strip_kw(s, "fun") {
            // Kotlin. Extension receivers (`fun Foo.bar`) keep the last ident.
            let name = r
                .split(|c: char| !is_word(c) && c != '.')
                .next()
                .unwrap_or("")
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_string();
            let kind = if class_stack.is_empty() { SymbolKind::Function } else { SymbolKind::Method };
            push_sym(&mut out, line_no, name.chars().take(MAX_NAME).collect(), kind, depth);
            continue;
        }
        // Java method-looking line inside a class: `Ret name(args…) {` — an
        // identifier hard against `(`, brace on the same line, and no `=`,
        // `.` or control keyword before the paren (filters calls & lambdas).
        if !class_stack.is_empty() && t.ends_with('{') {
            if let Some(paren) = s.find('(') {
                let head = &s[..paren];
                if !head.contains('=') && !head.contains('.') {
                    let name: String = head
                        .chars()
                        .rev()
                        .take_while(|&c| is_word(c))
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    if !name.is_empty() && !CONTROL_WORDS.contains(&name.as_str()) {
                        push_sym(
                            &mut out,
                            line_no,
                            name.chars().take(MAX_NAME).collect(),
                            SymbolKind::Method,
                            depth,
                        );
                    }
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------- Markdown

fn outline_markdown(body: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for (li, raw) in body.lines().enumerate() {
        if out.len() >= MAX_SYMBOLS {
            break;
        }
        let t = clip(raw).trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || !t.starts_with('#') {
            continue;
        }
        let level = t.chars().take_while(|&c| c == '#').count();
        if level > 6 {
            continue; // ####### is not a heading
        }
        let rest = &t[level..];
        if !rest.is_empty() && !rest.starts_with(' ') {
            continue; // "#hashtag" is not a heading
        }
        // Strip a closed-ATX trailer: `## Title ##`.
        let name: String = rest
            .trim()
            .trim_end_matches('#')
            .trim_end()
            .chars()
            .take(MAX_NAME)
            .collect();
        out.push(Symbol {
            line: li + 1,
            name,
            kind: SymbolKind::Heading,
            depth: level.min(u8::MAX as usize) as u8,
        });
    }
    out
}

/// Push a symbol unless its name came out empty (a scanner mis-hit).
fn push_sym(out: &mut Vec<Symbol>, line: usize, name: String, kind: SymbolKind, depth: u8) {
    if name.is_empty() {
        return;
    }
    out.push(Symbol { line, name, kind, depth });
}

// ---------------------------------------------------------------- folds

/// Brace folds: one pass matches every `{`…`}` pair by line, then each
/// outline symbol claims the block opening on its line (or the next two, for
/// brace-on-next-line styles — but never a line that is itself a symbol).
fn brace_folds(body: &str, lang: &str) -> Vec<(usize, usize)> {
    let mut close_of: HashMap<usize, usize> = HashMap::new();
    let mut open_stack: Vec<usize> = Vec::new();
    for (li, raw) in body.lines().enumerate() {
        for c in raw.chars() {
            match c {
                '{' => open_stack.push(li + 1),
                '}' => {
                    if let Some(open) = open_stack.pop() {
                        // Outermost pair on a line wins (it pops last).
                        close_of.insert(open, li + 1);
                    }
                }
                _ => {}
            }
        }
    }
    let syms = outline(body, lang);
    let sym_lines: HashSet<usize> = syms.iter().map(|s| s.line).collect();
    let mut out = Vec::new();
    for sym in &syms {
        if out.len() >= MAX_FOLDS {
            break;
        }
        for l in sym.line..=sym.line + 2 {
            if l > sym.line && sym_lines.contains(&l) {
                break; // that brace belongs to the next symbol
            }
            if let Some(&close) = close_of.get(&l) {
                if close > sym.line {
                    out.push((sym.line, close));
                }
                break;
            }
        }
    }
    out
}

/// Indentation folds for Python and YAML: a line folds to the last non-blank
/// line before the next line at its indent or shallower. Single pass with a
/// dedent stack; capped at [`MAX_FOLDS`] ranges (outermost-first after sort).
fn indent_folds(body: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (1-based line, indent)
    let mut last_nonblank = 0usize;
    let record = |start: usize, end: usize, out: &mut Vec<(usize, usize)>| {
        if end > start && out.len() < MAX_FOLDS * 4 {
            out.push((start, end));
        }
    };
    for (li, raw) in body.lines().enumerate() {
        let line = clip(raw);
        if line.trim().is_empty() {
            continue;
        }
        let indent = indent_of(line);
        while stack.last().is_some_and(|&(_, si)| si >= indent) {
            let (s, _) = stack.pop().unwrap();
            record(s, last_nonblank, &mut out);
        }
        stack.push((li + 1, indent));
        last_nonblank = li + 1;
    }
    while let Some((s, _)) = stack.pop() {
        record(s, last_nonblank, &mut out);
    }
    out.sort_unstable();
    out.truncate(MAX_FOLDS);
    out
}

/// Markdown folds: a heading folds to the line before the next heading at its
/// level or higher, or to end of file.
fn heading_folds(body: &str) -> Vec<(usize, usize)> {
    let heads = outline_markdown(body);
    let total = body.lines().count();
    let mut out = Vec::new();
    for (i, h) in heads.iter().enumerate() {
        if out.len() >= MAX_FOLDS {
            break;
        }
        let end = heads[i + 1..]
            .iter()
            .find(|n| n.depth <= h.depth)
            .map(|n| n.line - 1)
            .unwrap_or(total);
        if end > h.line {
            out.push((h.line, end));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use SymbolKind::*;

    /// (line, name, kind, depth) for terse assertions.
    fn flat(syms: &[Symbol]) -> Vec<(usize, &str, SymbolKind, u8)> {
        syms.iter().map(|s| (s.line, s.name.as_str(), s.kind, s.depth)).collect()
    }

    #[test]
    fn rust_outline_names_kinds_depths_lines() {
        let body = "\
use std::fmt;

pub const LIMIT: usize = 8;
static mut DIRTY: bool = false;

pub struct Watcher {
    count: usize,
}

pub enum State { Idle, Busy }

pub trait Tick {
    fn tick(&mut self);
}

impl Tick for Watcher {
    fn tick(&mut self) {
        if true {
        }
    }

    pub(crate) async fn drain(&mut self) {}
}

mod inner {
    pub fn helper() {}
}

pub fn free() {}
";
        let got = outline(body, "rs");
        assert_eq!(
            flat(&got),
            vec![
                (3, "LIMIT", Const, 0),
                (4, "DIRTY", Const, 0),
                (6, "Watcher", Type, 0),
                (10, "State", Type, 0),
                (12, "Tick", Type, 0),
                (13, "tick", Method, 1),
                (16, "Tick for Watcher", Type, 0),
                (17, "tick", Method, 1),
                (22, "drain", Method, 1),
                (25, "inner", Module, 0),
                (26, "helper", Function, 1),
                (29, "free", Function, 0),
            ]
        );
    }

    /// The impl-nesting part of the spec: an impl inside a mod still marks
    /// its fns as methods, and depth follows the nesting.
    #[test]
    fn rust_nested_impl_methods() {
        let body = "\
mod outer {
    impl<T> Foo<T> {
        const fn quiet() {}
    }
    fn loose() {}
}
";
        let got = outline(body, "rs");
        assert_eq!(
            flat(&got),
            vec![
                (1, "outer", Module, 0),
                (2, "Foo<T>", Type, 1),
                (3, "quiet", Method, 2),
                (5, "loose", Function, 1),
            ]
        );
    }

    #[test]
    fn python_outline_with_nested_defs() {
        let body = "\
import os

class Watcher:
    def poll(self):
        def retry():
            pass
        return retry

    async def drain(self):
        pass

def main():
    pass
";
        let got = outline(body, "py");
        assert_eq!(
            flat(&got),
            vec![
                (3, "Watcher", Type, 0),
                (4, "poll", Method, 1),
                (5, "retry", Function, 2),
                (9, "drain", Method, 1),
                (12, "main", Function, 0),
            ]
        );
    }

    #[test]
    fn js_outline_functions_arrows_and_class_methods() {
        let body = "\
export function start(x) {
}

const onTick = async (ev) => {
};

class Watcher {
  constructor(id) {
  }
  static async drain() {
  }
  get count() {
  }
  if (weird) {
  }
}

const notAFunction = 42;
";
        let got = outline(body, "js");
        assert_eq!(
            flat(&got),
            vec![
                (1, "start", Function, 0),
                (4, "onTick", Function, 0),
                (7, "Watcher", Type, 0),
                (8, "constructor", Method, 1),
                (10, "drain", Method, 1),
                (12, "count", Method, 1),
            ]
        );
    }

    #[test]
    fn ts_outline_interface_type_enum() {
        let body = "\
export interface Session {
  id: string;
}

type Verdict = 'ok' | 'stuck';

export enum Phase { Idle, Busy }

export const pick = (s: Session): Verdict => 'ok';
";
        let got = outline(body, "ts");
        assert_eq!(
            flat(&got),
            vec![
                (1, "Session", Type, 0),
                (5, "Verdict", Type, 0),
                (7, "Phase", Type, 0),
                (9, "pick", Function, 0),
            ]
        );
    }

    #[test]
    fn go_outline_funcs_methods_and_toplevel_decls() {
        let body = "\
package main

type Watcher struct {
}

const limit = 8

var dirty bool

func (w *Watcher) Poll() error {
\treturn nil
}

func main() {
}

const (
\ta = 1
)
";
        let got = outline(body, "go");
        assert_eq!(
            flat(&got),
            vec![
                (3, "Watcher", Type, 0),
                (6, "limit", Const, 0),
                (8, "dirty", Other, 0),
                (10, "Poll", Method, 0),
                (14, "main", Function, 0),
                // the grouped `const (` block has no name and is skipped
            ]
        );
    }

    #[test]
    fn java_outline_class_and_methods_not_control_flow() {
        let body = "\
public class Watcher {
    private int count;

    public Watcher(int count) {
        if (count > 0) {
        }
    }

    protected static String drain(int n) {
        for (int i = 0; i < n; i++) {
        }
        return null;
    }
}
";
        let got = outline(body, "java");
        assert_eq!(
            flat(&got),
            vec![
                (1, "Watcher", Type, 0),
                (4, "Watcher", Method, 1),
                (9, "drain", Method, 1),
            ]
        );
    }

    #[test]
    fn kotlin_fun_is_function_or_method() {
        let body = "\
class Watcher {
    fun poll(): Int = 0
}

fun main() {
}
";
        let got = outline(body, "kt");
        assert_eq!(
            flat(&got),
            vec![
                (1, "Watcher", Type, 0),
                (2, "poll", Method, 1),
                (5, "main", Function, 0),
            ]
        );
    }

    #[test]
    fn markdown_heading_depths_and_fences() {
        let body = "\
# Title

## Section ##

```sh
# not a heading, it is a comment in a fence
```

### Deep

#hashtag is not a heading
";
        let got = outline(body, "md");
        assert_eq!(
            flat(&got),
            vec![
                (1, "Title", Heading, 1),
                (3, "Section", Heading, 2),
                (9, "Deep", Heading, 3),
            ]
        );
    }

    #[test]
    fn unknown_language_yields_empty_never_a_guess() {
        let body = "fn main() {}\ndef x():\n# heading?";
        assert!(outline(body, "Makefile").is_empty());
        assert!(outline(body, "toml").is_empty());
        assert!(fold_ranges(body, "Makefile").is_empty());
    }

    #[test]
    fn rust_fold_ranges_follow_braces() {
        let body = "\
fn one() {
    if x {
    }
}

fn tiny() {}

impl Foo {
    fn inner(&self) {
        body();
    }
}
";
        let got = fold_ranges(body, "rs");
        // `tiny` opens and closes on its own line: nothing to fold.
        assert_eq!(got, vec![(1, 4), (8, 12), (9, 11)]);
    }

    /// A brace on the line *after* the symbol still folds, but a symbol with
    /// no block must not steal the next symbol's brace.
    #[test]
    fn brace_folds_handle_next_line_brace_but_not_theft() {
        let body = "\
func main()
{
\tbody()
}
";
        assert_eq!(fold_ranges(body, "go"), vec![(1, 4)]);

        let body2 = "\
const LIMIT: usize = 8;
fn after() {
    body();
}
";
        // LIMIT has no block; the fold on line 2 belongs to `after` alone.
        assert_eq!(fold_ranges(body2, "rs"), vec![(2, 4)]);
    }

    #[test]
    fn python_fold_ranges_follow_indentation() {
        let body = "\
class Watcher:
    def poll(self):
        a = 1
        b = 2

    def drain(self):
        pass

main()
";
        let got = fold_ranges(body, "py");
        // The class spans to the last non-blank body line; each def its own.
        assert_eq!(got, vec![(1, 7), (2, 4), (6, 7)]);
    }

    #[test]
    fn markdown_fold_ranges_span_sections() {
        let body = "\
# Title
intro
## Sub
detail
# Next
tail
";
        let got = fold_ranges(body, "md");
        assert_eq!(got, vec![(1, 4), (3, 4), (5, 6)]);
    }

    #[test]
    fn fold_ranges_are_capped() {
        // A python staircase with far more blocks than the cap.
        let mut body = String::new();
        for _ in 0..2000 {
            body.push_str("a:\n    b\n");
        }
        let got = fold_ranges(&body, "py");
        assert_eq!(got.len(), MAX_FOLDS);
    }

    #[test]
    fn occurrences_respect_word_boundaries() {
        let body = "read the thread\nreread read\nread(read)";
        let got = occurrences(body, "read");
        // "thread" and "reread" must not match; punctuation is a boundary.
        assert_eq!(got, vec![(1, 0), (2, 7), (3, 0), (3, 5)]);
    }

    #[test]
    fn occurrences_columns_are_chars_not_bytes() {
        let body = "é read café read";
        // é(0) space(1) r(2)… café at 7–10, second read at 12.
        assert_eq!(occurrences(body, "read"), vec![(1, 2), (1, 12)]);
        assert_eq!(occurrences(body, "café"), vec![(1, 7)]);
    }

    #[test]
    fn occurrences_reject_noise_words() {
        let body = "a aa a.a";
        assert!(occurrences(body, "").is_empty());
        assert!(occurrences(body, "a").is_empty(), "1-char words are noise");
        assert!(occurrences(body, "a.a").is_empty(), "boundary chars in the needle");
    }

    #[test]
    fn occurrences_are_capped() {
        let mut body = String::new();
        for _ in 0..(MAX_OCCURRENCES + 500) {
            body.push_str("hit\n");
        }
        assert_eq!(occurrences(&body, "hit").len(), MAX_OCCURRENCES);
    }

    #[test]
    fn word_at_finds_identifiers_and_multibyte_survives() {
        let line = "naïve café = wörk_1;";
        // n(0)a(1)ï(2)v(3)e(4) (5) c(6)a(7)f(8)é(9) (10) =(11) (12) w(13)…
        assert_eq!(word_at(line, 2).as_deref(), Some("naïve"));
        assert_eq!(word_at(line, 9).as_deref(), Some("café"));
        assert_eq!(word_at(line, 13).as_deref(), Some("wörk_1"));
        // A click just past a word's end still lands on it…
        assert_eq!(word_at(line, 10).as_deref(), Some("café"));
        // …but empty space with nothing to the left is nothing.
        assert_eq!(word_at(line, 12), None);
        assert_eq!(word_at("  x", 0), None);
        // Past the end of the line is nothing.
        assert_eq!(word_at(line, 99), None);
    }

    #[test]
    fn word_at_end_of_line_click() {
        // Clicking one past the final char is on the final word.
        assert_eq!(word_at("ab", 2).as_deref(), Some("ab"));
        assert_eq!(word_at("", 0), None);
    }

    /// The pathological-input clause: a huge single line clips instead of
    /// hanging, and refuses to call ten thousand chars an identifier.
    #[test]
    fn pathological_lines_clip_and_return() {
        let huge = "x".repeat(50_000);
        assert_eq!(word_at(&huge, 25_000), None, "a 50k-char run is not an identifier");
        let body = format!("fn {}() {{}}", "n".repeat(20_000));
        let syms = outline(&body, "rs");
        assert_eq!(syms.len(), 1);
        assert!(syms[0].name.chars().count() <= MAX_NAME, "names are capped");
        // A clipped multibyte line must not split a char (would panic).
        let multi = format!("# {}", "é".repeat(10_000));
        let _ = outline(&multi, "md");
    }

    #[test]
    fn outline_is_capped() {
        let mut body = String::new();
        for i in 0..(MAX_SYMBOLS + 100) {
            body.push_str(&format!("fn f{i}() {{}}\n"));
        }
        assert_eq!(outline(&body, "rs").len(), MAX_SYMBOLS);
    }
}
