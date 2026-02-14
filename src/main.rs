use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod config;
mod list;
mod sync;

use config::{
    Source, default_config, default_config_path, expand_tilde, load_config, normalize_path,
    save_config,
};

#[derive(Parser)]
#[command(name = "quiver", about = "Symlink manager for Claude Code skills")]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a default config file
    Init,
    /// Add a source path to the config
    Add {
        /// Path to the skills source directory
        path: String,
        /// Name for this source (used as default prefix)
        name: String,
        /// Optional prefix override for conflict resolution
        #[arg(long)]
        prefix: Option<String>,
    },
    /// Sync symlinks from all sources to the target directory
    Sync,
    /// List all skills in the target directory
    List,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = match cli.config {
        Some(p) => p,
        None => default_config_path()?,
    };

    match cli.command {
        Commands::Init => cmd_init(&config_path),
        Commands::Add { path, name, prefix } => {
            cmd_add(&path, &name, prefix.as_deref(), &config_path)
        }
        Commands::Sync => cmd_sync(&config_path),
        Commands::List => cmd_list(&config_path),
    }
}

fn cmd_init(config_path: &std::path::Path) -> Result<()> {
    if config_path.exists() {
        println!("Config already exists at {}", config_path.display());
        return Ok(());
    }
    save_config(&default_config(), config_path)?;
    println!("Created config at {}", config_path.display());
    Ok(())
}

fn cmd_add(
    path: &str,
    name: &str,
    prefix: Option<&str>,
    config_path: &std::path::Path,
) -> Result<()> {
    let mut config = load_config(config_path)?;

    let new_normalized = normalize_path(&expand_tilde(path));
    for existing in &config.sources {
        let existing_normalized = normalize_path(&expand_tilde(&existing.path));
        if existing_normalized == new_normalized {
            println!("Source path already exists in config: {path}");
            return Ok(());
        }
        if existing.name == name {
            anyhow::bail!("A source with name '{name}' already exists in config");
        }
    }

    config.sources.push(Source {
        path: path.to_string(),
        name: name.to_string(),
        prefix: prefix.map(String::from),
    });

    save_config(&config, config_path)?;
    println!("Added source '{name}' at {path}");
    Ok(())
}

fn cmd_sync(config_path: &std::path::Path) -> Result<()> {
    let config = load_config(config_path)?;
    if config.sources.is_empty() {
        println!("No sources configured. Use `quiver add <path> <name>` to add a source.");
        return Ok(());
    }
    sync::run_sync(&config)
}

fn cmd_list(config_path: &std::path::Path) -> Result<()> {
    let config = load_config(config_path)?;
    list::run_list(&config)
}
