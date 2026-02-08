import puppeteer, { type Browser } from 'puppeteer-core';

/** Default Chrome executable path */
const CHROME_PATH = process.env.CHROME_PATH
  || '/usr/bin/google-chrome-stable'

/** Launch Chrome with SwiftShader WebGL flags for headless rendering */
export async function launchBrowser(): Promise<Browser> {
  return puppeteer.launch({
    executablePath: CHROME_PATH,
    headless: true,
    args: [
      '--no-sandbox',
      '--disable-setuid-sandbox',
      '--use-gl=angle',
      '--use-angle=swiftshader',
      '--disable-gpu-sandbox',
      '--disable-dev-shm-usage',
    ],
  });
}
