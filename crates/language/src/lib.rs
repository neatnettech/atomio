//! Language services for atomio.
//!
//! This is the v0.1 seed of the `language` crate. It exposes a single
//! function, [`highlight_rust`], which parses a Rust source string with
//! tree-sitter and returns a flat list of byte-indexed [`Span`]s classified
//! into a small [`HighlightKind`] enum. The rendering layer in `atomio`
//! consumes these spans and maps them onto colours — that mapping is *not*
//! this crate's concern.
//!
//! ### Design notes
//!
//! - We deliberately avoid tree-sitter's `.scm` highlight queries for now.
//!   They work, but they require vendoring query files and carrying the
//!   `tree-sitter-highlight` crate. A direct AST walk over a short
//!   hand-picked list of node kinds is good enough to light up keywords,
//!   strings, numbers, comments, types, and function names — which is
//!   already a visible jump from the current plain-text view.
//! - The output is always sorted by start byte and contains no overlaps.
//!   Children that would overlap a classified parent are skipped, so the
//!   UI can render spans linearly without a stacking pass.
//! - Byte offsets, not char offsets. The caller is responsible for mapping
//!   bytes back to characters / columns if needed; that's cheap on ASCII
//!   code, which is the realistic input for a syntax highlighter.

use tree_sitter::{Node, Parser};

/// Coarse syntactic category of a token. Intentionally small — this is the
/// stable contract between `language` and the rendering layer. Adding a
/// category is a breaking change to themes, so we only add one when we have
/// a reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightKind {
    Keyword,
    String,
    Number,
    Comment,
    Type,
    Function,
    Attribute,
}

/// A classified byte range in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: HighlightKind,
}

/// Parse `source` as Rust and return the classified token spans.
///
/// Returns an empty vector if the parser fails to initialise (which on a
/// correctly-linked build should never happen) or if `source` is empty.
pub fn highlight_rust(source: &str) -> Vec<Span> {
    if source.is_empty() {
        return Vec::new();
    }
    let mut parser = Parser::new();
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    let mut spans = Vec::new();
    let mut cursor = tree.walk();
    walk(&mut cursor, source.as_bytes(), &mut spans);

    // The walker may emit children inside already-classified parents in
    // corner cases; drop overlaps greedily so the output is linear.
    spans.sort_by_key(|s| (s.start, s.end));
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for s in spans {
        if out.last().is_some_and(|last| s.start < last.end) {
            continue;
        }
        out.push(s);
    }
    out
}

fn walk(cursor: &mut tree_sitter::TreeCursor, src: &[u8], out: &mut Vec<Span>) {
    loop {
        let node = cursor.node();
        if let Some(kind) = classify(&node, src) {
            out.push(Span {
                start: node.start_byte(),
                end: node.end_byte(),
                kind,
            });
            // Don't descend into classified nodes — their children are
            // structural (e.g. the `"` tokens inside a `string_literal`).
        } else if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

fn classify(node: &Node, src: &[u8]) -> Option<HighlightKind> {
    let kind = node.kind();
    // Keyword-ish tokens. tree-sitter-rust exposes these as anonymous
    // nodes with their literal text as `kind()`; the set below is the
    // common subset that shows up in idiomatic code. It's intentionally
    // not exhaustive — unknown keywords just render as plain text.
    const KEYWORDS: &[&str] = &[
        "fn", "let", "mut", "const", "static", "if", "else", "match", "for", "while", "loop",
        "return", "break", "continue", "struct", "enum", "trait", "impl", "pub", "use", "mod",
        "crate", "self", "Self", "super", "as", "in", "where", "ref", "move", "async", "await",
        "dyn", "unsafe", "extern", "type",
    ];
    if KEYWORDS.contains(&kind) {
        return Some(HighlightKind::Keyword);
    }
    match kind {
        "string_literal" | "raw_string_literal" | "char_literal" => Some(HighlightKind::String),
        "integer_literal" | "float_literal" => Some(HighlightKind::Number),
        "line_comment" | "block_comment" => Some(HighlightKind::Comment),
        "primitive_type" | "type_identifier" => Some(HighlightKind::Type),
        "attribute_item" | "inner_attribute_item" => Some(HighlightKind::Attribute),
        // Function definitions and calls — pull the name child out.
        "function_item" => {
            if let Some(name) = node.child_by_field_name("name") {
                Some(classify_child_as(&name, src, HighlightKind::Function))?;
            }
            None
        }
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                Some(classify_child_as(&func, src, HighlightKind::Function))?;
            }
            None
        }
        _ => None,
    }
}

/// Classify a single identifier-ish child directly. This is a no-op on
/// nodes that don't carry a simple identifier (e.g. qualified paths); the
/// generic walker will revisit them through normal recursion.
fn classify_child_as(node: &Node, _src: &[u8], _kind: HighlightKind) -> Option<HighlightKind> {
    if node.kind() == "identifier" || node.kind() == "field_identifier" {
        Some(_kind)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(spans: &[Span]) -> Vec<HighlightKind> {
        spans.iter().map(|s| s.kind).collect()
    }

    #[test]
    fn empty_source_is_empty() {
        assert!(highlight_rust("").is_empty());
    }

    #[test]
    fn classifies_keywords_strings_and_numbers() {
        let src = r#"fn main() { let x = "hi"; let n = 42; }"#;
        let spans = highlight_rust(src);
        let ks = kinds(&spans);
        assert!(ks.contains(&HighlightKind::Keyword));
        assert!(ks.contains(&HighlightKind::String));
        assert!(ks.contains(&HighlightKind::Number));
    }

    #[test]
    fn classifies_line_comments() {
        let src = "// hello\nfn main() {}";
        let spans = highlight_rust(src);
        let first = spans.first().expect("at least one span");
        assert_eq!(first.kind, HighlightKind::Comment);
        assert_eq!(first.start, 0);
        assert_eq!(first.end, 8);
    }

    #[test]
    fn spans_are_sorted_and_non_overlapping() {
        let src = "struct Foo; fn bar() -> Foo { Foo }";
        let spans = highlight_rust(src);
        for pair in spans.windows(2) {
            assert!(pair[0].start <= pair[1].start);
            assert!(pair[0].end <= pair[1].start);
        }
    }

    #[test]
    fn malformed_input_still_parses_best_effort() {
        // tree-sitter is error-recovering; we should get spans even if the
        // input has a syntax error.
        let src = "fn oops( { let x = 1; }";
        let spans = highlight_rust(src);
        assert!(!spans.is_empty());
    }
}
