//! Blueprint types and structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{AnchorError, Result};

/// Metadata for a Blueprint, stored in YAML frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintMeta {
    /// Unique identifier for this blueprint
    pub id: String,

    /// Type of blueprint (e.g., "project", "repo", "notes")
    #[serde(rename = "type")]
    pub blueprint_type: String,

    /// Human-readable name
    pub name: String,

    /// When this blueprint was created
    pub created: DateTime<Utc>,

    /// When this blueprint was last updated
    pub updated: DateTime<Utc>,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional tags for categorization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl BlueprintMeta {
    /// Create new metadata with default values.
    pub fn new(id: &str) -> Self {
        let now = Utc::now();
        Self {
            id: id.to_string(),
            blueprint_type: "generic".to_string(),
            name: id.to_string(),
            created: now,
            updated: now,
            description: None,
            tags: Vec::new(),
        }
    }

    /// Set the blueprint type.
    pub fn with_type(mut self, blueprint_type: &str) -> Self {
        self.blueprint_type = blueprint_type.to_string();
        self
    }

    /// Set the name.
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Set the description.
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Add tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// A Blueprint is a single unit of memory.
///
/// Each blueprint is stored as a markdown file with YAML frontmatter:
///
/// ```markdown
/// ---
/// id: my_project
/// type: project
/// name: My Project
/// created: 2026-01-14T10:30:00Z
/// updated: 2026-01-14T14:00:00Z
/// ---
///
/// # My Project
///
/// Content goes here...
/// ```
#[derive(Debug, Clone)]
pub struct Blueprint {
    /// Metadata (stored in frontmatter)
    meta: BlueprintMeta,

    /// Content (markdown body)
    content: String,
}

impl Blueprint {
    /// Create a new Blueprint with the given ID and content.
    pub fn new(id: &str, content: &str) -> Self {
        Self {
            meta: BlueprintMeta::new(id),
            content: content.to_string(),
        }
    }

    /// Create a Blueprint with custom metadata.
    pub fn with_meta(meta: BlueprintMeta, content: &str) -> Self {
        Self {
            meta,
            content: content.to_string(),
        }
    }

    /// Get the blueprint ID.
    pub fn id(&self) -> &str {
        &self.meta.id
    }

    /// Get the metadata.
    pub fn meta(&self) -> &BlueprintMeta {
        &self.meta
    }

    /// Get mutable metadata.
    pub fn meta_mut(&mut self) -> &mut BlueprintMeta {
        &mut self.meta
    }

    /// Get the content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Update the content and refresh the updated timestamp.
    pub fn update_content(&mut self, content: &str) {
        self.content = content.to_string();
        self.meta.updated = Utc::now();
    }

    /// Serialize the blueprint to markdown with YAML frontmatter.
    pub fn to_markdown(&self) -> String {
        let frontmatter = serde_yaml::to_string(&self.meta)
            .unwrap_or_else(|_| "# Error serializing metadata".to_string());

        format!("---\n{}---\n\n{}", frontmatter, self.content)
    }

    /// Parse a blueprint from markdown with YAML frontmatter.
    pub fn from_markdown(markdown: &str) -> Result<Self> {
        // Find frontmatter boundaries
        let content = markdown.trim();
        
        if !content.starts_with("---") {
            return Err(AnchorError::ParseError(
                "Blueprint must start with YAML frontmatter (---)".to_string(),
            ));
        }

        // Find the closing ---
        let after_first = &content[3..];
        let end_pos = after_first.find("\n---")
            .ok_or_else(|| AnchorError::ParseError(
                "Could not find closing frontmatter delimiter (---)".to_string(),
            ))?;

        let yaml_content = &after_first[..end_pos].trim();
        let body_start = 3 + end_pos + 4; // Skip past closing ---\n
        let body = if body_start < content.len() {
            content[body_start..].trim()
        } else {
            ""
        };

        // Parse the YAML frontmatter
        let meta: BlueprintMeta = serde_yaml::from_str(yaml_content)
            .map_err(|e| AnchorError::ParseError(format!("Invalid YAML frontmatter: {}", e)))?;

        Ok(Self {
            meta,
            content: body.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blueprint_roundtrip() {
        let original = Blueprint::new("test_id", "# Hello\n\nThis is content.");
        let markdown = original.to_markdown();
        let parsed = Blueprint::from_markdown(&markdown).unwrap();

        assert_eq!(parsed.id(), "test_id");
        assert_eq!(parsed.content(), "# Hello\n\nThis is content.");
    }

    #[test]
    fn test_blueprint_with_meta() {
        let meta = BlueprintMeta::new("my_project")
            .with_type("project")
            .with_name("My Project")
            .with_description("A test project")
            .with_tags(vec!["rust".to_string(), "test".to_string()]);

        let bp = Blueprint::with_meta(meta, "# Content");
        
        assert_eq!(bp.meta().blueprint_type, "project");
        assert_eq!(bp.meta().name, "My Project");
        assert_eq!(bp.meta().description, Some("A test project".to_string()));
        assert_eq!(bp.meta().tags, vec!["rust", "test"]);
    }

    #[test]
    fn test_parse_error_no_frontmatter() {
        let result = Blueprint::from_markdown("# No frontmatter");
        assert!(result.is_err());
    }
}
