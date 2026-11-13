// Starts the real `saldo` binary (built with `--features embed-web`) on a
// fresh temporary database for the Playwright run.
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const port = process.env.E2E_PORT ?? "8099";
const binary = new URL("../../target/debug/saldo", import.meta.url).pathname;
if (!existsSync(binary)) {
  console.error(`${binary} is missing: run \`cargo build -p saldo-server --features embed-web\`.`);
  process.exit(1);
}
const folder = mkdtempSync(join(tmpdir(), "saldo-e2e-"));
const child = spawn(binary, ["serve"], {
  stdio: "inherit",
  env: {
    ...process.env,
    SALDO_DATABASE_URL: `sqlite://${folder}/saldo.db`,
    SALDO_BIND: `127.0.0.1:${port}`,
    SALDO_PUBLIC_URL: `http://127.0.0.1:${port}`,
    SALDO_LOG: "warn",
  },
});
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => child.kill("SIGTERM"));
}
child.on("exit", (code) => process.exit(code ?? 0));
