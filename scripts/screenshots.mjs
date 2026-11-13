// Fresh screenshots of every main screen, into docs/screenshots.
//
// Drives the dev app over the Chrome DevTools Protocol with sample hosts that all point at
// hopr-devsshd. Prereqs:
//   1. HOPR_DEVSSHD_HOME=<throwaway dir> cargo run -p hopr-devsshd   (127.0.0.1:2222)
//      The script fills that dir with a sample project (pass it as the second argument),
//      so no real file, user name or process shows up in the shots.
//   2. A fresh data folder, so the first shot is the create-vault screen:
//      HOPR_DATA_DIR=<empty dir> WEBVIEW2_USER_DATA_FOLDER=<empty dir>
//      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 pnpm tauri dev
// Usage: node scripts/screenshots.mjs <out dir> <devsshd home>
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const OUT = process.argv[2] ?? "docs/screenshots";
const HOME = process.argv[3];
if (!HOME) throw new Error("pass the hopr-devsshd HOME folder as the second argument");

// A small sample project to look at.
const PROJECT = join(HOME, "shop-api");
for (const d of ["src", "src/routes", "deploy", "logs"])
  mkdirSync(join(PROJECT, d), { recursive: true });
const NL = String.fromCharCode(10);
const files = {
  "README.md": ["# shop-api"],
  "package.json": ['{ "name": "shop-api", "version": "2.4.1" }'],
  Dockerfile: ["FROM node:22-alpine"],
  "docker-compose.yml": ["services:", "  api:", "    build: ."],
  "src/server.ts": ["export {};"],
  "src/routes/orders.ts": ["export {};"],
  "deploy/rollout.sh": ["#!/bin/sh"],
  ".env.example": ["PORT=8080"],
};
for (const [f, lines] of Object.entries(files)) {
  writeFileSync(join(PROJECT, f), lines.join(NL) + NL);
}
const levels = ["INFO", "INFO", "INFO", "WARN", "INFO", "INFO", "ERROR", "INFO"];
const paths = ["/api/orders", "/api/cart", "/api/products?page=2", "/healthz", "/api/checkout"];
const log = Array.from({ length: 40 }, (_, i) => {
  const l = levels[i % levels.length];
  const p = paths[i % paths.length];
  const ms = 8 + ((i * 37) % 180);
  const mm = String(12 + Math.floor(i / 6)).padStart(2, "0");
  const ss = String((i * 7) % 60).padStart(2, "0");
  return `2026-11-10T09:${mm}:${ss}Z ${l.padEnd(5)} GET ${p} ${l === "ERROR" ? 502 : 200} ${ms}ms`;
});
writeFileSync(join(PROJECT, "logs/app.log"), log.join(NL) + NL);
const PASSWORD = "screenshots-master-password";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function page() {
  for (let i = 0; i < 120; i++) {
    try {
      const pages = await (await fetch("http://127.0.0.1:9222/json")).json();
      const p = pages.find((x) => x.type === "page" && x.url.includes(":1430"));
      if (p) return p;
    } catch {
      // not up yet
    }
    await sleep(500);
  }
  throw new Error("Hopr dev window not found on port 9222");
}

const ws = new WebSocket((await page()).webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let seq = 0;
const pending = new Map();
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  pending.get(d.id)?.(d);
  pending.delete(d.id);
};
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = ++seq;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params }));
  });
async function ev(expression) {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails));
  return r.result?.result?.value;
}
mkdirSync(OUT, { recursive: true });
async function shot(name) {
  await sleep(700); // let transitions settle
  const r = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, `${name}.png`), Buffer.from(r.result.data, "base64"));
  console.log("saved", name);
}
const type = (text) => ev(`__hopr.type(${JSON.stringify(text)})`);

async function connected() {
  for (let i = 0; i < 150; i++) {
    const kind = await ev("__hopr.state()?.kind ?? null");
    if (kind === "ready") return;
    if (kind === "verifying_host_key") await ev(`__hopr.answerHostKey("trust")`);
    await sleep(100);
  }
  throw new Error("session didn't connect (is hopr-devsshd running on 2222?)");
}

for (let i = 0; i < 120 && !(await ev("typeof __hopr === 'object' && !!__hopr.vault()")); i++) {
  await sleep(250);
}
if (!(await ev("__hopr.vault().exists"))) await shot("01-create-vault");
await ev(`__hopr.ensureUnlocked(${JSON.stringify(PASSWORD)})`);
await sleep(800);

const host = (label, tags, environment = "none", notes = "") => ({
  label,
  hostname: "127.0.0.1",
  port: 2222,
  jumpHostId: null,
  agentForwarding: false,
  username: "dev",
  groupId: null,
  tags,
  auth: "password",
  keyId: null,
  favorite: false,
  environment,
  notes,
  password: { action: "set", value: "hopr" },
  totpSeed: { action: "keep" },
  proxy: null,
  proxyPassword: { action: "keep" },
});
const hosts = [
  host("web-01", ["web", "nginx"]),
  host("web-02", ["web", "nginx"]),
  host("api-staging", ["api"], "staging"),
  host("db-primary", ["db", "postgres"], "prod", "Primary Postgres. Ask before restarting."),
  host("cache-redis", ["db", "redis"], "prod"),
  host("ci-runner", ["ci"]),
];
const ids = [];
for (const h of hosts) {
  const r = await ev(`__hopr.saveHost(${JSON.stringify(h)})`);
  ids.push(r.id);
}
await ev("__hopr.refreshLibrary()");
await shot("02-hosts");

/** A neutral prompt in the sample project, then a clean screen. */
async function setUp(name) {
  await type(
    `function prompt { "dev@${name}:~/shop-api$ " }; Set-Location '${PROJECT}'; Clear-Host\r`,
  );
  await sleep(1200);
}

await ev(`__hopr.connectHost(${JSON.stringify(ids[0])})`);
await connected();
await sleep(600);
await setUp("web-01");
await type("Get-ChildItem -Recurse -File | Format-Table Length, Name -AutoSize\r");
await sleep(1500);
await shot("03-terminal");

await ev(`__hopr.split("row")`);
await connected();
await sleep(600);
await setUp("web-01");
await type("Get-Content logs/app.log -Tail 22\r");
await sleep(1500);
await shot("04-terminal-split");

await ev(`__hopr.filesOpen(${JSON.stringify(ids[0])})`);
await ev('__hopr.drawer("files")');
await sleep(1500);
await ev('__hopr.filesGo("/shop-api")');
await sleep(2500);
await shot("05-sftp-files");
await ev("__hopr.setDual(true)");
await sleep(2000);
await shot("06-sftp-dual-pane");
await ev("__hopr.setDual(false)");
await ev("__hopr.drawer(null)");

for (const [id, name] of [
  ["tunnels", "07-tunnels"],
  ["snippets", "08-snippets"],
  ["history", "09-history"],
  ["settings", "10-settings"],
]) {
  await ev(`__hopr.drawer(${JSON.stringify(id)})`);
  await shot(name);
}
await ev("__hopr.drawer(null)");

await ev("__hopr.showPalette()");
await shot("11-command-palette");
await send("Input.dispatchKeyEvent", {
  type: "keyDown",
  key: "Escape",
  code: "Escape",
  windowsVirtualKeyCode: 27,
});

await ev("__hopr.lock()");
await shot("12-locked");
ws.close();
process.exit(0);
