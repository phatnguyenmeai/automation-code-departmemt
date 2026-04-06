# OpenClaw Architecture Gap Analysis

## Context

The `automation-code-departmemt` repository is a Rust-based multi-agent orchestrator
that emulates a software engineering department (PM, BA, Dev, Frontend, Test). The
README states it is "modeled after OpenClaw's session architecture." This document
identifies architectural gaps between the current implementation and the full OpenClaw
platform, and provides prioritized recommendations for closing those gaps.

---

## What is OpenClaw?

OpenClaw is an open-source personal AI assistant and autonomous agent that runs on
your own infrastructure. Key characteristics:

- **Always-on Gateway** (control plane) on port 18789
- **Agent Runtime** executing the AI loop with context assembly, LLM invocation,
  tool execution, and state persistence
- **50+ messaging channels** (WhatsApp, Telegram, Slack, Discord, Signal, etc.)
- **Plugin-first architecture** with 4 plugin types (Channel, Memory, Tool, Provider)
- **Skills system** with ClawHub registry for publishing/discovering skills
- **Persistent memory** (SQLite default, pluggable backends)
- **Multi-provider LLM support** (Claude, OpenAI, DeepSeek, self-hosted)
- **Web UI** (Control dashboard + WebChat)
- **Scheduled jobs** and always-on operation

---

## Feature-by-Feature Comparison

| Feature Area | Current App | OpenClaw | Gap Severity |
|---|---|---|---|
| **Gateway / Control Plane** | Single-process tokio runtime, CLI-driven, one-shot execution | Always-on HTTP server, event-driven, serves UI + API | **Major** |
| **Message Routing** | LaneQueue with priority lanes (High/Normal/Low) per role | Channel-based routing to Agent Runtime | **Strength** |
| **Session Management** | In-memory `Arc<Mutex<Vec<TaskMessage>>>`, lost on exit | Persistent sessions with SQLite, survives restarts | **Critical** |
| **Persistence / Storage** | None — all ephemeral | SQLite default + pluggable memory backends (vector stores, knowledge graphs) | **Critical** |
| **LLM Provider Support** | Anthropic Claude only (`llm-claude` crate) | Multi-provider: Claude, OpenAI, DeepSeek, self-hosted models | **Major** |
| **Input Channels** | CLI only (`agentdept run`) | 50+ channels: WhatsApp, Telegram, Slack, Discord, Signal, iMessage, Teams, Matrix, etc. | **Major** |
| **Plugin System** | None — hardcoded agents and tools | 4 plugin types: Channel, Memory, Tool, Provider — dynamically loaded | **Major** |
| **Skills / Capabilities** | Fixed pipeline: PM → BA → Dev/FE → Test | Skills as directories with SKILL.md, ClawHub registry, moderation hooks | **Major** |
| **Web UI** | None | Control UI dashboard + WebChat interface | **Minor** |
| **Tool Execution** | Playwright MCP + reqwest HTTP calls | Bash, browser, file ops, canvas, scheduled jobs + extensible tool plugins | **Major** |
| **Authentication / Security** | API key env var only | User management, channel authentication, access controls | **Major** |
| **Agent Architecture** | Domain-specific agents (PM, BA, Dev, FE, Test) with specialized roles | General-purpose agent runtime with pluggable skills | **Strength** |
| **Priority Scheduling** | Biased `tokio::select!` with O(1) fast path, 3 priority levels | No priority lanes | **Strength** |
| **Multi-Agent Collaboration** | 5 agents with structured pipeline and fan-out/fan-in | Single agent runtime (no multi-agent orchestration) | **Strength** |
| **Error Handling** | Blocker messages routed to PM for resolution | Standard error handling | **Comparable** |
| **Configuration** | Single `workspace.toml` | Gateway config + per-plugin config | **Minor** |
| **Scheduled / Recurring Tasks** | Not supported | Built-in job scheduler | **Minor** |
| **Always-On Operation** | One-shot CLI execution | Continuous background service | **Major** |
| **Logging / Observability** | `tracing` with structured logging | Similar structured logging | **Comparable** |

---

## Current App Strengths (vs OpenClaw)

These are areas where the current implementation is **more advanced** than OpenClaw:

1. **Priority Lane Queue** (`crates/gateway/src/lane_queue.rs`):
   Biased priority scheduling with O(1) fast path is more sophisticated than
   OpenClaw's simple channel routing. The three-tier priority system
   (High/Normal/Low) with preemption ensures critical tasks are handled first.

2. **Multi-Agent Pipeline** (`crates/agents/src/`):
   Structured collaboration between 5 domain-specific agents with fan-out/fan-in
   patterns. OpenClaw uses a single agent runtime — it cannot natively orchestrate
   multiple specialized agents working in concert.

3. **Domain Specialization**:
   Each agent has deep role-specific prompts and behavior. The BA generates Gherkin
   stories, Dev creates API contracts, Frontend designs semantic selectors, Test
   drives both API and UI validation. This is far richer than OpenClaw's
   general-purpose approach.

4. **Two-Phase Test Strategy**:
   Opus for strategy generation, Sonnet for execution — intelligent model selection
   per phase optimizes cost and quality.

5. **Blocker Escalation**:
   Structured error routing back to PM for resolution via `AgentOutput::Blocked` is
   a robust pattern for handling failures in multi-agent pipelines.

---

## Gap Analysis by Severity

### Critical Gaps

#### 1. ~~No Persistence~~ **RESOLVED**

> **Implemented**: Added `crates/storage/` with `Storage` trait + SQLite backend.
> Sessions and messages are persisted via `--db <path>` flag. The `Session` type
> now supports fire-and-forget persistence on every `record()` call.

#### 2. ~~No Session Recovery~~ **RESOLVED**

> **Implemented**: Added `Workspace::resume()`, `Session::resume()`, and the
> `agentdept resume --session-id <uuid>` CLI command. Also added
> `agentdept sessions` to list past sessions with status, timestamps, and
> original requirements.

### Major Gaps

#### 3. Single LLM Provider

Locked to Anthropic Claude — no fallback, no cost optimization, no offline mode.

- **Current**: `crates/llm-claude/` (entire crate is Claude-specific)
- **OpenClaw**: Multi-provider with Claude, OpenAI, DeepSeek, self-hosted
- **Recommendation**: Extract an `LlmProvider` trait in `agent-core`; refactor
  `llm-claude` to implement it; add `llm-openai` crate

#### 4. ~~No Plugin System~~ **RESOLVED**

> **Implemented**: New `crates/plugin/` with `ToolPlugin` trait, `ChannelPlugin` trait,
> `ToolRegistry`, and `SkillRegistry`. Built-in tools: shell, file_read, file_write,
> http_request. Skills loaded from SKILL.md manifests with TOML frontmatter.

#### 5. ~~CLI-Only Interface~~ **RESOLVED**

> **Implemented**: New `crates/server/` with axum HTTP/WebSocket server. REST API
> endpoints (POST /api/run, GET /api/sessions, etc.), WebSocket real-time events at
> /ws, and webhook endpoints at /channels/{name}/webhook for channel integrations.

#### 6. ~~No Authentication / Access Control~~ **RESOLVED**

> **Implemented**: API key authentication with 4-tier role-based access control
> (admin, operator, viewer, channel). Keys stored as SHA-256 hashes in SQLite.
> Auth middleware on all API/WS/channel routes. Key management API (create, list,
> revoke). Bootstrap via `--admin-key` flag or `AGENTDEPT_ADMIN_KEY` env var.
> Runs without auth when no admin key is configured (backward compatible).

#### 7. ~~No Always-On Mode~~ **RESOLVED**

> **Implemented**: `agentdept serve --port 18789` keeps the gateway running
> continuously, accepting new tasks via REST API and WebSocket. Matches OpenClaw's
> default port convention.

#### 8. ~~Limited Tool Ecosystem~~ **RESOLVED**

> **Implemented**: `ToolRegistry` in `crates/plugin/` with 4 built-in tools
> (shell, file_read, file_write, http_request). New tools can be added by
> implementing the `ToolPlugin` trait. Tools are listed via GET /api/tools.

#### 9. ~~No Skills/Capability Registry~~ **RESOLVED**

> **Implemented**: `SkillRegistry` loads SKILL.md manifests from a configurable
> directory. Each skill declares tools, tags, and a prompt template. Skills are
> listed via GET /api/skills. Two sample skills included (api-tester, code-reviewer).

### Minor Gaps

#### 10. ~~No Web UI~~ **RESOLVED**

> **Implemented**: Embedded dashboard at `/` with real-time WebSocket event stream,
> session browser, tool registry viewer, skill registry viewer, and requirement
> submission form. Dark theme, no external dependencies.

#### 11. No Scheduled Jobs

Cannot trigger pipelines on a schedule or recurring basis.

- **Recommendation**: Add cron-like scheduler to the gateway serve mode

#### 12. Single Workspace Config

Limited configuration flexibility — no per-session overrides.

- **Recommendation**: Support per-session config overrides and environment-based config

---

## Prioritized Recommendations

| Priority | Gap | Effort | Impact | Description |
|---|---|---|---|---|
| **P0** | ~~Persistence~~ | ~~Medium~~ | ~~High~~ | **DONE** — `crates/storage/` with SQLite backend |
| **P1** | LLM Provider Trait | Medium | High | Extract provider trait, add OpenAI support |
| **P2** | ~~HTTP Gateway Server~~ | ~~Medium~~ | ~~High~~ | **DONE** — `crates/server/` with axum |
| **P3** | ~~Plugin Registry~~ | ~~High~~ | ~~High~~ | **DONE** — `crates/plugin/` with ToolPlugin + ChannelPlugin |
| **P4** | ~~Authentication~~ | ~~Low–Medium~~ | ~~Medium~~ | **DONE** — API key auth + 4-tier RBAC |
| **P5** | ~~Channel Adapters~~ | ~~Medium~~ | ~~Medium~~ | **DONE** — webhook endpoints at /channels/{name}/webhook |
| **P6** | ~~Skills Manifest~~ | ~~Medium~~ | ~~Medium~~ | **DONE** — SKILL.md loading + registry |
| **P7** | ~~Web UI~~ | ~~Medium~~ | ~~Low~~ | **DONE** — embedded dashboard at / |

---

## Key Files to Modify

| File | Changes Needed |
|---|---|
| `crates/gateway/src/session.rs` | Add persistence trait, SQLite impl |
| `crates/gateway/src/workspace.rs` | Session load/resume, HTTP server integration |
| `crates/agent-core/src/agent.rs` | Add `LlmProvider` trait, `ToolRegistry` trait |
| `crates/llm-claude/src/lib.rs` | Implement `LlmProvider` trait |
| `crates/cli/src/main.rs` | Add `serve` subcommand |
| `Cargo.toml` | New crates: `storage`, `llm-openai`, `server` |
| `config/workspace.toml` | Extended config for persistence, server, auth |

---

## Summary

The current application has **strong foundations** that exceed OpenClaw in several
areas — particularly multi-agent orchestration, priority scheduling, and domain
specialization. However, it lacks the **platform-level infrastructure** that makes
OpenClaw production-ready: persistence, multi-provider support, external channels,
plugin extensibility, and always-on operation.

The recommended path forward is to **preserve the existing strengths** (lane queue,
multi-agent pipeline, domain agents) while incrementally adding platform capabilities
starting with persistence (P0) and working through the priority list. This bridges
the gap to OpenClaw-level maturity while maintaining the unique multi-agent
architecture that OpenClaw itself does not offer.
