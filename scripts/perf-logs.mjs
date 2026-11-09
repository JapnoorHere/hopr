// Terminal responsiveness under fast logs: how far the screen runs behind the server, how
// smooth it stays, how quickly Ctrl+C takes effect, and raw throughput.
//
// Drives the real Hopr window over the Chrome DevTools Protocol against the bench server:
//   1. HOPR_DEVSSHD_PORT=2223 HOPR_DEVSSHD_BENCH=1 cargo run -p hopr-devsshd
//   2. WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 pnpm tauri dev
//      (HOPR_DATA_DIR=<throwaway dir> keeps it away from your own vault)
// Usage: node scripts/perf-logs.mjs [--search] [--json out.json]
//   --search  keeps the search bar open on a common word while logs stream.
import { writeFileSync } from "node:fs";

const CDP = "http://127.0.0.1:9222";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const args = process.argv.slice(2);
const withSearch = args.includes("--search");
const jsonOut = args.includes("--json") ? args[args.indexOf("--json") + 1] : null;

async function findPage() {
  for (let i = 0; i < 90; i++) {
    try {
      const pages = await (await fetch(`${CDP}/json`)).json();
      const page = pages.find((p) => p.type === "page" && p.url.includes(":1430"));
      if (page) return page;
    } catch {
      // app not up yet
    }
    await sleep(1000);
  }
  throw new Error("Hopr webview not found on the CDP port");
}

const page = await findPage();
const ws = new WebSocket(page.webSocketDebuggerUrl);
let seq = 0;
const pending = new Map();
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  pending.get(msg.id)?.(msg);
  pending.delete(msg.id);
};
await new Promise((r) => (ws.onopen = r));
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = ++seq;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails));
  return r.result?.result?.value;
};
const waitFor = async (expr, timeoutMs, every = 50) => {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (await evaluate(expr)) return Date.now() - start;
    await sleep(every);
  }
  return -1;
};

if ((await waitFor("typeof window.__hopr === 'object'", 60000, 500)) < 0) {
  throw new Error("window.__hopr missing: run a dev frontend");
}
await evaluate(`__hopr.ensureUnlocked("hopr-perf-master-password")`);
await evaluate(`__hopr.connect("127.0.0.1", 2223, "dev", "hopr")`);
for (let i = 0; i < 200; i++) {
  const kind = await evaluate("__hopr.state()?.kind");
  if (kind === "ready") break;
  if (kind === "verifying_host_key") await evaluate(`__hopr.answerHostKey("trust")`);
  await sleep(100);
}
if ((await waitFor(`__hopr.screen().includes("bench>")`, 20000)) < 0) throw new Error("no prompt");

// In-page sampler: every animation frame, how old the newest line on screen is (server
// stamp vs now, same clock), and the gap since the previous frame.
// Lines stamped before the run started (the previous run's) don't count.
const startSampler = `(() => {
  const s = { lags: [], gaps: [], last: performance.now(), on: true, since: Date.now() };
  window.__perf = s;
  const frame = () => {
    if (!s.on) return;
    const now = performance.now();
    s.gaps.push(now - s.last);
    s.last = now;
    const tail = __hopr.screen(4);
    const m = [...tail.matchAll(/T=(\\d{13})/g)].at(-1);
    if (m && Number(m[1]) >= s.since) s.lags.push(Date.now() - Number(m[1]));
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
})()`;
const stopSampler = `(() => { const s = window.__perf; s.on = false;
  return { lags: s.lags, gaps: s.gaps }; })()`;

const pct = (xs, p) => {
  if (!xs.length) return 0;
  const a = [...xs].sort((x, y) => x - y);
  return a[Math.min(a.length - 1, Math.floor((p / 100) * a.length))];
};
const summary = ({ lags, gaps }) => ({
  lagP50: Math.round(pct(lags, 50)),
  lagP95: Math.round(pct(lags, 95)),
  lagMax: Math.round(Math.max(0, ...lags)),
  fps: Math.round(1000 / (gaps.reduce((a, b) => a + b, 0) / Math.max(1, gaps.length))),
  frameMax: Math.round(Math.max(0, ...gaps)),
  janky: gaps.filter((g) => g > 50).length,
});

if (withSearch) {
  await evaluate(`__hopr.find?.("request")`);
  await sleep(300);
}

const results = {};
/**
 * Stream logs and stop them with Ctrl+C after `stopAfter` ms (ctrlC: until LOGSTOP shows).
 * With `stopAfter` null the run ends on its own; `drain` is then how long after the server
 * finished the screen caught up (the backlog).
 */
async function logsRun(name, rate, secs = 6, stopAfter = 4000) {
  await evaluate(startSampler);
  const started = Date.now();
  await evaluate(`__hopr.type("logs ${secs} ${rate}\\r")`);
  let extra;
  if (stopAfter === null) {
    // The previous run's marker scrolls away once this run's lines arrive.
    await waitFor(`!__hopr.screen(8).includes("LOGSTOP")`, 10000, 10);
    const ms = await waitFor(`__hopr.screen(8).includes("LOGSTOP")`, 120000, 10);
    extra = { drain: ms < 0 ? -1 : Date.now() - (started + secs * 1000) };
  } else {
    await sleep(stopAfter);
    const t0 = Date.now();
    await evaluate(`__hopr.type("\\x03")`);
    const ms = await waitFor(`__hopr.screen(8).includes("LOGSTOP")`, 30000, 10);
    extra = { ctrlC: ms < 0 ? -1 : Date.now() - t0 };
  }
  results[name] = { ...summary(await evaluate(stopSampler)), ...extra };
  console.log(name.padEnd(22), JSON.stringify(results[name]));
  await sleep(800);
}

await logsRun("logs 5k lines/s", 5000);
await logsRun("logs 20k lines/s", 20000);
await logsRun("logs 60k lines/s", 60000);
// Unthrottled: faster than any terminal can draw. Ctrl+C isn't measured here: the bench
// server's russh session can wedge on input mid-flood (with OpenSSH too), so it runs out.
await logsRun("logs unthrottled", 0, 5, null);

// Throughput: plain flood.
const FLOOD_MIB = Number(process.env.FLOOD_MIB ?? 100);
const f0 = Date.now();
await evaluate(`__hopr.type('flood ${FLOOD_MIB}\\r')`);
const floodMs = await waitFor(`__hopr.screen().includes("FLOODDONE")`, 180000, 100);
const secs = (Date.now() - f0) / 1000;
results.flood = { mbps: floodMs < 0 ? -1 : Number(((FLOOD_MIB * 1.048576) / secs).toFixed(1)) };
console.log("flood".padEnd(22), JSON.stringify(results.flood));

await evaluate(`__hopr.closeActive()`);
if (jsonOut) writeFileSync(jsonOut, JSON.stringify(results, null, 2));
ws.close();
