//! MongoDB tool plugin for database operations.
//!
//! Provides query execution, schema inspection, index management,
//! and data operations through `mongosh` commands.

use crate::tool::{ToolContext, ToolError, ToolPlugin, ToolResult};
use async_trait::async_trait;

/// Execute MongoDB operations via mongosh.
pub struct MongoTool;

#[async_trait]
impl ToolPlugin for MongoTool {
    fn name(&self) -> &str {
        "mongo_tool"
    }

    fn description(&self) -> &str {
        "Execute MongoDB operations: query, inspect schema, manage indexes, import/export data"
    }

    fn args_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["operation"],
            "properties": {
                "operation": {
                    "type": "string",
                    "description": "Operation to perform",
                    "enum": [
                        "query",
                        "list_collections",
                        "collection_stats",
                        "list_indexes",
                        "create_index",
                        "explain",
                        "aggregate",
                        "insert",
                        "count"
                    ]
                },
                "connection_string": {
                    "type": "string",
                    "description": "MongoDB connection string (default: mongodb://localhost:27017)",
                    "default": "mongodb://localhost:27017"
                },
                "database": {
                    "type": "string",
                    "description": "Database name"
                },
                "collection": {
                    "type": "string",
                    "description": "Collection name"
                },
                "filter": {
                    "type": "object",
                    "description": "Query filter document (JSON)"
                },
                "pipeline": {
                    "type": "array",
                    "description": "Aggregation pipeline stages (for aggregate operation)"
                },
                "index_keys": {
                    "type": "object",
                    "description": "Index key specification (for create_index)"
                },
                "index_options": {
                    "type": "object",
                    "description": "Index options (for create_index)"
                },
                "document": {
                    "type": "object",
                    "description": "Document to insert (for insert operation)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Limit results (default 20)",
                    "default": 20
                }
            }
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let operation = args["operation"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'operation'".into()))?;
        let conn = args["connection_string"]
            .as_str()
            .unwrap_or("mongodb://localhost:27017");
        let db = args["database"]
            .as_str()
            .unwrap_or("test");
        let collection = args["collection"].as_str().unwrap_or("");
        let limit = args["limit"].as_u64().unwrap_or(20);

        let js_code = match operation {
            "query" => {
                let filter = args.get("filter")
                    .map(|f| serde_json::to_string(f).unwrap_or_default())
                    .unwrap_or_else(|| "{}".to_string());
                format!(
                    "JSON.stringify(db.getSiblingDB('{db}').{collection}.find({filter}).limit({limit}).toArray())"
                )
            }
            "list_collections" => {
                format!(
                    "JSON.stringify(db.getSiblingDB('{db}').getCollectionNames())"
                )
            }
            "collection_stats" => {
                format!(
                    "JSON.stringify(db.getSiblingDB('{db}').{collection}.stats())"
                )
            }
            "list_indexes" => {
                format!(
                    "JSON.stringify(db.getSiblingDB('{db}').{collection}.getIndexes())"
                )
            }
            "create_index" => {
                let keys = args.get("index_keys")
                    .map(|k| serde_json::to_string(k).unwrap_or_default())
                    .unwrap_or_else(|| "{}".to_string());
                let opts = args.get("index_options")
                    .map(|o| serde_json::to_string(o).unwrap_or_default())
                    .unwrap_or_else(|| "{}".to_string());
                format!(
                    "JSON.stringify(db.getSiblingDB('{db}').{collection}.createIndex({keys}, {opts}))"
                )
            }
            "explain" => {
                let filter = args.get("filter")
                    .map(|f| serde_json::to_string(f).unwrap_or_default())
                    .unwrap_or_else(|| "{}".to_string());
                format!(
                    "JSON.stringify(db.getSiblingDB('{db}').{collection}.find({filter}).explain('executionStats'))"
                )
            }
            "aggregate" => {
                let pipeline = args.get("pipeline")
                    .map(|p| serde_json::to_string(p).unwrap_or_default())
                    .unwrap_or_else(|| "[]".to_string());
                format!(
                    "JSON.stringify(db.getSiblingDB('{db}').{collection}.aggregate({pipeline}).toArray())"
                )
            }
            "insert" => {
                let doc = args.get("document")
                    .map(|d| serde_json::to_string(d).unwrap_or_default())
                    .unwrap_or_else(|| "{}".to_string());
                format!(
                    "JSON.stringify(db.getSiblingDB('{db}').{collection}.insertOne({doc}))"
                )
            }
            "count" => {
                let filter = args.get("filter")
                    .map(|f| serde_json::to_string(f).unwrap_or_default())
                    .unwrap_or_else(|| "{}".to_string());
                format!(
                    "JSON.stringify({{count: db.getSiblingDB('{db}').{collection}.countDocuments({filter})}})"
                )
            }
            _ => {
                return Err(ToolError::InvalidArgs(format!(
                    "unknown operation: {operation}"
                )));
            }
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::process::Command::new("mongosh")
                .args(["--quiet", "--eval", &js_code, conn])
                .current_dir(&ctx.working_dir)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let code = output.status.code().unwrap_or(-1);

                // Try to parse stdout as JSON for structured output.
                let parsed = serde_json::from_str::<serde_json::Value>(stdout.trim())
                    .unwrap_or(serde_json::Value::String(stdout.clone()));

                Ok(ToolResult::ok(serde_json::json!({
                    "operation": operation,
                    "database": db,
                    "collection": collection,
                    "result": parsed,
                    "stderr": stderr,
                    "exit_code": code,
                })))
            }
            Ok(Err(e)) => Ok(ToolResult::err(format!("mongosh exec error: {e}"))),
            Err(_) => Err(ToolError::Timeout),
        }
    }
}
