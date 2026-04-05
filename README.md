# automation-code-departmemt

A multi-agent orchestrator that emulates a software engineering department
(PM, BA, Dev, Frontend, Test). Each role is a Claude-powered agent; they
collaborate through a lane-queue gateway modeled after OpenClaw's session
architecture, and the Test agent drives **integration + UI tests** by
calling the Playwright MCP server.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│  Gateway (tokio runtime, single process)                 │
│  ┌────────────────────────────────────────────────────┐  │
│  │ LaneQueue<Task>  (mpsc + priority lanes)           │  │
│  └──┬─────────┬─────────┬──────────┬──────────┬───────┘  │
│     ▼         ▼         ▼          ▼          ▼          │
│   ┌───┐    ┌────┐    ┌────┐    ┌────┐    ┌──────┐        │
│   │PM │───▶│ BA │───▶│Dev │───▶│ FE │───▶│ Test │        │
│   └───┘    └────┘    └────┘    └────┘    └──┬───┘        │
│     ▲                                        │           │
│     └────────── TestReport ──────────────────┘           │
└──────────────────────────────────────────────────────────┘
                          │
                          ▼
          ┌────────────────────────────────┐
          │  MCP Client → Playwright MCP   │
          │  (navigate, click, type, …)    │
          └────────────────────────────────┘
```

### Crates

| Crate | Purpose |
|-------|---------|
| `agent-core` | `Agent` trait, `TaskMessage`, `Role`, `Dispatcher` |
| `llm-claude` | Minimal Anthropic Messages API client |
| `gateway`    | `LaneQueue`, `Workspace`, `Session`, worker loop |
| `mcp-client` | Stdio JSON-RPC MCP client + Playwright wrapper |
| `agents`     | Concrete `PmAgent`, `BaAgent`, `DevAgent`, `FrontendAgent`, `TestAgent` |
| `cli`        | `agentdept` binary |

### Pipeline

1. **PM** receives the user requirement and forwards it to BA.
2. **BA** turns it into user stories (JSON), fans out to Dev **and** Frontend.
3. **Dev** designs an HTTP API contract from the stories.
4. **Frontend** designs pages/components with semantic selectors.
5. **Test** buffers both specs, then:
   - *Strategy phase* (Opus): produces a prioritized test plan with
     `ui` and `api` scenarios.
   - *Execution phase*: `api` steps run via `reqwest`; `ui` steps drive
     Playwright MCP. Results summarized by Sonnet.
6. **PM** aggregates the `TestReport` into a `FinalReport` (printed to stdout).

## Quickstart

```bash
# 1. Install prerequisites
#    - Rust (stable, 1.80+)
#    - Node.js + npx (for Playwright MCP server)
npx -y @playwright/mcp@latest --help   # prime the package

# 2. Configure
export ANTHROPIC_API_KEY=sk-ant-...

# 3. Build
cargo build --release

# 4. Run
./target/release/agentdept run \
  --requirement "Build a login page: email+password, success redirects to /dashboard, wrong password shows inline error." \
  --base-url https://example.com \
  --config config/workspace.toml
```

Skip browser tests (e.g. in CI without a display) with `--no-playwright`.

## Configuration

`config/workspace.toml` (see file for defaults):

```toml
[workspace]
id = "dev-department-default"

[models]
pm = "opus"
ba = "opus"
dev = "sonnet"
frontend = "sonnet"
test_strategy = "opus"
test_exec = "sonnet"

[test]
base_url = "http://localhost:3000"
enable_playwright = true
```

Allowed model aliases: `opus`, `sonnet`, `haiku`.

## Extending

- Add a new agent: implement `agent_core::Agent` and add a `Role` variant.
- Add a new test step action: extend `TestAgent::execute_step` in
  `crates/agents/src/test.rs` (current actions: `navigate`, `type`,
  `click`, `assert_text`, `http_get`, `http_post`).
- Swap MCP server: replace the `launch()` command in
  `crates/mcp-client/src/playwright.rs`.

## Tests

```bash
cargo test            # unit tests (lane queue priority, message serde, ...)
cargo build --release # full compile check
```

## License

MIT
