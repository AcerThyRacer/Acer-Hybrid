# Acer Hybrid

**A local-first AI operations platform for developers and teams.**

Route requests across local and cloud models, enforce security policies, redact sensitive data, run repeatable workflows, and inspect every interaction through structured traces.

## Features

- 🔄 **Multi-Provider Support** - Seamlessly switch between Ollama (local), OpenAI, Anthropic, Gemini, and custom providers
- 🔐 **Encrypted Secrets Vault** - Securely store API keys with AES-256-GCM encryption
- 🛡️ **Policy Engine** - Enforce per-project rules for cost, models, and content
- 🔍 **PII Redaction** - Automatic detection and redaction of sensitive data
- ♻️ **Prompt Cache & Replay** - Reuse prior runs and replay traces for debugging
- 📊 **Trace Store** - SQLite-based logging of every interaction
- 🌐 **Local API Gateway** - OpenAI-compatible endpoint for IDEs and tools
- 🧪 **Workflow Engine** - Execute repeatable YAML or TOML automation flows
- 🔒 **Tool Sandbox** - Gate shell commands through per-project policy rules
- 📺 **TUI Dashboard** - Inspect requests, latency, cost, and failures in real time
- 🌍 **Web Dashboard API** - Inspect stats and runs from `/dashboard`, `/api/stats`, and `/api/runs`
- 🔌 **Plugin Manifests** - Load custom OpenAI-compatible providers and workflow plugins
- 🏁 **Benchmark Mode** - Compare models on latency, token usage, cost, and output previews
- 👥 **Policy Packs** - Share and activate team policy profiles from `policy-packs/`
- 🖥️ **CLI & Daemon** - Use interactively or run as a background service

## Quick Start

### Installation

```bash
# Clone and build
git clone https://github.com/acer-hybrid/acer-hybrid.git
cd acer-hybrid
cargo build --release

# The binaries will be in target/release/
# - acer (CLI)
# - hybrid (dashboard-first CLI alias)
# - acerd (daemon)
```

### Initialize

```bash
# Initialize configuration
acer init

# Check system health
acer doctor
```

### Basic Usage

```bash
# Open the live dashboard from any directory
acer

# Same dashboard via the hybrid alias
hybrid

# Ask a question (uses Ollama by default)
acer ask "Explain Rust ownership"

# Use a specific model
acer ask --model gpt-4 "Write a haiku about code"

# List available models
acer models

# View usage statistics
acer stats

# Open the live dashboard explicitly
acer dashboard

# Run a sandboxed command
acer sandbox --isolated -- rg TODO .

# Compare models directly
acer benchmark "Summarize this module" --model llama2 --model openai:gpt-3.5-turbo

# List plugin manifests
acer plugins list
```

### Configure Cloud Providers

```bash
# Store your OpenAI API key securely
acer secrets set openai_api_key

# Now you can use OpenAI models
acer ask --model gpt-4 "Hello, GPT-4!"
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Acer Hybrid                          │
├─────────────────────────────────────────────────────────────┤
│  CLI (acer)          │  Daemon (acerd)    │  Gateway       │
│  - ask               │  - Background      │  - /v1/chat    │
│  - models            │    services        │  - /v1/models  │
│  - secrets           │  - Policy engine   │  - OpenAI-     │
│  - policy            │  - Trace store     │    compatible  │
│  - logs              │  - Vault           │                │
├─────────────────────────────────────────────────────────────┤
│                    Provider Layer                           │
│  ┌─────────┐ ┌─────────┐ ┌───────────┐ ┌─────────┐        │
│  │ Ollama  │ │ OpenAI  │ │ Anthropic │ │ Gemini  │        │
│  │ (local) │ │ (cloud) │ │  (cloud)  │ │ (cloud) │        │
│  └─────────┘ └─────────┘ └───────────┘ └─────────┘        │
└─────────────────────────────────────────────────────────────┘
```

## Commands

### Core Commands

| Command | Description |
|---------|-------------|
| `acer ask <prompt>` | Send a prompt to an AI model |
| `acer models` | List available models |
| `acer init` | Initialize configuration |
| `acer doctor` | Check system health |

### Secrets Management

| Command | Description |
|---------|-------------|
| `acer secrets set <key>` | Store a secret |
| `acer secrets get <key>` | Retrieve a secret |
| `acer secrets list` | List all secret keys |
| `acer secrets delete <key>` | Delete a secret |

### Policy Management

| Command | Description |
|---------|-------------|
| `acer policy show` | Show current policy |
| `acer policy check <prompt>` | Check a prompt against policy |
| `acer policy set <rule> <value>` | Set a policy rule |

### Observability

| Command | Description |
|---------|-------------|
| `acer logs` | View recent runs |
| `acer trace <run-id>` | Inspect a specific run |
| `acer stats` | Show usage statistics |
| `acer dashboard` | Open the terminal dashboard |
| `acer sandbox -- <cmd>` | Run a command under policy control |
| `acer benchmark <prompt> --model ...` | Compare multiple models |
| `acer plugins list` | List installed plugins |

### Gateway & Daemon

| Command | Description |
|---------|-------------|
| `acer gateway` | Start the local API gateway |
| `acer daemon start` | Start the background daemon |
| `acer daemon stop` | Stop the daemon |
| `acer daemon status` | Check daemon status |

### Workflow Automation

| Command | Description |
|---------|-------------|
| `acer run <workflow>` | Execute a workflow from YAML or TOML |
| `acer trace <run-id> --replay` | Replay a stored request |
| `acer trace <run-id> --diff-with <other>` | Compare trace outputs |
| `acer trace <run-id> --export out.json` | Export a run or DB snapshot |
| `acer policy packs` | List installed policy packs |
| `acer policy activate <name>` | Activate a shared profile |

## Configuration

Configuration is stored in `~/.config/acer-hybrid/config.toml`:

```toml
[providers.ollama]
base_url = "http://localhost:11434"
enabled = true
default_model = "llama2"

[providers.openai]
enabled = true
default_model = "gpt-3.5-turbo"

[providers.anthropic]
enabled = false
default_model = "claude-3-sonnet-20240229"

[providers.gemini]
enabled = false
default_model = "gemini-1.5-flash"

[gateway]
host = "127.0.0.1"
port = 8080
enabled = true

[policy.default]
max_cost_usd = 0.10
allow_remote = true
redact_pii = true
```

## Policy Engine

Define per-project policies:

```toml
[policy.default]
max_cost_usd = 0.10
allow_remote = true
redact_pii = true

[policy.projects.fintech-app]
allow_remote = false
allow_tools = ["grep", "rg", "git diff"]
block_patterns = ["customer_ssn", "api_secret"]
```

## API Gateway

Start the OpenAI-compatible gateway:

```bash
acer gateway --port 8080
```

Now any tool that supports OpenAI's API can use Acer Hybrid:

```bash
# Using curl
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama2",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'

# Configure your IDE or tool to use http://localhost:8080
```

Browser dashboard:

```text
http://localhost:8080/dashboard
```

## Security Features

### PII Redaction

Automatic detection and redaction of:
- API keys (OpenAI, AWS, etc.)
- Credit card numbers
- SSN
- Email addresses
- Phone numbers
- IP addresses
- Private keys

### Encrypted Vault

All secrets are encrypted with AES-256-GCM using a password-derived key (PBKDF2 with 100,000 iterations).

## Development

### Build

```bash
cargo build
```

### Test

```bash
cargo test
```

### Run

```bash
cargo run --bin acer -- ask "Hello"
cargo run --bin acerd
```

## Roadmap

### Phase 1 (Current)
- [x] CLI
- [x] Provider abstraction and routing
- [x] Encrypted secrets vault
- [x] Policy engine and redaction
- [x] SQLite trace store with cost/latency logging
- [x] Local API gateway
- [x] Daemon

### Phase 2 (Implemented)
- [x] Workflow runner (YAML/TOML)
- [x] Model routing rules
- [x] TUI dashboard
- [x] Prompt replay/diff
- [x] Export to JSON/SQLite

### Phase 3 (Future)
- [x] Tool sandbox
- [x] Plugin SDK
- [x] Web dashboard
- [x] Team profiles / policy packs
- [x] Benchmark mode

### Phase 4 (Platform Hardening)
- [x] Shared policy-pack activation
- [x] Browser and TUI observability surfaces
- [x] External provider and workflow plugin manifests
- [x] Benchmark-driven model comparison

## License

MIT License

## Contributing

Contributions are welcome! Please read our contributing guidelines before submitting PRs.
