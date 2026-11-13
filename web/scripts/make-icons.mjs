// Renders public/favicon.svg into the PNG icons the PWA manifest needs.
// Run once after changing the logo: `node scripts/make-icons.mjs`.
import { readFile } from "node:fs/promises";
import { chromium } from "@playwright/test";

const svg = await readFile(new URL("../public/favicon.svg", import.meta.url), "utf8");
const targets = [
  { file: "pwa-192.png", size: 192, padding: 0 },
  { file: "pwa-512.png", size: 512, padding: 0 },
  // Maskable icons need a safe zone: the logo sits inside the middle 80%.
  { file: "pwa-maskable-512.png", size: 512, padding: 0.1, background: "#0f766e" },
  { file: "apple-touch-icon.png", size: 180, padding: 0, background: "#0f766e" },
];

const browser = await chromium.launch();
const page = await browser.newPage();
for (const { file, size, padding, background = "transparent" } of targets) {
  const inner = Math.round(size * (1 - 2 * padding));
  await page.setViewportSize({ width: size, height: size });
  await page.setContent(
    `<html><body style="margin:0;background:${background};display:grid;place-items:center;height:${size}px">` +
      `<div style="width:${inner}px;height:${inner}px">${svg.replace("<svg ", '<svg width="100%" height="100%" ')}</div></body></html>`,
  );
  await page.screenshot({
    path: new URL(`../public/${file}`, import.meta.url).pathname,
    omitBackground: background === "transparent",
  });
}
await browser.close();
