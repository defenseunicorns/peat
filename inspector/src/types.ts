import type { Page } from 'puppeteer-core';

/** A single check to run against a page */
export interface Check {
  name: string;
  /** Return true for pass, false for fail, or a string error message for fail */
  fn: (page: Page) => Promise<boolean | string>;
  /** Timeout in ms for this check (default: 10000) */
  timeout?: number;
}

/** When to capture a screenshot */
export type ScreenshotWhen = 'before-checks' | 'after-checks' | 'on-fail';

/** Screenshot configuration */
export interface ScreenshotSpec {
  name: string;
  when: ScreenshotWhen;
}

/** Wait-for condition after navigation */
export type WaitForCondition = 'load' | 'domcontentloaded' | 'networkidle0' | 'networkidle2';

/** Viewport dimensions */
export interface Viewport {
  width: number;
  height: number;
}

/** Full inspection specification */
export interface InspectSpec {
  name: string;
  description?: string;
  url: string;
  viewport?: Viewport;
  waitFor?: WaitForCondition;
  /** Extra settle time in ms after waitFor condition */
  waitMs?: number;
  screenshots?: ScreenshotSpec[];
  checks: Check[];
}

/** Result status for a single check */
export type CheckStatus = 'PASS' | 'FAIL' | 'ERROR';

/** Result of running a single check */
export interface CheckResult {
  name: string;
  status: CheckStatus;
  message?: string;
  durationMs: number;
}

/** Full inspection report */
export interface InspectReport {
  spec: string;
  url: string;
  timestamp: string;
  checks: CheckResult[];
  screenshots: string[];
  consoleErrors: string[];
  passed: boolean;
}
