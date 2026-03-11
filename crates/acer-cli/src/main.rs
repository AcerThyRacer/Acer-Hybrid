//! Acer Hybrid CLI - Local-first AI operations platform

mod commands;
mod plugins;
mod runtime;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = env!("CARGO_BIN_NAME"))]
#[command(about = "Acer Hybrid - Local-first AI operations platform", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
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
        /// Reuse a cached response for identical prompts and models when available
        #[arg(long)]
        cache: bool,
        /// Apply project-specific policy rules
        #[arg(long)]
        project: Option<String>,
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
        /// Replay the stored request through the active runtime
        #[arg(long)]
        replay: bool,
        /// Diff this run against another run ID
        #[arg(long)]
        diff_with: Option<String>,
        /// Export this run or the trace database to a file path (.json or .db)
        #[arg(long)]
        export: Option<String>,
    },

    /// Run a workflow
    Run {
        /// Workflow file to run
        workflow: String,
        /// Input variables (key=value)
        #[arg(short, long)]
        var: Vec<String>,
        /// Apply project-specific policy rules during execution
        #[arg(long)]
        project: Option<String>,
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

    /// Open the terminal dashboard
    Dashboard {
        /// Refresh interval in milliseconds
        #[arg(long, default_value = "1500")]
        refresh_ms: u64,
    },

    /// Benchmark multiple models against the same prompt
    Benchmark {
        /// Prompt text to run
        prompt: String,
        /// Models to compare
        #[arg(long, required = true)]
        model: Vec<String>,
        /// Number of repetitions per model
        #[arg(long, default_value = "1")]
        repeat: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage Acer Hybrid plugins
    Plugins {
        #[command(subcommand)]
        command: PluginCommands,
    },

    /// Run a command inside Acer Hybrid's policy sandbox
    Sandbox {
        /// Use an isolated temporary working directory
        #[arg(long)]
        isolated: bool,
        /// Apply project-specific tool policy rules
        #[arg(long)]
        project: Option<String>,
        /// Command to execute
        #[arg(required = true, trailing_var_arg = true)]
        cmd: Vec<String>,
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
    /// List available policy packs
    Packs,
    /// Activate a policy profile or pack
    Activate {
        /// Profile or pack name
        name: String,
    },
}

#[derive(Subcommand)]
enum PluginCommands {
    /// List installed plugins
    List,
    /// Create starter plugin manifests
    Scaffold {
        /// Plugin name
        name: String,
        /// Plugin type (provider or workflow)
        #[arg(long, default_value = "provider")]
        plugin_type: String,
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
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        None => {
            commands::dashboard::execute(1500).await?;
        }
        Some(Commands::Ask {
            prompt,
            model,
            provider,
            temperature,
            max_tokens,
            attach,
            cache,
            project,
            verbose,
        }) => {
            commands::ask::execute(
                prompt,
                model,
                provider,
                temperature,
                max_tokens,
                attach,
                cache,
                project,
                verbose,
            )
            .await?;
        }
        Some(Commands::Models {
            provider,
            local,
            json,
        }) => {
            commands::models::execute(provider, local, json).await?;
        }
        Some(Commands::Secrets { command }) => {
            commands::secrets::execute(command).await?;
        }
        Some(Commands::Policy { command }) => {
            commands::policy::execute(command).await?;
        }
        Some(Commands::Logs {
            limit,
            model,
            provider,
            errors,
            json,
        }) => {
            commands::logs::execute(limit, model, provider, errors, json).await?;
        }
        Some(Commands::Trace {
            run_id,
            full,
            replay,
            diff_with,
            export,
        }) => {
            commands::trace::execute(run_id, full, replay, diff_with, export).await?;
        }
        Some(Commands::Run {
            workflow,
            var,
            project,
        }) => {
            commands::workflow::execute(workflow, var, project).await?;
        }
        Some(Commands::Gateway { port, host }) => {
            commands::gateway::execute(host, port).await?;
        }
        Some(Commands::Daemon { command }) => {
            commands::daemon::execute(command).await?;
        }
        Some(Commands::Doctor { fix }) => {
            commands::doctor::execute(fix).await?;
        }
        Some(Commands::Stats { period, json }) => {
            commands::stats::execute(period, json).await?;
        }
        Some(Commands::Benchmark {
            prompt,
            model,
            repeat,
            json,
        }) => {
            commands::benchmark::execute(prompt, model, repeat, json).await?;
        }
        Some(Commands::Plugins { command }) => {
            commands::plugins::execute(command).await?;
        }
        Some(Commands::Dashboard { refresh_ms }) => {
            commands::dashboard::execute(refresh_ms).await?;
        }
        Some(Commands::Sandbox {
            isolated,
            project,
            cmd,
        }) => {
            commands::sandbox::execute(cmd, isolated, project).await?;
        }
        Some(Commands::Init { force }) => {
            commands::init::execute(force).await?;
        }
        Some(Commands::Config { command }) => {
            commands::config::execute(command).await?;
        }
    }

    Ok(())
}
