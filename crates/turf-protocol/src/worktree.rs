use serde::{Deserialize, Serialize};

/// Git worktree metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub task_id: String,
    pub branch_name: String,
    pub worktree_path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub commit: Option<String>,
}

/// Result of an automated squash-merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SquashMergeResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub commit_hash: Option<String>,
    pub no_changes: bool,
    pub task_id: String,
    pub branch: String,
    pub target_branch: String,
}

/// Represents a co-author for Git trailers according to [MD-012].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoAuthor {
    pub name: String,
    pub email: String,
}

impl CoAuthor {
    pub fn new(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
        }
    }

    /// Formats as a Git trailer: `Co-authored-by: Name <email>`
    pub fn to_trailer(&self) -> String {
        format!(
            "Co-authored-by: {} <{}>",
            self.name.trim(),
            self.email.trim()
        )
    }

    /// Parses string such as:
    /// - "Co-authored-by: Alice <alice@example.com>"
    /// - "Alice <alice@example.com>"
    /// - "Alice" (defaults to alice@turfcode.local)
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        let without_prefix = if let Some(stripped) = trimmed.strip_prefix("Co-authored-by:") {
            stripped.trim()
        } else if let Some(stripped) = trimmed.strip_prefix("co-authored-by:") {
            stripped.trim()
        } else {
            trimmed
        };

        if let (Some(start), Some(end)) = (without_prefix.find('<'), without_prefix.rfind('>')) {
            if start < end {
                let name = without_prefix[..start].trim();
                let email = without_prefix[start + 1..end].trim();
                return Some(Self {
                    name: if name.is_empty() {
                        "Collaborator".into()
                    } else {
                        name.into()
                    },
                    email: email.into(),
                });
            }
        }

        // Fallback for single name or identifier
        let name = without_prefix;
        let email_name: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '.'
                }
            })
            .collect();
        Some(Self {
            name: name.into(),
            email: format!("{email_name}@turfcode.local"),
        })
    }
}

/// Formats a list of co-authors into git commit trailers according to [MD-012].
pub fn format_co_authors(co_authors: &[CoAuthor]) -> Vec<String> {
    co_authors.iter().map(|ca| ca.to_trailer()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_co_author_parse_and_format() {
        let ca1 = CoAuthor::parse("Ayush <ayush@turfcode.local>").unwrap();
        assert_eq!(ca1.name, "Ayush");
        assert_eq!(ca1.email, "ayush@turfcode.local");
        assert_eq!(
            ca1.to_trailer(),
            "Co-authored-by: Ayush <ayush@turfcode.local>"
        );

        let ca2 = CoAuthor::parse("Co-authored-by: Bob <bob@example.com>").unwrap();
        assert_eq!(ca2.name, "Bob");
        assert_eq!(ca2.email, "bob@example.com");

        let ca3 = CoAuthor::parse("Dev").unwrap();
        assert_eq!(ca3.name, "Dev");
        assert_eq!(ca3.email, "dev@turfcode.local");
        assert_eq!(ca3.to_trailer(), "Co-authored-by: Dev <dev@turfcode.local>");

        let trailers = format_co_authors(&[ca1, ca2]);
        assert_eq!(trailers.len(), 2);
    }
}
