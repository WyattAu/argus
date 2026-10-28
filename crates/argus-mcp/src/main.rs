//! argus-mcp — MCP server exposing the argus platform to agents.
//!
//! Implements the MCP stdio transport subset (newline-delimited JSON-RPC
//! 2.0): `initialize`, `tools/list`, `tools/call`, `ping`. Every tool is a
//! thin GET against a running argus core — this process holds no state, so
//! agents see exactly what the dashboard sees, under the same ACLs.

#![forbid(unsafe_code)]

use std::io::{BufRead, Write};

use clap::Parser;
use serde_json::{json, Value};

/// CLI arguments.
#[derive(Debug, Parser)]
#[command(name = "argus-mcp", version, about = "argus MCP server (stdio)")]
struct Args {
    /// Base URL of the argus core.
    #[arg(long, default_value = "http://127.0.0.1:8420")]
    url: String,
    /// Role header value for the core (`viewer` / `editor` / `admin`).
    #[arg(long, default_value = "viewer")]
    role: String,
}

/// Tool catalog: name, description, JSON schema, core path.
struct Tool {
    name: &'static str,
    description: &'static str,
    schema: Value,
    path: &'static str,
}

fn tools() -> Vec<Tool> {
    let object = |props: Value, required: &[&str]| {
        let mut schema = json!({"type": "object", "properties": props});
        if let Some(map) = schema.as_object_mut() {
            map.insert(String::from("required"), json!(required));
        }
        schema
    };
    vec![
        Tool {
            name: "argus_observations",
            description: "Recent world-intel observations (quakes, fires, CVEs, news) with source attribution",
            schema: object(
                json!({"limit": {"type": "integer", "description": "1..=500, default 100"}}),
                &[],
            ),
            path: "/api/v1/observations",
        },
        Tool {
            name: "argus_quotes",
            description: "Latest quote per symbol (sim + live Binance when enabled)",
            schema: object(json!({}), &[]),
            path: "/api/v1/quotes",
        },
        Tool {
            name: "argus_candles",
            description: "OHLCV candle history for one symbol",
            schema: object(
                json!({
                    "symbol": {"type": "string"},
                    "limit": {"type": "integer", "description": "1..=600, default 300"}
                }),
                &["symbol"],
            ),
            path: "/api/v1/candles",
        },
        Tool {
            name: "argus_search",
            description: "Full-text search over observation titles",
            schema: object(
                json!({
                    "q": {"type": "string"},
                    "limit": {"type": "integer", "description": "1..=50, default 20"}
                }),
                &["q"],
            ),
            path: "/api/v1/search",
        },
        Tool {
            name: "argus_entities",
            description: "Ontology entities (event/place/source/symbol) with provenance",
            schema: object(
                json!({
                    "kind": {"type": "string", "description": "event|place|source|symbol|all"},
                    "limit": {"type": "integer"}
                }),
                &[],
            ),
            path: "/api/v1/entities",
        },
        Tool {
            name: "argus_neighbors",
            description: "N-hop neighborhood of one entity, entity-joined",
            schema: object(
                json!({
                    "id": {"type": "string"},
                    "hops": {"type": "integer", "description": "1..=3, default 2"}
                }),
                &["id"],
            ),
            path: "/api/v1/entities/NEIGHBOR_ID/neighbors",
        },
        Tool {
            name: "argus_portfolio",
            description: "Portfolio positions marked to market + unrealized P&L",
            schema: object(json!({}), &[]),
            path: "/api/v1/backoffice/portfolio",
        },
        Tool {
            name: "argus_trial_balance",
            description: "Double-entry trial balance + accounting-equation check",
            schema: object(json!({}), &[]),
            path: "/api/v1/backoffice/trial-balance",
        },
        Tool {
            name: "argus_provenance",
            description: "Provenance chain head + verification status",
            schema: object(json!({}), &[]),
            path: "/api/v1/provenance",
        },
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let runtime = tokio::runtime::Runtime::new()?;
    let client = fetch_kit::Client::builder()
        .user_agent("argus-mcp/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            // Malformed JSON-RPC: respond with a parse error when an id is
            // knowable, else skip (spec: id null is acceptable; stay quiet
            // to keep dumb clients happy).
            eprintln!("argus-mcp: malformed json line skipped");
            continue;
        };
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let id = message.get("id").cloned();
        let is_notification = id.is_none();
        // Notifications and pings share empty bodies by design.
        #[allow(clippy::match_same_arms)]
        match (method, is_notification) {
            ("initialize", false) => {
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "argus", "version": env!("CARGO_PKG_VERSION")}
                });
                respond(&mut stdout, id, &result);
            }
            ("notifications/initialized" | "initialized", true) => {}
            ("ping", false) => {
                respond(&mut stdout, id, &json!({}));
            }
            ("tools/list", false) => {
                let catalog = tools()
                    .into_iter()
                    .map(|tool| {
                        json!({
                            "name": tool.name,
                            "description": tool.description,
                            "inputSchema": tool.schema,
                        })
                    })
                    .collect::<Vec<_>>();
                respond(&mut stdout, id, &json!({"tools": catalog}));
            }
            ("tools/call", false) => {
                let result = runtime.block_on(call_tool(&args.url, &args.role, &client, message.get("params")));
                let payload = match result {
                    Ok(text) => json!({
                        "content": [{"type": "text", "text": text}],
                        "isError": false
                    }),
                    Err(error) => json!({
                        "content": [{"type": "text", "text": error}],
                        "isError": true
                    }),
                };
                respond(&mut stdout, id, &payload);
            }
            ("shutdown" | "exit", false) => break,
            (_, false) => {
                respond(
                    &mut stdout,
                    id,
                    &json!({"error": {"code": -32601, "message": "method not found"}}),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

fn respond(stdout: &mut std::io::Stdout, id: Option<Value>, result: &Value) {
    let message = json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    });
    let Ok(line) = serde_json::to_string(&message) else { return };
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

#[allow(clippy::needless_pass_by_value)] // JSON-RPC params are optional by protocol
async fn call_tool(url: &str, role: &str, client: &fetch_kit::Client, params: Option<&Value>) -> Result<String, String> {
    let params = params.cloned().unwrap_or_else(|| json!({}));
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing tool name".to_string())?
        .to_string();
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let tool = tools()
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| format!("unknown tool: {name}"))?;

    // Path template: substitute {SYMBOL}-style placeholders from arguments.
    let mut path = tool.path.to_string();
    let mut query = Vec::new();
    if let Some(map) = arguments.as_object() {
        for (key, value) in map {
            let rendered = match value {
                Value::String(text) => Some(text.clone()),
                Value::Number(number) => Some(number.to_string()),
                _ => None,
            };
            if let Some(rendered) = rendered {
                let placeholder = format!("{{{}}}", key.to_uppercase());
                if path.contains(&placeholder) {
                    path = path.replace(&placeholder, &rendered);
                } else {
                    query.push(format!("{key}={rendered}"));
                }
            }
        }
    }
    let full = if query.is_empty() {
        format!("{url}{path}")
    } else {
        format!("{url}{path}?{}", query.join("&"))
    };

    let value = client
        .get(full)
        .header("x-argus-role", role)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .json::<Value>()
        .await
        .map_err(|error| error.to_string())?;
    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
}
