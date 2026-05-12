//! Language services for atomio.
//!
//! Tree-sitter parsing + capture-driven token classification for the
//! languages atomio cares about: Rust (host language) and the JS
//! family (TypeScript, TSX, JavaScript, JSON) which is what Metro
//! bundlers serve up.
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
//! ### How it works
//!
//! Each grammar ships a `highlights.scm` tree-sitter query as a public
//! `&'static str` (`HIGHLIGHTS_QUERY` / `HIGHLIGHT_QUERY` per crate).
//! On first use of a language we compile that query once into a
//! [`tree_sitter::Query`] cached in a [`OnceLock`], pre-compute a
//! capture-index -> [`HighlightKind`] table, and reuse both for every
//! subsequent `highlight()` call. Captures we don't care about
//! (punctuation, operators, embedded markers, etc.) map to `None` and
//! are silently dropped so the renderer never sees them.
//!
//! Overlap resolution is "smallest-span wins": when multiple captures
//! cover the same byte the more specific (shorter) one takes priority.
//! That matches how every other tree-sitter highlighter feels in
//! practice -- inner captures shadow outer ones -- without us having
//! to maintain a hand-tuned precedence table per language.

use std::path::Path;
use std::sync::OnceLock;

use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

/// Coarse syntactic category. Stable contract between this crate and
/// the rendering layer; adding a category is a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightKind {
    /// Reserved words (`fn`, `if`, `interface`...) and lifetime labels.
    Keyword,
    /// String / character literals, including escape sequences.
    String,
    /// Numeric literals.
    Number,
    /// Comments, including doc comments.
    Comment,
    /// Type names, type aliases, constructors.
    Type,
    /// Function / method / macro names (definitions and references).
    Function,
    /// Rust attributes, JSX/TSX attribute names, decorators.
    Attribute,
    /// Struct fields, object properties.
    Property,
    /// `null`, `true`, `false`, ALL_CAPS identifiers, builtin constants.
    Constant,
    /// Plain identifiers and parameters that don't fall into a more
    /// specific bucket. Lets themes give variables a different shade
    /// from punctuation / unstyled text.
    Variable,
    /// JSX / TSX component tag names (e.g. `<View>`, `<MyComponent>`).
    Tag,
}

impl HighlightKind {
    /// Map a tree-sitter capture name (e.g. `function.method`) to one
    /// of our kinds. Captures we don't surface (punctuation,
    /// operators, embedded markers, locals scopes) return `None` and
    /// are skipped by the highlighter.
    ///
    /// Sub-captures collapse to their base: `function.method`,
    /// `function.builtin`, and `function.macro` all become
    /// [`HighlightKind::Function`]. This keeps the table short while
    /// still letting themes tell the broad categories apart.
    fn from_capture(name: &str) -> Option<Self> {
        let base = name.split('.').next().unwrap_or(name);
        Some(match base {
            "keyword" | "label" => Self::Keyword,
            "string" | "escape" => Self::String,
            "number" => Self::Number,
            "comment" => Self::Comment,
            "type" | "constructor" => Self::Type,
            "function" => Self::Function,
            "attribute" => Self::Attribute,
            "property" => Self::Property,
            "constant" => Self::Constant,
            "variable" => Self::Variable,
            "tag" => Self::Tag,
            _ => return None,
        })
    }
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

    /// Highlight query source for this language. Combines upstream
    /// `highlights.scm` with small atomio-side overrides:
    ///
    /// - Rust: the upstream query tags `42` as `@constant.builtin`;
    ///   we append `[(integer_literal) (float_literal)] @number` so
    ///   numbers get their own kind. Appended captures land at higher
    ///   capture indices and so beat the upstream defaults on ties.
    /// - TypeScript and TSX inherit from JavaScript via the
    ///   `; inherits:` directive, which the runtime doesn't resolve
    ///   automatically, so we concatenate JS + TS queries manually.
    /// - JSX support (the `@tag`, JSX-shaped `@attribute`) lives in a
    ///   separate `highlights-jsx.scm`; we include it for both `.jsx`
    ///   files (Language::JavaScript) and TSX so `<View />` lights up.
    /// - The upstream JSX query gates `@tag` on a lowercase match
    ///   predicate (HTML elements only) which would leave React
    ///   Native's `<View />` etc. unstyled. We append a non-gated
    ///   override that tags any JSX element name regardless of case,
    ///   relying on the capture-index tiebreaker so it wins over the
    ///   stricter upstream pattern on the same node.
    fn highlights_query_text(self) -> String {
        match self {
            Language::Rust => format!(
                "{}\n\n; atomio override: classify numeric literals as @number\n\
                 [(integer_literal) (float_literal)] @number\n",
                tree_sitter_rust::HIGHLIGHTS_QUERY,
            ),
            Language::TypeScript => format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ),
            Language::Tsx => format!(
                "{}\n{}\n{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
                JSX_TAG_OVERRIDE,
            ),
            Language::JavaScript => format!(
                "{}\n{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                JSX_TAG_OVERRIDE,
            ),
            Language::Json => tree_sitter_json::HIGHLIGHTS_QUERY.to_string(),
        }
    }
}

/// JSX tag override: the upstream `highlights-jsx.scm` gates `@tag` on
/// `^[a-z][^.]*$`, which means React component names like `<View />`
/// fall through unclassified. Append these non-gated patterns so any
/// JSX element name lights up as a Tag, regardless of case, and so
/// dotted member-expression tags (`<Foo.Bar />`) work too.
const JSX_TAG_OVERRIDE: &str = "\
; atomio override: classify any JSX element name as @tag (cased or dotted)
(jsx_opening_element (identifier) @tag)
(jsx_closing_element (identifier) @tag)
(jsx_self_closing_element (identifier) @tag)
(jsx_opening_element (member_expression) @tag)
(jsx_closing_element (member_expression) @tag)
(jsx_self_closing_element (member_expression) @tag)
";

/// Compiled query + capture-index -> kind lookup, cached per language.
struct LanguageState {
    query: Query,
    capture_kinds: Vec<Option<HighlightKind>>,
}

fn language_state(lang: Language) -> Option<&'static LanguageState> {
    static RUST: OnceLock<Option<LanguageState>> = OnceLock::new();
    static TS: OnceLock<Option<LanguageState>> = OnceLock::new();
    static TSX: OnceLock<Option<LanguageState>> = OnceLock::new();
    static JS: OnceLock<Option<LanguageState>> = OnceLock::new();
    static JSON: OnceLock<Option<LanguageState>> = OnceLock::new();

    let slot = match lang {
        Language::Rust => &RUST,
        Language::TypeScript => &TS,
        Language::Tsx => &TSX,
        Language::JavaScript => &JS,
        Language::Json => &JSON,
    };
    slot.get_or_init(|| build_language_state(lang)).as_ref()
}

fn build_language_state(lang: Language) -> Option<LanguageState> {
    let ts_lang = lang.ts_language();
    let query_text = lang.highlights_query_text();
    let query = Query::new(&ts_lang, &query_text).ok()?;
    let capture_kinds: Vec<Option<HighlightKind>> = query
        .capture_names()
        .iter()
        .map(|name| HighlightKind::from_capture(name))
        .collect();
    Some(LanguageState {
        query,
        capture_kinds,
    })
}

/// Top-level entry: parse `source` as `language` and return classified
/// non-overlapping spans in start-byte order.
///
/// Returns an empty vec for empty input or any parse/query failure --
/// the editor renders unhighlighted text in that case rather than
/// erroring. Best-effort by design.
pub fn highlight(source: &str, language: Language) -> Vec<Span> {
    if source.is_empty() {
        return Vec::new();
    }
    let Some(state) = language_state(language) else {
        return Vec::new();
    };
    let ts_lang = language.ts_language();
    let mut parser = Parser::new();
    if parser.set_language(&ts_lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    let source_bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(&state.query, tree.root_node(), source_bytes);

    // Collect (start, end, kind, pattern_index) for every capture we
    // recognise. `pattern_index` is the capture's position in the
    // query's pattern list -- patterns appended later land at higher
    // indices, which lets atomio-side override patterns beat the
    // upstream defaults on ties. We deliberately use pattern_index
    // and not capture_index: capture names are deduplicated across
    // the query so two `@tag` patterns share one capture_index and
    // can't be ordered relative to each other or to other captures.
    let mut raw: Vec<(usize, usize, HighlightKind, usize)> = Vec::new();
    while let Some((mat, idx)) = captures.next() {
        let cap = &mat.captures[*idx];
        let Some(kind) = state.capture_kinds[cap.index as usize] else {
            continue;
        };
        let start = cap.node.start_byte();
        let end = cap.node.end_byte();
        if start >= end || end > source.len() {
            continue;
        }
        raw.push((start, end, kind, mat.pattern_index));
    }

    resolve_overlaps_split(raw, source.len())
}

/// Legacy: equivalent to `highlight(source, Language::Rust)`. Kept so
/// existing call sites keep compiling. New code should call [`highlight`].
pub fn highlight_rust(source: &str) -> Vec<Span> {
    highlight(source, Language::Rust)
}

/// Overlap resolution with span splitting: smaller (more specific)
/// captures claim their bytes first, then larger captures fill in the
/// uncovered runs around them with their own kind. Equal-length
/// captures break ties by higher capture index (later in the query --
/// lets appended overrides beat upstream defaults).
///
/// Concretely, a template-string outer @string with an inner
/// @variable for a substitution like `${name}` produces three spans:
/// `String[backtick..${]`, `Variable[name]`, `String[}..backtick]` --
/// instead of dropping the outer @string entirely, which is what a
/// naive "smallest-wins-then-skip-conflicts" approach would do.
///
/// Uses a per-byte coverage bitmap; O(source_len + total_span_len).
/// For the 50k-LoC cap that's a few hundred KB of scratch -- fine.
fn resolve_overlaps_split(
    mut raw: Vec<(usize, usize, HighlightKind, usize)>,
    source_len: usize,
) -> Vec<Span> {
    raw.sort_by(|a, b| {
        let la = a.1 - a.0;
        let lb = b.1 - b.0;
        // Smaller spans first (more specific); within equal length,
        // the pattern that appears later in the query wins so
        // appended overrides take effect over upstream defaults.
        la.cmp(&lb).then(b.3.cmp(&a.3))
    });

    let mut covered = vec![false; source_len];
    let mut out: Vec<Span> = Vec::new();
    for (start, end, kind, _) in raw {
        // Walk [start, end) and emit each maximal uncovered run as a
        // span. Already-covered bytes are skipped because a more
        // specific capture already claimed them.
        let mut i = start;
        while i < end {
            while i < end && covered[i] {
                i += 1;
            }
            if i >= end {
                break;
            }
            let run_start = i;
            while i < end && !covered[i] {
                covered[i] = true;
                i += 1;
            }
            out.push(Span {
                start: run_start,
                end: i,
                kind,
            });
        }
    }
    out.sort_by_key(|s| s.start);
    out
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
        let src = "const App = () => <View>{count}</View>;";
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
    fn json_empty_object_has_no_classified_runs() {
        // Braces map to @punctuation.bracket which our table drops, so
        // a bare `{}` still produces zero highlight spans.
        let spans = highlight("{}", Language::Json);
        assert!(spans.is_empty());
    }

    // ------- new coverage for expanded kinds -------

    #[test]
    fn js_classifies_identifiers_as_variables() {
        let src = "function add(a, b) { return a + b; }";
        let ks = kinds(&highlight(src, Language::JavaScript));
        assert!(ks.contains(&HighlightKind::Variable));
    }

    #[test]
    fn tsx_classifies_jsx_tag() {
        let src = "const App = () => <View />;";
        let ks = kinds(&highlight(src, Language::Tsx));
        assert!(ks.contains(&HighlightKind::Tag));
    }

    #[test]
    fn capture_name_collapses_subcaptures_to_base() {
        assert_eq!(
            HighlightKind::from_capture("function.method"),
            Some(HighlightKind::Function)
        );
        assert_eq!(
            HighlightKind::from_capture("function.builtin"),
            Some(HighlightKind::Function)
        );
        assert_eq!(
            HighlightKind::from_capture("variable.parameter"),
            Some(HighlightKind::Variable)
        );
    }

    #[test]
    fn capture_name_drops_unknown_categories() {
        assert_eq!(HighlightKind::from_capture("punctuation.bracket"), None);
        assert_eq!(HighlightKind::from_capture("operator"), None);
        assert_eq!(HighlightKind::from_capture("embedded"), None);
    }

    #[test]
    fn overlap_resolver_splits_outer_around_inner() {
        // Smaller (5..10) wins for its bytes; the outer (0..20) gets
        // split into two surviving pieces around it. Same shape as a
        // template-string outer @string with an inner @variable.
        let raw = vec![
            (0, 20, HighlightKind::String, 0),
            (5, 10, HighlightKind::Variable, 1),
        ];
        let spans = resolve_overlaps_split(raw, 20);
        assert_eq!(spans.len(), 3);
        assert_eq!(
            spans[0],
            Span {
                start: 0,
                end: 5,
                kind: HighlightKind::String,
            }
        );
        assert_eq!(
            spans[1],
            Span {
                start: 5,
                end: 10,
                kind: HighlightKind::Variable,
            }
        );
        assert_eq!(
            spans[2],
            Span {
                start: 10,
                end: 20,
                kind: HighlightKind::String,
            }
        );
    }

    #[test]
    fn overlap_resolver_breaks_equal_length_tie_by_pattern_index() {
        // Equal-length captures: the later pattern in the query
        // (higher pattern_index) wins. This is how atomio overrides
        // beat upstream patterns of the same shape.
        let raw = vec![
            (0, 5, HighlightKind::Constant, 3),
            (0, 5, HighlightKind::Number, 7),
        ];
        let spans = resolve_overlaps_split(raw, 5);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, HighlightKind::Number);
    }
}
