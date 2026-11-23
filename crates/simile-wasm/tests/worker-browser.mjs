import { createServer } from "node:http";
import { createServer as createSocket } from "node:net";
import { readFile } from "node:fs/promises";
import { resolve, dirname, extname, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import assert from "node:assert/strict";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const server = createServer(async (request, response) => {
  try {
    if (request.url === "/") {
      response.setHeader("Content-Type", "text/html");
      response.end('<!doctype html><title>Simile Worker tests</title><script type="module" src="/tests/worker-page.mjs"></script>');
      return;
    }
    const shared = {
      "/shared/service-described.hcl": "../simile-core/tests/fixtures/service-described.hcl",
      "/shared/run-orders.yaml": "../simile-core/tests/fixtures/run-orders.yaml",
      "/shared/http-parity.yaml": "../simile-http/tests/fixtures/parity.yaml",
    }[request.url];
    if (shared) { response.end(await readFile(resolve(root, shared))); return; }
    const file = resolve(root, `.${new URL(request.url, "http://localhost").pathname}`);
    if (!file.startsWith(root + sep)) throw new Error("invalid path");
    const bytes = await readFile(file);
    response.setHeader("Content-Type", { ".wasm": "application/wasm", ".js": "text/javascript", ".mjs": "text/javascript", ".json": "application/json" }[extname(file)] ?? "application/octet-stream");
    response.end(bytes);
  } catch { response.writeHead(404).end(); }
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const portProbe = createSocket();
await new Promise(resolve => portProbe.listen(0, "127.0.0.1", resolve));
const driverPort = portProbe.address().port;
await new Promise(resolve => portProbe.close(resolve));
const driver = spawn(process.env.CHROMEDRIVER ?? "chromedriver", [`--port=${driverPort}`], { stdio: ["ignore", "pipe", "pipe"] });
let logs = "";
driver.stdout.on("data", data => { logs += data; });
driver.stderr.on("data", data => { logs += data; });
driver.on("error", error => { logs += error.message; });
const base = `http://127.0.0.1:${driverPort}`;
const delay = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
async function command(path, method = "GET", body) {
  const response = await fetch(base + path, { method, ...(body ? { headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) } : {}) });
  const { value } = await response.json();
  if (!response.ok) throw new Error(JSON.stringify(value));
  return value;
}
let session;
try {
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    try { await command("/status"); ready = true; break; } catch { await delay(100); }
  }
  assert(ready, `Chromedriver did not start: ${logs}`);
  const chrome = { args: ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage"], ...(process.env.CHROME_BIN ? { binary: process.env.CHROME_BIN } : {}) };
  session = (await command("/session", "POST", { capabilities: { alwaysMatch: { browserName: "chrome", "goog:chromeOptions": chrome } } })).sessionId;
  await command(`/session/${session}/url`, "POST", { url: `http://127.0.0.1:${server.address().port}/` });
  let result;
  for (let attempt = 0; attempt < 600; attempt++) {
    result = await command(`/session/${session}/execute/sync`, "POST", { script: "return globalThis.simileTestResult ?? null", args: [] });
    if (result) break;
    await delay(100);
  }
  assert(result, "Worker browser tests timed out");
  assert.equal(result.ok, true, result.error);
  console.log(`Chromium Worker: ${result.assertions} assertions passed`);
} finally {
  if (session) await command(`/session/${session}`, "DELETE").catch(() => {});
  driver.kill();
  await new Promise(resolve => server.close(resolve));
}
