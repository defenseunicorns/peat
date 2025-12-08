#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { SSEServerTransport } from "@modelcontextprotocol/sdk/server/sse.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import Database from "better-sqlite3";
import { fileURLToPath } from "url";
import { dirname, join } from "path";
import express from "express";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Check for SSE mode
const useSSE = process.argv.includes("--sse");
const ssePort = parseInt(process.env.COORDINATOR_PORT || "3100");

// Initialize SQLite database
const dbPath = process.env.COORDINATOR_DB_PATH || join(__dirname, "..", "coordinator.db");
const db = new Database(dbPath);

// Initialize schema
db.exec(`
  CREATE TABLE IF NOT EXISTS team_status (
    team TEXT PRIMARY KEY,
    current_issue TEXT,
    status TEXT DEFAULT 'idle',
    last_update TEXT DEFAULT CURRENT_TIMESTAMP,
    notes TEXT
  );

  CREATE TABLE IF NOT EXISTS blockers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team TEXT NOT NULL,
    blocked_by TEXT,
    issue_number TEXT,
    description TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    resolved_at TEXT,
    resolved_by TEXT
  );

  CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_team TEXT NOT NULL,
    to_team TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    read_at TEXT
  );

  CREATE TABLE IF NOT EXISTS integration_ready (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team TEXT NOT NULL,
    issue_number TEXT NOT NULL,
    description TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    integrated_at TEXT
  );

  CREATE TABLE IF NOT EXISTS dependencies (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    dependent_team TEXT NOT NULL,
    dependent_issue TEXT NOT NULL,
    blocking_team TEXT NOT NULL,
    blocking_issue TEXT NOT NULL,
    status TEXT DEFAULT 'waiting',
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
  );
`);

// Initialize teams if not present
const teams = ["core", "atak", "experiments", "ai", "pm"];
const insertTeam = db.prepare(
  "INSERT OR IGNORE INTO team_status (team, status) VALUES (?, 'idle')"
);
teams.forEach((team) => insertTeam.run(team));

// Create MCP server
const server = new Server(
  {
    name: "hive-coordinator",
    version: "0.1.0",
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

// Define tools
const TOOLS = [
  {
    name: "report_status",
    description:
      "Report your team's current status. Call this when starting work on an issue, making progress, or completing work.",
    inputSchema: {
      type: "object",
      properties: {
        team: {
          type: "string",
          enum: teams,
          description: "Your team name",
        },
        issue: {
          type: "string",
          description: "Current issue number (e.g., '284')",
        },
        status: {
          type: "string",
          enum: ["idle", "working", "blocked", "reviewing", "done"],
          description: "Current status",
        },
        notes: {
          type: "string",
          description: "Optional notes about progress or blockers",
        },
      },
      required: ["team", "status"],
    },
  },
  {
    name: "get_all_status",
    description:
      "Get status of all teams. Use this to understand what other teams are working on.",
    inputSchema: {
      type: "object",
      properties: {},
    },
  },
  {
    name: "get_team_status",
    description: "Get detailed status of a specific team.",
    inputSchema: {
      type: "object",
      properties: {
        team: {
          type: "string",
          enum: teams,
          description: "Team to query",
        },
      },
      required: ["team"],
    },
  },
  {
    name: "report_blocker",
    description:
      "Report that you are blocked by another team or issue. This notifies PM and the blocking team.",
    inputSchema: {
      type: "object",
      properties: {
        team: {
          type: "string",
          enum: teams,
          description: "Your team name",
        },
        blocked_by: {
          type: "string",
          enum: teams,
          description: "Team blocking you",
        },
        issue_number: {
          type: "string",
          description: "Issue number you're blocked on",
        },
        description: {
          type: "string",
          description: "Description of what you need",
        },
      },
      required: ["team", "description"],
    },
  },
  {
    name: "get_blockers",
    description: "Get all active blockers, optionally filtered by team.",
    inputSchema: {
      type: "object",
      properties: {
        team: {
          type: "string",
          enum: teams,
          description: "Filter by team (optional)",
        },
        include_resolved: {
          type: "boolean",
          description: "Include resolved blockers",
        },
      },
    },
  },
  {
    name: "resolve_blocker",
    description: "Mark a blocker as resolved.",
    inputSchema: {
      type: "object",
      properties: {
        blocker_id: {
          type: "number",
          description: "ID of the blocker to resolve",
        },
        resolved_by: {
          type: "string",
          enum: teams,
          description: "Team that resolved the blocker",
        },
      },
      required: ["blocker_id", "resolved_by"],
    },
  },
  {
    name: "send_message",
    description: "Send a message to another team.",
    inputSchema: {
      type: "object",
      properties: {
        from_team: {
          type: "string",
          enum: teams,
          description: "Your team name",
        },
        to_team: {
          type: "string",
          enum: [...teams, "all"],
          description: "Recipient team (or 'all' for broadcast)",
        },
        message: {
          type: "string",
          description: "Message content",
        },
      },
      required: ["from_team", "to_team", "message"],
    },
  },
  {
    name: "get_messages",
    description: "Get messages for your team.",
    inputSchema: {
      type: "object",
      properties: {
        team: {
          type: "string",
          enum: teams,
          description: "Your team name",
        },
        unread_only: {
          type: "boolean",
          description: "Only return unread messages",
        },
      },
      required: ["team"],
    },
  },
  {
    name: "notify_ready_for_integration",
    description:
      "Notify that your work is ready for integration testing. This alerts dependent teams.",
    inputSchema: {
      type: "object",
      properties: {
        team: {
          type: "string",
          enum: teams,
          description: "Your team name",
        },
        issue_number: {
          type: "string",
          description: "Issue number that's ready",
        },
        description: {
          type: "string",
          description: "What's ready for integration",
        },
      },
      required: ["team", "issue_number"],
    },
  },
  {
    name: "get_integration_ready",
    description: "Get list of items ready for integration.",
    inputSchema: {
      type: "object",
      properties: {
        pending_only: {
          type: "boolean",
          description: "Only show items not yet integrated",
        },
      },
    },
  },
  {
    name: "register_dependency",
    description:
      "Register that your issue depends on another team's issue. You'll be notified when it's ready.",
    inputSchema: {
      type: "object",
      properties: {
        dependent_team: {
          type: "string",
          enum: teams,
          description: "Your team name",
        },
        dependent_issue: {
          type: "string",
          description: "Your issue number",
        },
        blocking_team: {
          type: "string",
          enum: teams,
          description: "Team you depend on",
        },
        blocking_issue: {
          type: "string",
          description: "Issue you depend on",
        },
      },
      required: [
        "dependent_team",
        "dependent_issue",
        "blocking_team",
        "blocking_issue",
      ],
    },
  },
  {
    name: "get_dependencies",
    description: "Get dependency graph showing what's blocking what.",
    inputSchema: {
      type: "object",
      properties: {
        team: {
          type: "string",
          enum: teams,
          description: "Filter by team (optional)",
        },
      },
    },
  },
  {
    name: "get_dashboard",
    description:
      "Get a full dashboard view of all coordination state - teams, blockers, messages, integrations.",
    inputSchema: {
      type: "object",
      properties: {},
    },
  },
  {
    name: "complete_task",
    description:
      "Call this when you finish a task. It marks the task complete, checks for new messages, and returns your next assignment if available. ALWAYS use this instead of waiting idle.",
    inputSchema: {
      type: "object",
      properties: {
        team: {
          type: "string",
          enum: teams,
          description: "Your team name",
        },
        completed_issue: {
          type: "string",
          description: "Issue number you just completed (e.g., '284')",
        },
        summary: {
          type: "string",
          description: "Brief summary of what was accomplished",
        },
      },
      required: ["team", "completed_issue", "summary"],
    },
  },
];

// Tool handlers
const handlers = {
  report_status: ({ team, issue, status, notes }) => {
    const stmt = db.prepare(`
      UPDATE team_status
      SET current_issue = ?, status = ?, notes = ?, last_update = CURRENT_TIMESTAMP
      WHERE team = ?
    `);
    stmt.run(issue || null, status, notes || null, team);
    return { success: true, message: `${team} status updated to ${status}` };
  },

  get_all_status: () => {
    const rows = db.prepare("SELECT * FROM team_status ORDER BY team").all();
    return { teams: rows };
  },

  get_team_status: ({ team }) => {
    const row = db
      .prepare("SELECT * FROM team_status WHERE team = ?")
      .get(team);
    const blockers = db
      .prepare(
        "SELECT * FROM blockers WHERE team = ? AND resolved_at IS NULL"
      )
      .all(team);
    const messages = db
      .prepare(
        "SELECT * FROM messages WHERE to_team = ? AND read_at IS NULL"
      )
      .all(team);
    return { status: row, active_blockers: blockers, unread_messages: messages };
  },

  report_blocker: ({ team, blocked_by, issue_number, description }) => {
    const stmt = db.prepare(`
      INSERT INTO blockers (team, blocked_by, issue_number, description)
      VALUES (?, ?, ?, ?)
    `);
    const result = stmt.run(team, blocked_by || null, issue_number || null, description);

    // Update team status to blocked
    db.prepare("UPDATE team_status SET status = 'blocked' WHERE team = ?").run(team);

    return {
      success: true,
      blocker_id: result.lastInsertRowid,
      message: `Blocker #${result.lastInsertRowid} created for ${team}`
    };
  },

  get_blockers: ({ team, include_resolved }) => {
    let query = "SELECT * FROM blockers";
    const params = [];
    const conditions = [];

    if (team) {
      conditions.push("(team = ? OR blocked_by = ?)");
      params.push(team, team);
    }
    if (!include_resolved) {
      conditions.push("resolved_at IS NULL");
    }
    if (conditions.length > 0) {
      query += " WHERE " + conditions.join(" AND ");
    }
    query += " ORDER BY created_at DESC";

    return { blockers: db.prepare(query).all(...params) };
  },

  resolve_blocker: ({ blocker_id, resolved_by }) => {
    const stmt = db.prepare(`
      UPDATE blockers
      SET resolved_at = CURRENT_TIMESTAMP, resolved_by = ?
      WHERE id = ?
    `);
    stmt.run(resolved_by, blocker_id);
    return { success: true, message: `Blocker #${blocker_id} resolved by ${resolved_by}` };
  },

  send_message: ({ from_team, to_team, message }) => {
    if (to_team === "all") {
      const stmt = db.prepare(`
        INSERT INTO messages (from_team, to_team, message)
        VALUES (?, ?, ?)
      `);
      teams.filter(t => t !== from_team).forEach(t => stmt.run(from_team, t, message));
      return { success: true, message: `Broadcast sent to all teams` };
    } else {
      const stmt = db.prepare(`
        INSERT INTO messages (from_team, to_team, message)
        VALUES (?, ?, ?)
      `);
      stmt.run(from_team, to_team, message);
      return { success: true, message: `Message sent to ${to_team}` };
    }
  },

  get_messages: ({ team, unread_only }) => {
    let query = "SELECT * FROM messages WHERE to_team = ?";
    if (unread_only) {
      query += " AND read_at IS NULL";
    }
    query += " ORDER BY created_at DESC LIMIT 50";

    const messages = db.prepare(query).all(team);

    // Mark as read
    if (messages.length > 0) {
      const ids = messages.map(m => m.id);
      db.prepare(`UPDATE messages SET read_at = CURRENT_TIMESTAMP WHERE id IN (${ids.join(",")})`).run();
    }

    return { messages };
  },

  notify_ready_for_integration: ({ team, issue_number, description }) => {
    const stmt = db.prepare(`
      INSERT INTO integration_ready (team, issue_number, description)
      VALUES (?, ?, ?)
    `);
    const result = stmt.run(team, issue_number, description || null);

    // Check for any dependencies waiting on this
    const waiting = db.prepare(`
      SELECT * FROM dependencies
      WHERE blocking_team = ? AND blocking_issue = ? AND status = 'waiting'
    `).all(team, issue_number);

    // Notify dependent teams
    waiting.forEach(dep => {
      db.prepare(`
        INSERT INTO messages (from_team, to_team, message)
        VALUES (?, ?, ?)
      `).run(team, dep.dependent_team,
        `Issue #${issue_number} is now ready! Your issue #${dep.dependent_issue} is unblocked.`);

      // Update dependency status
      db.prepare("UPDATE dependencies SET status = 'ready' WHERE id = ?").run(dep.id);
    });

    return {
      success: true,
      notified_teams: waiting.map(w => w.dependent_team),
      message: `Integration ready for #${issue_number}`
    };
  },

  get_integration_ready: ({ pending_only }) => {
    let query = "SELECT * FROM integration_ready";
    if (pending_only) {
      query += " WHERE integrated_at IS NULL";
    }
    query += " ORDER BY created_at DESC";
    return { ready_items: db.prepare(query).all() };
  },

  register_dependency: ({ dependent_team, dependent_issue, blocking_team, blocking_issue }) => {
    const stmt = db.prepare(`
      INSERT INTO dependencies (dependent_team, dependent_issue, blocking_team, blocking_issue)
      VALUES (?, ?, ?, ?)
    `);
    const result = stmt.run(dependent_team, dependent_issue, blocking_team, blocking_issue);
    return {
      success: true,
      dependency_id: result.lastInsertRowid,
      message: `Registered: ${dependent_team}#${dependent_issue} depends on ${blocking_team}#${blocking_issue}`
    };
  },

  get_dependencies: ({ team }) => {
    let query = "SELECT * FROM dependencies";
    if (team) {
      query += " WHERE dependent_team = ? OR blocking_team = ?";
      return { dependencies: db.prepare(query).all(team, team) };
    }
    return { dependencies: db.prepare(query).all() };
  },

  get_dashboard: () => {
    const teamStatus = db.prepare("SELECT * FROM team_status ORDER BY team").all();
    const activeBlockers = db.prepare("SELECT * FROM blockers WHERE resolved_at IS NULL").all();
    const recentMessages = db.prepare("SELECT * FROM messages ORDER BY created_at DESC LIMIT 20").all();
    const integrationReady = db.prepare("SELECT * FROM integration_ready WHERE integrated_at IS NULL").all();
    const waitingDeps = db.prepare("SELECT * FROM dependencies WHERE status = 'waiting'").all();

    return {
      teams: teamStatus,
      active_blockers: activeBlockers,
      recent_messages: recentMessages,
      integration_ready: integrationReady,
      waiting_dependencies: waitingDeps,
      summary: {
        teams_working: teamStatus.filter(t => t.status === 'working').length,
        teams_blocked: teamStatus.filter(t => t.status === 'blocked').length,
        open_blockers: activeBlockers.length,
        items_ready_for_integration: integrationReady.length,
      }
    };
  },

  complete_task: ({ team, completed_issue, summary }) => {
    // 1. Update status to done
    db.prepare(`
      UPDATE team_status
      SET current_issue = ?, status = 'done', notes = ?, last_update = CURRENT_TIMESTAMP
      WHERE team = ?
    `).run(completed_issue, `Completed: ${summary}`, team);

    // 2. Notify PM
    db.prepare(`
      INSERT INTO messages (from_team, to_team, message)
      VALUES (?, 'pm', ?)
    `).run(team, `Completed #${completed_issue}: ${summary}`);

    // 3. Get unread messages for this team
    const messages = db.prepare(`
      SELECT * FROM messages WHERE to_team = ? AND read_at IS NULL
      ORDER BY created_at DESC LIMIT 20
    `).all(team);

    // Mark messages as read
    if (messages.length > 0) {
      const ids = messages.map(m => m.id);
      db.prepare(`UPDATE messages SET read_at = CURRENT_TIMESTAMP WHERE id IN (${ids.join(",")})`).run();
    }

    // 4. Check for waiting dependencies that might now be unblocked
    const unblocked = db.prepare(`
      SELECT * FROM dependencies
      WHERE blocking_team = ? AND blocking_issue = ? AND status = 'waiting'
    `).all(team, completed_issue);

    // Notify dependent teams
    unblocked.forEach(dep => {
      db.prepare(`
        INSERT INTO messages (from_team, to_team, message)
        VALUES (?, ?, ?)
      `).run(team, dep.dependent_team,
        `Issue #${completed_issue} is complete! Your #${dep.dependent_issue} may be unblocked.`);
      db.prepare("UPDATE dependencies SET status = 'ready' WHERE id = ?").run(dep.id);
    });

    return {
      success: true,
      completed: `#${completed_issue}`,
      messages: messages,
      unblocked_teams: unblocked.map(d => d.dependent_team),
      next_action: messages.length > 0
        ? "You have new messages above - check for your next task"
        : "No pending messages. Use get_dashboard to see overall status or wait for PM assignment."
    };
  },
};

// Register tool list handler
server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: TOOLS,
}));

// Register tool call handler
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  if (!handlers[name]) {
    throw new Error(`Unknown tool: ${name}`);
  }

  try {
    const result = handlers[name](args || {});
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify(result, null, 2),
        },
      ],
    };
  } catch (error) {
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify({ error: error.message }, null, 2),
        },
      ],
      isError: true,
    };
  }
});

// Start server
async function main() {
  if (useSSE) {
    // SSE mode - HTTP server for remote connections
    const app = express();
    app.use(express.json());

    // Store active transports by sessionId
    const transports = {};

    // SSE endpoint for MCP connections
    app.get("/sse", async (req, res) => {
      console.error(`New SSE connection from ${req.ip}`);

      // Create transport - it will send the endpoint event with sessionId
      const transport = new SSEServerTransport("/messages", res);
      const sessionId = transport.sessionId;
      transports[sessionId] = transport;

      console.error(`Session created: ${sessionId}`);

      res.on("close", () => {
        delete transports[sessionId];
        console.error(`SSE connection closed for session ${sessionId}`);
      });

      // Connect the MCP server to this transport
      await server.connect(transport);
    });

    // Message endpoint for client->server communication
    app.post("/messages", async (req, res) => {
      const sessionId = req.query.sessionId;
      const transport = transports[sessionId];

      if (transport) {
        await transport.handlePostMessage(req, res, req.body);
      } else {
        console.error(`No transport for sessionId: ${sessionId}`);
        res.status(400).json({ error: "No active SSE connection for this sessionId" });
      }
    });

    // Health check endpoint
    app.get("/health", (req, res) => {
      res.json({
        status: "ok",
        mode: "sse",
        connections: Object.keys(transports).length,
        sessions: Object.keys(transports)
      });
    });

    app.listen(ssePort, "0.0.0.0", () => {
      console.error(`HIVE Coordinator MCP server running in SSE mode on http://0.0.0.0:${ssePort}`);
      console.error(`Remote clients connect to: http://<your-ip>:${ssePort}/sse`);
    });
  } else {
    // Stdio mode - local connections
    const transport = new StdioServerTransport();
    await server.connect(transport);
    console.error("HIVE Coordinator MCP server running in stdio mode");
  }
}

main().catch(console.error);
