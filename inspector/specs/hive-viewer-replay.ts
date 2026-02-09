import type { InspectSpec } from '../src/types.js';
import {
  selectorExists,
  textPresent,
  textInSelector,
  canvasPresent,
} from '../src/checks.js';

/**
 * Phase 3 QA: Replay engine + timeline scrubber.
 *
 * Validates that:
 *  - JSONL file can be loaded via ?replay= query param
 *  - Replay mode activates (REPLAY badge, Exit Replay button, timeline controls)
 *  - Timeline scrubber renders with frame counter
 *  - Seek/step controls work (cursor advances)
 *  - State reconstructs correctly on seek (nodes appear)
 *  - Play/pause toggles speed
 *  - No JS errors
 *
 * Prerequisites:
 *   1. Record a test file:
 *      cd hive-sim/port-ops && PYTHONPATH=bridge/src:agent/src \
 *        python -m port_agent.main --mode multi --agents 2c1a \
 *        --provider dry-run --max-cycles 5 --time-compression 600 \
 *        2>/dev/null > /tmp/test-replay.jsonl
 *   2. Start viewer with the file served:
 *      cd hive-viewer-ui && npx vite --port 3001 &
 *   3. Run inspector:
 *      cd inspector && npx tsx src/cli.ts specs/hive-viewer-replay.ts --verbose
 *
 * The spec uses ?replay= pointing to the JSONL file served by Vite's
 * public directory. Copy the recording to hive-viewer-ui/public/ first,
 * or use the --url flag to override.
 */
export default {
  name: 'hive-viewer-replay',
  description: 'Phase 3 QA: Replay engine — JSONL load, timeline scrubber, seek/step',
  url: 'http://localhost:3001/?replay=/test-replay.jsonl',
  viewport: { width: 1920, height: 1080 },
  waitFor: 'networkidle2',
  waitMs: 3000, // settle for replay to load + parse
  screenshots: [
    { name: 'replay-loaded', when: 'after-checks' },
    { name: 'on-failure', when: 'on-fail' },
  ],
  checks: [
    // ── Layout still intact ──────────────────────────────────
    selectorExists('header', 'Header present'),
    textInSelector('header', 'HIVE'),
    selectorExists('footer', 'Footer present'),
    canvasPresent(200, 200),

    // ── Replay mode activated ────────────────────────────────

    {
      name: 'REPLAY badge visible in header',
      fn: async (page) => {
        const found = await page.evaluate(() =>
          document.body.innerText.includes('REPLAY'),
        );
        return found || 'REPLAY badge not found';
      },
    },

    {
      name: 'Exit Replay button visible',
      fn: async (page) => {
        const found = await page.evaluate(() =>
          document.body.innerText.includes('Exit Replay'),
        );
        return found || 'Exit Replay button not found';
      },
    },

    {
      name: 'Load JSONL button hidden during replay',
      fn: async (page) => {
        const found = await page.evaluate(() =>
          document.body.innerText.includes('Load JSONL'),
        );
        return !found || 'Load JSONL button should be hidden in replay mode';
      },
    },

    // ── Timeline scrubber controls ───────────────────────────

    {
      name: 'Timeline range input present',
      fn: async (page) => {
        const slider = await page.$('input[type="range"]');
        return slider !== null || 'Timeline range input not found';
      },
    },

    {
      name: 'Frame counter shows total frames in footer',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const footer = document.querySelector('footer');
          if (!footer) return false;
          // Match pattern like "0/51" or "25/51" in the footer specifically
          return /\d+\/\d+/.test(footer.innerText);
        });
        return found || 'Frame counter (N/M format) not found in footer';
      },
    },

    {
      name: 'Step navigation buttons present (5 buttons)',
      fn: async (page) => {
        // Home, StepBack, PlayPause, StepForward, End
        const count = await page.evaluate(() => {
          const footer = document.querySelector('footer');
          if (!footer) return 0;
          return footer.querySelectorAll('button').length;
        });
        // 5 nav + 6 speed = 11 minimum
        return count >= 10 || `Found ${count} buttons in footer, expected >= 10`;
      },
    },

    // ── Seek functionality ───────────────────────────────────

    {
      name: 'Seek to end populates nodes',
      fn: async (page) => {
        // Click the End button (⏭) to seek to last frame
        const seeked = await page.evaluate(() => {
          const footer = document.querySelector('footer');
          if (!footer) return false;
          const buttons = footer.querySelectorAll('button');
          // 5th button is End (⏭)
          if (buttons.length >= 5) {
            (buttons[4] as HTMLElement).click();
            return true;
          }
          return false;
        });
        if (!seeked) return 'Could not find End button to click';

        // Wait for state reconstruction
        await new Promise((r) => setTimeout(r, 500));

        // Verify nodes appeared
        const hasNodes = await page.evaluate(() => {
          const text = document.body.innerText;
          return text.includes('crane-') || text.includes('hold-agg');
        });
        return hasNodes || 'No agent nodes visible after seeking to end';
      },
    },

    {
      name: 'Frame counter advances after seek-to-end',
      fn: async (page) => {
        // Read frame counter from the footer (not body — HUD has "0/20" too)
        const counter = await page.evaluate(() => {
          const footer = document.querySelector('footer');
          if (!footer) return null;
          const text = footer.innerText;
          const match = text.match(/(\d+)\/(\d+)/);
          if (!match) return null;
          return { cursor: parseInt(match[1]), total: parseInt(match[2]) };
        });
        if (!counter) return 'Frame counter not found in footer';
        if (counter.cursor === 0) return `Cursor still at 0 after seek (total: ${counter.total})`;
        if (counter.cursor < counter.total - 2)
          return `Cursor ${counter.cursor} not near end ${counter.total}`;
        return true;
      },
    },

    {
      name: 'Seek to home resets cursor to 0',
      fn: async (page) => {
        // Click the Home button (⏮) — first button in footer
        await page.evaluate(() => {
          const footer = document.querySelector('footer');
          if (!footer) return;
          const buttons = footer.querySelectorAll('button');
          if (buttons.length > 0) (buttons[0] as HTMLElement).click();
        });
        await new Promise((r) => setTimeout(r, 500));

        const counter = await page.evaluate(() => {
          const text = document.body.innerText;
          const match = text.match(/(\d+)\/(\d+)/);
          return match ? parseInt(match[1]) : -1;
        });
        return counter === 0 || `Cursor is ${counter} after Home, expected 0`;
      },
    },

    // ── Speed buttons ────────────────────────────────────────

    {
      name: 'Higher speed buttons available (8x, 16x)',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          return text.includes('8\u00D7') && text.includes('16\u00D7');
        });
        return found || '8x/16x speed buttons not found in replay mode';
      },
    },

    // ── Error-free ───────────────────────────────────────────

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
