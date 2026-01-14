//! Anchor CLI - Command line interface for testing and debugging.

use anchor::{Anchor, Blueprint, BlueprintMeta};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "anchor")]
#[command(about = "Anchor CLI - Deterministic structural memory for AI", long_about = None)]
struct Cli {
    /// Path to the .anchor directory (default: ./.anchor)
    #[arg(short, long, default_value = ".anchor")]
    path: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Anchor store
    Init,

    /// Create a new blueprint
    Create {
        /// Blueprint ID (unique identifier)
        id: String,

        /// Blueprint content (or read from stdin if not provided)
        #[arg(short, long)]
        content: Option<String>,

        /// Blueprint type (default: generic)
        #[arg(short = 't', long, default_value = "generic")]
        blueprint_type: String,

        /// Human-readable name
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Read a blueprint
    Get {
        /// Blueprint ID
        id: String,

        /// Output only the content (no frontmatter)
        #[arg(long)]
        content_only: bool,
    },

    /// Update a blueprint's content
    Update {
        /// Blueprint ID
        id: String,

        /// New content
        content: String,
    },

    /// Delete a blueprint
    Delete {
        /// Blueprint ID
        id: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// List all blueprints
    List,

    /// Show storage info
    Info,
}

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> anchor::Result<()> {
    match cli.command {
        Commands::Init => {
            let anchor = Anchor::init(&cli.path)?;
            println!("✓ Initialized Anchor store at {:?}", anchor.root());
        }

        Commands::Create {
            id,
            content,
            blueprint_type,
            name,
        } => {
            let anchor = Anchor::open(&cli.path)?;

            let content = content.unwrap_or_else(|| {
                format!("# {}\n\nAdd your content here.", name.as_ref().unwrap_or(&id))
            });

            let meta = BlueprintMeta::new(&id)
                .with_type(&blueprint_type)
                .with_name(name.as_ref().unwrap_or(&id));

            let bp = Blueprint::with_meta(meta, &content);
            
            // Write directly through storage (we need to add this method)
            anchor.create_blueprint(&id, &content)?;

            println!("✓ Created blueprint: {}", id);
        }

        Commands::Get { id, content_only } => {
            let anchor = Anchor::open(&cli.path)?;
            let bp = anchor.get_blueprint(&id)?;

            if content_only {
                println!("{}", bp.content());
            } else {
                println!("{}", bp.to_markdown());
            }
        }

        Commands::Update { id, content } => {
            let anchor = Anchor::open(&cli.path)?;
            anchor.update_blueprint(&id, &content)?;
            println!("✓ Updated blueprint: {}", id);
        }

        Commands::Delete { id, force } => {
            if !force {
                println!("Are you sure you want to delete '{}'? Use --force to confirm.", id);
                return Ok(());
            }

            let anchor = Anchor::open(&cli.path)?;
            anchor.delete_blueprint(&id)?;
            println!("✓ Deleted blueprint: {}", id);
        }

        Commands::List => {
            let anchor = Anchor::open(&cli.path)?;
            let blueprints = anchor.list_blueprints()?;

            if blueprints.is_empty() {
                println!("No blueprints found.");
            } else {
                println!("Blueprints ({}):", blueprints.len());
                for id in blueprints {
                    println!("  - {}", id);
                }
            }
        }

        Commands::Info => {
            let anchor = Anchor::open(&cli.path)?;
            let blueprints = anchor.list_blueprints()?;

            println!("Anchor Store Info");
            println!("─────────────────");
            println!("Path: {:?}", anchor.root());
            println!("Blueprints: {}", blueprints.len());
        }
    }

    Ok(())
}
