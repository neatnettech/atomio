//! Language services for atomio.
//!
//! Tree-sitter parsing + flat token classification for the languages atomio
//! cares about: Rust (host language) and the JS family (TypeScript, TSX,
//! JavaScript, JSON) which is what Metro bundlers serve up.
//!
//! ### Public API
//!
//! - [`Language`] -- enum of supported grammars; [`Language::from_path`]
//!   maps a file path to a language.
//! - [`HighlightKind`] -- coarse classification used by the renderer.
//! - [`Span`] -- byte-indexed classified range.
//! - [`highlight`] -- top-level dispatcher.
//! - [`highlight_rust`] -- legacy alias kept so existing callers keep
//!   compiling. Prefer [`highlight`] for new code.
//!
//! The classifier walks the AST directly and matches on node kinds rather
//! than running tree-sitter highlight queries. That trades query DSL
//! flexibility for zero query-file vendoring and a single short table per
//! language. Adding a language is dozens of lines, not hundreds.

use std::path::Path;
use tree_sitter::{Node, Parser};

/// Coarse syntactic category. Stable contract between this crate and the
/// rendering layer; adding a category is a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightKind {
    Keyword,
    String,
    Number,
    Comment,
    Type,
    Function,
    Attribute,
    Property,
    Constant,
}

/// A classified byte range in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: HighlightKind,
}

/// Languages we have a tree-sitter grammar wired up for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Json,
}

impl Language {
    /// Map a file path to a language by extension. Case-insensitive.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "rs" => Language::Rust,
            "ts" | "mts" | "cts" => Language::TypeScript,
            "tsx" => Language::Tsx,
            "js" | "mjs" | "cjs" | "jsx" => Language::JavaScript,
            "json" | "jsonc" => Language::Json,
            _ => return None,
        })
    }

    /// Short human-readable label suitable for status bar / pickers.
    pub fn label(self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::TypeScript => "TS",
            Language::Tsx => "TSX",
            Language::JavaScript => "JS",
            Language::Json => "JSON",
        }
    }

    fn ts_language(self) -> tree_sitter::Language {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Language::Json => tree_sitter_json::LANGUAGE.into(),
        }
    }
}

/// Top-level entry: parse `source` as `language` and return classified spans.
pub fn highlight(source: &str, language: Language) -> Vec<Span> {
    if source.is_empty() {
        return Vec::new();
    }
    let mut parser = Parser::new();
    let lang = language.ts_language();
    if parser.set_language(&lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    let mut spans = Vec::new();
    let mut cursor = tree.walk();
    walk(&mut cursor, language, &mut spans);

    // Drop overlaps so the renderer can emit runs linearly.
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

/// Legacy: equivalent to `highlight(source, Language::Rust)`. Kept so
/// existing call sites keep compiling. New code should call [`highlight`].
pub fn highlight_rust(source: &str) -> Vec<Span> {
    highlight(source, Language::Rust)
}

fn walk(cursor: &mut tree_sitter::TreeCursor, language: Language, out: &mut Vec<Span>) {
    loop {
        let node = cursor.node();
        if let Some(kind) = classify(&node, language) {
            out.push(Span {
                start: node.start_byte(),
                end: node.end_byte(),
                kind,
            });
            // Don't descend into a classified node -- its children are
            // structural (e.g. quote tokens inside `string_literal`).
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

fn classify(node: &Node, language: Language) -> Option<HighlightKind> {
    let kind = node.kind();

    // Common across grammars.
    match kind {
        "line_comment" | "block_comment" | "comment" => return Some(HighlightKind::Comment),
        _ => {}
    }

    match language {
        Language::Rust => classify_rust(kind, node),
        Language::TypeScript | Language::Tsx => classify_ts(kind),
        Language::JavaScript => classify_js(kind),
        Language::Json => classify_json(kind),
    }
}

fn classify_rust(kind: &str, node: &Node) -> Option<HighlightKind> {
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
        "primitive_type" | "type_identifier" => Some(HighlightKind::Type),
        "attribute_item" | "inner_attribute_item" => Some(HighlightKind::Attribute),
        "identifier" => {
            // Tag function-name identifiers so the whole function_item
            // can still be walked for keywords + body coverage.
            let parent = node.parent()?;
            if parent.kind() == "function_item"
                && parent.child_by_field_name("name").map(|n| n.id()) == Some(node.id())
            {
                Some(HighlightKind::Function)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn classify_ts(kind: &str) -> Option<HighlightKind> {
    // tree-sitter-typescript shares most node kinds with -javascript and
    // adds type-system kinds on top.
    if let Some(k) = classify_js(kind) {
        return Some(k);
    }
    match kind {
        "type_identifier" | "predefined_type" => Some(HighlightKind::Type),
        "type_alias_declaration"
        | "interface_declaration"
        | "enum_declaration"
        | "type"
        | "interface"
        | "namespace"
        | "module"
        | "abstract"
        | "implements"
        | "readonly"
        | "keyof"
        | "infer"
        | "satisfies"
        | "is"
        | "asserts" => Some(HighlightKind::Keyword),
        _ => None,
    }
}

fn classify_js(kind: &str) -> Option<HighlightKind> {
    const KEYWORDS: &[&str] = &[
        "var",
        "let",
        "const",
        "function",
        "if",
        "else",
        "for",
        "while",
        "do",
        "switch",
        "case",
        "default",
        "break",
        "continue",
        "return",
        "throw",
        "try",
        "catch",
        "finally",
        "new",
        "delete",
        "typeof",
        "instanceof",
        "in",
        "of",
        "void",
        "yield",
        "async",
        "await",
        "class",
        "extends",
        "super",
        "this",
        "import",
        "export",
        "from",
        "as",
        "static",
        "get",
        "set",
        "null",
        "undefined",
        "true",
        "false",
    ];
    if KEYWORDS.contains(&kind) {
        // null/undefined/true/false are punctuation-y constants in JS
        // but lexically arrive as keyword nodes. Map the literal trio
        // separately so themes can colour them differently.
        return Some(match kind {
            "null" | "undefined" | "true" | "false" => HighlightKind::Constant,
            _ => HighlightKind::Keyword,
        });
    }
    match kind {
        "string" | "string_fragment" | "template_string" | "template_chars" | "regex"
        | "regex_pattern" => Some(HighlightKind::String),
        "number" => Some(HighlightKind::Number),
        "property_identifier" | "shorthand_property_identifier" => Some(HighlightKind::Property),
        "decorator" => Some(HighlightKind::Attribute),
        _ => None,
    }
}

fn classify_json(kind: &str) -> Option<HighlightKind> {
    match kind {
        "string" | "string_content" => Some(HighlightKind::String),
        "number" => Some(HighlightKind::Number),
        "true" | "false" | "null" => Some(HighlightKind::Constant),
        // JSON object keys appear as a string node already, no separate
        // property kind.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(spans: &[Span]) -> Vec<HighlightKind> {
        spans.iter().map(|s| s.kind).collect()
    }

    // ------- existing rust coverage -------

    #[test]
    fn empty_source_is_empty() {
        assert!(highlight_rust("").is_empty());
    }

    #[test]
    fn rust_classifies_keywords_strings_numbers() {
        let src = r#"fn main() { let x = "hi"; let n = 42; }"#;
        let ks = kinds(&highlight_rust(src));
        assert!(ks.contains(&HighlightKind::Keyword));
        assert!(ks.contains(&HighlightKind::String));
        assert!(ks.contains(&HighlightKind::Number));
    }

    #[test]
    fn rust_classifies_line_comments() {
        let src = "// hello\nfn main() {}";
        let spans = highlight_rust(src);
        let first = spans.first().expect("at least one span");
        assert_eq!(first.kind, HighlightKind::Comment);
        assert_eq!(first.start, 0);
        assert_eq!(first.end, 8);
    }

    #[test]
    fn rust_spans_sorted_non_overlapping() {
        let src = "struct Foo; fn bar() -> Foo { Foo }";
        let spans = highlight_rust(src);
        for pair in spans.windows(2) {
            assert!(pair[0].start <= pair[1].start);
            assert!(pair[0].end <= pair[1].start);
        }
    }

    #[test]
    fn malformed_input_still_parses_best_effort() {
        let src = "fn oops( { let x = 1; }";
        assert!(!highlight_rust(src).is_empty());
    }

    // ------- language detection -------

    #[test]
    fn from_path_dispatches_by_extension() {
        assert_eq!(
            Language::from_path(Path::new("foo.rs")),
            Some(Language::Rust)
        );
        assert_eq!(
            Language::from_path(Path::new("foo.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            Language::from_path(Path::new("foo.tsx")),
            Some(Language::Tsx)
        );
        assert_eq!(
            Language::from_path(Path::new("foo.js")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            Language::from_path(Path::new("foo.jsx")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            Language::from_path(Path::new("foo.json")),
            Some(Language::Json)
        );
        assert_eq!(Language::from_path(Path::new("README")), None);
        assert_eq!(Language::from_path(Path::new("foo.txt")), None);
    }

    #[test]
    fn from_path_is_case_insensitive() {
        assert_eq!(
            Language::from_path(Path::new("App.TSX")),
            Some(Language::Tsx)
        );
    }

    // ------- typescript -------

    #[test]
    fn ts_classifies_keywords_and_types() {
        let src = "interface Foo { bar: number; }\nconst x: string = \"hi\";";
        let ks = kinds(&highlight(src, Language::TypeScript));
        assert!(ks.contains(&HighlightKind::Keyword));
        assert!(ks.contains(&HighlightKind::Type));
        assert!(ks.contains(&HighlightKind::String));
    }

    #[test]
    fn tsx_classifies_jsx_function_component() {
        let src = "const App = () => <div>{count}</div>;";
        let spans = highlight(src, Language::Tsx);
        assert!(!spans.is_empty());
    }

    // ------- javascript -------

    #[test]
    fn js_classifies_template_strings_and_constants() {
        let src = "const x = `hello ${name}`; let y = null; let z = true;";
        let ks = kinds(&highlight(src, Language::JavaScript));
        assert!(ks.contains(&HighlightKind::String));
        assert!(ks.contains(&HighlightKind::Constant));
        assert!(ks.contains(&HighlightKind::Keyword));
    }

    // ------- json -------

    #[test]
    fn json_classifies_strings_numbers_constants() {
        let src = r#"{"name": "atomio", "version": 1, "active": true, "x": null}"#;
        let ks = kinds(&highlight(src, Language::Json));
        assert!(ks.contains(&HighlightKind::String));
        assert!(ks.contains(&HighlightKind::Number));
        assert!(ks.contains(&HighlightKind::Constant));
    }

    #[test]
    fn json_empty_object() {
        let spans = highlight("{}", Language::Json);
        assert!(spans.is_empty());
    }
}
