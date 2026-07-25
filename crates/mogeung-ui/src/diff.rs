//! Diff presentation: highlighting, word-level diffing, and side-by-side
//! pairing. Roadmap `R-D4`, `R-D5`, `R-D6`.
//!
//! All of it is pure — text in, spans out — so the interesting parts are
//! testable without a window, which is not true of anything that touches egui.
//!
//! **The highlighter is a tokenizer, not a parser.** No tree-sitter, no
//! grammars, no per-language configuration: it recognises strings, comments,
//! numbers and a shared keyword set. It will mis-colour things. That is an
//! acceptable trade for a reading aid with no dependencies and no build step;
//! if it ever needs to be *correct* rather than *helpful*, it should be
//! replaced wholesale rather than patched toward accuracy.

/// What a token is, for colouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Plain,
    Keyword,
    Str,
    Comment,
    Number,
    Type,
}

/// Keywords shared across the languages this tool actually sees. Deliberately
/// one flat set: per-language tables would be more correct and would need
/// language detection, which is a bigger commitment than a reading aid earns.
const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "pub", "use", "mod", "impl", "trait", "struct", "enum", "match", "if",
    "else", "for", "while", "loop", "return", "async", "await", "move", "ref", "where", "const",
    "static", "unsafe", "dyn", "as", "in", "self", "super", "crate", "type", "def", "class",
    "import", "from", "function", "var", "new", "this", "export", "default", "public", "private",
    "protected", "extends", "implements", "interface", "package", "func", "go", "defer", "chan",
    "range", "map", "nil", "None", "True", "False", "true", "false", "null", "undefined", "try",
    "catch", "except", "finally", "raise", "throw", "with", "yield", "lambda", "elif", "not",
    "and", "or", "is", "del", "pass", "break", "continue", "switch", "case", "do", "end",
];

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Split one line of code into coloured spans.
///
/// Line-local: a string or block comment spanning lines is not tracked, because
/// a diff hunk routinely starts in the middle of one and there is no reliable
/// way to know. Mis-colouring a fragment is better than mis-colouring the rest
/// of the file.
pub fn highlight(line: &str) -> Vec<(Tok, String)> {
    let mut out: Vec<(Tok, String)> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    let push = |out: &mut Vec<(Tok, String)>, tok: Tok, s: String| {
        if s.is_empty() {
            return;
        }
        // Merge with the previous span when it has the same colour, so the
        // renderer emits far fewer widgets.
        if let Some((last_tok, last)) = out.last_mut() {
            if *last_tok == tok {
                last.push_str(&s);
                return;
            }
        }
        out.push((tok, s));
    };

    while i < chars.len() {
        let c = chars[i];

        // Comments run to end of line.
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("//") || rest.starts_with('#') || rest.starts_with("--") {
            push(&mut out, Tok::Comment, rest);
            break;
        }

        // Strings, with backslash escapes.
        if c == '"' || c == '\'' || c == '`' {
            let quote = c;
            let mut s = String::from(c);
            i += 1;
            while i < chars.len() {
                let d = chars[i];
                s.push(d);
                i += 1;
                if d == '\\' && i < chars.len() {
                    s.push(chars[i]);
                    i += 1;
                    continue;
                }
                if d == quote {
                    break;
                }
            }
            push(&mut out, Tok::Str, s);
            continue;
        }

        // Numbers, including hex and decimals.
        if c.is_ascii_digit() {
            let mut s = String::new();
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_')
            {
                s.push(chars[i]);
                i += 1;
            }
            push(&mut out, Tok::Number, s);
            continue;
        }

        // Identifiers: keyword, type-ish (leading capital), or plain.
        if is_ident(c) {
            let mut s = String::new();
            while i < chars.len() && is_ident(chars[i]) {
                s.push(chars[i]);
                i += 1;
            }
            let tok = if KEYWORDS.contains(&s.as_str()) {
                Tok::Keyword
            } else if s.chars().next().map(char::is_uppercase).unwrap_or(false) {
                Tok::Type
            } else {
                Tok::Plain
            };
            push(&mut out, tok, s);
            continue;
        }

        push(&mut out, Tok::Plain, c.to_string());
        i += 1;
    }
    out
}

/// A run of characters shared or not shared between two lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub changed: bool,
}

/// Word-level diff between a removed line and its added counterpart. `R-D5`.
///
/// Common prefix and suffix are found on *word* boundaries, and everything
/// between is marked changed. That is not a minimal edit script — a proper
/// Myers diff would highlight less — but it is O(n), never produces the
/// confetti that character-level diffing does on reformatted code, and answers
/// the only question being asked: *which part of this line actually moved?*
pub fn word_diff(old: &str, new: &str) -> (Vec<Span>, Vec<Span>) {
    // The leading `-`/`+` is a diff marker, not content. Including it would
    // make every pair differ at position 0, so the common-prefix scan would
    // stop immediately and light up the entire line — exactly the noise this
    // function exists to remove.
    let (old_sign, old_body) = split_marker(old);
    let (new_sign, new_body) = split_marker(new);

    let a = split_words(old_body);
    let b = split_words(new_body);

    let mut pre = 0;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre && suf < b.len() - pre && a[a.len() - 1 - suf] == b[b.len() - 1 - suf]
    {
        suf += 1;
    }

    let build = |sign: &str, words: &[&str]| -> Vec<Span> {
        let mut out: Vec<Span> = Vec::new();
        let mut add = |text: String, changed: bool| {
            if text.is_empty() {
                return;
            }
            if let Some(last) = out.last_mut() {
                if last.changed == changed {
                    last.text.push_str(&text);
                    return;
                }
            }
            out.push(Span { text, changed });
        };
        add(sign.to_string(), false);
        add(words[..pre].concat(), false);
        add(words[pre..words.len() - suf].concat(), true);
        add(words[words.len() - suf..].concat(), false);
        out
    };
    (build(old_sign, &a), build(new_sign, &b))
}

/// Split a diff line into its leading marker and its body.
fn split_marker(line: &str) -> (&str, &str) {
    match line.chars().next() {
        Some('+') | Some('-') | Some(' ') => line.split_at(1),
        _ => ("", line),
    }
}

/// Split into words, keeping separators as their own units so reassembly is
/// lossless.
fn split_words(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut prev_ident = false;
    for (i, c) in s.char_indices() {
        let ident = is_ident(c);
        if i > start && ident != prev_ident {
            out.push(&s[start..i]);
            start = i;
        }
        prev_ident = ident;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// One row of a side-by-side view. `R-D6`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub left: Option<String>,
    pub right: Option<String>,
}

/// Pair up a unified hunk's lines into left/right rows.
///
/// Runs of removals and additions are zipped so a modified line sits opposite
/// the line it replaced — which is what makes side-by-side worth having, and
/// what enables the word diff on that pair. Unmatched leftovers get a blank
/// on the other side.
pub fn side_by_side(lines: &[String]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut dels: Vec<String> = Vec::new();
    let mut adds: Vec<String> = Vec::new();

    fn flush(dels: &mut Vec<String>, adds: &mut Vec<String>, rows: &mut Vec<Row>) {
        let n = dels.len().max(adds.len());
        for i in 0..n {
            rows.push(Row {
                left: dels.get(i).cloned(),
                right: adds.get(i).cloned(),
            });
        }
        dels.clear();
        adds.clear();
    }

    for l in lines {
        match l.chars().next() {
            Some('-') => dels.push(l.clone()),
            Some('+') => adds.push(l.clone()),
            _ => {
                flush(&mut dels, &mut adds, &mut rows);
                rows.push(Row {
                    left: Some(l.clone()),
                    right: Some(l.clone()),
                });
            }
        }
    }
    flush(&mut dels, &mut adds, &mut rows);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(line: &str) -> Vec<Tok> {
        highlight(line).into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn highlighting_is_lossless() {
        // The single property that matters: colouring must never change the
        // text. A renderer that drops or duplicates characters is worse than
        // no highlighting at all.
        for line in [
            "let x = \"hi\"; // note",
            "fn main() { let n = 0xFF_u8; }",
            "  # python comment",
            "s = 'it\\'s escaped'",
            "",
            "   ",
            "→ unicode ✓ and emoji 🎉",
            "let s = \"unterminated",
        ] {
            let rebuilt: String = highlight(line).into_iter().map(|(_, s)| s).collect();
            assert_eq!(rebuilt, line, "highlight() altered the text");
        }
    }

    #[test]
    fn recognises_the_obvious_tokens() {
        assert!(toks("let x = 1").contains(&Tok::Keyword));
        assert!(toks("x = \"str\"").contains(&Tok::Str));
        assert!(toks("x = 42").contains(&Tok::Number));
        assert!(toks("// hi").contains(&Tok::Comment));
        assert!(toks("let s = Session::new()").contains(&Tok::Type));
    }

    #[test]
    fn a_hash_inside_a_string_is_not_a_comment() {
        let got = highlight(r#"let url = "http://x/#frag";"#);
        // The `#` is inside the string span, so nothing is a comment.
        assert!(!got.iter().any(|(t, _)| *t == Tok::Comment));
    }

    #[test]
    fn word_diff_marks_only_what_moved() {
        let (old, new) = word_diff("-let timeout = 30;", "+let timeout = 60;");
        let changed_old: String = old.iter().filter(|s| s.changed).map(|s| s.text.clone()).collect();
        let changed_new: String = new.iter().filter(|s| s.changed).map(|s| s.text.clone()).collect();
        assert_eq!(changed_old, "30");
        assert_eq!(changed_new, "60");
    }

    #[test]
    fn word_diff_is_lossless_in_both_directions() {
        let cases = [
            ("-abc", "+abd"),
            ("-", "+"),
            ("-same", "+same"),
            ("-totally different", "+nothing alike here"),
            ("-a(b, c)", "+a(b, c, d)"),
        ];
        for (a, b) in cases {
            let (l, r) = word_diff(a, b);
            assert_eq!(l.iter().map(|s| s.text.clone()).collect::<String>(), a);
            assert_eq!(r.iter().map(|s| s.text.clone()).collect::<String>(), b);
        }
    }

    /// The marker is not content. A line moved verbatim from one side to the
    /// other has nothing highlighted.
    #[test]
    fn identical_bodies_have_nothing_changed() {
        let (l, r) = word_diff("-x", "+x");
        assert_eq!(l.iter().filter(|s| s.changed).count(), 0);
        assert_eq!(r.iter().filter(|s| s.changed).count(), 0);
    }

    /// Regression: the first implementation compared the `-`/`+` markers as
    /// content, so the common-prefix scan stopped at position 0 and every
    /// replacement highlighted the whole line — which is no better than not
    /// having a word diff at all.
    #[test]
    fn the_marker_never_widens_the_highlight() {
        let (old, new) = word_diff("-    self.timeout = 30;", "+    self.timeout = 60;");
        let changed_old: String =
            old.iter().filter(|s| s.changed).map(|s| s.text.clone()).collect();
        let changed_new: String =
            new.iter().filter(|s| s.changed).map(|s| s.text.clone()).collect();
        assert_eq!(changed_old, "30");
        assert_eq!(changed_new, "60");
    }

    #[test]
    fn side_by_side_pairs_a_replacement() {
        let lines: Vec<String> = vec![
            " ctx".into(),
            "-old one".into(),
            "+new one".into(),
            " tail".into(),
        ];
        let rows = side_by_side(&lines);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].left.as_deref(), Some("-old one"));
        assert_eq!(rows[1].right.as_deref(), Some("+new one"));
        // Context appears on both sides.
        assert_eq!(rows[0].left, rows[0].right);
    }

    #[test]
    fn side_by_side_handles_lopsided_runs() {
        let lines: Vec<String> = vec![
            "-a".into(),
            "-b".into(),
            "-c".into(),
            "+x".into(),
        ];
        let rows = side_by_side(&lines);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].right.as_deref(), Some("+x"));
        assert!(rows[1].right.is_none(), "extra removals need a blank right");
        assert!(rows[2].right.is_none());
    }

    #[test]
    fn side_by_side_preserves_every_line() {
        let lines: Vec<String> = vec![
            " a".into(), "-b".into(), "+c".into(), "-d".into(), " e".into(), "+f".into(),
        ];
        let rows = side_by_side(&lines);
        let mut seen: Vec<String> = Vec::new();
        for r in &rows {
            if let Some(l) = &r.left {
                if l.starts_with('-') || l.starts_with(' ') {
                    seen.push(l.clone());
                }
            }
            if let Some(x) = &r.right {
                if x.starts_with('+') {
                    seen.push(x.clone());
                }
            }
        }
        seen.sort();
        let mut want = lines.clone();
        want.sort();
        assert_eq!(seen, want, "side-by-side dropped or invented a line");
    }
}
