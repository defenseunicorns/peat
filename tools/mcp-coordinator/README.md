# HIVE MCP Coordinator

An MCP server for coordinating multiple Claude Code team sessions working on the HIVE Protocol demo.

## Architecture

```
                    ┌─────────────────────┐
                    │  Coordination MCP   │
                    │      Server         │
                    │  ─────────────────  │
                    │  • Team status      │
                    │  • Blockers queue   │
                    │  • Dependency graph │
                    │  • Message passing  │
                    └─────────────────────┘
                           ▲  ▲  ▲  ▲
                           │  │  │  │
         ┌─────────────────┼──┼──┼──┼─────────────────┐
         │                 │  │  │  │                 │
    ┌────┴────┐      ┌────┴──┴──┴──┴────┐      ┌────┴────┐
    │   PM    │      │  Core  ATAK  AI  │      │  Exper  │
    │ Session │      │    Sessions      │      │ Session │
    └─────────┘      └──────────────────┘      └─────────┘
```

## Installation

```bash
cd tools/mcp-coordinator
npm install
```

## Configuration

Add to your Claude Code MCP configuration (`~/.claude.json` or project `.claude/settings.local.json`):

```json
{
  "mcpServers": {
    "hive-coordinator": {
      "command": "node",
      "args": ["/path/to/hive/tools/mcp-coordinator/src/index.js"],
      "env": {
        "COORDINATOR_DB_PATH": "/path/to/shared/coordinator.db"
      }
    }
  }
}
```

**Important**: All team sessions must use the same `COORDINATOR_DB_PATH` to share state.

## Available Tools

### Status Management

| Tool | Description |
|------|-------------|
| `report_status` | Report your team's current status (idle, working, blocked, reviewing, done) |
| `get_all_status` | Get status of all teams |
| `get_team_status` | Get detailed status of a specific team |

### Blocker Management

| Tool | Description |
|------|-------------|
| `report_blocker` | Report that you're blocked by another team |
| `get_blockers` | Get active blockers |
| `resolve_blocker` | Mark a blocker as resolved |

### Messaging

| Tool | Description |
|------|-------------|
| `send_message` | Send a message to another team (or broadcast to all) |
| `get_messages` | Get messages for your team |

### Integration Coordination

| Tool | Description |
|------|-------------|
| `notify_ready_for_integration` | Notify that your work is ready |
| `get_integration_ready` | Get list of items ready for integration |

### Dependency Tracking

| Tool | Description |
|------|-------------|
| `register_dependency` | Register that your issue depends on another |
| `get_dependencies` | Get dependency graph |

### Dashboard

| Tool | Description |
|------|-------------|
| `get_dashboard` | Full dashboard of all coordination state |

## Team Session Protocol

When starting a team session, the Claude Code instance should:

1. **On startup**: Call `report_status` with status `working` and current issue
2. **Check messages**: Call `get_messages` to see if other teams sent anything
3. **Check blockers**: Call `get_blockers` to see if any blockers exist
4. **When blocked**: Call `report_blocker` to notify PM and blocking team
5. **When completing work**: Call `notify_ready_for_integration`
6. **When done**: Call `report_status` with status `done`

## Example Usage

### Core Team Session

```
// Start of session
mcp__hive-coordinator__report_status({
  team: "core",
  issue: "284",
  status: "working",
  notes: "Starting CapabilityAdvertisement schema"
})

// Check for messages
mcp__hive-coordinator__get_messages({ team: "core" })

// When schema is ready
mcp__hive-coordinator__notify_ready_for_integration({
  team: "core",
  issue_number: "284",
  description: "CapabilityAdvertisement schema ready for AI team"
})
```

### AI Team Session

```
// Register dependency
mcp__hive-coordinator__register_dependency({
  dependent_team: "ai",
  dependent_issue: "299",
  blocking_team: "core",
  blocking_issue: "284"
})

// When blocked
mcp__hive-coordinator__report_blocker({
  team: "ai",
  blocked_by: "core",
  issue_number: "284",
  description: "Need CapabilityAdvertisement schema to implement struct"
})
```

### PM Session

```
// Get full dashboard
mcp__hive-coordinator__get_dashboard()

// Send broadcast
mcp__hive-coordinator__send_message({
  from_team: "pm",
  to_team: "all",
  message: "Sprint 1 kickoff - all teams please report status"
})
```

## Database

The server uses SQLite for persistence. Tables:

- `team_status` - Current status of each team
- `blockers` - Active and resolved blockers
- `messages` - Inter-team messages
- `integration_ready` - Items ready for integration
- `dependencies` - Issue dependency graph

## Running Standalone (for debugging)

```bash
# View current state
sqlite3 coordinator.db "SELECT * FROM team_status"
sqlite3 coordinator.db "SELECT * FROM blockers WHERE resolved_at IS NULL"
```
