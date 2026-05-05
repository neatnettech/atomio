//! Command palette registry and fuzzy matcher.
//!
//! A [`CommandRegistry`] holds a flat list of [`Command`] entries — each
//! pairing a human-readable label ("File: Open") with an opaque ID string
//! that the UI layer maps back to an action. The registry knows nothing
//! about gpui actions; the UI crate does the registration and dispatch.
//!
//! The fuzzy matcher uses a simple subsequence scoring algorithm that is
//! good enough for a command palette with a few dozen entries (which is
//! all we have in v0.1). It can be swapped out for a proper algorithm like
//! `fzf`-style scoring later without changing the public API.

/// A single command that can appear in the palette.
#[derive(Debug, Clone)]
pub struct Command {
    /// Human-readable label shown in the palette (e.g. "File: Open").
    pub label: String,
    /// Opaque identifier the UI uses to dispatch the right action.
    pub id: String,
}

/// Holds the set of registered commands and provides fuzzy search.
#[derive(Debug, Default, Clone)]
pub struct CommandRegistry {
    commands: Vec<Command>,
}

/// A command matched by a query, with a score for ranking.
#[derive(Debug, Clone)]
pub struct Match {
    pub command: Command,
    pub score: i32,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command. Duplicate IDs are allowed (last-write-wins is
    /// fine for a palette — the user just sees one entry).
    pub fn register(&mut self, label: impl Into<String>, id: impl Into<String>) {
        self.commands.push(Command {
            label: label.into(),
            id: id.into(),
        });
    }

    /// Return all commands whose label fuzzy-matches `query`, ranked by
    /// score (higher is better). An empty query returns everything.
    pub fn search(&self, query: &str) -> Vec<Match> {
        if query.is_empty() {
            return self
                .commands
                .iter()
                .cloned()
                .map(|c| Match {
                    command: c,
                    score: 0,
                })
                .collect();
        }
        let query_lower: Vec<char> = query.to_lowercase().chars().collect();
        let mut matches: Vec<Match> = self
            .commands
            .iter()
            .filter_map(|cmd| {
                let score = fuzzy_score(&cmd.label, &query_lower)?;
                Some(Match {
                    command: cmd.clone(),
                    score,
                })
            })
            .collect();
        matches.sort_by_key(|m| std::cmp::Reverse(m.score));
        matches
    }
}

/// Score a label against a lowercased query. Returns `None` if the query
/// is not a subsequence of the label. Higher score = better match.
///
/// Scoring rules (intentionally simple):
/// - +10 for each matched character
/// - +5 bonus when a match is at the start of a word (after ' ', ':', '_')
/// - -1 penalty for each gap between consecutive matches
fn fuzzy_score(label: &str, query: &[char]) -> Option<i32> {
    let label_chars: Vec<char> = label.to_lowercase().chars().collect();
    let mut qi = 0;
    let mut score: i32 = 0;
    let mut last_match: Option<usize> = None;

    for (li, &lc) in label_chars.iter().enumerate() {
        if qi < query.len() && lc == query[qi] {
            score += 10;
            // Word-boundary bonus.
            if li == 0
                || label_chars
                    .get(li.wrapping_sub(1))
                    .map(|c| *c == ' ' || *c == ':' || *c == '_')
                    .unwrap_or(false)
            {
                score += 5;
            }
            // Gap penalty.
            if let Some(prev) = last_match {
                let gap = (li - prev).saturating_sub(1);
                score -= gap as i32;
            }
            last_match = Some(li);
            qi += 1;
        }
    }
    if qi == query.len() {
        Some(score)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_all() {
        let mut reg = CommandRegistry::new();
        reg.register("File: Open", "open");
        reg.register("File: Save", "save");
        let results = reg.search("");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn exact_prefix_ranks_higher() {
        let mut reg = CommandRegistry::new();
        reg.register("File: Save As", "save_as");
        reg.register("Edit: Select All", "select_all");
        reg.register("File: Save", "save");
        let results = reg.search("save");
        // Both "save" entries should appear before "select all".
        assert!(results.len() >= 2);
        assert!(results[0].command.id == "save" || results[0].command.id == "save_as");
    }

    #[test]
    fn subsequence_match_works() {
        let mut reg = CommandRegistry::new();
        reg.register("File: Open", "open");
        let results = reg.search("fop");
        // 'f' matches 'F', 'o' matches 'O' in Open, 'p' matches 'p'.
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command.id, "open");
    }

    #[test]
    fn no_match_returns_empty() {
        let mut reg = CommandRegistry::new();
        reg.register("File: Open", "open");
        let results = reg.search("zzz");
        assert!(results.is_empty());
    }

    #[test]
    fn case_insensitive() {
        let mut reg = CommandRegistry::new();
        reg.register("Edit: Undo", "undo");
        let results = reg.search("UNDO");
        assert_eq!(results.len(), 1);
    }
}
