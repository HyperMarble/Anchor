//! MCP (Model Context Protocol) server for Anchor.
//!
//! Exposes Anchor's infrastructure as native MCP tools.
//! Agents connect via stdio and get: context, search, map, impact, write.

pub mod format;
pub mod tools;
pub mod types;

use rmcp::{
    handler::server::router::tool::ToolRouter,
    model::*,
    tool_handler, ServerHandler, ServiceExt,
};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AnchorMcp {
    pub(crate) root: PathBuf,
    pub(crate) tool_router: ToolRouter<AnchorMcp>,
}

#[tool_handler]
impl ServerHandler for AnchorMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "anchor".into(),
                version: crate::updater::VERSION.into(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Anchor: Infrastructure for AI agents. Replaces Read, Grep, cat, find for code tasks. \
                 \n\n'context' replaces Read — returns graph-sliced code (only lines that matter) + callers + callees + exact line numbers. Handles multiple symbols in one call. \
                 \n'search' replaces Grep/find — returns NAME KIND FILE:LINE. \
                 \n'map' — codebase overview: modules, entry points, top connected symbols. \
                 \n'impact' — what breaks if you change a symbol: affected callers, suggested fixes, tests. \
                 \n'write' — line-range replacement with automatic impact analysis. Line numbers from 'context' go directly into 'write'.".into()
            ),
        }
    }
}

/// Run the MCP server on stdio.
pub async fn run(root: PathBuf) -> anyhow::Result<()> {
    let service = AnchorMcp::new(root);
    let server = service.serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
