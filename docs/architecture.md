# Acer Hybrid - Architecture

## Overview

Acer Hybrid is a local-first AI operations platform built in Rust. It provides a unified interface for interacting with multiple AI model providers while enforcing security policies and maintaining comprehensive audit trails.

## Components

### 1. CLI (`acer-cli`)

The command-line interface for interactive use:

```
acer ask "prompt"     - Send a prompt to a model
acer models           - List available models
acer dashboard        - Live TUI observability
acer sandbox -- ...   - Policy-enforced tool execution
acer secrets          - Manage encrypted secrets
acer policy           - Manage policies
acer logs             - View trace logs
acer gateway          - Start API gateway
acer daemon           - Manage background daemon
acer doctor           - Health check
acer stats            - Usage statistics
```

### 2. Daemon (`acerd`)

Background service that:
- Maintains the local API gateway
- Handles provider connections
- Runs the policy engine
- Manages the trace store
- Provides the encrypted vault

### 3. Core Library (`acer-core`)

Shared types and utilities:
- `AcerConfig` - Configuration management
- `ModelRequest/Response` - Request/response types
- `RunRecord` - Trace record structure
- Error types

### 4. Provider Layer (`acer-provider`)

Abstraction over model providers:
- `Provider` trait - Common interface
- `OllamaProvider` - Local models via Ollama
- `OpenAIProvider` - OpenAI API
- `AnthropicProvider` - Claude API
- `GeminiProvider` - Google Gemini API
- `CustomProvider` - OpenAI-compatible endpoints
- `ModelRouter` - Intelligent routing

### 5. Policy Engine (`acer-policy`)

Security and compliance:
- `PolicyEngine` - Rule evaluation
- `PolicyRules` - Configuration structure
- `RedactionEngine` - PII detection/removal
- Project-aware tool and prompt controls

### 6. Secrets Vault (`acer-vault`)

Secure credential storage:
- AES-256-GCM encryption
- PBKDF2 key derivation (100,000 iterations)
- Password-protected vault file

### 7. Trace Store (`acer-trace`)

SQLite-based audit logging:
- Run records with full request/response
- Cost tracking
- Usage statistics
- Retention policies
- Prompt cache/replay via prompt hashes

### 8. API Gateway (`acer-gateway`)

OpenAI-compatible HTTP server:
- `POST /v1/chat/completions`
- `GET /v1/models`
- Policy enforcement
- Automatic redaction
- Structured trace logging
- Browser dashboard at `/dashboard`
- JSON stats and runs at `/api/stats` and `/api/runs`

### 9. Plugin Layer

Plugin manifests live in `~/.config/acer-hybrid/plugins`:
- Provider plugins register OpenAI-compatible backends
- Workflow plugins execute external commands inside workflow steps

### 10. Policy Packs

Shared governance packs live in `~/.config/acer-hybrid/policy-packs`:
- Packs load as named profiles
- The active profile is merged into runtime policy evaluation
- Teams can version and distribute TOML packs

## Data Flow

```
User Request
     │
     ▼
┌─────────────┐
│  CLI/Daemon │
└─────────────┘
     │
     ▼
┌─────────────┐     ┌─────────────┐
│   Policy    │────▶│  Redaction  │
│   Engine    │     │   Engine    │
└─────────────┘     └─────────────┘
     │
     ▼
┌─────────────┐
│   Router    │
└─────────────┘
     │
     ▼
┌─────────────┐
│  Provider   │
└─────────────┘
     │
     ▼
┌─────────────┐
│ Trace Store │
└─────────────┘
```

## Security Model

### Secrets Storage

1. User provides password
2. Password is stretched using PBKDF2-SHA256 (100,000 iterations)
3. Derived key encrypts secrets with AES-256-GCM
4. Encrypted data stored in `~/.local/share/acer-hybrid/vault.json`

### PII Redaction

The redaction engine detects:
- API keys (OpenAI, AWS, Anthropic patterns)
- Credit card numbers
- SSN patterns
- Email addresses
- Phone numbers
- IP addresses
- JWT tokens
- Private keys

### Policy Enforcement

Policies can:
- Block requests exceeding cost limits
- Restrict model access
- Block content matching patterns
- Require confirmation for dangerous actions
- Force local-only mode

## Configuration

### File Locations

| File | Location |
|------|----------|
| Config | `~/.config/acer-hybrid/config.toml` |
| Vault | `~/.local/share/acer-hybrid/vault.json` |
| Traces | `~/.local/share/acer-hybrid/traces.db` |
| PID | `~/.local/share/acer-hybrid/acerd.pid` |

### Configuration Structure

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

## Extending Acer Hybrid

### Adding a New Provider

1. Implement the `Provider` trait in `acer-provider`
2. Add configuration in `acer-core/src/config.rs`
3. Register in the router

### Adding Redaction Patterns

1. Add pattern to `acer-policy/src/redaction.rs`
2. Use regex pattern and replacement string

### Adding CLI Commands

1. Create new module in `acer-cli/src/commands/`
2. Add to `main.rs` enum
3. Implement `execute()` function
