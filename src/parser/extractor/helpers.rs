//
//  helpers.rs
//  Anchor
//
//  Created by hak (tharun)
//

use tree_sitter::Node;

/// Get the full text of a node.
pub fn node_text(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

/// Extract full code snippet from a node (no truncation).
/// Slicing happens at display time using graph knowledge.
pub fn bounded_snippet(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}
