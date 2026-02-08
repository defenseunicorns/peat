import * as path from 'node:path';
import type { InspectSpec, InspectReport, CheckResult } from './types.js';
import { runInspection } from './runner.js';

function parseArgs(argv: string[]) {
  const args = argv.slice(2);
  let specFile: string | undefined;
  let urlOverride: string | undefined;
  let json = false;
  let verbose = false;

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--url' && args[i + 1]) {
      urlOverride = args[++i];
    } else if (args[i] === '--json') {
      json = true;
    } else if (args[i] === '--verbose') {
      verbose = true;
    } else if (!args[i].startsWith('-')) {
      specFile = args[i];
    }
  }

  return { specFile, urlOverride, json, verbose };
}

function statusIcon(status: CheckResult['status']): string {
  switch (status) {
    case 'PASS': return '\u2713';
    case 'FAIL': return '\u2717';
    case 'ERROR': return '!';
  }
}

function printSummary(report: InspectReport, verbose: boolean): void {
  console.log('');
  console.log(`Inspector: ${report.spec}`);
  console.log(`URL:       ${report.url}`);
  console.log(`Time:      ${report.timestamp}`);
  console.log('');

  for (const check of report.checks) {
    const icon = statusIcon(check.status);
    const time = verbose ? ` (${check.durationMs}ms)` : '';
    const msg = check.message ? ` - ${check.message}` : '';
    console.log(`  ${icon} ${check.status.padEnd(5)} ${check.name}${time}${msg}`);
  }

  console.log('');

  if (report.consoleErrors.length > 0) {
    console.log(`Console errors (${report.consoleErrors.length}):`);
    for (const err of report.consoleErrors.slice(0, 10)) {
      console.log(`  ${err}`);
    }
    if (report.consoleErrors.length > 10) {
      console.log(`  ... and ${report.consoleErrors.length - 10} more`);
    }
    console.log('');
  }

  if (report.screenshots.length > 0 && verbose) {
    console.log('Screenshots:');
    for (const ss of report.screenshots) {
      console.log(`  ${ss}`);
    }
    console.log('');
  }

  const passed = report.checks.filter(c => c.status === 'PASS').length;
  const total = report.checks.length;
  const result = report.passed ? 'PASSED' : 'FAILED';
  console.log(`Result: ${result} (${passed}/${total} checks passed)`);
}

async function main() {
  const { specFile, urlOverride, json, verbose } = parseArgs(process.argv);

  if (!specFile) {
    console.error('Usage: npx tsx src/cli.ts <spec-file> [--url <url>] [--json] [--verbose]');
    process.exit(2);
  }

  const specPath = path.resolve(specFile);
  const mod = await import(specPath);
  const spec: InspectSpec = mod.default;

  if (urlOverride) {
    spec.url = urlOverride;
  }

  if (verbose) {
    console.log(`Loading spec: ${spec.name}`);
    console.log(`Target: ${spec.url}`);
    console.log(`Checks: ${spec.checks.length}`);
    console.log('');
  }

  const report = await runInspection(spec);

  if (json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    printSummary(report, verbose);
  }

  process.exit(report.passed ? 0 : 1);
}

main().catch((err) => {
  console.error('Inspector failed:', err);
  process.exit(2);
});
