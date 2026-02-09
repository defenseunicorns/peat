import type { InspectSpec } from '../src/types.js';
import {
  selectorExists,
  textPresent,
  textInSelector,
  noConsoleErrors,
  elementCount,
  canvasPresent,
} from '../src/checks.js';

/**
 * Phase 2 QA: Lifecycle integration checks for the HIVE port-ops viewer.
 *
 * Validates that:
 *  - The 3-column layout is intact (hierarchy, spatial center, events right)
 *  - Capability Cards panel renders with lifecycle sections
 *  - EventStream renders lifecycle events with detail text
 *  - No JS errors from the lifecycle store/components
 *
 * Run with the phase2 pipeline active:
 *   cd hive-sim/port-ops && make phase2-dry &
 *   cd inspector && npx tsx src/cli.ts specs/hive-viewer-lifecycle.ts --verbose
 */
export default {
  name: 'hive-viewer-lifecycle',
  description: 'Phase 2 QA: lifecycle degradation, resources, gap analysis in viewer',
  url: 'http://localhost:3001',
  viewport: { width: 1920, height: 1080 },
  waitFor: 'networkidle2',
  waitMs: 6000, // extra settle for OODA cycles + lifecycle events to arrive
  screenshots: [
    { name: 'full', when: 'after-checks' },
    { name: 'on-failure', when: 'on-fail' },
  ],
  checks: [
    // ── Layout structure ──────────────────────────────────────

    // 1. Header with HIVE branding
    selectorExists('header', 'Header present'),
    textInSelector('header', 'HIVE'),

    // 2. Sidebar present
    selectorExists('aside', 'Sidebar present'),

    // 3. Footer present
    selectorExists('footer', 'Footer present'),

    // 4. 3-column layout (aside + 2 sections in main)
    elementCount('main > *', 3),

    // 5. Three.js canvas for spatial view
    canvasPresent(200, 200),

    // ── Capability Cards (lifecycle rendering) ────────────────

    // 6. "Capabilities" heading present in sidebar (CSS uppercase)
    textPresent('CAPABILITIES'),

    // 7. "Hierarchy" heading present in sidebar (CSS uppercase)
    textPresent('HIERARCHY'),

    // 8. At least one capability card rendered (agent connected)
    {
      name: 'At least one capability card rendered',
      fn: async (page) => {
        const count = await page.evaluate(() => {
          // Cards are bg-gray-900 divs inside the Capabilities section
          const aside = document.querySelector('aside');
          if (!aside) return 0;
          return aside.querySelectorAll('.bg-gray-900').length;
        });
        if (count >= 1) return true;
        return `Found ${count} capability cards, expected at least 1`;
      },
    },

    // 9. Capability card shows node ID (crane-1 or crane-2)
    {
      name: 'Capability card shows a crane node ID',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          return text.includes('crane-1') || text.includes('crane-2');
        });
        return found || 'No crane node ID (crane-1 or crane-2) found on page';
      },
    },

    // 10. Equipment Health section appears (subsystem degradation, CSS uppercase)
    {
      name: 'Equipment Health section visible',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          return document.body.innerText.includes('EQUIPMENT HEALTH');
        });
        return found || '"EQUIPMENT HEALTH" label not found — no degradation data rendered';
      },
    },

    // 11. Resources section appears (resource consumption, CSS uppercase)
    {
      name: 'Resources section visible',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          return document.body.innerText.includes('RESOURCES');
        });
        return found || '"RESOURCES" label not found — no resource data rendered';
      },
    },

    // 12. Health bars render with percentage values
    {
      name: 'Health bars show percentage values',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          // Look for percentage text like "85%" or "97%" in the aside
          const aside = document.querySelector('aside');
          if (!aside) return false;
          const text = aside.innerText;
          return /\d{1,3}%/.test(text);
        });
        return found || 'No percentage values found in capability cards';
      },
    },

    // ── Event Stream (right panel, lifecycle events) ──────────

    // 13. "Event Stream" heading present (CSS uppercase)
    textPresent('EVENT STREAM'),

    // 14. Event stream has entries
    {
      name: 'Event stream has entries',
      fn: async (page) => {
        // The event count appears as "(N)" next to the heading (CSS uppercase)
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          const match = text.match(/EVENT STREAM\s*\((\d+)\)/);
          return match ? parseInt(match[1], 10) : 0;
        });
        if (found > 0) return true;
        return `Event stream shows ${found} entries, expected > 0`;
      },
    },

    // 15. Lifecycle events appear in stream (CAPABILITY DEGRADED or RESOURCE CONSUMED)
    {
      name: 'Lifecycle events in stream',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          return (
            text.includes('CAPABILITY DEGRADED') ||
            text.includes('Capability Degraded') ||
            text.includes('RESOURCE CONSUMED') ||
            text.includes('Resource Consumed')
          );
        });
        return found || 'No lifecycle events (CAPABILITY_DEGRADED or RESOURCE_CONSUMED) found in event stream';
      },
    },

    // 16. Lifecycle event details show subsystem/resource names
    {
      name: 'Lifecycle event details present',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          // Check for subsystem names or resource names from lifecycle.py
          return (
            text.includes('hydraulic') ||
            text.includes('spreader') ||
            text.includes('electrical') ||
            text.includes('battery') ||
            text.includes('fuel')
          );
        });
        return found || 'No lifecycle detail text (hydraulic/spreader/battery/fuel) found';
      },
    },

    // ── Playback + Connection ─────────────────────────────────

    // 17. Playback control visible
    textPresent('1\u00D7'),

    // 18. Play/pause button present
    {
      name: 'Play/pause button present',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const buttons = Array.from(document.querySelectorAll('footer button'));
          return buttons.some((b) => {
            const text = b.textContent ?? '';
            return text.includes('\u25B6') || text.includes('\u23F8');
          });
        });
        return found || 'No play/pause button (\u25B6/\u23F8) found in footer';
      },
    },

    // 19. Restart button present
    {
      name: 'Restart button present',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const buttons = Array.from(document.querySelectorAll('footer button'));
          return buttons.some((b) => (b.textContent ?? '').includes('\u27F2'));
        });
        return found || 'No restart button (\u27F2) found in footer';
      },
    },

    // 20. Connection status shows connected
    {
      name: 'Connection status shows connected',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText.toLowerCase();
          return text.includes('connected') || text.includes('live');
        });
        return found || 'Connection status does not show "connected" or "live"';
      },
    },

    // ── Error-free ────────────────────────────────────────────

    // 21. No JS console errors (ignore WebSocket reconnect noise)
    {
      name: 'No JS console errors (lifecycle-safe)',
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
