import type { InspectSpec } from '../src/types.js';
import {
  canvasPresent,
  selectorExists,
  textPresent,
  textInSelector,
  noConsoleErrors,
  elementCount,
} from '../src/checks.js';

export default {
  name: 'hive-viewer-spatial',
  description: 'QA checks for the HIVE Viewer spatial Three.js UI',
  url: 'http://localhost:9090',
  viewport: { width: 1920, height: 1080 },
  waitFor: 'networkidle0',
  waitMs: 4000,
  screenshots: [
    { name: 'full', when: 'after-checks' },
    { name: 'on-failure', when: 'on-fail' },
  ],
  checks: [
    // 1. Three.js canvas present with minimum size
    canvasPresent(200, 200),

    // 2. Header present
    selectorExists('header', 'Header present'),

    // 3. Sidebar present
    selectorExists('aside', 'Sidebar present'),

    // 4. Footer present
    selectorExists('footer', 'Footer present'),

    // 5. 3-column layout (aside + 2 sections in main)
    elementCount('main > *', 3),

    // 6. "HIVE" text in header
    textInSelector('header', 'HIVE'),

    // 7. "Viewer" text in header
    textInSelector('header', 'Viewer'),

    // 8. "HOLD-3 SUMMARY" in HUD (CSS uppercase applied)
    textPresent('HOLD-3 SUMMARY'),

    // 9. HUD shows "/20" (container count)
    textPresent('/20'),

    // 10. Aggregator status shows ACTIVE or IDLE
    {
      name: 'Aggregator shows ACTIVE or IDLE',
      fn: async (page) => {
        const found = await page.evaluate(() => {
          const text = document.body.innerText;
          return text.includes('ACTIVE') || text.includes('IDLE');
        });
        return found || 'Neither "ACTIVE" nor "IDLE" found on page';
      },
    },

    // 11. Canvas fills container (>400px wide)
    {
      name: 'Canvas fills container (>400px wide)',
      fn: async (page) => {
        const width = await page.evaluate(() => {
          const canvas = document.querySelector('canvas');
          return canvas ? canvas.clientWidth : 0;
        });
        if (width > 400) return true;
        return `Canvas width is ${width}px, expected >400px`;
      },
    },

    // 12. No JS console errors (ignore network 404s)
    {
      name: 'No JS console errors',
      fn: async (page) => {
        const errors: string[] = await page.evaluate(() => {
          return ((window as any).__inspectorConsoleErrors || []) as string[];
        });
        const jsErrors = errors.filter(e => !e.includes('Failed to load resource'));
        if (jsErrors.length === 0) return true;
        return `${jsErrors.length} JS error(s): ${jsErrors[0]}`;
      },
    },
  ],
} satisfies InspectSpec;
