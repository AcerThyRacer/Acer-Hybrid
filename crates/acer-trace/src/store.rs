//! SQLite-based trace store

use crate::{DbRunRecord, ModelStats, ProviderStats, UsageStats, CREATE_SCHEMA};
use acer_core::{AcerConfig, AcerError, CostEntry, Result, RunId, RunRecord};
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// Trace store for recording runs and costs
pub struct TraceStore {
    pool: SqlitePool,
}

impl TraceStore {
    /// Create a new trace store
    pub async fn new(path: &std::path::Path) -> Result<Self> {
        Self::with_max_connections(path, 5).await
    }

    pub async fn from_config(config: &AcerConfig) -> Result<Self> {
        let path = config
            .tracing
            .database_path
            .clone()
            .unwrap_or_else(|| AcerConfig::data_dir().join("traces.db"));
        Self::with_max_connections(&path, config.tracing.max_connections).await
    }

    pub async fn with_max_connections(
        path: &std::path::Path,
        max_connections: u32,
    ) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections.max(1))
            .connect(&url)
            .await
            .map_err(|e| AcerError::TraceStore(format!("Failed to connect to database: {}", e)))?;

        // Run migrations
        sqlx::query(CREATE_SCHEMA)
            .execute(&pool)
            .await
            .map_err(|e| AcerError::TraceStore(format!("Failed to create schema: {}", e)))?;

        Ok(Self { pool })
    }

    /// Create an in-memory trace store (for testing)
    pub async fn in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .map_err(|e| {
                AcerError::TraceStore(format!("Failed to create in-memory database: {}", e))
            })?;

        sqlx::query(CREATE_SCHEMA)
            .execute(&pool)
            .await
            .map_err(|e| AcerError::TraceStore(format!("Failed to create schema: {}", e)))?;

        Ok(Self { pool })
    }

    /// Store a run record
    pub async fn store_run(&self, run: &RunRecord) -> Result<()> {
        let db_run = DbRunRecord::from(run.clone());

        sqlx::query(
            r#"
            INSERT INTO runs (
                id, timestamp, prompt_hash, model, provider,
                request_json, response_json, redactions_json,
                policy_decision_json, cost_usd, latency_ms,
                success, error, metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        )
        .bind(&db_run.id)
        .bind(&db_run.timestamp)
        .bind(&db_run.prompt_hash)
        .bind(&db_run.model)
        .bind(&db_run.provider)
        .bind(&db_run.request_json)
        .bind(&db_run.response_json)
        .bind(&db_run.redactions_json)
        .bind(&db_run.policy_decision_json)
        .bind(db_run.cost_usd)
        .bind(db_run.latency_ms)
        .bind(db_run.success as i32)
        .bind(&db_run.error)
        .bind(&db_run.metadata_json)
        .execute(&self.pool)
        .await
        .map_err(|e| AcerError::TraceStore(format!("Failed to store run: {}", e)))?;

        Ok(())
    }

    /// Store a cost entry
    pub async fn store_cost(&self, entry: &CostEntry) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO costs (
                timestamp, provider, model,
                prompt_tokens, completion_tokens, total_tokens,
                cost_usd, run_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        )
        .bind(entry.timestamp.to_rfc3339())
        .bind(entry.provider.to_string())
        .bind(&entry.model)
        .bind(entry.tokens.prompt_tokens as i64)
        .bind(entry.tokens.completion_tokens as i64)
        .bind(entry.tokens.total_tokens as i64)
        .bind(entry.cost_usd)
        .bind(entry.run_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| AcerError::TraceStore(format!("Failed to store cost: {}", e)))?;

        Ok(())
    }

    /// Get a run by ID
    pub async fn get_run(&self, id: &RunId) -> Result<Option<RunRecord>> {
        let row = sqlx::query_as::<_, DbRunRecord>("SELECT * FROM runs WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AcerError::TraceStore(format!("Failed to get run: {}", e)))?;

        match row {
            Some(db_run) => {
                let run: RunRecord = db_run.try_into().map_err(|e| AcerError::TraceStore(e))?;
                Ok(Some(run))
            }
            None => Ok(None),
        }
    }

    /// List recent runs
    pub async fn list_runs(&self, limit: i64) -> Result<Vec<RunRecord>> {
        let rows =
            sqlx::query_as::<_, DbRunRecord>("SELECT * FROM runs ORDER BY timestamp DESC LIMIT ?")
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AcerError::TraceStore(format!("Failed to list runs: {}", e)))?;

        rows.into_iter()
            .map(|db_run| db_run.try_into().map_err(AcerError::TraceStore))
            .collect()
    }

    /// Get runs by prompt hash (for replay)
    pub async fn get_runs_by_hash(&self, hash: &str) -> Result<Vec<RunRecord>> {
        let rows = sqlx::query_as::<_, DbRunRecord>(
            "SELECT * FROM runs WHERE prompt_hash = ? ORDER BY timestamp DESC",
        )
        .bind(hash)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AcerError::TraceStore(format!("Failed to get runs by hash: {}", e)))?;

        rows.into_iter()
            .map(|db_run| db_run.try_into().map_err(AcerError::TraceStore))
            .collect()
    }

    /// Get usage statistics
    pub async fn get_stats(&self, since: DateTime<Utc>) -> Result<UsageStats> {
        let since_str = since.to_rfc3339();

        // Get overall stats
        let overall: (i64, i64, i64, f64, f64) = sqlx::query_as(
            r#"
            SELECT 
                COUNT(*) as total,
                COALESCE(SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END), 0) as successful,
                COALESCE(SUM(CASE WHEN success = 0 THEN 1 ELSE 0 END), 0) as failed,
                COALESCE(SUM(cost_usd), 0.0) as total_cost,
                COALESCE(AVG(latency_ms), 0.0) as avg_latency
            FROM runs WHERE timestamp >= ?
        "#,
        )
        .bind(&since_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcerError::TraceStore(format!("Failed to get stats: {}", e)))?;

        // Get token stats
        let tokens: (i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT 
                COALESCE(SUM(total_tokens), 0) as total,
                COALESCE(SUM(prompt_tokens), 0) as prompt,
                COALESCE(SUM(completion_tokens), 0) as completion
            FROM costs WHERE timestamp >= ?
        "#,
        )
        .bind(&since_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcerError::TraceStore(format!("Failed to get token stats: {}", e)))?;

        // Get stats by provider
        let provider_rows: Vec<(String, i64, i64, f64)> = sqlx::query_as(r#"
            SELECT provider, COUNT(*) as requests, 
                   COALESCE((SELECT SUM(total_tokens) FROM costs c WHERE c.provider = runs.provider AND c.timestamp >= ?), 0) as tokens,
                   COALESCE(SUM(cost_usd), 0.0) as cost
            FROM runs WHERE timestamp >= ?
            GROUP BY provider
        "#)
        .bind(&since_str)
        .bind(&since_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AcerError::TraceStore(format!("Failed to get provider stats: {}", e)))?;

        let mut by_provider = std::collections::HashMap::new();
        for (provider, requests, tokens, cost) in provider_rows {
            by_provider.insert(
                provider.clone(),
                ProviderStats {
                    requests: requests as u64,
                    tokens: tokens as u64,
                    cost_usd: cost,
                },
            );
        }

        // Get stats by model
        let model_rows: Vec<(String, i64, i64, f64, f64)> = sqlx::query_as(r#"
            SELECT model, COUNT(*) as requests,
                   COALESCE((SELECT SUM(total_tokens) FROM costs c WHERE c.model = runs.model AND c.timestamp >= ?), 0) as tokens,
                   COALESCE(SUM(cost_usd), 0.0) as cost,
                   COALESCE(AVG(latency_ms), 0.0) as avg_latency
            FROM runs WHERE timestamp >= ?
            GROUP BY model
        "#)
        .bind(&since_str)
        .bind(&since_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AcerError::TraceStore(format!("Failed to get model stats: {}", e)))?;

        let mut by_model = std::collections::HashMap::new();
        for (model, requests, tokens, cost, avg_latency) in model_rows {
            by_model.insert(
                model.clone(),
                ModelStats {
                    requests: requests as u64,
                    tokens: tokens as u64,
                    cost_usd: cost,
                    avg_latency_ms: avg_latency,
                },
            );
        }

        Ok(UsageStats {
            total_requests: overall.0 as u64,
            successful_requests: overall.1 as u64,
            failed_requests: overall.2 as u64,
            total_tokens: tokens.0 as u64,
            prompt_tokens: tokens.1 as u64,
            completion_tokens: tokens.2 as u64,
            total_cost_usd: overall.3,
            avg_latency_ms: overall.4,
            by_provider,
            by_model,
        })
    }

    /// Delete old records
    pub async fn cleanup(&self, older_than_days: u32) -> Result<u64> {
        let cutoff = Utc::now() - chrono::Duration::days(older_than_days as i64);
        let cutoff_str = cutoff.to_rfc3339();

        sqlx::query("DELETE FROM costs WHERE timestamp < ?")
            .bind(&cutoff_str)
            .execute(&self.pool)
            .await
            .map_err(|e| AcerError::TraceStore(format!("Failed to cleanup costs: {}", e)))?;

        let result = sqlx::query("DELETE FROM runs WHERE timestamp < ?")
            .bind(&cutoff_str)
            .execute(&self.pool)
            .await
            .map_err(|e| AcerError::TraceStore(format!("Failed to cleanup runs: {}", e)))?;

        Ok(result.rows_affected())
    }

    /// Export runs to JSON
    pub async fn export_json(&self, since: chrono::DateTime<Utc>) -> Result<String> {
        let rows = sqlx::query_as::<_, DbRunRecord>(
            "SELECT * FROM runs WHERE timestamp >= ? ORDER BY timestamp DESC",
        )
        .bind(since.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AcerError::TraceStore(format!("Failed to export runs: {}", e)))?;

        let runs: Result<Vec<RunRecord>> = rows
            .into_iter()
            .map(|db_run| db_run.try_into().map_err(AcerError::TraceStore))
            .collect();

        serde_json::to_string_pretty(&runs?)
            .map_err(|e| AcerError::TraceStore(format!("Failed to export: {}", e)))
    }

    /// Close the connection pool
    pub async fn close(self) {
        self.pool.close().await;
    }
}
