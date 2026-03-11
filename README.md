# Acer Hybrid

**A local-first AI operations platform for developers and teams.**

Route requests across local and cloud models, enforce security policies, redact sensitive data, run repeatable workflows, and inspect every interaction through structured traces.

## Features

- 🔄 **Multi-Provider Support** - Seamlessly switch between Ollama (local), OpenAI, Anthropic, Gemini, and custom providers
- 🔐 **Encrypted Secrets Vault** - Securely store API keys with AES-256-GCM encryption
- 🛡️ **Policy Engine** - Enforce per-project rules for cost, models, and content
- 🔍 **PII Redaction** - Automatic detection and redaction of sensitive data
- 📊 **Trace Store** - SQLite-based logging of every interaction
- 🌐 **Local API Gateway** - OpenAI-compatible endpoint for IDEs and tools
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
# Ask a question (uses Ollama by default)
acer ask "Explain Rust ownership"

# Use a specific model
acer ask --model gpt-4 "Write a haiku about code"

# List available models
acer models

# View usage statistics
acer stats
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

### Gateway & Daemon

| Command | Description |
|---------|-------------|
| `acer gateway` | Start the local API gateway |
| `acer daemon start` | Start the background daemon |
| `acer daemon stop` | Stop the daemon |
| `acer daemon status` | Check daemon status |

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
- [x] CLI skeleton
- [x] Provider abstraction (Ollama, OpenAI, Anthropic, Gemini)
- [x] Encrypted secrets vault
- [x] Policy engine
- [x] SQLite trace store
- [x] Local API gateway
- [x] Daemon

### Phase 2 (Planned)
- [ ] Workflow runner (YAML/TOML)
- [ ] Model routing rules
- [ ] TUI dashboard
- [ ] Prompt replay/diff
- [ ] Export to JSON/SQLite

### Phase 3 (Future)
- [ ] Tool sandbox
- [ ] Plugin SDK
- [ ] Web dashboard
- [ ] Team profiles
- [ ] Benchmark mode

## License

MIT License

## Contributing

Contributions are welcome! Please read our contributing guidelines before submitting PRs.