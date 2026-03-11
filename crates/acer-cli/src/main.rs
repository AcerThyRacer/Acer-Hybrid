//! Acer Hybrid CLI - Local-first AI operations platform

mod commands;

use clap::{Parser, Subcommand};
use commands::*;

#[derive(Parser)]
#[command(name = "acer")]
#[command(about = "Acer Hybrid - Local-first AI operations platform", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a prompt to an AI model
    Ask {
        /// The prompt to send
        prompt: String,
        /// Model to use (e.g., llama2, gpt-4)
        #[arg(short, long)]
        model: Option<String>,
        /// Provider to use (ollama, openai, anthropic)
        #[arg(short, long)]
        provider: Option<String>,
        /// Temperature (0.0-2.0)
        #[arg(short, long)]
        temperature: Option<f32>,
        /// Maximum tokens in response
        #[arg(long)]
        max_tokens: Option<usize>,
        /// Attach files for context
        #[arg(short, long)]
        attach: Option<String>,
        /// Enable verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// List available models
    Models {
        /// Provider to list models from
        #[arg(short, long)]
        provider: Option<String>,
        /// Show only local models
        #[arg(long)]
        local: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage secrets and API keys
    Secrets {
        #[command(subcommand)]
        command: SecretsCommands,
    },

    /// Manage policies
    Policy {
        #[command(subcommand)]
        command: PolicyCommands,
    },

    /// View logs and traces
    Logs {
        /// Number of entries to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Filter by model
        #[arg(short, long)]
        model: Option<String>,
        /// Filter by provider
        #[arg(short, long)]
        provider: Option<String>,
        /// Show only errors
        #[arg(long)]
        errors: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Inspect a specific run/trace
    Trace {
        /// Run ID to inspect
        run_id: String,
        /// Show full request/response
        #[arg(long)]
        full: bool,
    },

    /// Run a workflow
    Run {
        /// Workflow file to run
        workflow: String,
        /// Input variables (key=value)
        #[arg(short, long)]
        var: Vec<String>,
    },

    /// Start the local API gateway
    Gateway {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },

    /// Manage the daemon
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },

    /// Check system health and configuration
    Doctor {
        /// Fix issues automatically
        #[arg(long)]
        fix: bool,
    },

    /// Show usage statistics
    Stats {
        /// Time period (1h, 24h, 7d, 30d)
        #[arg(short, long, default_value = "24h")]
        period: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Initialize configuration
    Init {
        /// Overwrite existing config
        #[arg(long)]
        force: bool,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum SecretsCommands {
    /// Store a secret
    Set {
        /// Secret key name
        key: String,
        /// Secret value (will prompt if not provided)
        value: Option<String>,
    },
    /// Retrieve a secret
    Get {
        /// Secret key name
        key: String,
    },
    /// Delete a secret
    Delete {
        /// Secret key name
        key: String,
    },
    /// List all secret keys
    List,
    /// Unlock the vault
    Unlock,
    /// Lock the vault
    Lock,
    /// Rotate encryption key
    Rotate,
}

#[derive(Subcommand)]
enum PolicyCommands {
    /// Show current policy
    Show {
        /// Project name
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Check a prompt against policy
    Check {
        /// Prompt to check
        prompt: String,
    },
    /// Set a policy rule
    Set {
        /// Rule name
        rule: String,
        /// Rule value
        value: String,
        /// Project name
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Test policy simulation
    Test {
        /// Prompt to test
        prompt: String,
    },
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Start the daemon
    Start,
    /// Stop the daemon
    Stop,
    /// Check daemon status
    Status,
    /// Restart the daemon
    Restart,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Set a configuration value
    Set {
        /// Configuration key (e.g., providers.ollama.base_url)
        key: String,
        /// Configuration value
        value: String,
    },
    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },
    /// Edit configuration in editor
    Edit,
    /// Reset to defaults
    Reset {
        /// Confirm reset
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive(tracing::Level::INFO.into()))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Ask { prompt, model, provider, temperature, max_tokens, attach, verbose } => {
            commands::ask::execute(prompt, model, provider, temperature, max_tokens, attach, verbose).await?;
        }
        Commands::Models { provider, local, json } => {
            commands::models::execute(provider, local, json).await?;
        }
        Commands::Secrets { command } => {
            commands::secrets::execute(command).await?;
        }
        Commands::Policy { command } => {
            commands::policy::execute(command).await?;
        }
        Commands::Logs { limit, model, provider, errors, json } => {
            commands::logs::execute(limit, model, provider, errors, json).await?;
        }
        Commands::Trace { run_id, full } => {
            commands::trace::execute(run_id, full).await?;
        }
        Commands::Run { workflow, var } => {
            commands::workflow::execute(workflow, var).await?;
        }
        Commands::Gateway { port, host } => {
            commands::gateway::execute(host, port).await?;
        }
        Commands::Daemon { command } => {
            commands::daemon::execute(command).await?;
        }
        Commands::Doctor { fix } => {
            commands::doctor::execute(fix).await?;
        }
        Commands::Stats { period, json } => {
            commands::stats::execute(period, json).await?;
        }
        Commands::Init { force } => {
            commands::init::execute(force).await?;
        }
        Commands::Config { command } => {
            commands::config::execute(command).await?;
        }
    }

    Ok(())
}