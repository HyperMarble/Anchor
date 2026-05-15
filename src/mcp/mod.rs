//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

pub mod format;
pub mod tools;
pub mod types;

use rmcp::{
    handler::server::router::tool::ToolRouter, model::*, tool_handler, ServerHandler, ServiceExt,
};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::graph::CodeGraph;
use crate::lock::LockManager;

#[derive(Clone)]
pub struct AnchorMcp {
    pub(crate) root: PathBuf,
    pub(crate) tool_router: ToolRouter<AnchorMcp>,
    pub(crate) graph: Arc<RwLock<CodeGraph>>,
    pub(crate) lock_manager: Arc<LockManager>,
}

impl std::fmt::Debug for AnchorMcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnchorMcp")
            .field("root", &self.root)
            .finish()
    }
}

#[tool_handler]
impl ServerHandler for AnchorMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "anchor".into(),
                version: crate::updater::VERSION.into(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Anchor: repo-local execution harness for coding AI agents. \
                 \n\nUse search to find candidate symbols and files. \
                 \nUse context to request focused working context. \
                 \nUse write for validated create/edit/delete operations as the MVP evolves. \
                 \nUse verify to record checks once the verifier lands. \
                 \nLegacy graph-backed tools may still appear during the .anchor index migration."
                    .into(),
            ),
        }
    }
}

/// Run the MCP server on stdio.
pub async fn run(roots: Vec<PathBuf>) -> anyhow::Result<()> {
    let service = AnchorMcp::new(roots);
    let server = service.serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
