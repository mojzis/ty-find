use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ty-find")]
#[command(about = "Find Python function definitions using ty's LSP server")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,
    
    #[arg(short, long)]
    pub verbose: bool,
    
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
}

#[derive(Subcommand)]
pub enum Commands {
    Definition {
        file: PathBuf,
        
        #[arg(short, long)]
        line: u32,
        
        #[arg(short, long)]
        column: u32,
    },
    
    Find {
        file: PathBuf,
        
        symbol: String,
    },
    
    Interactive {
        file: Option<PathBuf>,
    },
}

#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
    Csv,
    Paths,
}