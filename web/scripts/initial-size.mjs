// Prints the gzipped size of what the browser downloads for the first screen
// (index.html's entry plus the transactions or login page chunk) and fails above
// the Phase 4 budget of 300 kB. Run after `npm run build`.
import { readFile } from "node:fs/promises";
import { gzipSync } from "node:zlib";

const BUDGET_BYTES = 300 * 1024;
const dist = new URL("../dist/", import.meta.url);
const manifest = JSON.parse(await readFile(new URL(".vite/manifest.json", dist), "utf8"));

/** Every file a manifest entry loads up front: its static imports and CSS. */
function closure(key, seen = new Set()) {
  const entry = manifest[key];
  if (!entry || seen.has(entry.file)) return seen;
  seen.add(entry.file);
  for (const css of entry.css ?? []) seen.add(css);
  for (const imported of entry.imports ?? []) closure(imported, seen);
  return seen;
}

async function gzippedSize(files) {
  let total = 0;
  for (const file of files) total += gzipSync(await readFile(new URL(file, dist))).length;
  return total;
}

let failed = false;
for (const page of [
  "src/features/transactions/TransactionsPage.tsx",
  "src/features/auth/LoginPage.tsx",
]) {
  const files = closure(page, closure("index.html"));
  const bytes = await gzippedSize(files);
  const kb = (bytes / 1024).toFixed(1);
  console.log(`${page}: ${kb} kB gzipped (${files.size} files)`);
  if (bytes > BUDGET_BYTES) failed = true;
}
if (failed) {
  console.error("Initial route is over the 300 kB budget.");
  process.exit(1);
}
