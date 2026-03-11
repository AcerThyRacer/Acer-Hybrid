use crate::plugins::{load_plugins, PluginType};
use acer_core::{
    validate_identifier, validate_max_tokens, validate_temperature, AcerConfig, CostEntry, Message,
    ModelRequest, ModelResponse, RunRecord,
};
use acer_policy::{PolicyConfig as RuntimePolicyConfig, PolicyEngine};
use acer_provider::{
    AnthropicProvider, CustomProvider, GeminiProvider, ModelRouter, OllamaProvider, OpenAIProvider,
};
use acer_trace::TraceStore;
use acer_vault::{keys, SecretsVault};
use anyhow::{anyhow, Result};
use chrono::Utc;

pub struct ExecutionResult {
    pub response: ModelResponse,
    pub run: RunRecord,
    pub cached: bool,
}

pub async fn build_router(
    config: &AcerConfig,
    preferred_provider: Option<&str>,
    prompt_for_password: bool,
) -> Result<ModelRouter> {
    let http_config = config.providers.http.clone();
    let mut router = ModelRouter::new();
    if let Some(default_provider) =
        preferred_provider.or(config.providers.default_provider.as_deref())
    {
        router.set_default(default_provider);
    }

    if config.providers.ollama.enabled {
        router
            .register_provider(
                "ollama".to_string(),
                Box::new(OllamaProvider::with_http_config(
                    config.providers.ollama.base_url.clone(),
                    http_config.clone(),
                )),
            )
            .await;
    }

    let mut vault = load_vault(config, prompt_for_password)?;

    if config.providers.openai.enabled {
        if let Some(api_key) = get_secret(&mut vault, keys::OPENAI_API_KEY)? {
            router
                .register_provider(
                    "openai".to_string(),
                    Box::new(OpenAIProvider::with_http_config(
                        api_key,
                        http_config.clone(),
                    )),
                )
                .await;
        }
    }

    if config.providers.anthropic.enabled {
        if let Some(api_key) = get_secret(&mut vault, keys::ANTHROPIC_API_KEY)? {
            router
                .register_provider(
                    "anthropic".to_string(),
                    Box::new(AnthropicProvider::with_http_config(
                        api_key,
                        http_config.clone(),
                    )),
                )
                .await;
        }
    }

    if config.providers.gemini.enabled {
        if let Some(api_key) = get_secret(&mut vault, keys::GEMINI_API_KEY)? {
            router
                .register_provider(
                    "gemini".to_string(),
                    Box::new(GeminiProvider::with_http_config(
                        api_key,
                        http_config.clone(),
                    )),
                )
                .await;
        }
    }

    for plugin in load_plugins()? {
        if plugin.plugin_type != PluginType::Provider {
            continue;
        }
        if let Some(provider) = plugin.provider {
            let api_key = provider
                .api_key_env
                .as_ref()
                .and_then(|key| std::env::var(key).ok())
                .or_else(|| {
                    provider
                        .vault_key
                        .as_ref()
                        .and_then(|key| get_secret(&mut vault, key).ok().flatten())
                });

            router
                .register_provider(
                    plugin.name.clone(),
                    Box::new(CustomProvider::with_http_config(
                        plugin.name,
                        provider.base_url,
                        api_key,
                        http_config.clone(),
                    )),
                )
                .await;
        }
    }

    Ok(router)
}

pub fn policy_engine(config: &AcerConfig, project: Option<&str>) -> PolicyEngine {
    let mut policy_config = RuntimePolicyConfig::from(config.policy.clone());
    load_policy_packs_into(&mut policy_config).ok();
    let mut engine = PolicyEngine::with_config(policy_config);
    if let Some(profile) = config.policy.active_profile.as_deref() {
        engine.set_profile(profile);
    }
    if let Some(project) = project {
        engine.set_project(project);
    }
    engine
}

pub async fn trace_store(config: &AcerConfig) -> Result<TraceStore> {
    TraceStore::from_config(config).await.map_err(Into::into)
}

pub async fn execute_request(
    config: &AcerConfig,
    request: ModelRequest,
    preferred_provider: Option<&str>,
    use_cache: bool,
    project: Option<&str>,
) -> Result<ExecutionResult> {
    if let Some(provider) = preferred_provider {
        validate_identifier("provider", provider).map_err(|e| anyhow!(e.to_string()))?;
    }

    let policy = policy_engine(config, project);
    let (prepared_request, decision) = policy.prepare_request(&request)?;
    if !decision.allowed {
        return Err(anyhow!(
            "{}",
            decision
                .reason
                .unwrap_or_else(|| "Policy violation".to_string())
        ));
    }

    let store = trace_store(config).await?;
    let mut run = RunRecord::new(prepared_request.clone());
    run.policy_decision = Some(decision.clone());
    run.metadata = serde_json::json!({
        "project": project,
        "preferred_provider": preferred_provider,
        "cached": false
    });

    if use_cache {
        if let Some(cached) = find_cached_run(&store, &run.prompt_hash, &request.model).await? {
            return Ok(ExecutionResult {
                response: cached
                    .response
                    .clone()
                    .ok_or_else(|| anyhow!("Cached run had no response payload"))?,
                run: cached,
                cached: true,
            });
        }
    }

    let router = build_router(config, preferred_provider, true).await?;
    match router
        .route(prepared_request.clone(), run.policy_decision.as_ref())
        .await
    {
        Ok(response) => {
            run.provider = response.provider;
            run.model = response.model.clone();
            run.latency_ms = response.latency_ms;
            run.cost_usd = router.estimate_cost(&response).await;
            run.success = true;
            run.response = Some(response.clone());
            store.store_run(&run).await?;
            if let Some(cost_usd) = run.cost_usd {
                store
                    .store_cost(&CostEntry {
                        timestamp: Utc::now(),
                        provider: response.provider,
                        model: response.model.clone(),
                        tokens: response.usage.clone(),
                        cost_usd,
                        run_id: run.id.clone(),
                    })
                    .await?;
            }
            Ok(ExecutionResult {
                response,
                run,
                cached: false,
            })
        }
        Err(error) => {
            run.success = false;
            run.error = Some(error.to_string());
            store.store_run(&run).await?;
            Err(anyhow!(error.to_string()))
        }
    }
}

pub fn model_for_provider(
    config: &AcerConfig,
    requested_model: Option<String>,
    preferred_provider: Option<&str>,
) -> String {
    match (requested_model, preferred_provider) {
        (Some(model), _) => model,
        (None, Some("openai")) => config
            .providers
            .openai
            .default_model
            .clone()
            .unwrap_or_else(|| "gpt-3.5-turbo".to_string()),
        (None, Some("anthropic")) => config
            .providers
            .anthropic
            .default_model
            .clone()
            .unwrap_or_else(|| "claude-3-sonnet-20240229".to_string()),
        (None, Some("gemini")) => config
            .providers
            .gemini
            .default_model
            .clone()
            .unwrap_or_else(|| "gemini-1.5-flash".to_string()),
        _ => config
            .providers
            .ollama
            .default_model
            .clone()
            .unwrap_or_else(|| "llama2".to_string()),
    }
}

pub fn request_from_prompt(
    model: String,
    prompt: String,
    attach: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<usize>,
) -> Result<ModelRequest> {
    validate_request_options(temperature, max_tokens)?;
    validate_identifier("model", &model).map_err(|e| anyhow!(e.to_string()))?;
    let mut messages = vec![];
    if let Some(path) = attach {
        let content = std::fs::read_to_string(&path)?;
        messages.push(Message::system(format!(
            "Attached file content from {}:\n\n{}",
            path, content
        )));
    }
    messages.push(Message::user(prompt));
    Ok(ModelRequest {
        model,
        messages,
        temperature,
        max_tokens,
        stream: None,
    })
}

pub fn validate_request_options(temperature: Option<f32>, max_tokens: Option<usize>) -> Result<()> {
    validate_temperature(temperature).map_err(|e| anyhow!(e.to_string()))?;
    validate_max_tokens(max_tokens, None).map_err(|e| anyhow!(e.to_string()))
}

fn load_vault(config: &AcerConfig, prompt_for_password: bool) -> Result<Option<SecretsVault>> {
    let vault_path = config
        .vault
        .vault_path
        .clone()
        .unwrap_or_else(|| AcerConfig::data_dir().join("vault.json"));
    if !vault_path.exists() {
        return Ok(None);
    }

    let mut vault = SecretsVault::load(vault_path, None)?;
    match std::env::var("ACER_VAULT_PASSWORD") {
        Ok(password) => vault.unlock(&password)?,
        Err(_) if prompt_for_password && !vault.list_keys().is_empty() => {
            let password = rpassword::prompt_password("Vault password: ")?;
            vault.unlock(&password)?;
        }
        Err(_) => {}
    }
    Ok(Some(vault))
}

fn get_secret(vault: &mut Option<SecretsVault>, key: &str) -> Result<Option<String>> {
    match vault {
        Some(vault) if vault.is_unlocked() && vault.exists(key) => {
            vault.get(key).map_err(Into::into)
        }
        Some(vault) if vault.exists(key) => Err(anyhow!(
            "Vault contains '{}' but is locked. Set ACER_VAULT_PASSWORD or unlock interactively.",
            key
        )),
        _ => Ok(None),
    }
}

async fn find_cached_run(
    store: &TraceStore,
    prompt_hash: &str,
    model: &str,
) -> Result<Option<RunRecord>> {
    let runs = store.get_runs_by_hash(prompt_hash).await?;
    Ok(runs
        .into_iter()
        .find(|run| run.success && run.model == model && run.response.is_some()))
}

fn load_policy_packs_into(config: &mut RuntimePolicyConfig) -> Result<()> {
    let dir = AcerConfig::policy_packs_dir();
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| anyhow!("Invalid policy pack filename"))?
            .to_string();
        let content = std::fs::read_to_string(path)?;
        let pack: RuntimePolicyConfig = toml::from_str(&content)?;
        config.profiles.insert(name, pack.default);
        config.projects.extend(pack.projects);
    }

    Ok(())
}
