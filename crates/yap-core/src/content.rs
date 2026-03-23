//! Content serialization and deserialization
//!
//! Transforms content between editor view (with [[wiki::links]]) and
//! storage format (with {N} placeholders and links array).
//!
//! # Editor View
//!
//! ```text
//! See [[research::ml::attention]] for details.
//! Also related to [[./sibling]] and [[projects::"my doc.md"]].
//! ```
//!
//! # Storage Format
//!
//! ```text
//! content_template: "See {0} for details.\nAlso related to {1} and {2}."
//! links: [uuid-1, uuid-2, uuid-3]
//! ```

use uuid::Uuid;

use crate::error::Result;
use crate::links::{format_embed_link, format_link, parse_links};
use crate::store::Store;

/// Result of serializing editor content for storage
#[derive(Debug, Clone)]
pub struct SerializedContent {
    /// Content with {N} placeholders instead of wiki links
    pub template: String,
    /// Lineage IDs referenced by the placeholders, in order
    pub links: Vec<Uuid>,
    /// Any unresolved links that couldn't be converted (kept as literal text)
    pub unresolved: Vec<String>,
}

/// Serialize editor content to storage format.
///
/// Extracts wiki links, resolves them to lineage IDs via `db`, and replaces
/// them with `{N}` placeholders. Unresolved links are kept as literal text.
pub async fn serialize_content(
    db: &dyn Store,
    content: &str,
    context_namespace: Option<&str>,
) -> Result<SerializedContent> {
    let parsed_links = parse_links(content);

    if parsed_links.is_empty() {
        return Ok(SerializedContent {
            template: content.to_string(),
            links: Vec::new(),
            unresolved: Vec::new(),
        });
    }

    let mut template = String::new();
    let mut links: Vec<Uuid> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    let mut last_end = 0;

    for link in &parsed_links {
        template.push_str(&content[last_end..link.start]);

        let resolved = db
            .resolve_link_to_lineage(
                &link.segments,
                link.is_relative,
                link.parent_levels,
                context_namespace,
            )
            .await?;

        match resolved {
            Some(lineage_id) => {
                let placeholder_idx = links.len();
                if link.is_embed {
                    template.push_str(&format!("!{{{}}}", placeholder_idx));
                } else {
                    template.push_str(&format!("{{{}}}", placeholder_idx));
                }
                links.push(lineage_id);
            }
            None => {
                template.push_str(&link.original);
                unresolved.push(link.original.clone());
            }
        }

        last_end = link.end;
    }

    template.push_str(&content[last_end..]);

    Ok(SerializedContent {
        template,
        links,
        unresolved,
    })
}

/// Deserialize storage format back to editor view.
///
/// Replaces `{N}` placeholders with `[[wiki::link]]` syntax and
/// `!{N}` placeholders with `![[wiki::link]]` embed syntax, using
/// the canonical path for each lineage. Broken links render as `[[?]]`.
pub async fn deserialize_content(db: &dyn Store, template: &str, links: &[Uuid]) -> Result<String> {
    if links.is_empty() {
        return Ok(template.to_string());
    }

    let mut result = template.to_string();

    // Process in reverse order so earlier placeholder indices stay valid.
    for (idx, lineage_id) in links.iter().enumerate().rev() {
        let info = db.get_link_display_info(*lineage_id).await?;
        let segments: Option<Vec<&str>> = info.as_ref().map(|i| i.namespace.split("::").collect());

        // Replace embed placeholder !{N} first (before {N}, since {N} is a substring of !{N})
        let embed_placeholder = format!("!{{{}}}", idx);
        if result.contains(&embed_placeholder) {
            let link_text = match &segments {
                Some(segs) => format_embed_link(segs),
                None => "![[?]]".to_string(),
            };
            result = result.replace(&embed_placeholder, &link_text);
        }

        // Replace regular placeholder {N}
        let placeholder = format!("{{{}}}", idx);
        let link_text = match &segments {
            Some(segs) => format_link(segs),
            None => "[[?]]".to_string(),
        };
        result = result.replace(&placeholder, &link_text);
    }

    Ok(result)
}

/// Sync version of serialize — does not resolve links (for testing / offline use).
pub fn serialize_content_sync(content: &str) -> SerializedContent {
    let parsed_links = parse_links(content);
    SerializedContent {
        template: content.to_string(),
        links: Vec::new(),
        unresolved: parsed_links.iter().map(|l| l.original.clone()).collect(),
    }
}

/// Sync version of deserialize — replaces placeholders with `[[?]]` and `![[?]]`.
pub fn deserialize_content_sync(template: &str, links: &[Uuid]) -> String {
    let mut result = template.to_string();
    for (idx, _) in links.iter().enumerate().rev() {
        // Replace embed placeholders first (before regular, since {N} is a substring of !{N})
        result = result.replace(&format!("!{{{}}}", idx), "![[?]]");
        result = result.replace(&format!("{{{}}}", idx), "[[?]]");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_basic_sync() {
        let result = serialize_content_sync("See [[foo::bar]] for details");
        assert_eq!(result.template, "See [[foo::bar]] for details");
        assert!(result.links.is_empty());
        assert_eq!(result.unresolved, vec!["[[foo::bar]]"]);
    }

    #[test]
    fn test_serialize_multiple_links_sync() {
        let result = serialize_content_sync("See [[foo]] and [[bar::baz]]");
        assert_eq!(result.unresolved.len(), 2);
        assert!(result.unresolved.contains(&"[[foo]]".to_string()));
        assert!(result.unresolved.contains(&"[[bar::baz]]".to_string()));
    }

    #[test]
    fn test_deserialize_basic_sync() {
        let links = vec![Uuid::now_v7()];
        assert_eq!(
            deserialize_content_sync("See {0} for details", &links),
            "See [[?]] for details"
        );
    }

    #[test]
    fn test_deserialize_multiple_sync() {
        let links = vec![Uuid::now_v7(), Uuid::now_v7()];
        assert_eq!(
            deserialize_content_sync("See {0} and {1}", &links),
            "See [[?]] and [[?]]"
        );
    }

    #[test]
    fn test_no_links() {
        let result = serialize_content_sync("Plain text with no links");
        assert_eq!(result.template, "Plain text with no links");
        assert!(result.links.is_empty());
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn test_serialize_sync_no_content() {
        let result = serialize_content_sync("");
        assert_eq!(result.template, "");
        assert!(result.links.is_empty());
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn test_deserialize_sync_no_placeholders() {
        assert_eq!(deserialize_content_sync("Plain text", &[]), "Plain text");
    }

    #[test]
    fn test_deserialize_sync_extra_links_ignored() {
        let links = vec![Uuid::now_v7(), Uuid::now_v7()];
        assert_eq!(deserialize_content_sync("One: {0}", &links), "One: [[?]]");
    }
}

#[cfg(test)]
mod proptest_content {
    use super::*;
    use proptest::prelude::*;

    fn arb_plain_text() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?\n]{0,50}".prop_filter("no wiki links", |s| !s.contains("[["))
    }

    fn arb_link_segments() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec("[a-zA-Z0-9_]{1,10}", 1..=3)
    }

    proptest! {
        #[test]
        fn sync_serialize_preserves_non_link_text(content in "[a-zA-Z0-9 .,!?\n]{1,100}"
            .prop_filter("no wiki links", |s| !s.contains("[[")))
        {
            let result = serialize_content_sync(&content);
            prop_assert_eq!(&result.template, &content);
            prop_assert!(result.unresolved.is_empty());
        }

        #[test]
        fn sync_serialize_captures_all_links(
            n in 1usize..=5,
            segments in prop::collection::vec(arb_link_segments(), 1..=5),
            texts in prop::collection::vec(arb_plain_text(), 1..=6),
        ) {
            let n = n.min(segments.len());
            // Build content with exactly n wiki links interspersed with plain text
            let mut content = String::new();
            for i in 0..n {
                if i < texts.len() {
                    content.push_str(&texts[i]);
                }
                let link = format!("[[{}]]", segments[i].join("::"));
                content.push_str(&link);
            }
            if n < texts.len() {
                content.push_str(&texts[n]);
            }

            let result = serialize_content_sync(&content);
            // sync version doesn't resolve, so all links should be unresolved
            prop_assert_eq!(result.unresolved.len(), n);
        }

        #[test]
        fn deserialize_sync_placeholder_count(n in 1usize..=10) {
            let uuids: Vec<Uuid> = (0..n).map(|_| Uuid::now_v7()).collect();
            // Build template with {0}, {1}, ..., {N-1} interspersed with text
            let mut template = String::new();
            for i in 0..n {
                template.push_str(&format!("text{} {{{}}} ", i, i));
            }

            let result = deserialize_content_sync(&template, &uuids);
            let placeholder_count = result.matches("[[?]]").count();
            prop_assert_eq!(placeholder_count, n);
            // No {N} placeholders should remain
            for i in 0..n {
                prop_assert!(!result.contains(&format!("{{{}}}", i)),
                    "Placeholder {{{}}} still present in result: {}", i, result);
            }
        }

        #[test]
        fn deserialize_sync_reverse_iteration_safety(n in 2usize..=5) {
            let uuids: Vec<Uuid> = (0..n).map(|_| Uuid::now_v7()).collect();
            // Build template with adjacent placeholders: {0}{1}{2}...
            let template: String = (0..n).map(|i| format!("{{{}}}", i)).collect();

            let result = deserialize_content_sync(&template, &uuids);
            let expected: String = (0..n).map(|_| "[[?]]").collect();
            prop_assert_eq!(result, expected);
        }

        #[test]
        fn literal_brace_passthrough(text in "[a-zA-Z ]{1,20}", inner in "[a-zA-Z ]{1,10}") {
            // Template with literal braces that are NOT placeholders
            let template = format!("{}{{{}}}{}", text, inner, text);
            let result = deserialize_content_sync(&template, &[]);
            prop_assert_eq!(result, template);
        }

    }
}
