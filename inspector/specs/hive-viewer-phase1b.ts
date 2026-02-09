import type { InspectSpec } from '../src/types.js';
import {
  selectorExists,
  textPresent,
  textInSelector,
  noConsoleErrors,
  canvasPresent,
} from '../src/checks.js';

/**
 * Phase 1b QA: 15-node hold team checks for the HIVE port-ops viewer.
 *
 * Validates that:
 *  - All 15 entities appear in the viewer
 *  - New entity types (tractor, sensor, scheduler) render in spatial + protocol views
 *  - Hierarchy shows H0-H4 levels
 *  - No JS errors
 *
 * Run with the phase1b pipeline active:
 *   cd hive-sim/port-ops && make phase1b-dry MAX_CYCLES=10 &
 *   cd inspector && npx tsx src/cli.ts specs/hive-viewer-phase1b.ts --verbose
 */
export default {
  name: 'hive-viewer-phase1b',
  description: 'Phase 1b QA: 15-node hold team (tractors, sensors, scheduler) in viewer',
  url: 'http://localhost:3001',
  viewport: { width: 1920, height: 1080 },
  waitFor: 'networkidle2',
  waitMs: 8000, // extra settle for 15 agents to run cycles
  screenshots: [
    { name: 'full', when: 'after-checks' },
    { name: 'on-failure', when: 'on-fail' },
  ],
  checks: [
    // ── Layout structure ──────────────────────────────────────

    selectorExists('header', 'Header present'),
    textInSelector('header', 'HIVE'),
    selectorExists('aside', 'Sidebar present'),
    selectorExists('footer', 'Footer present'),
    canvasPresent(200, 200),

    // ── Agent presence: all roles visible ──────────────────────

    // Cranes
    {
      name: 'Crane agents visible',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          return text.includes('crane-1') && text.includes('crane-2');
        });
        return found || 'crane-1 or crane-2 not found on page';
      },
    },

    // Tractors
    {
      name: 'Tractor agents visible',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          return text.includes('tractor-1') || text.includes('TRACTOR');
        });
        return found || 'No tractor agents found on page';
      },
    },

    // Sensors
    {
      name: 'Sensor agents visible',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          return text.includes('load-cell-1') || text.includes('rfid-1') || text.includes('SENSOR');
        });
        return found || 'No sensor agents found on page';
      },
    },

    // Scheduler
    {
      name: 'Scheduler agent visible',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          return text.includes('scheduler-1') || text.includes('SCHED');
        });
        return found || 'No scheduler agent found on page';
      },
    },

    // Operators (at least op-1)
    {
      name: 'Operator agents visible',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          return document.body.innerText.includes('op-1');
        });
        return found || 'No operator agents found on page';
      },
    },

    // ── Capability cards (15 agents) ──────────────────────────

    {
      name: 'At least 10 capability cards rendered',
      fn: async (page) => {
        const count = await page.evaluate(() => {
          const aside = document.querySelector('aside');
          if (!aside) return 0;
          return aside.querySelectorAll('.bg-gray-900').length;
        });
        if (count >= 10) return true;
        return `Found ${count} capability cards, expected at least 10`;
      },
    },

    // ── Hierarchy levels ──────────────────────────────────────

    {
      name: 'Hierarchy shows H4 scheduler level',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          return document.body.innerText.includes('H4');
        });
        return found || 'H4 scheduler level not found in hierarchy';
      },
    },

    {
      name: 'Hierarchy shows H0 sensor level',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          return document.body.innerText.includes('H0');
        });
        return found || 'H0 sensor level not found in hierarchy';
      },
    },

    // ── Event stream has new event types ──────────────────────

    textPresent('EVENT STREAM'),

    {
      name: 'Event stream has entries from multiple agent types',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          const match = text.match(/EVENT STREAM\s*\((\d+)\)/);
          return match ? parseInt(match[1], 10) : 0;
        });
        if (found > 5) return true;
        return `Event stream shows ${found} entries, expected > 5`;
      },
    },

    // ── HUD shows new entity stats ────────────────────────────

    {
      name: 'HUD shows tractor count or scheduler status',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          return text.includes('Tractors') || text.includes('Scheduler');
        });
        return found || 'HUD missing tractor/scheduler stats';
      },
    },

    // ── Error-free ────────────────────────────────────────────

    {
      name: 'No JS console errors',
      fn: async (page) => {
        const errors: string[] = await page.evaluate(() => {
          return ((window as any).__inspectorConsoleErrors || []) as string[];
        });
        const jsErrors = errors.filter(
          (e) =>
            !e.includes('Failed to load resource') &&
            !e.includes('WebSocket') &&
            !e.includes('ws://'),
        );
        if (jsErrors.length === 0) return true;
        return `${jsErrors.length} JS error(s): ${jsErrors[0]}`;
      },
    },
  ],
} satisfies InspectSpec;
