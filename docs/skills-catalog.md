# Skills & Plugins Catalog

## Overview

The dev department agent system uses a skill-based architecture inspired by
open source coding agent frameworks (MetaGPT, CrewAI, OpenHands, SWE-agent).
Skills are self-contained capability modules that agents can load at runtime
without recompiling Rust code.

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Agent Pipeline                        │
│                                                         │
│  ┌────┐   ┌────┐   ┌─────┐   ┌──────────┐   ┌──────┐  │
│  │ PM │──▶│ BA │──▶│ Dev │──▶│ Frontend │──▶│ Test │  │
│  └────┘   └────┘   └─────┘   └──────────┘   └──────┘  │
│    │                  │           │             │        │
│    ▼                  ▼           ▼             ▼        │
│ ┌──────────────────────────────────────────────────┐    │
│ │              Skill Registry                       │    │
│ │  pm-sprint-planner | ba-requirements | rust-      │    │
│ │  backend | mongodb | vuejs-frontend | playwright- │    │
│ │  e2e | code-reviewer | api-tester                 │    │
│ └──────────────────────────────────────────────────┘    │
│    │                                                     │
│    ▼                                                     │
│ ┌──────────────────────────────────────────────────┐    │
│ │              Tool Registry (Plugins)              │    │
│ │  shell | file_read | file_write | http_request |  │    │
│ │  cargo_tool | mongo_tool | playwright_runner      │    │
│ └──────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

## Skills

### 1. Rust Backend (`rust-backend`)
**Tags**: `rust`, `backend`, `api`, `axum`, `cargo`
**Tools**: `shell`, `file_read`, `file_write`, `cargo_tool`

Rust backend development covering:
- Cargo workspace patterns and crate organization
- Axum/Actix-web service patterns (routing, middleware, extractors)
- Error handling with `thiserror`/`anyhow`
- Repository pattern for data access
- Quality checks (fmt, clippy, test, audit)

**Scripts**:
- `scaffold-service.sh` — Generate a new service crate with standard structure
- `check-quality.sh` — Run fmt, clippy, test, audit in sequence

**References**:
- `axum-patterns.md` — Middleware, extractors, pagination, graceful shutdown
- `error-handling.md` — Library vs application errors, conversion chains

---

### 2. MongoDB (`mongodb`)
**Tags**: `mongodb`, `database`, `nosql`, `schema`, `aggregation`
**Tools**: `shell`, `file_read`, `file_write`, `mongo_tool`

MongoDB integration covering:
- Schema design patterns (embedding, referencing, bucket, polymorphic)
- Index strategy (ESR rule, compound, text, TTL, partial)
- Aggregation pipeline optimization
- Rust `mongodb` driver patterns (connection, repository, aggregation)

**Scripts**:
- `setup-replica.sh` — Set up a local replica set for development
- `create-indexes.sh` — Apply indexes from a JSON definition file
- `seed-data.sh` — Import test data from JSON files

**References**:
- `schema-patterns.md` — Pattern selection guide, versioning, ESR rule

**Assets**:
- `indexes.json` — Standard index definitions for common collections

---

### 3. Vue.js Frontend (`vuejs-frontend`)
**Tags**: `vuejs`, `frontend`, `typescript`, `pinia`, `vite`
**Tools**: `shell`, `file_read`, `file_write`

Vue.js 3 frontend development covering:
- Composition API with `<script setup lang="ts">`
- Pinia store patterns (setup syntax)
- Component design (props, emits, slots, composables)
- API client patterns
- Router configuration with auth guards

**Scripts**:
- `scaffold-page.sh` — Generate a new page component with route
- `scaffold-composable.sh` — Generate a new composable function

**References**:
- `composition-api.md` — Quick reference for reactivity, lifecycle, communication

**Assets**:
- `component.vue.template` — Standard component template
- `store.ts.template` — Pinia store template

---

### 4. PM Sprint Planner (`pm-sprint-planner`)
**Tags**: `pm`, `sprint`, `planning`, `management`, `agile`
**Tools**: `file_read`, `file_write`, `shell`

Project management covering:
- Sprint planning and capacity management
- Task decomposition and dependency mapping
- Risk assessment and mitigation
- Task routing rules between agents
- Sprint reporting and velocity tracking

**Scripts**:
- `generate-sprint.sh` — Generate a sprint plan from requirements JSON

**References**:
- `agile-ceremonies.md` — Sprint planning, standup, review, retrospective

**Assets**:
- `sprint-template.json` — Sprint plan and report template

---

### 5. BA Requirements (`ba-requirements`)
**Tags**: `ba`, `requirements`, `user-stories`, `gherkin`, `domain-modeling`
**Tools**: `file_read`, `file_write`

Business analysis covering:
- Requirements elicitation framework
- User story mapping and INVEST criteria
- Acceptance criteria in Given/When/Then (Gherkin) format
- Domain modeling (entities, relationships, aggregates)
- Non-functional requirements templates

**Scripts**:
- `validate-stories.sh` — Validate user stories JSON against quality checks

**References**:
- `gherkin-guide.md` — Given/When/Then structure, scenario templates, INVEST checklist

**Assets**:
- `story-template.json` — User story with domain model template

---

### 6. Playwright E2E (`playwright-e2e`)
**Tags**: `testing`, `e2e`, `playwright`, `automation`, `qa`, `accessibility`
**Tools**: `shell`, `file_read`, `file_write`, `http_request`

End-to-end testing covering:
- Test strategy matrix (P0-P3 categorization)
- Page Object Model pattern
- Authentication fixtures
- Accessibility testing (axe-core / WCAG 2.1 AA)
- CI integration and artifact collection

**Scripts**:
- `run-tests.sh` — Run Playwright tests with configurable options
- `scaffold-test.sh` — Generate Page Object + test spec for a page

**References**:
- `selectors-guide.md` — Selector priority, common patterns, assertions, waiting strategies

**Assets**:
- `playwright.config.ts.template` — Multi-browser Playwright configuration
- `BasePage.ts.template` — Base Page Object class

---

### 7. Code Reviewer (`code-reviewer`)
**Tags**: `review`, `quality`, `security`
**Tools**: `file_read`, `shell`

**Scripts**:
- `review-diff.sh` — Extract git diff for review

**References**:
- `review-checklist.md` — Correctness, security, performance, style checklists

---

### 8. API Tester (`api-tester`)
**Tags**: `testing`, `api`, `integration`
**Tools**: `http_request`, `shell`

**Scripts**:
- `test-endpoint.sh` — Quick API endpoint tester

**References**:
- `http-status-codes.md` — HTTP status code quick reference

---

## Tool Plugins

| Tool | Description | Used By |
|------|-------------|---------|
| `shell` | Execute shell commands | All skills |
| `file_read` | Read file contents | All skills |
| `file_write` | Write/create files | All skills |
| `http_request` | HTTP requests (GET/POST/PUT/DELETE) | api-tester, playwright-e2e |
| `cargo_tool` | Cargo commands (check/clippy/test/fmt/build) | rust-backend |
| `mongo_tool` | MongoDB operations (query/indexes/aggregate) | mongodb |
| `playwright_runner` | Run Playwright test suites | playwright-e2e |

## Adding a New Skill

1. Create a directory under `skills/<skill-name>/`
2. Add a `SKILL.md` with TOML frontmatter (`+++`) defining name, description, tools, tags
3. Write the prompt template in the markdown body after the frontmatter
4. Optionally add `scripts/`, `references/`, and `assets/` subdirectories
5. The skill registry auto-discovers skills on startup

## Research References

This system draws from patterns in these open source projects:

- **MetaGPT** — Multi-agent role-based software company simulation (PM/Architect/Engineer/QA)
- **CrewAI** — Role-based agent crews with task delegation
- **OpenHands (OpenDevin)** — Autonomous coding agent with sandboxed execution
- **SWE-agent** — Software engineering agent for GitHub issue resolution
- **Aider** — AI pair programming with git integration
- **AutoCodeRover** — Automated program repair and improvement
- **ChatDev** — Virtual software company with multi-agent chat chains
- **Claude Code** — Skill/plugin system with SKILL.md manifests
- **MCP (Model Context Protocol)** — Standardized tool interface for LLM agents
