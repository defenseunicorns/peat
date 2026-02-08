import type { Check } from './types.js';

/** Check that a CSS selector exists in the DOM */
export function selectorExists(selector: string, label?: string): Check {
  return {
    name: label || `Selector exists: ${selector}`,
    fn: async (page) => {
      const el = await page.$(selector);
      return el !== null || `Selector "${selector}" not found`;
    },
  };
}

/** Check that a <canvas> exists with minimum dimensions */
export function canvasPresent(minWidth = 0, minHeight = 0): Check {
  return {
    name: `Canvas present (min ${minWidth}x${minHeight})`,
    fn: async (page) => {
      const result = await page.evaluate((mw, mh) => {
        const canvas = document.querySelector('canvas');
        if (!canvas) return 'No <canvas> element found';
        const w = canvas.clientWidth || canvas.width;
        const h = canvas.clientHeight || canvas.height;
        if (w < mw || h < mh) return `Canvas too small: ${w}x${h} (need ${mw}x${mh})`;
        return true;
      }, minWidth, minHeight);
      return result;
    },
  };
}

/** Check that text appears anywhere in body.innerText */
export function textPresent(text: string): Check {
  return {
    name: `Text present: "${text}"`,
    fn: async (page) => {
      const found = await page.evaluate((t) => {
        return document.body.innerText.includes(t);
      }, text);
      return found || `Text "${text}" not found on page`;
    },
  };
}

/** Check that text appears within a specific element */
export function textInSelector(selector: string, text: string): Check {
  return {
    name: `Text "${text}" in ${selector}`,
    fn: async (page) => {
      const found = await page.evaluate((sel, t) => {
        const el = document.querySelector(sel);
        if (!el) return `Selector "${sel}" not found`;
        return (el as HTMLElement).innerText.includes(t) || `Text "${t}" not found in ${sel}`;
      }, selector, text);
      return found;
    },
  };
}

/** Check that no console errors were logged (uses collected errors from runner) */
export function noConsoleErrors(): Check {
  return {
    name: 'No console errors',
    fn: async (page) => {
      // Access the collected errors from the page's custom property
      const errors = await page.evaluate(() => {
        return (window as any).__inspectorConsoleErrors || [];
      });
      if (errors.length === 0) return true;
      return `${errors.length} console error(s): ${errors[0]}`;
    },
  };
}

/** Check that at least N elements match a selector */
export function elementCount(selector: string, min: number): Check {
  return {
    name: `At least ${min} elements: ${selector}`,
    fn: async (page) => {
      const count = await page.evaluate((sel) => {
        return document.querySelectorAll(sel).length;
      }, selector);
      if (count >= min) return true;
      return `Found ${count} elements matching "${selector}", need at least ${min}`;
    },
  };
}
