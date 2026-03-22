//! Wiki link parsing and resolution
//!
//! This module handles parsing `[[wiki::link]]` syntax and resolving
//! link paths to atom IDs.
//!
//! # Link Syntax
//!
//! - `[[foo::bar]]` - Absolute path with segments separated by `::`
//! - `[["file name.md"]]` - Quoted segment for names with special characters
//! - `[[foo::"my file"]]` - Mixed quoted/unquoted segments
//! - `[[./sibling]]` - Relative to current namespace
//! - `[[../uncle]]` - Parent's sibling
//! - `[[..]]` - Parent namespace
//! - `[[/foo::bar]]` - Explicit absolute (same as no prefix)
//!
//! # Escape Sequences (within quotes)
//!
//! - `\"` - Literal quote
//! - `\\` - Literal backslash

/// Parsed link from content
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLink {
    /// The full original text including brackets: `[[foo::bar]]`
    pub original: String,
    /// The path segments: `["foo", "bar"]`
    pub segments: Vec<String>,
    /// Whether this is a relative link (starts with ./ or ../)
    pub is_relative: bool,
    /// Number of parent levels to traverse (0 for absolute, 1 for ./, 2 for ../, etc.)
    pub parent_levels: usize,
    /// Start position in the original content
    pub start: usize,
    /// End position in the original content
    pub end: usize,
}

/// Parser state machine
#[derive(Debug, Clone, PartialEq)]
enum ParseState {
    /// Looking for opening [[
    LookingForOpen,
    /// Just found first [
    FoundFirstBracket,
    /// Inside the link, parsing segments
    InLink,
    /// Inside a quoted segment
    InQuote,
    /// Found escape character inside quote
    InQuoteEscape,
    /// Found first ] - might be end of link
    FoundFirstClose,
    /// Just saw a : - might be :: separator
    FoundFirstColon,
}

/// Parse all wiki links from content
///
/// Extracts all `[[...]]` links from the given content string,
/// returning their parsed structure and positions.
///
/// # Example
///
/// ```
/// use yap_core::links::parse_links;
///
/// let content = "See [[foo::bar]] and [[./sibling]]";
/// let links = parse_links(content);
///
/// assert_eq!(links.len(), 2);
/// assert_eq!(links[0].segments, vec!["foo", "bar"]);
/// assert!(!links[0].is_relative);
/// assert_eq!(links[1].segments, vec!["sibling"]);
/// assert!(links[1].is_relative);
/// ```
pub fn parse_links(content: &str) -> Vec<ParsedLink> {
    let mut links = Vec::new();
    let mut state = ParseState::LookingForOpen;
    let mut link_start = 0;
    let mut current_segment = String::new();
    let mut segments: Vec<String> = Vec::new();
    let mut first_segment_quoted = false;

    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        match state {
            ParseState::LookingForOpen => {
                if c == '[' {
                    state = ParseState::FoundFirstBracket;
                    link_start = i;
                }
            }

            ParseState::FoundFirstBracket => {
                if c == '[' {
                    state = ParseState::InLink;
                    segments.clear();
                    current_segment.clear();
                    first_segment_quoted = false;
                } else {
                    state = ParseState::LookingForOpen;
                    // Don't advance - reprocess this character
                    if c == '[' {
                        continue;
                    }
                }
            }

            ParseState::InLink => {
                if c == ']' {
                    state = ParseState::FoundFirstClose;
                } else if c == '"' {
                    // Track if the first segment is quoted (for relative path disambiguation)
                    if segments.is_empty() && current_segment.is_empty() {
                        first_segment_quoted = true;
                    }
                    state = ParseState::InQuote;
                } else if c == ':' {
                    state = ParseState::FoundFirstColon;
                } else {
                    current_segment.push(c);
                }
            }

            ParseState::InQuote => {
                if c == '\\' {
                    state = ParseState::InQuoteEscape;
                } else if c == '"' {
                    state = ParseState::InLink;
                } else {
                    current_segment.push(c);
                }
            }

            ParseState::InQuoteEscape => {
                // Accept any character after escape, but only \ and " are documented
                current_segment.push(c);
                state = ParseState::InQuote;
            }

            ParseState::FoundFirstColon => {
                if c == ':' {
                    // Complete :: separator - finish current segment
                    if !current_segment.is_empty() {
                        segments.push(current_segment.clone());
                        current_segment.clear();
                    }
                    state = ParseState::InLink;
                } else {
                    // Single colon - just part of the segment
                    current_segment.push(':');
                    current_segment.push(c);
                    state = ParseState::InLink;
                }
            }

            ParseState::FoundFirstClose => {
                if c == ']' {
                    // Complete ]] - link is done
                    if !current_segment.is_empty() {
                        segments.push(current_segment.clone());
                    }

                    let link_end = i + 1;
                    let original: String = chars[link_start..link_end].iter().collect();

                    // Determine if relative and parent levels
                    // If the first segment was quoted, treat it as literal (not a path prefix)
                    let (is_relative, parent_levels, final_segments) = if first_segment_quoted {
                        (false, 0, segments.clone())
                    } else {
                        process_relative_path(&segments)
                    };

                    // Only add if we have at least one segment (or it's a parent reference)
                    if !final_segments.is_empty() || parent_levels > 0 {
                        links.push(ParsedLink {
                            original,
                            segments: final_segments,
                            is_relative,
                            parent_levels,
                            start: link_start,
                            end: link_end,
                        });
                    }

                    segments.clear();
                    current_segment.clear();
                    first_segment_quoted = false;
                    state = ParseState::LookingForOpen;
                } else {
                    // Single ] - part of content (unusual but handle it)
                    current_segment.push(']');
                    if c == ']' {
                        state = ParseState::FoundFirstClose;
                    } else {
                        current_segment.push(c);
                        state = ParseState::InLink;
                    }
                }
            }
        }

        i += 1;
    }

    links
}

/// Process relative path prefixes and determine path type
///
/// Returns (is_relative, levels_up, cleaned_segments) where:
/// - levels_up = 0 for ./ (add child to current namespace)
/// - levels_up = 1 for ../ or [[..]] (go up one level)
/// - levels_up = 2 for ../../ (go up two levels)
fn process_relative_path(segments: &[String]) -> (bool, usize, Vec<String>) {
    if segments.is_empty() {
        return (false, 0, Vec::new());
    }

    let first = &segments[0];

    // Handle explicit absolute with /
    if let Some(stripped) = first.strip_prefix('/') {
        let mut result = vec![stripped.to_string()];
        result.extend(segments[1..].iter().cloned());
        // Filter out empty segments
        let result: Vec<String> = result.into_iter().filter(|s| !s.is_empty()).collect();
        return (false, 0, result);
    }

    // Handle relative paths with . or .. as separate segments
    if first == "." || first == ".." {
        let mut levels_up = 0;
        let mut start_idx = 0;
        let mut saw_dot = false;

        for (idx, seg) in segments.iter().enumerate() {
            if seg == ".." {
                levels_up += 1;
                start_idx = idx + 1;
            } else if seg == "." {
                saw_dot = true;
                start_idx = idx + 1;
            } else {
                break;
            }
        }

        let final_segments: Vec<String> = segments[start_idx..].to_vec();

        // If we only saw "." with no "..", levels_up stays 0 (stay at current level)
        // If we saw "..", levels_up counts how many levels to go up
        // Mark as relative if we saw either . or ..
        let is_relative = levels_up > 0 || saw_dot;
        return (is_relative, levels_up, final_segments);
    }

    // Handle ./ or ../ at start of first segment (e.g., "./sibling" as single segment)
    if let Some(rest) = first.strip_prefix("./") {
        let mut result = if rest.is_empty() {
            Vec::new()
        } else {
            vec![rest.to_string()]
        };
        result.extend(segments[1..].iter().cloned());
        return (true, 0, result); // levels_up = 0 for ./
    }

    if let Some(rest) = first.strip_prefix("../") {
        let mut result = if rest.is_empty() {
            Vec::new()
        } else {
            vec![rest.to_string()]
        };
        result.extend(segments[1..].iter().cloned());
        return (true, 1, result); // levels_up = 1 for ../
    }

    // Absolute path
    (false, 0, segments.to_vec())
}

/// Resolve a link path to a full namespace path
///
/// Given a parsed path and an optional context namespace, resolves
/// relative references and returns the full namespace path as segments.
///
/// # Arguments
///
/// * `segments` - The path segments from parsing
/// * `is_relative` - Whether this is a relative path
/// * `levels_up` - Number of levels to go up (0 = stay at current, 1 = parent, etc.)
/// * `context_namespace` - The namespace of the block containing this link
///
/// # Returns
///
/// The resolved full namespace path as segments, or None if invalid
///
/// # Example
///
/// ```
/// use yap_core::links::resolve_path;
///
/// // Absolute path - returns as-is
/// let resolved = resolve_path(&["foo", "bar"], false, 0, None);
/// assert_eq!(resolved, Some(vec!["foo".to_string(), "bar".to_string()]));
///
/// // Relative path ./sibling from context research::ml (levels_up=0, stay at current)
/// let resolved = resolve_path(&["sibling"], true, 0, Some("research::ml"));
/// assert_eq!(resolved, Some(vec!["research".to_string(), "ml".to_string(), "sibling".to_string()]));
///
/// // Parent reference ../uncle from context research::ml::attention (levels_up=1)
/// let resolved = resolve_path(&["uncle"], true, 1, Some("research::ml::attention"));
/// assert_eq!(resolved, Some(vec!["research".to_string(), "ml".to_string(), "uncle".to_string()]));
/// ```
pub fn resolve_path(
    segments: &[impl AsRef<str>],
    is_relative: bool,
    levels_up: usize,
    context_namespace: Option<&str>,
) -> Option<Vec<String>> {
    let segments: Vec<String> = segments.iter().map(|s| s.as_ref().to_string()).collect();

    if !is_relative {
        // Absolute path - return as-is
        if segments.is_empty() {
            return None;
        }
        return Some(segments);
    }

    // Relative path - need context
    let context = context_namespace?;
    let context_segments: Vec<&str> = if context.is_empty() {
        Vec::new()
    } else {
        context.split("::").collect()
    };

    // Calculate how many context segments to keep
    // levels_up=0 (./) means stay at current level
    // levels_up=1 (../) means go up one level (parent)
    // levels_up=2 (../../) means go up two levels (grandparent)
    let keep_count = context_segments.len().saturating_sub(levels_up);

    if keep_count == 0 && segments.is_empty() {
        // Can't go above root with no target
        return None;
    }

    let mut result: Vec<String> = context_segments[..keep_count]
        .iter()
        .map(|s| s.to_string())
        .collect();
    result.extend(segments);

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Format segments back to namespace path string
///
/// # Example
///
/// ```
/// use yap_core::links::format_namespace;
///
/// let ns = format_namespace(&["foo", "bar", "baz"]);
/// assert_eq!(ns, "foo::bar::baz");
/// ```
pub fn format_namespace(segments: &[impl AsRef<str>]) -> String {
    segments
        .iter()
        .map(|s| s.as_ref())
        .collect::<Vec<_>>()
        .join("::")
}

/// Format segments as a wiki link
///
/// # Example
///
/// ```
/// use yap_core::links::format_link;
///
/// let link = format_link(&["foo", "bar"]);
/// assert_eq!(link, "[[foo::bar]]");
///
/// // Segments needing quotes
/// let link = format_link(&["foo", "my file.md"]);
/// assert_eq!(link, "[[foo::\"my file.md\"]]");
/// ```
pub fn format_link(segments: &[impl AsRef<str>]) -> String {
    let formatted_segments: Vec<String> = segments
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let s = s.as_ref();
            // Quote if segment contains characters that the parser treats specially,
            // or if the first segment could be misinterpreted as a relative path prefix
            let needs_quoting = s.contains(':')
                || s.contains(' ')
                || s.contains('"')
                || s.contains('[')
                || s.contains(']')
                || (i == 0
                    && (s == "."
                        || s == ".."
                        || s.starts_with("./")
                        || s.starts_with("../")
                        || s.starts_with('/')));
            if needs_quoting {
                // Escape quotes and backslashes within
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{}\"", escaped)
            } else {
                s.to_string()
            }
        })
        .collect();

    format!("[[{}]]", formatted_segments.join("::"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Phase 1.5 Tests - Link Parser
    // =========================================================================

    #[test]
    fn test_parse_simple_link() {
        let content = "See [[foo::bar]] for details";
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].original, "[[foo::bar]]");
        assert_eq!(links[0].segments, vec!["foo", "bar"]);
        assert!(!links[0].is_relative);
        assert_eq!(links[0].parent_levels, 0);
        assert_eq!(links[0].start, 4);
        assert_eq!(links[0].end, 16); // Exclusive end
    }

    #[test]
    fn test_parse_quoted_link() {
        let content = r#"File: [["my file.md"]]"#;
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].segments, vec!["my file.md"]);
    }

    #[test]
    fn test_parse_quoted_with_colons() {
        let content = r#"Special: [["name::with::colons"]]"#;
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].segments, vec!["name::with::colons"]);
    }

    #[test]
    fn test_parse_escaped_link() {
        let content = r#"Escaped: [[foo::"bar \"quoted\" baz"]]"#;
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].segments, vec!["foo", "bar \"quoted\" baz"]);
    }

    #[test]
    fn test_parse_relative_link() {
        let content = "See [[./sibling]] for more";
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].segments, vec!["sibling"]);
        assert!(links[0].is_relative);
        assert_eq!(links[0].parent_levels, 0); // ./ means stay at current level (0 levels up)
    }

    #[test]
    fn test_parse_parent_link() {
        let content = "Parent: [[..]]";
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert!(links[0].segments.is_empty());
        assert!(links[0].is_relative);
        assert_eq!(links[0].parent_levels, 1); // 1 level up
    }

    #[test]
    fn test_parse_parent_relative_link() {
        let content = "Uncle: [[../uncle]]";
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].segments, vec!["uncle"]);
        assert!(links[0].is_relative);
        assert_eq!(links[0].parent_levels, 1); // 1 level up
    }

    #[test]
    fn test_parse_absolute_link() {
        let content = "Explicit: [[/foo::bar]]";
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].segments, vec!["foo", "bar"]);
        assert!(!links[0].is_relative);
    }

    #[test]
    fn test_extract_all_links() {
        let content = "See [[foo]] and [[bar::baz]] with [[./relative]]";
        let links = parse_links(content);

        assert_eq!(links.len(), 3);
        assert_eq!(links[0].segments, vec!["foo"]);
        assert_eq!(links[1].segments, vec!["bar", "baz"]);
        assert_eq!(links[2].segments, vec!["relative"]);
    }

    #[test]
    fn test_no_links() {
        let content = "No links here, just text with [single brackets]";
        let links = parse_links(content);

        assert!(links.is_empty());
    }

    #[test]
    fn test_malformed_links() {
        // Single bracket
        let links = parse_links("Text [single] bracket");
        assert!(links.is_empty());

        // Unclosed
        let links = parse_links("Text [[unclosed");
        assert!(links.is_empty());

        // Empty
        let links = parse_links("Text [[]] empty");
        assert!(links.is_empty());
    }

    #[test]
    fn test_parse_mixed_quotes() {
        let content = r#"Mixed: [[foo::bar::"quoted segment"::baz]]"#;
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].segments,
            vec!["foo", "bar", "quoted segment", "baz"]
        );
    }

    #[test]
    fn test_parse_nested_quotes() {
        let content = r#"Nested: [[foo::"bar \"baz\" qux"]]"#;
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].segments, vec!["foo", "bar \"baz\" qux"]);
    }

    #[test]
    fn test_parse_double_parent() {
        // Note: Multi-level relative paths like ../../ are not fully supported in MVP
        // The design spec only shows ./, ../, and [[..]]
        // This test documents current behavior: only one level of ../ is stripped
        let content = "Grandparent: [[../../cousin]]";
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        // Currently parses as single level up with "../cousin" as remaining
        // (the second ../ is left as-is in the segment)
        assert_eq!(links[0].segments, vec!["../cousin"]);
        assert!(links[0].is_relative);
        assert_eq!(links[0].parent_levels, 1);
    }

    #[test]
    fn test_positions_accurate() {
        let content = "abc [[foo]] def [[bar]]";
        let links = parse_links(content);

        assert_eq!(links.len(), 2);

        // First link
        assert_eq!(links[0].start, 4);
        assert_eq!(links[0].end, 11);
        assert_eq!(&content[links[0].start..links[0].end], "[[foo]]");

        // Second link
        assert_eq!(links[1].start, 16);
        assert_eq!(links[1].end, 23);
        assert_eq!(&content[links[1].start..links[1].end], "[[bar]]");
    }

    // =========================================================================
    // Phase 1.6 Tests - Link Resolver
    // =========================================================================

    #[test]
    fn test_resolve_absolute() {
        let resolved = resolve_path(&["foo", "bar"], false, 0, None);
        assert_eq!(resolved, Some(vec!["foo".to_string(), "bar".to_string()]));
    }

    #[test]
    fn test_resolve_relative() {
        // ./sibling from research::ml (levels_up=0 means stay at current)
        let resolved = resolve_path(&["sibling"], true, 0, Some("research::ml"));
        assert_eq!(
            resolved,
            Some(vec![
                "research".to_string(),
                "ml".to_string(),
                "sibling".to_string()
            ])
        );
    }

    #[test]
    fn test_resolve_parent() {
        // [[..]] from research::ml::attention -> research::ml (levels_up=1)
        let empty: &[&str] = &[];
        let resolved = resolve_path(empty, true, 1, Some("research::ml::attention"));
        assert_eq!(
            resolved,
            Some(vec!["research".to_string(), "ml".to_string()])
        );
    }

    #[test]
    fn test_resolve_parent_sibling() {
        // ../uncle from research::ml::attention -> research::ml::uncle (levels_up=1)
        let resolved = resolve_path(&["uncle"], true, 1, Some("research::ml::attention"));
        assert_eq!(
            resolved,
            Some(vec![
                "research".to_string(),
                "ml".to_string(),
                "uncle".to_string()
            ])
        );
    }

    #[test]
    fn test_resolve_missing_context() {
        // Relative path without context should return None
        let resolved = resolve_path(&["sibling"], true, 1, None);
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_resolve_above_root() {
        // Can't go above root
        let empty: &[&str] = &[];
        let resolved = resolve_path(empty, true, 2, Some("foo"));
        assert_eq!(resolved, None);
    }

    // =========================================================================
    // Utility Function Tests
    // =========================================================================

    #[test]
    fn test_format_namespace() {
        assert_eq!(format_namespace(&["foo", "bar", "baz"]), "foo::bar::baz");
        assert_eq!(format_namespace(&["single"]), "single");
        let empty: &[&str] = &[];
        assert_eq!(format_namespace(empty), "");
    }

    #[test]
    fn test_format_link() {
        assert_eq!(format_link(&["foo", "bar"]), "[[foo::bar]]");
        assert_eq!(
            format_link(&["foo", "my file.md"]),
            "[[foo::\"my file.md\"]]"
        );
        assert_eq!(format_link(&["has::colons"]), "[[\"has::colons\"]]");
    }

    #[test]
    fn test_format_link_escaping() {
        assert_eq!(
            format_link(&["foo", "quote\"here"]),
            "[[foo::\"quote\\\"here\"]]"
        );
    }

    // =========================================================================
    // Additional Parser Edge Cases
    // =========================================================================

    #[test]
    fn test_parse_link_at_start_of_content() {
        let content = "[[foo::bar]] rest of text";
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].start, 0);
        assert_eq!(links[0].end, 12);
        assert_eq!(links[0].segments, vec!["foo", "bar"]);
    }

    #[test]
    fn test_parse_link_at_end_of_content() {
        let content = "start of text [[foo::bar]]";
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].end, content.len());
        assert_eq!(links[0].segments, vec!["foo", "bar"]);
    }

    #[test]
    fn test_parse_consecutive_links_no_space() {
        let content = "[[foo]][[bar]]";
        let links = parse_links(content);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].segments, vec!["foo"]);
        assert_eq!(links[1].segments, vec!["bar"]);
        // They should be adjacent with no gap
        assert_eq!(links[0].end, links[1].start);
    }

    #[test]
    fn test_parse_link_only_content() {
        // Content is exactly one link, nothing else
        let content = "[[sole::link]]";
        let links = parse_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].start, 0);
        assert_eq!(links[0].end, content.len());
    }

    #[test]
    fn test_resolve_multilevel_parent() {
        // ../../grandparent from a::b::c → a::grandparent (2 levels up from c, then navigate)
        // parent_levels=2 means strip 2 segments from context before appending
        let resolved = resolve_path(&["grandparent"], true, 2, Some("a::b::c"));
        assert_eq!(
            resolved,
            Some(vec!["a".to_string(), "grandparent".to_string()])
        );
    }

    #[test]
    fn test_resolve_multilevel_parent_exact_root() {
        // From a::b, going up 1 level with no additional segments -> just "a"
        let empty: &[&str] = &[];
        let resolved = resolve_path(empty, true, 1, Some("a::b"));
        assert_eq!(resolved, Some(vec!["a".to_string()]));
    }

    #[test]
    fn test_format_link_backslash_in_segment() {
        // Segments with backslashes need quoting (they contain a special char)
        let result = format_link(&["foo", "back\\slash"]);
        // Should be quoted since backslash is a special character
        assert!(result.starts_with("[[") && result.ends_with("]]"));
        // Must be parseable back to the original segment
        let parsed = parse_links(&result);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].segments, vec!["foo", "back\\slash"]);
    }
}

#[cfg(test)]
mod proptest_links {
    use super::*;
    use proptest::prelude::*;

    /// Strategy: alphanumeric + selected special chars, 1-20 chars, non-empty after trim
    fn arb_segment() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 :._-]{1,20}"
            .prop_filter("segment must be non-empty after trim", |s| {
                let trimmed = s.trim();
                !trimmed.is_empty()
            })
            .prop_map(|s| s.trim().to_string())
    }

    /// Strategy: Vec of 1..5 segments
    fn arb_segments() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec(arb_segment(), 1..=5)
    }

    /// Strategy: text that can't accidentally contain `[[`
    fn arb_plain_text() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?;]{0,30}"
    }

    /// Strategy: content with embedded links, returns (content_string, list_of_segment_vecs)
    fn arb_content_with_links() -> impl Strategy<Value = (String, Vec<Vec<String>>)> {
        prop::collection::vec((arb_plain_text(), arb_segments()), 1..=5).prop_map(|parts| {
            let mut content = String::new();
            let mut all_segs = Vec::new();
            for (text, segs) in &parts {
                content.push_str(text);
                content.push_str(&format_link(segs));
                all_segs.push(segs.clone());
            }
            (content, all_segs)
        })
    }

    proptest! {
        // 1. parse_format_roundtrip — parse(format(segs)) recovers original segments
        #[test]
        fn parse_format_roundtrip(segs in arb_segments()) {
            let formatted = format_link(&segs);
            let parsed = parse_links(&formatted);
            prop_assert_eq!(parsed.len(), 1, "expected exactly one link from formatted string: {}", formatted);
            prop_assert_eq!(&parsed[0].segments, &segs, "roundtrip failed for formatted: {}", formatted);
        }

        // 2. format_parse_format_idempotent — formatted string stable after one roundtrip
        #[test]
        fn format_parse_format_idempotent(segs in arb_segments()) {
            let first = format_link(&segs);
            let parsed = parse_links(&first);
            prop_assert_eq!(parsed.len(), 1);
            let second = format_link(&parsed[0].segments);
            prop_assert_eq!(&first, &second, "format not idempotent: first={}, second={}", first, second);
        }

        // 3. position_correctness — char-based slicing recovers original text
        #[test]
        fn position_correctness(
            prefix in arb_plain_text(),
            segs in arb_segments(),
            suffix in arb_plain_text(),
        ) {
            let link_str = format_link(&segs);
            let content = format!("{}{}{}", prefix, link_str, suffix);
            let parsed = parse_links(&content);
            prop_assert!(!parsed.is_empty(), "should find at least one link in: {}", content);

            let chars: Vec<char> = content.chars().collect();
            for link in &parsed {
                let slice: String = chars[link.start..link.end].iter().collect();
                prop_assert_eq!(&slice, &link.original,
                    "char-based slice mismatch: slice='{}', original='{}'", slice, link.original);
            }
        }

        // 4. position_correctness_utf8 — verify char-based slicing works with multi-byte chars,
        //    and document that byte indices differ from char indices
        #[test]
        fn position_correctness_utf8(
            prefix_chars in prop::collection::vec(prop::char::range('\u{00e0}', '\u{00ff}'), 1..=5),
            segs in arb_segments(),
        ) {
            let prefix: String = prefix_chars.into_iter().collect();
            let link_str = format_link(&segs);
            let content = format!("{}{}", prefix, link_str);
            let parsed = parse_links(&content);
            prop_assert!(!parsed.is_empty(), "should find at least one link in: {}", content);

            // Char-based slicing should work
            let chars: Vec<char> = content.chars().collect();
            for link in &parsed {
                let slice: String = chars[link.start..link.end].iter().collect();
                prop_assert_eq!(&slice, &link.original,
                    "char-based slice should work with multi-byte prefix");
            }

            // Document: char indices != byte indices when multi-byte chars are present
            // The parser returns char indices. Byte-based &content[start..end] would fail.
            let first_link = &parsed[0];
            let byte_offset_of_link = prefix.len(); // byte length of prefix
            let char_offset_of_link = content.chars().take_while(|_| true)
                .collect::<Vec<_>>()
                .len();
            let _ = char_offset_of_link; // suppress unused
            // With multi-byte prefix, byte offset != char offset
            if prefix.len() != prefix.chars().count() {
                prop_assert_ne!(first_link.start, byte_offset_of_link,
                    "char index should differ from byte index when multi-byte chars present");
            }
        }

        // 5. no_orphan_links — after removing all parsed links, no [[ ]] pairs remain
        #[test]
        fn no_orphan_links((content, _expected_segs) in arb_content_with_links()) {
            let parsed = parse_links(&content);
            prop_assert!(!parsed.is_empty(), "should find links in generated content");

            // Remove links in reverse order (by char positions)
            let chars: Vec<char> = content.chars().collect();
            let mut remaining_chars = chars.clone();
            let mut sorted_links = parsed.clone();
            sorted_links.sort_by(|a, b| b.start.cmp(&a.start)); // reverse order
            for link in &sorted_links {
                remaining_chars.drain(link.start..link.end);
            }
            let remaining: String = remaining_chars.into_iter().collect();

            // Should have no complete [[ ]] pairs left
            let leftover_links = parse_links(&remaining);
            prop_assert_eq!(leftover_links.len(), 0,
                "orphan links found after removal in: '{}'", remaining);
        }

        // 6. segment_quoting_safety — segments with special chars roundtrip correctly
        #[test]
        fn segment_quoting_safety(base in arb_segment()) {
            // Segment with space
            let with_space = format!("{} x", base);
            let formatted = format_link(std::slice::from_ref(&with_space));
            let parsed = parse_links(&formatted);
            prop_assert_eq!(parsed.len(), 1);
            prop_assert_eq!(&parsed[0].segments[0], &with_space);

            // Segment with ::
            let with_colons = format!("{}::y", base);
            let formatted = format_link(std::slice::from_ref(&with_colons));
            let parsed = parse_links(&formatted);
            prop_assert_eq!(parsed.len(), 1);
            prop_assert_eq!(&parsed[0].segments[0], &with_colons);
        }

        // 7. single_colon_preserved — single colon is part of segment, not a separator
        #[test]
        fn single_colon_preserved(
            a in "[a-zA-Z0-9]{1,10}",
            b in "[a-zA-Z0-9]{1,10}",
        ) {
            let content = format!("[[{}:{}]]", a, b);
            let parsed = parse_links(&content);
            prop_assert_eq!(parsed.len(), 1, "should parse as one link: {}", content);
            prop_assert_eq!(parsed[0].segments.len(), 1,
                "single colon should not split segments: {:?}", parsed[0].segments);
            let expected = format!("{}:{}", a, b);
            prop_assert_eq!(&parsed[0].segments[0], &expected);
        }

        // 8. empty_bracket_rejection — [[]] padded with text yields no links
        #[test]
        fn empty_bracket_rejection(
            prefix in arb_plain_text(),
            suffix in arb_plain_text(),
        ) {
            let content = format!("{}[[]]{}",  prefix, suffix);
            let parsed = parse_links(&content);
            // [[]] should produce no valid links (empty brackets are rejected)
            for link in &parsed {
                prop_assert!(!link.segments.is_empty(),
                    "empty bracket link should be rejected, got: {:?}", link);
            }
        }

        // 9. relative_dot_resolution — ./ appends target to context
        #[test]
        fn relative_dot_resolution(
            ctx_segs in prop::collection::vec("[a-zA-Z]{2,8}", 2..=3),
            target in "[a-zA-Z]{2,8}",
        ) {
            let context = ctx_segs.join("::");
            let resolved = resolve_path(&[&target], true, 0, Some(&context));
            let mut expected: Vec<String> = ctx_segs.iter().map(|s| s.to_string()).collect();
            expected.push(target.to_string());
            prop_assert_eq!(resolved, Some(expected));
        }

        // 10. relative_dotdot_resolution — ../ strips last context segment and appends target
        #[test]
        fn relative_dotdot_resolution(
            ctx_segs in prop::collection::vec("[a-zA-Z]{2,8}", 2..=4),
            target in "[a-zA-Z]{2,8}",
        ) {
            let context = ctx_segs.join("::");
            let resolved = resolve_path(&[&target], true, 1, Some(&context));
            let mut expected: Vec<String> = ctx_segs[..ctx_segs.len()-1].iter().map(|s| s.to_string()).collect();
            expected.push(target.to_string());
            prop_assert_eq!(resolved, Some(expected));
        }

        // 11. absolute_ignores_context — absolute path returns segments as-is
        #[test]
        fn absolute_ignores_context(
            ctx_segs in prop::collection::vec("[a-zA-Z]{2,8}", 1..=3),
            segs in prop::collection::vec("[a-zA-Z]{2,8}", 1..=3),
        ) {
            let context = ctx_segs.join("::");
            let resolved = resolve_path(&segs, false, 0, Some(&context));
            let expected: Vec<String> = segs.iter().map(|s| s.to_string()).collect();
            prop_assert_eq!(resolved, Some(expected));
        }

        // 12. parent_level_clamping — parent_levels > context length doesn't panic
        #[test]
        fn parent_level_clamping(
            ctx_segs in prop::collection::vec("[a-zA-Z]{2,8}", 1..=3),
            extra_levels in 1..=5usize,
        ) {
            let context = ctx_segs.join("::");
            let parent_levels = ctx_segs.len() + extra_levels; // exceeds context depth
            // Should not panic
            let resolved = resolve_path(&["target"], true, parent_levels, Some(&context));
            // Result should be either None or Some (but not a panic)
            // When all context is stripped, we get just ["target"], which is non-empty, so Some
            // Actually: keep_count = ctx_segs.len().saturating_sub(parent_levels) = 0
            // result = ["target"], which is non-empty, so Some(["target"])
            prop_assert!(resolved.is_some() || resolved.is_none(),
                "resolve_path should not panic with parent_levels > context depth");
        }

        // 13. consecutive_links — two adjacent formatted links are both parsed
        #[test]
        fn consecutive_links(
            segs1 in arb_segments(),
            segs2 in arb_segments(),
        ) {
            let link1 = format_link(&segs1);
            let link2 = format_link(&segs2);
            let content = format!("{}{}", link1, link2);
            let parsed = parse_links(&content);
            prop_assert_eq!(parsed.len(), 2, "should find exactly 2 links in: {}", content);
            prop_assert_eq!(parsed[0].end, parsed[1].start,
                "consecutive links should be adjacent: link1.end={}, link2.start={}", parsed[0].end, parsed[1].start);
        }

        // 14. single_close_bracket_inside_link — [[a]b]] parses as one link
        #[test]
        fn single_close_bracket_inside_link(
            prefix in arb_plain_text(),
            suffix in arb_plain_text(),
        ) {
            let content = format!("{}[[a]b]]{}", prefix, suffix);
            let parsed = parse_links(&content);
            prop_assert_eq!(parsed.len(), 1,
                "[[a]b]] should parse as one link, got {} links in: {}", parsed.len(), content);
            prop_assert_eq!(&parsed[0].segments, &vec!["a]b".to_string()],
                "segment should be 'a]b', got: {:?}", parsed[0].segments);
        }

        // 15. nested_open_bracket_inside_link — [[a[[b]] doesn't panic
        #[test]
        fn nested_open_bracket_inside_link(
            prefix in arb_plain_text(),
            suffix in arb_plain_text(),
        ) {
            let content = format!("{}[[a[[b]]{}", prefix, suffix);
            // Should not panic — parser must handle gracefully
            let parsed = parse_links(&content);
            // We don't assert specific behavior, just that it doesn't crash
            // and returns some reasonable result
            let _ = parsed;
        }
    }

    // Non-proptest test demonstrating the byte-index vs char-index issue
    #[test]
    fn byte_index_vs_char_index_demo() {
        // "café" is 5 bytes (c=1, a=1, f=1, é=2) but 4 chars
        let content = "café [[foo]]";
        let parsed = parse_links(content);
        assert_eq!(parsed.len(), 1);

        // Parser returns char index (4+1=5 for space, then 5 for [[foo]])
        // actually: c(0) a(1) f(2) é(3) ' '(4) so [[foo]] starts at char 5
        assert_eq!(parsed[0].start, 5, "char index of [[ should be 5");

        // But byte offset of [[ is 6 (café = 5 bytes + space = 1 byte)
        let byte_offset = "café ".len();
        assert_eq!(byte_offset, 6, "byte offset should be 6");

        // Demonstrate: char index != byte index
        assert_ne!(
            parsed[0].start, byte_offset,
            "char index ({}) should differ from byte index ({}) with multi-byte chars",
            parsed[0].start, byte_offset
        );

        // Char-based slicing works correctly
        let chars: Vec<char> = content.chars().collect();
        let slice: String = chars[parsed[0].start..parsed[0].end].iter().collect();
        assert_eq!(slice, "[[foo]]");

        // Byte-based slicing with char index would panic or give wrong result
        // content[5..12] would panic because index 5 is in the middle of the é byte sequence
        // (We can't easily test the panic here since it would abort the test)
    }
}
