// Native end-to-end smoke test for the SSH → xterm data path.
//
// Drives the real Hopr window (WebView2) over the Chrome DevTools Protocol, against
// hopr-devsshd serving a real local shell. Prereqs (see `just e2e`):
//   1. cargo run -p hopr-devsshd            (127.0.0.1:2222, password "hopr")
//   2. WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 pnpm tauri dev
// Usage: node scripts/e2e-smoke.mjs [screenshot.png]
import { writeFileSync } from "node:fs";

const CDP = "http://127.0.0.1:9222";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function findPage() {
  for (let i = 0; i < 60; i++) {
    try {
      const pages = await (await fetch(`${CDP}/json`)).json();
      const page = pages.find((p) => p.type === "page" && p.url.includes("1420"));
      if (page) return page;
    } catch {
      // app not up yet
    }
    await sleep(1000);
  }
  throw new Error("Hopr webview not found on the CDP port");
}

function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  let seq = 0;
  const pending = new Map();
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      const id = ++seq;
      pending.set(id, resolve);
      ws.send(JSON.stringify({ id, method, params }));
    });
  return new Promise((resolve) => {
    ws.onopen = () => resolve({ ws, send });
  });
}

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `  (${detail})` : ""}`);
}

const page = await findPage();
const { ws, send } = await connect(page.webSocketDebuggerUrl);
const evaluate = async (expr) => {
  const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails));
  return r.result?.result?.value;
};
const waitFor = async (expr, timeoutMs) => {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (await evaluate(expr)) return Date.now() - start;
    await sleep(100);
  }
  return -1;
};

await waitFor("typeof window.__hopr === 'object'", 15000);

// 1. Connect + reach a live shell prompt.
await evaluate(`__hopr.connect("127.0.0.1", 2222, "dev", "hopr")`);
const readyMs = await waitFor(`__hopr.state()?.kind === "ready"`, 20000);
check("session reaches Ready", readyMs >= 0, `${readyMs} ms`);
const promptMs = await waitFor(`/PS .*>|\\$ ?$/m.test(__hopr.screen())`, 20000);
check("remote shell prompt renders in xterm", promptMs >= 0, `${promptMs} ms`);

// 2. Round trip: typed input runs remotely and its output comes back.
await evaluate(`__hopr.type('Write-Output ("hopr-" + 6*7)\\r')`);
const echoMs = await waitFor(`__hopr.screen().includes("hopr-42")`, 10000);
check("typed command executes and output returns", echoMs >= 0, `${echoMs} ms`);

// 3. Throughput + responsiveness: flood output while sampling UI event-loop lag.
const lagProbe = `(() => { window.__lag = 0; window.__probe = setInterval(() => {
  const t = performance.now(); setTimeout(() => { window.__lag = Math.max(window.__lag, performance.now() - t); }, 0);
}, 50); })()`;
await evaluate(lagProbe);
const floodStart = Date.now();
await evaluate(`__hopr.type('1..100000 | ForEach-Object { "line $_ ................................................" }\\r')`);
const floodMs = await waitFor(`__hopr.screen().includes("line 100000 ")`, 120000);
const lag = await evaluate(`(clearInterval(window.__probe), window.__lag)`);
check("100k-line flood fully rendered", floodMs >= 0, `${Date.now() - floodStart} ms`);
check("UI stays responsive during flood (max event-loop lag < 200 ms)", lag < 200, `${Math.round(lag)} ms`);

// 4. Screenshot of the live window.
const shotPath = process.argv[2];
if (shotPath) {
  const shot = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(shotPath, Buffer.from(shot.result.data, "base64"));
  console.log(`screenshot: ${shotPath}`);
}

// 5. Remote exit is reported as a clean session end.
await evaluate(`__hopr.type('exit\\r')`);
const exitMs = await waitFor(`__hopr.state()?.kind === "closed"`, 15000);
const reason = await evaluate(`__hopr.state()?.reason`);
check("remote exit reported as Closed(remote_exited)", exitMs >= 0 && reason === "remote_exited", reason);

// 6. Wrong password surfaces a typed auth error.
await evaluate(`__hopr.connect("127.0.0.1", 2222, "dev", "wrong")`);
const failMs = await waitFor(`__hopr.state()?.kind === "failed"`, 20000);
const code = await evaluate(`__hopr.state()?.error?.code`);
check("wrong password → Failed(auth_failed)", failMs >= 0 && code === "auth_failed", code);

ws.close();
const failed = results.filter((r) => !r.ok).length;
console.log(`\n${results.length - failed}/${results.length} checks passed`);
process.exit(failed ? 1 : 0);
