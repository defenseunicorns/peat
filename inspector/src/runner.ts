import * as path from 'node:path';
import * as fs from 'node:fs';
import type { Page } from 'puppeteer-core';
import type { InspectSpec, CheckResult, InspectReport, ScreenshotWhen } from './types.js';
import { launchBrowser } from './browser.js';

const DEFAULT_CHECK_TIMEOUT = 10_000;
const INSPECTOR_ROOT = path.resolve(import.meta.dirname, '..');

/** Run a single check with timeout */
async function runCheck(page: Page, check: { name: string; fn: (page: Page) => Promise<boolean | string>; timeout?: number }): Promise<CheckResult> {
  const start = performance.now();
  const timeout = check.timeout ?? DEFAULT_CHECK_TIMEOUT;

  try {
    const result = await Promise.race([
      check.fn(page),
      new Promise<string>((_, reject) =>
        setTimeout(() => reject(new Error(`Timeout after ${timeout}ms`)), timeout)
      ),
    ]);

    const durationMs = Math.round(performance.now() - start);

    if (result === true) {
      return { name: check.name, status: 'PASS', durationMs };
    }
    return { name: check.name, status: 'FAIL', message: String(result), durationMs };
  } catch (err) {
    const durationMs = Math.round(performance.now() - start);
    return {
      name: check.name,
      status: 'ERROR',
      message: err instanceof Error ? err.message : String(err),
      durationMs,
    };
  }
}

/** Capture a screenshot and return the file path */
async function captureScreenshot(page: Page, specName: string, screenshotName: string): Promise<string> {
  const dir = path.join(INSPECTOR_ROOT, 'screenshots');
  fs.mkdirSync(dir, { recursive: true });
  const filename = `${specName}-${screenshotName}-${Date.now()}.png`;
  const filepath = path.join(dir, filename);
  await page.screenshot({ path: filepath, fullPage: true });
  return filepath;
}

/** Take screenshots matching a specific trigger */
async function takeScreenshots(
  page: Page,
  spec: InspectSpec,
  when: ScreenshotWhen,
  captured: string[]
): Promise<void> {
  const specs = spec.screenshots?.filter(s => s.when === when) ?? [];
  for (const ss of specs) {
    const filepath = await captureScreenshot(page, spec.name, ss.name);
    captured.push(filepath);
  }
}

/** Map waitFor shorthand to Puppeteer's waitUntil */
function mapWaitFor(wf?: string): 'load' | 'domcontentloaded' | 'networkidle0' | 'networkidle2' {
  if (wf === 'networkidle') return 'networkidle0';
  if (wf === 'load' || wf === 'domcontentloaded' || wf === 'networkidle0' || wf === 'networkidle2') return wf;
  return 'load';
}

/** Run a full inspection against a spec */
export async function runInspection(spec: InspectSpec): Promise<InspectReport> {
  const consoleErrors: string[] = [];
  const screenshots: string[] = [];
  const results: CheckResult[] = [];

  const browser = await launchBrowser();

  try {
    const page = await browser.newPage();

    // Set viewport
    if (spec.viewport) {
      await page.setViewport(spec.viewport);
    }

    // Inject console error collector
    await page.evaluateOnNewDocument(() => {
      (window as any).__inspectorConsoleErrors = [];
    });

    page.on('console', (msg) => {
      if (msg.type() === 'error') {
        const text = msg.text();
        consoleErrors.push(text);
        // Also push to page for noConsoleErrors() check
        page.evaluate((t) => {
          (window as any).__inspectorConsoleErrors?.push(t);
        }, text).catch(() => {});
      }
    });

    page.on('pageerror', (err) => {
      const text = `Uncaught: ${(err as Error).message ?? err}`;
      consoleErrors.push(text);
      page.evaluate((t) => {
        (window as any).__inspectorConsoleErrors?.push(t);
      }, text).catch(() => {});
    });

    // Navigate
    await page.goto(spec.url, {
      waitUntil: mapWaitFor(spec.waitFor),
      timeout: 30_000,
    });

    // Extra settle time
    if (spec.waitMs) {
      await new Promise(r => setTimeout(r, spec.waitMs));
    }

    // Before-checks screenshots
    await takeScreenshots(page, spec, 'before-checks', screenshots);

    // Run checks
    for (const check of spec.checks) {
      const result = await runCheck(page, check);
      results.push(result);
    }

    // After-checks screenshots
    await takeScreenshots(page, spec, 'after-checks', screenshots);

    // On-fail screenshots
    const hasFail = results.some(r => r.status !== 'PASS');
    if (hasFail) {
      await takeScreenshots(page, spec, 'on-fail', screenshots);
    }
  } finally {
    await browser.close();
  }

  const report: InspectReport = {
    spec: spec.name,
    url: spec.url,
    timestamp: new Date().toISOString(),
    checks: results,
    screenshots,
    consoleErrors,
    passed: results.every(r => r.status === 'PASS'),
  };

  // Write JSON report
  const resultsDir = path.join(INSPECTOR_ROOT, 'results');
  fs.mkdirSync(resultsDir, { recursive: true });
  const reportFile = path.join(resultsDir, `${spec.name}-${Date.now()}.json`);
  fs.writeFileSync(reportFile, JSON.stringify(report, null, 2));

  return report;
}
