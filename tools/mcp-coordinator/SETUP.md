# MCP Coordinator Setup

## Quick Start

```bash
# 1. Install dependencies
cd tools/mcp-coordinator
npm install

# 2. Create shared database directory
mkdir -p ~/.hive-coordinator

# 3. Add to Claude Code config
# Edit ~/.claude.json and add to mcpServers section
```

## Claude Code Configuration

Add this to `~/.claude.json`:

```json
{
  "mcpServers": {
    "hive-coordinator": {
      "command": "node",
      "args": ["/Users/kit/Code/hive-pm/tools/mcp-coordinator/src/index.js"],
      "env": {
        "COORDINATOR_DB_PATH": "/Users/kit/.hive-coordinator/coordinator.db"
      }
    }
  }
}
```

**Adjust paths for your system!**

## Verify Installation

After adding to config, restart Claude Code and run:

```
/mcp
```

You should see `hive-coordinator` listed with these tools:
- report_status
- get_all_status
- get_team_status
- report_blocker
- get_blockers
- resolve_blocker
- send_message
- get_messages
- notify_ready_for_integration
- get_integration_ready
- register_dependency
- get_dependencies
- get_dashboard

## Team Session Setup

Each team session (Core, ATAK, AI, Experiments, PM) connects to the same coordinator.

### Session Startup Checklist

1. Verify MCP connection: `/mcp`
2. Report status: `Use hive-coordinator to report status as [team] working on issue #X`
3. Check messages: `Use hive-coordinator to get messages for [team]`
4. Check blockers: `Use hive-coordinator to get blockers`

### Example First Message for Core Team

```
I'm the Core team session. First, let me:
1. Report my status as working on #284
2. Check for any messages from other teams
3. Check for any blockers

Then I'll read issue #284 and begin work on the CapabilityAdvertisement schema.
```

## Debugging

```bash
# View database directly
sqlite3 ~/.hive-coordinator/coordinator.db

# Useful queries
.tables
SELECT * FROM team_status;
SELECT * FROM blockers WHERE resolved_at IS NULL;
SELECT * FROM messages ORDER BY created_at DESC LIMIT 10;
```
