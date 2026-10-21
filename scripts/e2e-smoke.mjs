// Native end-to-end smoke test for the SSH → xterm data path.
//
// Drives the real Hopr window (WebView2) over the Chrome DevTools Protocol, against
// hopr-devsshd serving a real local shell. Prereqs (see `just e2e`):
//   1. HOPR_DEVSSHD_HOME=<throwaway dir> HOPR_DEVSSHD_PROXY_PORT=2280 cargo run -p hopr-devsshd
//      (127.0.0.1:2222, password "hopr"; deploy-key writes go to that HOME, never yours)
//      and HOPR_DEVSSHD_PORT=2223 HOPR_DEVSSHD_BENCH=1 cargo run -p hopr-devsshd (throughput)
//   2. WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 pnpm tauri dev
// Usage: node scripts/e2e-smoke.mjs [screenshot.png]
import { execSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CDP = "http://127.0.0.1:9222";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function findPage() {
  for (let i = 0; i < 60; i++) {
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
  const r = await send("Runtime.evaluate", {
    expression: expr,
    awaitPromise: true,
    returnByValue: true,
  });
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

if ((await waitFor("typeof window.__hopr === 'object'", 30000)) < 0) {
  throw new Error("window.__hopr missing: run a dev frontend (pnpm tauri dev [--release])");
}

// 0. Vault: created on first run (HOPR_DATA_DIR points at a throwaway folder), else unlocked.
const VAULT_PW = "hopr-e2e-master-password";
const recovery = await evaluate(`__hopr.ensureUnlocked(${JSON.stringify(VAULT_PW)})`);
check(
  "vault created/unlocked",
  await evaluate("__hopr.vault()?.unlocked === true"),
  recovery ? "created, recovery key shown" : "unlocked",
);

/** Wait for Ready, trusting an unknown host key if asked. */
const readyTrusting = async (timeoutMs) => {
  const start = Date.now();
  let prompted = false;
  let maxHop = 0;
  while (Date.now() - start < timeoutMs) {
    const kind = await evaluate("__hopr.state()?.kind");
    if (kind === "connecting")
      maxHop = Math.max(maxHop, await evaluate("__hopr.state()?.hop ?? 0"));
    if (kind === "ready") return { ms: Date.now() - start, prompted, maxHop };
    if (kind === "verifying_host_key") {
      prompted = true;
      await evaluate(`__hopr.answerHostKey("trust")`);
    }
    if (kind === "failed" || kind === "closed") break;
    await sleep(100);
  }
  return { ms: -1, prompted, maxHop };
};

// 1. Connect + reach a live shell prompt.
await evaluate(`__hopr.connect("127.0.0.1", 2222, "dev", "hopr")`);
const first = await readyTrusting(20000);
const readyMs = first.ms;
check("session reaches Ready", readyMs >= 0, `${readyMs} ms`);
check("unknown host key is verified before connecting", first.prompted);
const promptMs = await waitFor(`/PS .*>|\\$ ?$/m.test(__hopr.screen())`, 20000);
check("remote shell prompt renders in xterm", promptMs >= 0, `${promptMs} ms`);

// 2. Round trip: typed input runs remotely and its output comes back.
await evaluate(`__hopr.type('Write-Output ("hopr-" + 6*7)\\r')`);
const echoMs = await waitFor(`__hopr.screen().includes("hopr-42")`, 10000);
check("typed command executes and output returns", echoMs >= 0, `${echoMs} ms`);

// 3. Throughput + responsiveness against the bench server (port 2223, no ConPTY in the
//    path), flooding while sampling UI event-loop lag. Budget (§1.5): ≥ 50 MB/s, no freeze.
const FLOOD_MIB = Number(process.env.FLOOD_MIB ?? 100);
const shellTab = await evaluate("__hopr.count()");
await evaluate(`__hopr.connect("127.0.0.1", 2223, "dev", "hopr")`);
await readyTrusting(20000);
await waitFor(`__hopr.screen().includes("bench>")`, 20000);
const lagProbe = `(() => { window.__lag = 0; window.__probe = setInterval(() => {
  const t = performance.now(); setTimeout(() => { window.__lag = Math.max(window.__lag, performance.now() - t); }, 0);
}, 50); })()`;
await evaluate(lagProbe);
const floodStart = Date.now();
await evaluate(`__hopr.type('flood ${FLOOD_MIB}\\r')`);
const floodMs = await waitFor(`__hopr.screen().includes("FLOODDONE")`, 180000);
const lag = await evaluate(`(clearInterval(window.__probe), window.__lag)`);
const secs = (Date.now() - floodStart) / 1000;
const mbps = (FLOOD_MIB * 1.048576) / secs;
check(`${FLOOD_MIB} MiB flood fully rendered`, floodMs >= 0, `${secs.toFixed(2)} s`);
check("throughput ≥ 50 MB/s", floodMs >= 0 && mbps >= 50, `${mbps.toFixed(1)} MB/s`);
check(
  "UI stays responsive during flood (max event-loop lag < 200 ms)",
  lag < 200,
  `${Math.round(lag)} ms`,
);
await evaluate(`__hopr.closeActive()`);
await waitFor(`__hopr.count() === ${shellTab}`, 5000);

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
check(
  "remote exit reported as Closed(remote_exited)",
  exitMs >= 0 && reason === "remote_exited",
  reason,
);

// 6. Wrong password surfaces a typed auth error.
await evaluate(`__hopr.connect("127.0.0.1", 2222, "dev", "wrong")`);
const failMs = await waitFor(`__hopr.state()?.kind === "failed"`, 20000);
const code = await evaluate(`__hopr.state()?.error?.code`);
check("wrong password → Failed(auth_failed)", failMs >= 0 && code === "auth_failed", code);
await evaluate("__hopr.closeActive()");

// 7. Saved host with a saved password: connects from the vault, no host-key prompt (trusted).
const hostInput = (label, password, port = 2222, jumpHostId = null) =>
  JSON.stringify({
    label,
    hostname: "127.0.0.1",
    port,
    jumpHostId,
    agentForwarding: false,
    username: "dev",
    groupId: null,
    tags: ["e2e"],
    auth: "password",
    keyId: null,
    favorite: false,
    environment: "none",
    notes: "",
    password: password ? { action: "set", value: password } : { action: "keep" },
    totpSeed: { action: "keep" },
    proxy: null,
    proxyPassword: { action: "keep" },
  });
const saved = await evaluate(`__hopr.saveHost(${hostInput(`e2e-saved-${Date.now()}`, "hopr")})`);
await evaluate(`__hopr.connectHost(${JSON.stringify(saved.id)})`);
const savedRun = await readyTrusting(20000);
check("saved host connects with vault password", savedRun.ms >= 0, `${savedRun.ms} ms`);
check("trusted host key is not asked again", !savedRun.prompted);

// 8. Saved host without a password: asks, then remembers it in the vault.
const bare = await evaluate(`__hopr.saveHost(${hostInput(`e2e-prompt-${Date.now()}`, null)})`);
await evaluate(`__hopr.connectHost(${JSON.stringify(bare.id)})`);
const askedMs = await waitFor(`__hopr.state()?.kind === "prompting"`, 20000);
check("missing password is prompted for", askedMs >= 0, `${askedMs} ms`);
await evaluate(`__hopr.answerPrompt(["hopr"], true)`);
const afterPrompt = await waitFor(`__hopr.state()?.kind === "ready"`, 20000);
await sleep(300); // the save runs right after Ready
await evaluate("__hopr.refreshLibrary()");
const remembered = await evaluate(`__hopr.host(${JSON.stringify(bare.id)})?.hasPassword`);
check("typed password logs in and is saved to the vault", afterPrompt >= 0 && remembered === true);

// 9. Lock keeps sessions running; wrong master password is refused; right one unlocks.
await evaluate("__hopr.lock()");
const locked = await evaluate("__hopr.vault()?.unlocked === false");
const stillLive = await evaluate(`__hopr.state()?.kind === "ready"`);
const wrong = await evaluate(`__hopr.unlock("nope").then(r => r.ok)`);
const right = await evaluate(`__hopr.unlock(${JSON.stringify(VAULT_PW)}).then(r => r.ok)`);
check(
  "lock/unlock (sessions survive, wrong password refused)",
  locked && stillLive && !wrong && right,
);
await evaluate("__hopr.closeActive()");
await evaluate("__hopr.closeActive()");

// 10. Jump chain: reach the bench server (2223) through the dev server (2222) as a bastion.
const bastion = await evaluate(
  `__hopr.saveHost(${hostInput(`e2e-bastion-${Date.now()}`, "hopr")})`,
);
const behind = await evaluate(
  `__hopr.saveHost(${hostInput(`e2e-behind-${Date.now()}`, "hopr", 2223, bastion.id)})`,
);
await evaluate(`__hopr.connectHost(${JSON.stringify(behind.id)})`);
const jumpRun = await readyTrusting(20000);
const benchPrompt = await waitFor(`__hopr.screen().includes("bench>")`, 10000);
const trail = await evaluate("__hopr.hops()");
check(
  "jump chain: target reached through bastion, tab shows the hop trail",
  jumpRun.ms >= 0 && benchPrompt >= 0 && trail.length === 2,
  `${jumpRun.ms} ms, tab trail: laptop › ${trail.join(" › ")}`,
);
await evaluate("__hopr.closeActive()");

// 11. Workspace keys, sent as real keystrokes into the focused terminal: app chords must
//     reach the app (not the shell) while plain keys still reach the shell.
const MOD = { alt: 1, ctrl: 2, shift: 8 };
const VK = {
  D: 68,
  E: 69,
  F: 70,
  P: 80,
  W: 87,
  0: 48,
  "=": 187,
  Enter: 13,
  Escape: 27,
  ArrowLeft: 37,
  ArrowRight: 39,
};
const press = async (key, mods = []) => {
  const modifiers = mods.reduce((m, k) => m | MOD[k], 0);
  const code =
    key === "=" ? "Equal" : /^\d$/.test(key) ? `Digit${key}` : key.length === 1 ? `Key${key}` : key;
  const base = {
    modifiers,
    key,
    code,
    windowsVirtualKeyCode: VK[key],
    nativeVirtualKeyCode: VK[key],
  };
  await send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
  await sleep(150);
};
const benchHost = await evaluate(
  `__hopr.saveHost(${hostInput(`e2e-split-${Date.now()}`, "hopr", 2223)})`,
);
await evaluate(`__hopr.connectHost(${JSON.stringify(benchHost.id)})`);
await readyTrusting(20000);
await waitFor(`__hopr.screen().includes("bench>")`, 10000);
const firstPane = await evaluate("__hopr.focused()");

await press("D", ["ctrl", "shift"]); // split right
const split1 = await waitFor("__hopr.panes() === 2", 5000);
const splitReady = await readyTrusting(20000);
check("Ctrl+Shift+D splits the pane and connects the new one", split1 >= 0 && splitReady.ms >= 0);

await press("ArrowLeft", ["ctrl", "alt"]);
const movedBack = (await evaluate("__hopr.focused()")) === firstPane;
await press("Enter", ["ctrl", "shift"]);
const zoomedFirst = (await evaluate("__hopr.zoomed()")) === firstPane;
await press("Enter", ["ctrl", "shift"]);
check(
  "Ctrl+Alt+Arrow moves focus; Ctrl+Shift+Enter zooms",
  movedBack && zoomedFirst && !(await evaluate("__hopr.zoomed()")),
);

// Plain typing still goes to the shell in the focused pane.
for (const ch of "echo-keys") {
  await send("Input.dispatchKeyEvent", { type: "char", text: ch });
}
const typed = await waitFor(`__hopr.screen().includes("echo-keys")`, 5000);
check("plain keys still reach the shell", typed >= 0);

await press("P", ["ctrl", "shift"]);
const opened = await waitFor("__hopr.paletteOpen()", 3000);
await send("Input.insertText", { text: "split down" });
await sleep(200);
await press("Enter");
const viaPalette = await waitFor("__hopr.panes() === 3", 5000);
check("Ctrl+Shift+P palette runs a command by name", opened >= 0 && viaPalette >= 0);
await readyTrusting(20000);

await press("W", ["ctrl", "shift"]);
const closedOne = await waitFor("__hopr.panes() === 2", 5000);
check("Ctrl+Shift+W closes the focused pane", closedOne >= 0);
const tabsBefore = await evaluate("__hopr.tabs()");
await evaluate("__hopr.closeActive()");
await evaluate("__hopr.closeActive()");
await waitFor(`__hopr.tabs() === ${tabsBefore - 1}`, 5000);

// 12. Terminal features with real keys: search, font zoom, OSC 52 clipboard gating.
await evaluate(`__hopr.connect("127.0.0.1", 2222, "dev", "hopr")`);
await readyTrusting(20000);
await waitFor(`/PS .*>/.test(__hopr.screen())`, 20000);
await evaluate(`__hopr.type('Write-Output ("find-" + "me")' + String.fromCharCode(13))`);
await waitFor(`__hopr.screen().includes("find-me")`, 10000);
await press("F", ["ctrl", "shift"]);
const barOpen = await waitFor(`!!document.querySelector('[role="search"] input')`, 3000);
await send("Input.insertText", { text: "find-me" });
const counted = await waitFor(
  `(document.querySelector('[role="search"] [aria-live]')?.textContent ?? "").includes(" / ")`,
  5000,
);
const countText = await evaluate(
  `document.querySelector('[role="search"] [aria-live]')?.textContent?.trim()`,
);
await press("Escape");
const barClosed = await waitFor(`!document.querySelector('[role="search"]')`, 3000);
check(
  "Ctrl+Shift+F finds text in output with a match count",
  barOpen >= 0 && counted >= 0 && barClosed >= 0,
  countText,
);

const size0 = await evaluate("__hopr.fontSize()");
await press("=", ["ctrl"]);
const bigger = await evaluate("__hopr.fontSize()");
await press("0", ["ctrl"]);
const reset = await evaluate("__hopr.fontSize()");
check(
  "Ctrl+= / Ctrl+0 zoom the terminal font",
  bigger === size0 + 1 && reset === 14,
  `${size0} → ${bigger} → ${reset}`,
);

await evaluate(
  `__hopr.type('[Console]::Write([char]27 + "]52;c;aG9wci1jbGlw" + [char]7)' + String.fromCharCode(13))`,
);
const asked = await waitFor(
  `!!document.querySelector('[aria-label="Allow clipboard write"]')`,
  10000,
);
await evaluate(
  `[...document.querySelectorAll('[aria-label="Allow clipboard write"] button')].find((b) => b.textContent.trim() === "Deny")?.click()`,
);
const gone = await waitFor(`!document.querySelector('[aria-label="Allow clipboard write"]')`, 3000);
check("OSC 52 clipboard write asks before copying", asked >= 0 && gone >= 0);
await evaluate("__hopr.closeActive()");

// 13. Hosts & security (3d): duplicate, bulk edit, export, ping, proxy, deploy key,
//     clipboard auto-clear and the audit log. Needs hopr-devsshd started with
//     HOPR_DEVSSHD_PROXY_PORT=2280 and a sandbox HOPR_DEVSSHD_HOME.
const closeAllTabs = async () => {
  for (let i = 0; i < 20 && (await evaluate("__hopr.tabs()")) > 0; i++) {
    await evaluate("__hopr.closeActive()");
    await sleep(100);
  }
};
const click = (selector) =>
  evaluate(
    `(() => { const el = document.querySelector(${JSON.stringify(selector)}); el?.click(); return !!el; })()`,
  );
const clickText = (selector, text) =>
  evaluate(
    `(() => { const el = [...document.querySelectorAll(${JSON.stringify(selector)})].find((b) => b.textContent.trim() === ${JSON.stringify(text)}); el?.click(); return !!el; })()`,
  );
const hostsNamed = (name) =>
  evaluate(`__hopr.hosts().filter((h) => h.label === ${JSON.stringify(name)})`);
await closeAllTabs();

const stamp = Date.now();
const base = `e2e3d-${stamp}`;
const hostA = await evaluate(`__hopr.saveHost(${hostInput(base, "hopr")})`);
await waitFor(`!!document.querySelector('[aria-label="More actions for ${base}"]')`, 5000);
await click(`[aria-label="More actions for ${base}"]`);
await clickText('[role="menuitem"]', "Duplicate");
const dupWait = await waitFor(`__hopr.hosts().some((h) => h.label === "${base} (copy)")`, 5000);
const [copy] = await hostsNamed(`${base} (copy)`);
check("row menu duplicates a host (settings and password)", dupWait >= 0 && !!copy);

await click(`input[aria-label="Select ${base}"]`);
await click(`input[aria-label="Select ${base} (copy)"]`);
await evaluate(`(() => { const el = document.querySelector('input[aria-label="Add tag"]');
  el.value = "bulk3d"; el.dispatchEvent(new Event("input", { bubbles: true })); el.form.requestSubmit(); })()`);
const tagged = await waitFor(
  `__hopr.hosts().filter((h) => h.label.startsWith("${base}") && h.tags.includes("bulk3d")).length === 2`,
  5000,
);
await clickText('[role="toolbar"] button', "Clear");
check("multi-select + bulk bar tags several hosts at once", tagged >= 0);

const exported = await evaluate(
  `__hopr.exportHosts("ssh_config", [${JSON.stringify(hostA.id)}, ${JSON.stringify(copy?.id)}])`,
);
check(
  "export writes an ssh_config without secrets",
  !!exported &&
    exported.includes(`Host ${base}\n`) &&
    exported.includes(`Host ${base}-copy\n`) &&
    !/\bhopr\b/.test(exported.replaceAll("Hopr", "")),
);

const closed = await evaluate(`__hopr.saveHost(${hostInput(`${base}-closed`, null, 1)})`);
await evaluate(`__hopr.ping([${JSON.stringify(hostA.id)}, ${JSON.stringify(closed.id)}])`);
const reachA = await evaluate(`__hopr.reach(${JSON.stringify(hostA.id)})`);
const reachClosed = await evaluate(`__hopr.reach(${JSON.stringify(closed.id)})`);
const dot = await evaluate(
  `document.querySelector('[data-host-id="${hostA.id}"] .dot')?.dataset.reach`,
);
check(
  "ping marks reachable and unreachable hosts",
  reachA?.status === "reachable" && reachClosed?.status === "unreachable" && dot === "reachable",
  `${reachA?.ms} ms`,
);

// Proxy: "via-proxy.test" only resolves through the dev proxy, so reaching it proves the proxy.
const proxyInput = (label, proxy) => {
  const o = JSON.parse(hostInput(label, "hopr"));
  o.hostname = "via-proxy.test";
  o.proxy = proxy;
  return JSON.stringify(o);
};
const direct = await evaluate(`__hopr.saveHost(${proxyInput(`${base}-noproxy`, null)})`);
await evaluate(`__hopr.connectHost(${JSON.stringify(direct.id)})`);
const directFail = await waitFor(`__hopr.state()?.kind === "failed"`, 20000);
await evaluate("__hopr.closeActive()");
const viaSocks = await evaluate(
  `__hopr.saveHost(${proxyInput(`${base}-socks`, { protocol: "socks5", host: "127.0.0.1", port: 2280, username: null })})`,
);
await evaluate(`__hopr.connectHost(${JSON.stringify(viaSocks.id)})`);
const socksRun = await readyTrusting(20000);
await evaluate("__hopr.closeActive()");
const viaHttp = await evaluate(
  `__hopr.saveHost(${proxyInput(`${base}-http`, { protocol: "http", host: "127.0.0.1", port: 2280, username: null })})`,
);
await evaluate(`__hopr.connectHost(${JSON.stringify(viaHttp.id)})`);
const httpRun = await readyTrusting(20000);
await evaluate("__hopr.closeActive()");
check(
  "SOCKS5 and HTTP proxies reach a host only the proxy can resolve",
  directFail >= 0 && socksRun.ms >= 0 && httpRun.ms >= 0,
  `socks ${socksRun.ms} ms, http ${httpRun.ms} ms`,
);
await closeAllTabs();

// Deploy key through the UI, then log in with it.
const keyLabel = `e2e-deploy-${stamp}`;
const keyId = await evaluate(`__hopr.generateKey(${JSON.stringify(keyLabel)})`);
await waitFor(`!!document.querySelector('[aria-label="More actions for ${base}"]')`, 5000);
await click(`[aria-label="More actions for ${base}"]`);
await clickText('[role="menuitem"]', "Deploy key…");
await waitFor(`!!document.querySelector('select[aria-label="Key to deploy"]')`, 3000);
await evaluate(`(() => { const s = document.querySelector('select[aria-label="Key to deploy"]');
  s.value = ${JSON.stringify(keyId)}; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
await clickText("button", "Deploy");
const deployed = await waitFor(
  `(document.querySelector('[aria-label="Deploy key"] [role="status"]')?.textContent ?? "").includes("now logs in with this key")`,
  20000,
);
const deployMsg = await evaluate(
  `document.querySelector('[aria-label="Deploy key"] [role="alert"], [aria-label="Deploy key"] [role="status"]')?.textContent?.trim()`,
);
await clickText("button", "Done");
await evaluate("__hopr.refreshLibrary()");
const authNow = await evaluate(`__hopr.host(${JSON.stringify(hostA.id)})?.auth`);
await evaluate(`__hopr.connectHost(${JSON.stringify(hostA.id)})`);
const keyRun = await readyTrusting(20000);
await evaluate("__hopr.closeActive()");
check(
  "deploy key adds it once, switches the host, and key login works",
  deployed >= 0 && authNow === "key" && keyRun.ms >= 0,
  deployMsg,
);

// Copy password: lands on the OS clipboard, then is cleared.
const clipboard = () => {
  try {
    return execSync('powershell -NoProfile -Command "Get-Clipboard"', { encoding: "utf8" }).trim();
  } catch {
    return "";
  }
};
await evaluate("__hopr.setClipboardClear(2)");
await closeAllTabs();
await waitFor(`!!document.querySelector('[aria-label="More actions for ${base} (copy)"]')`, 5000);
await click(`[aria-label="More actions for ${base} (copy)"]`);
await clickText('[role="menuitem"]', "Copy password");
await sleep(500);
const copiedPw = clipboard();
await sleep(3000);
const afterClear = clipboard();
await evaluate("__hopr.setClipboardClear(30)");
check(
  "copy password reaches the clipboard and is cleared after the delay",
  copiedPw === "hopr" && afterClear === "",
);

const audit = await evaluate("__hopr.audit(200)");
const kinds = new Set(audit.map((e) => e.kind));
const want = [
  "vault.unlock",
  "session.connect",
  "host_key.trust",
  "hosts.export",
  "key.deploy",
  "host.copy_password",
  "vault.lock",
];
const missing = want.filter((k) => !kinds.has(k));
check(
  "audit log records unlocks, connects, host keys, exports, deploys and copies",
  missing.length === 0,
  missing.length ? `missing ${missing.join(", ")}` : `${audit.length} events`,
);
const leaked = audit.some((e) => /\bhopr\b|hopr-e2e-master/.test(e.detail));
check("audit entries never contain a secret", !leaked);

// 14. Platform (3e): workspaces, tear-off without reconnecting, quick terminal + global
//     hotkey, close to tray, backups, Windows Hello gating, signed updates.
const invoke = (cmd, args = {}) =>
  evaluate(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}).then(
      (v) => ({ ok: true, v }), (e) => ({ ok: false, e }))`,
  );
/** Connect to another Hopr window's web view by its label. */
const openPage = async (label, timeoutMs = 15000) => {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const pages = await (await fetch(`${CDP}/json`)).json();
    for (const p of pages.filter((x) => x.type === "page" && x.id !== page.id)) {
      const c = await connect(p.webSocketDebuggerUrl);
      const ev = async (expr) => {
        const r = await c.send("Runtime.evaluate", {
          expression: expr,
          awaitPromise: true,
          returnByValue: true,
        });
        return r.result?.result?.value;
      };
      const l = await ev(
        "window.__hopr?.label ?? window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label",
      );
      if (typeof l === "string" && (label instanceof RegExp ? label.test(l) : l === label)) {
        return { ...c, ev, id: p.id, label: l };
      }
      c.ws.close();
    }
    await sleep(300);
  }
  return null;
};
const pageGone = async (id, timeoutMs = 8000) => {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const pages = await (await fetch(`${CDP}/json`)).json();
    if (!pages.some((p) => p.id === id)) return true;
    await sleep(200);
  }
  return false;
};
await closeAllTabs();

// Local shell (ConPTY): it must draw its prompt and run a command.
await evaluate("__hopr.openLocal()");
await waitFor("(__hopr.bytesIn().at(-1) ?? 0) > 200", 30000);
await evaluate(`__hopr.type("echo local-" + "ok" + String.fromCharCode(13))`);
const localOk = await waitFor(`__hopr.serialize().includes("local-ok")`, 15000);
check("local shell draws its prompt and runs a command", localOk >= 0);
await evaluate("__hopr.closeAll()");

// Workspaces: two tabs, one split; save, close everything, reopen by name; autosave.
const wsHost = await evaluate(`__hopr.saveHost(${hostInput(`e2e-ws-${stamp}`, "hopr")})`);
await evaluate(`__hopr.connectHost(${JSON.stringify(wsHost.id)})`);
await readyTrusting(20000);
await evaluate(`__hopr.split("row")`);
await readyTrusting(20000);
await evaluate("__hopr.openLocal()");
await waitFor("__hopr.tabs() === 2", 5000);
const savedWs = await evaluate(`__hopr.saveWorkspace("e2e-ws-${stamp}")`);
await sleep(2200); // autosave debounce
const autosaved = (await evaluate("__hopr.workspaces()")).find((w) => w.autosave);
await evaluate("__hopr.closeAll()");
await waitFor("__hopr.tabs() === 0", 5000);
const reopened = await evaluate(`__hopr.openWorkspace("e2e-ws-${stamp}")`);
await waitFor("__hopr.tabs() === 2", 5000);
const panesBack = await evaluate("__hopr.sessionIds().length");
const wsReady = await readyTrusting(20000);
check(
  "workspace saves tabs + splits and reopens them; last session autosaves",
  savedWs &&
    reopened === 2 &&
    panesBack === 3 &&
    wsReady.ms >= 0 &&
    !!autosaved?.layoutJson.includes(wsHost.id),
  `${reopened} tabs, ${panesBack} panes`,
);
await evaluate("__hopr.closeAll()");

// Tear-off: the session moves to a new window with its scrollback, without reconnecting.
await evaluate(`__hopr.connectHost(${JSON.stringify(wsHost.id)})`);
await readyTrusting(20000);
await waitFor(`/PS .*>/.test(__hopr.screen())`, 20000);
await evaluate(`__hopr.type('Write-Output ("tear-" + "marker")' + String.fromCharCode(13))`);
await waitFor(`__hopr.screen().includes("tear-marker")`, 10000);
const idBefore = (await evaluate("__hopr.sessionIds()"))[0];
const connectsBefore = (await evaluate("__hopr.audit(500)")).filter(
  (e) => e.kind === "session.connect",
).length;
await evaluate("__hopr.tearOff()");
const torn = await openPage(/^tab-/);
const mainTabs = await evaluate("__hopr.tabs()");
let tornOk = false;
let tornDetail = "no window";
if (torn) {
  const has = async (expr, ms) => {
    const start = Date.now();
    while (Date.now() - start < ms) {
      if (await torn.ev(expr)) return true;
      await sleep(150);
    }
    return false;
  };
  const adopted = await has("window.__hopr?.tabs() === 1", 10000);
  const replayed = await has(`__hopr.screen().includes("tear-marker")`, 5000);
  const idAfter = (await torn.ev("__hopr.sessionIds()"))?.[0];
  await torn.ev(`__hopr.type('Write-Output ("after-" + "move")' + String.fromCharCode(13))`);
  const live = await has(`__hopr.screen().includes("after-move")`, 10000);
  const connectsAfter = (await evaluate("__hopr.audit(500)")).filter(
    (e) => e.kind === "session.connect",
  ).length;
  tornOk =
    adopted &&
    replayed &&
    live &&
    idAfter === idBefore &&
    mainTabs === 0 &&
    connectsAfter === connectsBefore;
  tornDetail = `same session ${idAfter === idBefore}, scrollback ${replayed}, live ${live}, reconnects ${connectsAfter - connectsBefore}`;
  await torn.ev("__hopr.closeActive()");
  const closedWin = await pageGone(torn.id);
  tornOk &&= closedWin;
  torn.ws.close();
}
check(
  "tear-off moves a live tab to a new window (scrollback kept, no reconnect)",
  tornOk,
  tornDetail,
);

// Quick terminal: the real system-wide hotkey (sent through Windows, not the web view).
const sendHotkey = () =>
  execSync(
    `powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^{\`}')"`,
  );
const visible = async (label) => (await invoke("window_visible", { label })).v === true;
const quickBefore = await visible("quick");
sendHotkey();
let quickShown = false;
for (let i = 0; i < 40 && !quickShown; i++) {
  await sleep(250);
  quickShown = await visible("quick");
}
const quick = quickShown ? await openPage("quick") : null;
let quickShell = false;
if (quick) {
  // Any prompt style: wait until the shell has drawn and gone quiet (prompt frameworks
  // like oh-my-posh take a few seconds and can drop input typed before they are ready).
  let last = -1;
  let quiet = 0;
  for (let i = 0; i < 160 && quiet < 6; i++) {
    const n = (await quick.ev("window.__hopr?.bytesIn?.()?.[0] ?? 0")) ?? 0;
    quiet = n > 200 && n === last ? quiet + 1 : 0;
    last = n;
    await sleep(250);
  }
  for (let attempt = 0; attempt < 2 && !quickShell; attempt++) {
    await quick.ev(`__hopr.type("echo quick-" + "ok" + String.fromCharCode(13))`);
    for (let i = 0; i < 40 && !quickShell; i++) {
      quickShell = !!(await quick.ev(`(window.__hopr?.serialize?.() ?? "").includes("quick-ok")`));
      await sleep(250);
    }
  }
}
sendHotkey();
let quickHidden = false;
for (let attempt = 0; attempt < 2 && !quickHidden; attempt++) {
  // One retry: a press that lands while the app is busy can be missed.
  if (attempt > 0) sendHotkey();
  for (let i = 0; i < 16 && !quickHidden; i++) {
    await sleep(250);
    quickHidden = !(await visible("quick"));
  }
}
quick?.ws.close();
check(
  "Ctrl+` (global hotkey) slides the quick terminal down with a local shell, and back",
  !quickBefore && quickShown && quickShell && quickHidden,
  `was hidden ${!quickBefore}, shown ${quickShown}, shell ${quickShell}, hidden again ${quickHidden}`,
);

// Close to tray: closing the main window hides it; "Show Hopr" brings it back.
await evaluate(
  "window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' }).catch(() => {})",
);
await sleep(800);
const hiddenToTray = !(await visible("main"));
await invoke("main_show");
await sleep(500);
const shownAgain = await visible("main");
check("closing the window keeps Hopr in the tray; it comes back", hiddenToTray && shownAgain);

// Backups: an automatic one exists after unlock; back up now; restore needs that password.
const list1 = await invoke("backups_list");
const auto = list1.ok && list1.v.some((b) => b.automatic);
const made = await invoke("backup_now");
const wrongPw = await invoke("backup_restore", {
  name: made.v?.name,
  bytes: null,
  password: "nope",
});
const rightPw = await invoke("backup_restore", {
  name: made.v?.name,
  bytes: null,
  password: VAULT_PW,
});
const pending = (await invoke("backup_restore_pending")).v;
check(
  "daily backup on unlock, backup now, restore checks the master password and stages",
  auto && made.ok && !wrongPw.ok && rightPw.ok && pending === true,
  made.v?.name,
);

// Windows Hello: the stored key never unlocks unless the check passed.
const st = (await invoke("vault_status")).v;
let helloOk;
let helloDetail;
if (st.biometricAvailable) {
  const on = await invoke("vault_set_biometric", { enabled: true });
  await evaluate("__hopr.lock()");
  const sneaky = await invoke("vault_unlock_remembered");
  await evaluate(`__hopr.unlock(${JSON.stringify(VAULT_PW)})`);
  await invoke("vault_set_biometric", { enabled: false });
  helloOk = on.ok && !sneaky.ok;
  helloDetail = "available; remembered key refused while Hello is on";
} else {
  const on = await invoke("vault_set_biometric", { enabled: true });
  helloOk = !on.ok && String(on.e?.message ?? "").includes("Windows Hello");
  helloDetail = "not set up on this machine; enabling explains why";
}
check("Windows Hello gates the stored vault key", helloOk, helloDetail);

// Signed updates: a correctly signed payload downloads; a tampered one is rejected.
const keyPath = `${process.env.USERPROFILE}\\.tauri\\hopr-updater.key`;
let updOk = false;
let updDetail = "skipped (no signing key or HOPR_E2E_UPDATE_PORT)";
if (existsSync(keyPath) && process.env.HOPR_E2E_UPDATE_PORT) {
  const port = Number(process.env.HOPR_E2E_UPDATE_PORT);
  const dir = join(tmpdir(), `hopr-upd-${stamp}`);
  mkdirSync(dir, { recursive: true });
  const good = join(dir, "hopr-9.9.9-setup.exe");
  writeFileSync(good, randomBytes(64 * 1024));
  execSync(`pnpm -s tauri signer sign -f "${keyPath}" -p "" "${good}"`, { stdio: "ignore" });
  const sig = readFileSync(`${good}.sig`, "utf8").trim();
  const bad = Buffer.from(readFileSync(good));
  bad[100] ^= 0xff;
  let serveBad = false;
  const server = createServer((req, res) => {
    if (req.url === "/latest.json") {
      const url = `http://127.0.0.1:${port}/${serveBad ? "bad" : "good"}.exe`;
      res.setHeader("content-type", "application/json");
      res.end(
        JSON.stringify({
          version: "9.9.9",
          notes: "e2e",
          pub_date: new Date().toISOString(),
          platforms: {
            "windows-x86_64": { signature: sig, url },
            "windows-x86_64-nsis": { signature: sig, url },
          },
        }),
      );
    } else if (req.url === "/good.exe") res.end(readFileSync(good));
    else if (req.url === "/bad.exe") res.end(bad);
    else {
      res.statusCode = 404;
      res.end();
    }
  });
  await new Promise((r) => server.listen(port, "127.0.0.1", r));
  const found = await invoke("update_check");
  const okDl = await invoke("update_download");
  serveBad = true;
  await invoke("update_check");
  const badDl = await invoke("update_download");
  server.close();
  updOk = found.ok && found.v?.version === "9.9.9" && okDl.ok && !badDl.ok;
  updDetail = `found ${found.v?.version}, signed ok ${okDl.ok}, tampered rejected ${!badDl.ok}`;
}
check("updates: signed payload accepted, tampered payload rejected", updOk, updDetail);

// 15. Files (4a): SFTP drawer, file operations, preview, permissions, transfers with
//     pause/resume, and edit locally with a conflict. Checked against the dev server's
//     sandbox folder on disk (HOPR_DEVSSHD_HOME).
const sandbox = process.env.HOPR_DEVSSHD_HOME ?? "D:\\hopr-tmp\\devhome";
const sbox = (p) => join(sandbox, ...p.split("/").filter(Boolean));
const area = `sftp-${stamp}`;
mkdirSync(sbox(area), { recursive: true });
writeFileSync(sbox(`${area}/notes.txt`), "hello from the server\nline two\n");
await closeAllTabs();
const filesHost = await evaluate(`__hopr.saveHost(${hostInput(`e2e-files-${stamp}`, "hopr")})`);
await waitFor(
  `!!document.querySelector('[aria-label="More actions for e2e-files-${stamp}"]')`,
  5000,
);
await click(`[aria-label="More actions for e2e-files-${stamp}"]`);
await clickText('[role="menuitem"]', "Browse files");
const rowSel = (name) => `[aria-label="Files"] [role="option"][data-path$="/${name}"]`;
const drawerUp = await waitFor(`!!document.querySelector('${rowSel(area)}')`, 20000);
await evaluate(
  `document.querySelector('${rowSel(area)}').dispatchEvent(new MouseEvent("dblclick", { bubbles: true }))`,
);
const entered = await waitFor(`!!document.querySelector('${rowSel("notes.txt")}')`, 10000);
check(
  "files drawer opens a host over SFTP and browses into a folder",
  drawerUp >= 0 && entered >= 0,
);

// New folder through the UI.
await click('[aria-label="New folder"]');
await evaluate(`(() => { const i = document.querySelector('[aria-label="Files"] input[aria-label="Name"]');
  i.value = "made-here"; i.dispatchEvent(new Event("input", { bubbles: true })); i.form.requestSubmit(); })()`);
const madeUi = await waitFor(`!!document.querySelector('${rowSel("made-here")}')`, 10000);
check(
  "new folder appears in the list and on the server",
  madeUi >= 0 && existsSync(sbox(`${area}/made-here`)),
);

// Preview.
await click(`[aria-label="Actions for notes.txt"]`);
await clickText('[role="menuitem"]', "Preview");
const previewed = await waitFor(
  `(document.querySelector('[aria-label="Preview"] pre')?.textContent ?? "").includes("hello from the server")`,
  10000,
);
await clickText('[aria-label="Preview"] button', "Close");
check("text preview shows the file's content", previewed >= 0);

// Permissions through the dialog.
await click(`[aria-label="Actions for notes.txt"]`);
await clickText('[role="menuitem"]', "Permissions…");
await waitFor(`!!document.querySelector('[aria-label="Permissions"]')`, 3000);
await evaluate(`(() => { const i = document.querySelector('[aria-label="Octal permissions"]');
  i.value = "640"; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
await evaluate(
  `[...document.querySelectorAll('[aria-label="Permissions"] button')].find((b) => b.textContent.includes("Apply"))?.click()`,
);
const modeOk = await waitFor(
  `document.querySelector('${rowSel("notes.txt")} .col-mode')?.textContent === "rw-r-----"`,
  10000,
);
check("permissions dialog changes the mode (640 → rw-r-----)", modeOk >= 0);

// Upload and download, compared byte for byte.
const hid = filesHost.id;
const tmp = join(tmpdir(), `hopr-e2e-files-${stamp}`);
mkdirSync(tmp, { recursive: true });
const body = randomBytes(6 * 1024 * 1024);
writeFileSync(join(tmp, "upload.bin"), body);
await invoke("transfers_upload", {
  policy: "overwrite",
  hostId: hid,
  localPaths: [join(tmp, "upload.bin")],
  remoteDir: `/${area}`,
});
const upDone = await waitFor(
  `__hopr.transfers().some((t) => t.name === "upload.bin" && t.status === "done")`,
  60000,
);
const upSame = upDone >= 0 && Buffer.compare(readFileSync(sbox(`${area}/upload.bin`)), body) === 0;
await invoke("transfers_download", {
  policy: "overwrite",
  hostId: hid,
  remotePaths: [`/${area}/upload.bin`],
  localDir: join(tmp, "dl"),
});
const dlDone = await waitFor(
  `__hopr.transfers().some((t) => t.direction === "download" && t.name === "upload.bin" && t.status === "done")`,
  60000,
);
const dlSame =
  dlDone >= 0 && Buffer.compare(readFileSync(join(tmp, "dl", "upload.bin")), body) === 0;
const pill = await evaluate(
  `document.querySelector('button[aria-label="Transfers"]')?.textContent?.trim()`,
);
check(
  "upload and download arrive byte-identical; the transfer pill reports them",
  upSame && dlSame && !!pill,
  pill,
);

// Pause and resume a big transfer: it continues from where it stopped.
const big = randomBytes(96 * 1024 * 1024);
writeFileSync(sbox(`${area}/big.bin`), big);
await invoke("transfers_download", {
  policy: "overwrite",
  hostId: hid,
  remotePaths: [`/${area}/big.bin`],
  localDir: join(tmp, "big"),
});
await waitFor(
  `__hopr.transfers().some((t) => t.name === "big.bin" && t.status === "running" && t.done > 0)`,
  30000,
);
const bigId = await evaluate(`__hopr.transfers().find((t) => t.name === "big.bin")?.id`);
await invoke("transfer_pause", { id: bigId });
const paused = await waitFor(
  `__hopr.transfers().find((t) => t.id === "${bigId}")?.status === "paused"`,
  10000,
);
const doneAtPause = await evaluate(`__hopr.transfers().find((t) => t.id === "${bigId}")?.done`);
await invoke("transfer_resume", { id: bigId });
const bigDone = await waitFor(
  `__hopr.transfers().find((t) => t.id === "${bigId}")?.status === "done"`,
  120000,
);
const bigSame =
  bigDone >= 0 && Buffer.compare(readFileSync(join(tmp, "big", "big.bin")), big) === 0;
check(
  "pause keeps the partial file and resume finishes it byte-identical",
  paused >= 0 && doneAtPause > 0 && doneAtPause < big.length && bigSame,
  `paused at ${Math.round((doneAtPause ?? 0) / 1024 / 1024)} MB`,
);

// Edit locally: saving the local copy uploads it; a server-side change raises a conflict.
const readUntil = async (path, want) => {
  for (let i = 0; i < 40; i++) {
    if (readFileSync(path, "utf8") === want) return true;
    await sleep(250);
  }
  return false;
};
const edit = await invoke("sftp_edit", { hostId: hid, path: `/${area}/notes.txt`, open: false });
const editPath = (await invoke("sftp_edit_path", { editId: edit.v })).v;
await sleep(1200);
writeFileSync(editPath, "edited locally\n");
const editSaved = await readUntil(sbox(`${area}/notes.txt`), "edited locally\n");
await sleep(1500);
writeFileSync(sbox(`${area}/notes.txt`), "changed on the server\n");
await sleep(1200);
writeFileSync(editPath, "my second edit\n");
const conflict = await waitFor(`!!document.querySelector('[aria-label="Edit conflict"]')`, 15000);
await clickText('[aria-label="Edit conflict"] button', "Upload mine");
const overwritten = await readUntil(sbox(`${area}/notes.txt`), "my second edit\n");
await invoke("sftp_edit_stop", { editId: edit.v });
check(
  "edit locally uploads on save and asks before overwriting a server change",
  editSaved && conflict >= 0 && overwritten,
);

// Delete through the UI (with confirmation).
await click(`[aria-label="Actions for made-here"]`);
await clickText('[role="menuitem"]', "Delete");
await clickText('[aria-label="Confirm delete"] button', "Delete");
const deleted = await waitFor(`!document.querySelector('${rowSel("made-here")}')`, 10000);
check(
  "delete asks first, then removes from the list and the server",
  deleted >= 0 && !existsSync(sbox(`${area}/made-here`)),
);
await invoke("sftp_close", { hostId: hid });

// 16. Dual pane (4b): drag between local and remote panes, "already exists" choices,
//     speed limit and concurrency settings.
const localDir = join(tmp, "dual");
mkdirSync(localDir, { recursive: true });
writeFileSync(join(localDir, "drag.txt"), "dragged across\n");
await evaluate("__hopr.setDual(true)");
await waitFor(`!!document.querySelector('[aria-label="Dual pane"]')`, 5000);
await evaluate(`(() => { const i = document.querySelector('[aria-label="Local path"]');
  i.value = ${JSON.stringify(localDir)}; i.dispatchEvent(new Event("input", { bubbles: true })); i.form.requestSubmit(); })()`);
await waitFor(`!!document.querySelector('[data-local="drag.txt"]')`, 5000);
await evaluate(`__hopr.filesOpen(${JSON.stringify(hid)})`);
await evaluate(`__hopr.filesGo("/${area}")`);
await waitFor(`!!document.querySelector('[data-remote="notes.txt"]')`, 10000);
/** Drag a row onto a pane with real DragEvents (the same handlers a mouse drag runs). */
const dragTo = (rowSel2, paneLabel) =>
  evaluate(`(() => {
    const dt = new DataTransfer();
    const row = document.querySelector(${JSON.stringify(rowSel2)});
    const pane = document.querySelector('[aria-label=${JSON.stringify(paneLabel)}]');
    row.dispatchEvent(new DragEvent("dragstart", { bubbles: true, dataTransfer: dt }));
    pane.dispatchEvent(new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt }));
    pane.dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: dt }));
    return !!row && !!pane;
  })()`);
const countOf = (name) =>
  evaluate(`__hopr.transfers().filter((t) => t.name === ${JSON.stringify(name)}).length`);
const waitFile = async (path, want) => {
  for (let i = 0; i < 60; i++) {
    if (existsSync(path) && (want === undefined || readFileSync(path, "utf8") === want))
      return true;
    await sleep(250);
  }
  return false;
};
await dragTo('[data-local="drag.txt"]', "Remote files");
const dragged = await waitFor(
  `__hopr.transfers().some((t) => t.name === "drag.txt" && t.status === "done")`,
  20000,
);
check(
  "dragging a local file onto the remote pane uploads it",
  dragged >= 0 && existsSync(sbox(`${area}/drag.txt`)),
);

// Same file again: asked once; Skip queues nothing, Replace overwrites.
const countBefore = await countOf("drag.txt");
await dragTo('[data-local="drag.txt"]', "Remote files");
const clashAsked = await waitFor(`(__hopr.clash() ?? []).includes("drag.txt")`, 10000);
await clickText('[aria-label="Already exists"] button', "Skip");
await sleep(800);
const afterSkip = await countOf("drag.txt");
writeFileSync(join(localDir, "drag.txt"), "replaced version\n");
await dragTo('[data-local="drag.txt"]', "Remote files");
await waitFor(`(__hopr.clash() ?? []).includes("drag.txt")`, 10000);
await clickText('[aria-label="Already exists"] button', "Replace");
const replaced = await waitFile(sbox(`${area}/drag.txt`), "replaced version\n");
check(
  "an existing target asks once: Skip queues nothing, Replace overwrites",
  clashAsked >= 0 && afterSkip === countBefore && replaced,
);

// Remote → local by dragging the other way.
await dragTo('[data-remote="notes.txt"]', "Local files");
const pulled = await waitFile(join(localDir, "notes.txt"));
const listed = await waitFor(`!!document.querySelector('[data-local="notes.txt"]')`, 5000);
check("dragging a remote file onto the local pane downloads it there", pulled && listed >= 0);

// Speed limit (shared by all transfers) and concurrency are applied and remembered.
await invoke("transfers_set_settings", { settings: { concurrency: 2, limitKbps: 1024 } });
const slowFile = join(tmp, "slow.bin");
writeFileSync(slowFile, randomBytes(2 * 1024 * 1024));
const t0 = Date.now();
await invoke("transfers_upload", {
  hostId: hid,
  localPaths: [slowFile],
  remoteDir: `/${area}`,
  policy: "overwrite",
});
await waitFor(
  `__hopr.transfers().some((t) => t.name === "slow.bin" && t.status === "done")`,
  30000,
);
const slowSecs = (Date.now() - t0) / 1000;
const kept = (await invoke("transfers_settings")).v;
await invoke("transfers_set_settings", { settings: { concurrency: 3, limitKbps: 0 } });
check(
  "speed limit slows transfers (2 MB at 1 MB/s) and settings are kept",
  slowSecs >= 1.5 && kept?.concurrency === 2 && kept?.limitKbps === 1024,
  `${slowSecs.toFixed(1)} s`,
);
await evaluate("__hopr.setDual(false)");

// 17. Terminal history (4c): typed (approximate) history, shell integration (exit codes,
//     cwd, prompt marks, exit markers), files drawer following the terminal's folder,
//     recording + replay, connection history, exclusions.
await closeAllTabs();
VK.O = 79;
const histArea = `hist-${stamp}`;
mkdirSync(sbox(histArea), { recursive: true });
writeFileSync(sbox(`${histArea}/here.txt`), "followed\n");
await invoke("history_set_settings", {
  settings: {
    enabled: true,
    ignoreSpace: true,
    exclude: ["*mypassword*"],
    recordAll: true,
    notifyAfterSecs: 10,
  },
});
const histLabel = `e2e-hist-${stamp}`;
const histHost = await evaluate(`__hopr.saveHost(${hostInput(histLabel, "hopr")})`);
await evaluate(`__hopr.connectHost(${JSON.stringify(histHost.id)})`);
await readyTrusting(20000);
/** Wait until the shell stops printing (the prompt is drawn). */
const settle = async (quietMs = 1500, maxMs = 25000) => {
  const t0 = Date.now();
  let last = -1;
  let since = Date.now();
  while (Date.now() - t0 < maxMs) {
    const b = (await evaluate("__hopr.bytesIn()")).reduce((a, x) => a + x, 0);
    if (b !== last) {
      last = b;
      since = Date.now();
    } else if (Date.now() - since >= quietMs && b > 0) return true;
    await sleep(200);
  }
  return false;
};
const focusTerm = () =>
  evaluate(
    `(() => { const t = [...document.querySelectorAll(".xterm-helper-textarea")].find((x) => x.closest(".xterm")?.offsetParent); t?.focus(); return !!t; })()`,
  );
/** Type a line with real input events, then Enter. */
const typeLine = async (text) => {
  await focusTerm();
  await send("Input.insertText", { text });
  await sleep(150);
  await press("Enter");
};
await settle();
const autoRec = (await evaluate("__hopr.sessionInfo()"))?.recording;
check("“Record every session” records a new session from the start", !!autoRec, autoRec ?? "");

// Without shell integration: read back from what was typed.
await typeLine(`echo typed-${stamp}`);
await settle(800);
const typedRows = await evaluate(`__hopr.history("commands", "typed-${stamp}")`);
check(
  "without shell integration, typed commands are saved (approximate)",
  typedRows.length === 1 &&
    typedRows[0].command === `echo typed-${stamp}` &&
    !typedRows[0].exact &&
    typedRows[0].hostLabel === histLabel,
  typedRows[0]?.command ?? "none",
);

// Enable integration from Settings → History → Shell integration → Enable here.
await click('nav[aria-label="Primary"] [aria-label="Settings"]');
await waitFor(`!!document.querySelector('[aria-label="Shell"]')`, 5000);
await evaluate(`(() => { const s = document.querySelector('[aria-label="Shell"]');
  s.value = "pwsh"; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
await sleep(200);
const enableClicked = await clickText("button", "Enable here");
await press("Escape");
const integrated = await waitFor(`__hopr.sessionInfo()?.integrated === true`, 20000);
await settle();
const info = await evaluate("__hopr.sessionInfo()");
check(
  "Enable here turns on shell integration (prompt marks, working folder)",
  enableClicked && integrated >= 0 && !!info?.cwd,
  info?.cwd ?? "",
);

await typeLine("cmd /c exit 3");
const failedCmd = await waitFor(
  `__hopr.sessionInfo()?.lastCommand?.command === "cmd /c exit 3" && __hopr.sessionInfo()?.lastCommand?.exit === 3`,
  15000,
);
await settle(800);
await typeLine(`echo exact-${stamp}`);
const okCmd = await waitFor(
  `__hopr.sessionInfo()?.lastCommand?.command === "echo exact-${stamp}" && __hopr.sessionInfo()?.lastCommand?.exit === 0`,
  15000,
);
await settle(800);
const marks = await evaluate("__hopr.exitMarks()");
check(
  "commands report exit codes and get exit markers on their prompt",
  failedCmd >= 0 && okCmd >= 0 && marks.includes("3") && marks.includes("0"),
  `markers: ${marks.join(",")}`,
);
const exactRows = await evaluate(`__hopr.history("commands", "exact-${stamp}")`);
check(
  "integrated commands are saved exactly, with exit code and folder",
  exactRows.length === 1 && exactRows[0].exact && exactRows[0].exitCode === 0 && !!exactRows[0].cwd,
  exactRows[0]?.cwd ?? "none",
);

// Prompt jumping: scroll up to an earlier prompt and back down.
await typeLine('1..120 | % { "filler $_" }');
await settle(800);
const scrollTop = async () => (await evaluate("__hopr.sessionInfo()"))?.viewportY ?? -1;
const bottom = await scrollTop();
await evaluate("__hopr.jumpPrompt(-1)");
await sleep(300);
const up = await scrollTop();
for (let i = 0; i < 10; i++) await evaluate("__hopr.jumpPrompt(1)");
await sleep(300);
const back = await scrollTop();
check(
  "prompt jumping scrolls to the previous prompt and back",
  up >= 0 && up < bottom && back === bottom,
  `${bottom} → ${up} → ${back}`,
);

// Exclusions: a matching command is never saved.
await typeLine("echo mypassword=1");
await settle(800);
const excluded = await evaluate(`__hopr.history("commands", "mypassword")`);
check("commands matching an exclude pattern are not saved", excluded.length === 0);

// Files follow the terminal's folder: while a command reports a folder (OSC 7) and keeps
// running, Ctrl+Shift+O opens the drawer there. Its run time is saved too.
await typeLine(
  `Write-Host -NoNewline "$([char]27)]7;file://dev/${histArea}$([char]7)"; Start-Sleep 6`,
);
await waitFor(`__hopr.sessionInfo()?.cwd === "/${histArea}"`, 10000);
await focusTerm();
await press("O", ["ctrl", "shift"]);
const followed = await waitFor(
  `__hopr.files().cwd === "/${histArea}" && __hopr.files().names.includes("here.txt")`,
  15000,
);
check(
  "Ctrl+Shift+O opens files at the terminal's folder",
  followed >= 0,
  (await evaluate("__hopr.files()")).cwd,
);
await press("Escape");
const slept = await waitFor(
  `!!__hopr.sessionInfo()?.lastCommand?.command?.includes("Start-Sleep 6")`,
  15000,
);
const sleepRow = (await evaluate(`__hopr.history("commands", "Start-Sleep 6")`))[0];
check(
  "a command's run time is measured",
  slept >= 0 && (sleepRow?.durationMs ?? 0) >= 5500,
  `${sleepRow?.durationMs ?? 0} ms`,
);

// Stop recording; replay it: scrub to the end, search, close.
await settle(800);
const stopped = await evaluate("__hopr.toggleRecording()");
const recs = await evaluate(`__hopr.history("recordings")`);
const rec = recs.find((r) => r.name === autoRec);
check(
  "stopping keeps the recording, listed with its length",
  stopped === null &&
    (await evaluate("__hopr.sessionInfo()"))?.recording === null &&
    !!rec &&
    rec.duration > 3,
  rec ? `${rec.duration.toFixed(1)} s, ${rec.size} bytes` : "missing",
);
const raw = (await invoke("recording_read", { name: autoRec })).v ?? "";
const castHead = JSON.parse(raw.split("\n")[0] || "{}");
check(
  "the recording is asciicast v2 with output events only",
  castHead.version === 2 &&
    castHead.title === histLabel &&
    raw.includes('"o",') &&
    !raw.includes('"i",'),
);
await evaluate(`__hopr.replay(${JSON.stringify(autoRec)})`);
const playerUp = await waitFor(`!!__hopr.player() && __hopr.player().duration > 0`, 10000);
await evaluate(`(() => { const r = document.querySelector('[aria-label="Position"]');
  r.value = r.max; r.dispatchEvent(new Event("input", { bubbles: true })); })()`);
const atEnd = await waitFor(`__hopr.playerText().includes("exact-${stamp}")`, 5000);
await evaluate(`(() => { const i = document.querySelector('[aria-label="Search recording"]');
  i.value = "typed-${stamp}"; i.dispatchEvent(new Event("input", { bubbles: true })); i.form.requestSubmit(); })()`);
// The seek redraws asynchronously: wait for it instead of a fixed pause.
const found =
  (await waitFor(
    `(() => { const p = __hopr.player(); const t = __hopr.playerText(); return !!p && p.pos < p.duration && t.includes("typed-${stamp}") && !t.includes("filler 120"); })()`,
    5000,
  )) >= 0;
await press("Escape");
const replayGone = await waitFor(`!document.querySelector('[aria-label="Replay"]')`, 3000);
check(
  "replay scrubs to the end, finds text in the recording, and closes with Esc",
  playerUp >= 0 && atEnd >= 0 && found && replayGone >= 0,
  `opened ${playerUp >= 0}, end ${atEnd >= 0}, found ${found}, closed ${replayGone >= 0}`,
);

// Connection history: closing the tab ends the connection with its byte counts.
await evaluate("__hopr.closeActive()");
await sleep(800);
const conns = await evaluate(`__hopr.history("connections")`);
const conn = conns.find((c) => c.hostLabel === histLabel);
check(
  "connection history records the session with its end and traffic",
  !!conn && conn.endReason === "user_closed" && conn.bytesIn > 1000 && conn.bytesOut > 10,
  conn ? `${conn.bytesIn} in / ${conn.bytesOut} out` : "missing",
);
await invoke("history_set_settings", {
  settings: {
    enabled: true,
    ignoreSpace: true,
    exclude: [],
    recordAll: false,
    notifyAfterSecs: 10,
  },
});

// 18. Reach (4d): telnet host, SSH through a ProxyCommand, certificate login, importers
//     (PuTTY .reg, MobaXterm, CSV), serial port listing, and the German translation.
await closeAllTabs();
const telnetPort = Number(process.env.HOPR_DEVSSHD_TELNET_PORT ?? 2323);
const reachInput = (label, patch) =>
  JSON.stringify({ ...JSON.parse(hostInput(label, null)), ...patch });

// Telnet: negotiation, login prompt, window size reported over NAWS.
const tnLabel = `e2e-telnet-${stamp}`;
const tn = await evaluate(
  `__hopr.saveHost(${reachInput(tnLabel, { protocol: "telnet", port: telnetPort, username: "" })})`,
);
await evaluate(`__hopr.connectHost(${JSON.stringify(tn.id)})`);
const tnReady = await waitFor(`__hopr.state()?.kind === "ready"`, 15000);
const tnLogin = await waitFor(`__hopr.screen().includes("login:")`, 10000);
await evaluate(`__hopr.type("e2e\\r")`);
const tnWelcome = await waitFor(`__hopr.screen().includes("Welcome, e2e")`, 10000);
await evaluate(`__hopr.type("size\\r")`);
const tnSize = await waitFor(`/size \\d+x\\d+/.test(__hopr.screen())`, 10000);
const sizeText = (await evaluate("__hopr.screen()")).match(/size (\d+x\d+)/)?.[1] ?? "";
check(
  "telnet host connects, logs in at the prompt, and reports its window size",
  tnReady >= 0 && tnLogin >= 0 && tnWelcome >= 0 && tnSize >= 0 && sizeText !== "0x0",
  sizeText,
);
await evaluate(`__hopr.type("exit\\r")`);
const tnClosed = await waitFor(`__hopr.state()?.kind === "closed"`, 10000);
check("telnet host closing its end ends the session", tnClosed >= 0);
await closeAllTabs();

// SSH through a ProxyCommand: a PowerShell TCP relay that leaves a marker when it runs.
const relayDir = join(tmp, "proxycmd");
mkdirSync(relayDir, { recursive: true });
const relay = join(relayDir, "relay.ps1");
const relayMarker = join(relayDir, `ran-${stamp}`);
writeFileSync(
  relay,
  [
    "Set-Content -Path $args[2] -Value 1",
    "$c = New-Object Net.Sockets.TcpClient($args[0], [int]$args[1])",
    "$s = $c.GetStream()",
    "$i = [Console]::OpenStandardInput()",
    "$o = [Console]::OpenStandardOutput()",
    "$t = $i.CopyToAsync($s)",
    "$s.CopyTo($o)",
  ].join("\n"),
);
const pcLabel = `e2e-proxycmd-${stamp}`;
const pc = await evaluate(
  `__hopr.saveHost(${reachInput(pcLabel, {
    password: { action: "set", value: "hopr" },
    proxyCommand: `powershell -NoProfile -ExecutionPolicy Bypass -File "${relay}" %h %p "${relayMarker}"`,
  })})`,
);
await evaluate(`__hopr.connectHost(${JSON.stringify(pc.id)})`);
const pcReady = await readyTrusting(30000);
const viaAudit = (await evaluate("__hopr.audit(50)")).some(
  (e) => e.hostLabel === pcLabel && e.detail.includes("via ProxyCommand"),
);
check(
  "SSH through a ProxyCommand connects (the command ran; audit says so)",
  pcReady.ms >= 0 && existsSync(relayMarker) && viaAudit,
  `${pcReady.ms} ms`,
);
await closeAllTabs();

// Certificate login: a key the server doesn't know, signed by a CA it trusts.
const keygen = "C:\\Windows\\System32\\OpenSSH\\ssh-keygen.exe";
const certDir = join(tmp, `cert-${stamp}`);
mkdirSync(certDir, { recursive: true });
const caFile = join(certDir, "ca");
execSync(`"${keygen}" -q -t ed25519 -N "" -C e2e-ca -f "${caFile}"`);
mkdirSync(sbox(".ssh"), { recursive: true });
writeFileSync(sbox(".ssh/trusted_user_ca_keys"), readFileSync(`${caFile}.pub`, "utf8"));
const certKeyId = await evaluate(`__hopr.generateKey("e2e-cert-${stamp}")`);
const certKey = (await evaluate("__hopr.keys()")).find((k) => k.id === certKeyId);
const pubFile = join(certDir, "user.pub");
writeFileSync(pubFile, `${certKey.publicKey}\n`);
execSync(`"${keygen}" -q -s "${caFile}" -I e2e-${stamp} -n certuser -V -5m:+1h "${pubFile}"`);
const certText = readFileSync(join(certDir, "user-cert.pub"), "utf8");
const attached = await invoke("key_set_certificate", { id: certKeyId, certificate: certText });
const crtLabel = `e2e-cert-host-${stamp}`;
const crt = await evaluate(
  `__hopr.saveHost(${reachInput(crtLabel, { username: "certuser", auth: "key", keyId: certKeyId })})`,
);
await evaluate(`__hopr.connectHost(${JSON.stringify(crt.id)})`);
const crtReady = await readyTrusting(20000);
check(
  "a CA-signed certificate logs in where the bare key isn't authorized",
  attached.ok && attached.v?.principals?.includes("certuser") && crtReady.ms >= 0,
  attached.ok ? `CA ${attached.v.caFingerprint.slice(0, 20)}…` : JSON.stringify(attached.e),
);
await closeAllTabs();
const wrongKey = await invoke("key_set_certificate", {
  id: certKeyId,
  certificate: "ssh-ed25519 AAAA nope",
});
check(
  "a malformed certificate is refused with a clear error",
  !wrongKey.ok && !!wrongKey.e?.message,
);

// Importers: PuTTY (.reg in UTF-16, as regedit writes it), MobaXterm, CSV.
const regText = [
  "Windows Registry Editor Version 5.00",
  "",
  `[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\putty%20${stamp}]`,
  `"HostName"="dev@putty-${stamp}.test"`,
  `"PortNumber"=dword:000008ae`,
  `"Protocol"="ssh"`,
  "",
  `[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\console-${stamp}]`,
  `"Protocol"="serial"`,
  `"SerialLine"="COM9"`,
  `"SerialSpeed"=dword:0001c200`,
  "",
].join("\r\n");
const regFile = join(tmp, `putty-${stamp}.reg`);
writeFileSync(regFile, Buffer.concat([Buffer.from([0xff, 0xfe]), Buffer.from(regText, "utf16le")]));
const mobaFile = join(tmp, `moba-${stamp}.mxtsessions`);
writeFileSync(
  mobaFile,
  `[Bookmarks_1]\r\nSubRep=E2E ${stamp}\\Routers\r\nImgNum=41\r\nmoba-${stamp}= #98#1%localhost%${telnetPort}%%%2#MobaFont%10#0# #-1\r\nrdp-${stamp}= #91#4%10.0.0.9%3389%admin%0#MobaFont#0\r\n`,
);
const csvFile = join(tmp, `hosts-${stamp}.csv`);
writeFileSync(
  csvFile,
  `Label,Hostname,Port,User,Group\n"csv, ${stamp}",csv-${stamp}.test,2222,dev,CSV ${stamp}\n`,
);
const imports = [];
for (const [source, path, labels] of [
  ["putty", regFile, [`putty ${stamp}`, `console-${stamp}`]],
  ["mobaxterm", mobaFile, [`moba-${stamp}`]],
  ["csv", csvFile, [`csv, ${stamp}`]],
]) {
  const preview = await invoke("import_foreign_preview", { source, path });
  const applied = await invoke("import_foreign_apply", { source, path, labels });
  imports.push({
    source,
    found: preview.v?.hosts?.length ?? -1,
    warnings: preview.v?.warnings ?? [],
    created: applied.v?.hosts ?? -1,
  });
}
await evaluate("__hopr.refreshLibrary()");
const lib = (await invoke("library_get")).v;
const byLabel = (l) => lib.hosts.find((h) => h.label === l);
const importedOk =
  byLabel(`putty ${stamp}`)?.port === 2222 &&
  byLabel(`putty ${stamp}`)?.username === "dev" &&
  byLabel(`console-${stamp}`)?.protocol === "serial" &&
  byLabel(`console-${stamp}`)?.serial?.baud === 115200 &&
  byLabel(`moba-${stamp}`)?.protocol === "telnet" &&
  lib.groups.some((g) => g.name === `E2E ${stamp} / Routers`) &&
  byLabel(`csv, ${stamp}`)?.groupId === lib.groups.find((g) => g.name === `CSV ${stamp}`)?.id &&
  imports[1].warnings.length === 1;
check(
  "PuTTY (.reg, UTF-16), MobaXterm and CSV imports create hosts, groups and serial settings",
  importedOk && imports.every((i) => i.created >= 1),
  imports.map((i) => `${i.source} ${i.created}/${i.found}`).join(", "),
);
// The imported telnet session works.
await evaluate(`__hopr.connectHost(${JSON.stringify(byLabel(`moba-${stamp}`)?.id)})`);
const mobaLogin = await waitFor(`__hopr.screen().includes("login:")`, 15000);
check("an imported MobaXterm telnet session connects", mobaLogin >= 0);
await closeAllTabs();
const ports = await invoke("serial_ports");
check(
  "serial ports can be listed (for the serial host form)",
  ports.ok && Array.isArray(ports.v),
  `${ports.v?.length ?? 0} found`,
);

// Language: German in Settings changes the rail; back to the system language.
await click('nav [aria-label="Settings"]');
await waitFor(`!!document.querySelector('[aria-label="Language"]')`, 5000);
await evaluate(`(() => { const s = document.querySelector('[aria-label="Language"]');
  s.value = "de"; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
const german = await waitFor(
  `!!document.querySelector('nav [aria-label="Einstellungen"]') && document.documentElement.lang === "de"`,
  5000,
);
await evaluate(`(() => { const s = document.querySelector('[aria-label="Language"]');
  s.value = "system"; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
const english = await waitFor(`!!document.querySelector('nav [aria-label="Settings"]')`, 5000);
await press("Escape");
check(
  "switching the language to German translates the app frame, and back",
  german >= 0 && english >= 0,
);

// 19. Phase 5: tunnels (local / SOCKS / remote, port in use, stop), snippets with a
//     variable and a vault secret, live broadcast to split panes, run on hosts.
await closeAllTabs();
const net = await import("node:net");
const echo = net.createServer((s) => s.pipe(s));
await new Promise((r) => echo.listen(0, "127.0.0.1", r));
const echoPort = echo.address().port;
/** Send `msg` to 127.0.0.1:port (optionally through a SOCKS5 handshake) and read it back. */
const roundtrip = (port, msg, socksTo = null) =>
  new Promise((resolve) => {
    const c = net.connect(port, "127.0.0.1");
    let buf = Buffer.alloc(0);
    let stage = socksTo ? "hello" : "data";
    const timer = setTimeout(() => {
      c.destroy();
      resolve(null);
    }, 8000);
    c.on("connect", () => {
      if (socksTo) c.write(Buffer.from([5, 1, 0]));
      else c.write(msg);
    });
    c.on("data", (d) => {
      buf = Buffer.concat([buf, d]);
      if (stage === "hello" && buf.length >= 2) {
        buf = buf.subarray(2);
        stage = "reply";
        const host = Buffer.from(socksTo.host);
        c.write(
          Buffer.concat([
            Buffer.from([5, 1, 0, 3, host.length]),
            host,
            Buffer.from([socksTo.port >> 8, socksTo.port & 255]),
          ]),
        );
      } else if (stage === "reply" && buf.length >= 10) {
        buf = buf.subarray(10);
        stage = "data";
        c.write(msg);
      } else if (stage === "data" && buf.toString().includes(msg)) {
        clearTimeout(timer);
        c.end();
        resolve(buf.toString());
      }
    });
    c.on("error", () => {
      clearTimeout(timer);
      resolve(null);
    });
  });

const tunHost = await evaluate(`__hopr.saveHost(${hostInput(`e2e-tunnel-${stamp}`, "hopr")})`);
const tunnelInput = (patch) => ({
  hostId: tunHost.id,
  label: "",
  kind: "local",
  bindAddr: "127.0.0.1",
  bindPort: 0,
  destHost: "127.0.0.1",
  destPort: echoPort,
  autoStart: false,
  ...patch,
});
const localPort = (await invoke("port_pick", { addr: "127.0.0.1" })).v;
const lt = (
  await invoke("tunnel_save", {
    id: null,
    input: tunnelInput({ label: `pg-${stamp}`, bindPort: localPort }),
  })
).v;
// Start it from the Tunnels drawer's switch.
await click('nav [aria-label="Tunnels"]');
await waitFor(`!!document.querySelector('[aria-label="Start pg-${stamp}"]')`, 5000);
await click(`[aria-label="Start pg-${stamp}"]`);
const ltUp = await waitFor(
  `__hopr.tunnelStatus(${JSON.stringify(lt.id)}).state === "running"`,
  20000,
);
const viaL = await roundtrip(localPort, `via-L-${stamp}`);
await sleep(1500);
const ltStatus = await evaluate(`__hopr.tunnelStatus(${JSON.stringify(lt.id)})`);
check(
  "a local tunnel started from the drawer forwards traffic, with live counts",
  ltUp >= 0 &&
    viaL?.includes(`via-L-${stamp}`) &&
    ltStatus.total >= 1 &&
    ltStatus.up > 0 &&
    ltStatus.down > 0,
  `↑${ltStatus.up} ↓${ltStatus.down}, ${ltStatus.total} connection(s)`,
);

// SOCKS (-D) by name, through the host.
const dt = (
  await invoke("tunnel_save", {
    id: null,
    input: tunnelInput({ kind: "dynamic", destHost: "", destPort: 0 }),
  })
).v;
const dStart = (await invoke("tunnel_start", { id: dt.id })).v;
const viaD = await roundtrip(dStart.boundPort, `via-D-${stamp}`, {
  host: "localhost",
  port: echoPort,
});
check(
  "a SOCKS tunnel (port picked automatically) reaches a target by name",
  !!dStart.boundPort && viaD?.includes(`via-D-${stamp}`),
  `:${dStart.boundPort}`,
);

// Remote (-R): a port on the host comes back here.
const rt = (await invoke("tunnel_save", { id: null, input: tunnelInput({ kind: "remote" }) })).v;
const rStart = (await invoke("tunnel_start", { id: rt.id })).v;
const viaR = await roundtrip(rStart.boundPort, `via-R-${stamp}`);
check(
  "a remote tunnel brings connections on the host back to this computer",
  !!rStart.boundPort && viaR?.includes(`via-R-${stamp}`),
  `host :${rStart.boundPort}`,
);

// Port in use, then stopping frees the port.
const busy = (
  await invoke("tunnel_save", { id: null, input: tunnelInput({ bindPort: localPort }) })
).v;
const busyStart = await invoke("tunnel_start", { id: busy.id });
await click(`[aria-label="Stop pg-${stamp}"]`);
await waitFor(`__hopr.tunnelStatus(${JSON.stringify(lt.id)}).state === "stopped"`, 5000);
const freed = (await invoke("port_free", { addr: "127.0.0.1", port: localPort })).v;
check(
  "a busy port is refused with a clear message; stopping frees it",
  !busyStart.ok && /already in use/.test(busyStart.e?.message ?? "") && freed === true,
  busyStart.e?.message ?? "",
);
for (const t of [dt, rt, busy, lt]) await invoke("tunnel_delete", { id: t.id });
await press("Escape");

// Snippets: a variable and a vault secret, typed into the terminal.
await invoke("secret_set", { name: "e2e_token", value: `tok-${stamp}` });
await invoke("snippet_save", {
  id: null,
  input: {
    name: `greet-${stamp}`,
    folder: "E2E",
    tags: ["e2e"],
    description: "",
    body: "echo snip-{{who:world}}-{{secret:e2e_token}}",
    runMode: "paste_enter",
  },
});
const lib5 = await evaluate("__hopr.snippets()");
await evaluate(`__hopr.connectHost(${JSON.stringify(tunHost.id)})`);
await readyTrusting(20000);
await settle();
await click('nav [aria-label="Snippets"]');
const snipAsked = await evaluate(`__hopr.runSnippet(${JSON.stringify(`greet-${stamp}`)})`);
await waitFor(
  `!!document.querySelector('[aria-label="Run snippet"] input[aria-label="who"]')`,
  5000,
);
await evaluate(`(() => { const i = document.querySelector('[aria-label="Run snippet"] input[aria-label="who"]');
  i.value = "e2e"; i.dispatchEvent(new Event("input", { bubbles: true })); i.form.requestSubmit(); })()`);
const ranSnippet = await waitFor(`__hopr.screen().includes("snip-e2e-tok-${stamp}")`, 15000);
check(
  "a snippet asks for its variable and types it with the vault secret filled in",
  snipAsked?.fields?.includes("who") &&
    ranSnippet >= 0 &&
    lib5.secrets.includes("e2e_token") &&
    !JSON.stringify(lib5).includes(`tok-${stamp}`),
  (await evaluate("__hopr.snippetNotice()")).notice ?? "",
);
await press("Escape");

// Broadcast: split the pane, type once, both panes get it.
await focusTerm();
await press("D", ["ctrl", "shift"]);
await waitFor("__hopr.panes() === 2", 5000);
await readyTrusting(20000);
await settle();
const started = await evaluate("__hopr.broadcastTab()");
const bannerUp = await waitFor(`!!document.querySelector('[aria-label="Broadcasting"]')`, 3000);
await typeLine(`echo bcast-${stamp}`);
const both = await waitFor(
  `__hopr.screens(40).filter((s) => s.includes("bcast-${stamp}")).length === 2`,
  15000,
);
const bcastScreens = await evaluate(`__hopr.screens(40).map((s) => s.includes("bcast-${stamp}"))`);
const bcastState = await evaluate("__hopr.broadcast()");
await evaluate("__hopr.broadcastStop()");
check(
  "broadcast types into every pane of the tab, with a banner",
  started && bannerUp >= 0 && both >= 0,
  `started ${started}, banner ${bannerUp >= 0}, panes with text ${JSON.stringify(bcastScreens)}, ${JSON.stringify(bcastState)}`,
);
await closeAllTabs();

// Run on hosts: two SSH hosts and a telnet one (refused), s5Results per host.
const second = await evaluate(`__hopr.saveHost(${hostInput(`e2e-run-${stamp}`, "hopr")})`);
const runIds = [tunHost.id, second.id, tn.id];
await evaluate(`__hopr.groupRun(${JSON.stringify(runIds)}, "echo grp-ok-${stamp}", 2, false)`);
const finished = await waitFor(
  `__hopr.groupResults().length === 3 && __hopr.groupResults().every((r) => r.state === "ok" || r.state === "failed")`,
  60000,
);
const s5Results = await evaluate("__hopr.groupResults()");
const oks = s5Results.filter(
  (r) => r.state === "ok" && r.stdout.includes(`grp-ok-${stamp}`) && r.exitStatus === 0,
);
const refused = s5Results.find((r) => r.hostId === tn.id);
const gridRows = await evaluate(
  `document.querySelectorAll('[aria-label="Results"] tr[data-result]').length`,
);
check(
  "run on hosts runs on each SSH host, refuses the telnet one, and shows a s5Results grid",
  finished >= 0 &&
    oks.length === 2 &&
    refused?.state === "failed" &&
    /SSH/.test(refused?.error ?? "") &&
    gridRows === 3,
  s5Results.map((r) => `${r.label}:${r.state}`).join(", "),
);
await evaluate("__hopr.closeGroupRun()");
echo.close();

// 20. Sync (5d): turn it on from Settings into a fresh folder, check the folder holds
//     only sealed data, and that a wrong passphrase can't join it.
await closeAllTabs();
const syncDir = join(tmp, `sync-${stamp}`);
mkdirSync(syncDir, { recursive: true });
await click('nav [aria-label="Settings"]');
await waitFor(`!!document.querySelector('[aria-label="Sync folder"]')`, 5000);
const fill = (label, value) =>
  evaluate(`(() => { const i = document.querySelector('[aria-label=${JSON.stringify(label)}]');
    i.value = ${JSON.stringify(value)}; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
await fill("Sync folder", syncDir);
await fill("Device name", `E2E ${stamp}`);
await fill("Sync passphrase", `sync-pass-${stamp}`);
await fill("Repeat sync passphrase", `sync-pass-${stamp}`);
await clickText('[aria-label="Sync"] button', "Turn on sync");
const syncOn = await waitFor(
  `__hopr.syncStatus().then((s) => !!s?.enabled && !!s.lastSync)`,
  60000,
);
const devicesDir = join(syncDir, "devices");
const { readdirSync } = await import("node:fs");
const snapFiles = existsSync(devicesDir)
  ? readdirSync(devicesDir).filter((f) => f.endsWith(".hsync"))
  : [];
const sealedBytes = snapFiles.length
  ? readFileSync(join(devicesDir, snapFiles[0]))
  : Buffer.alloc(0);
const leaks = ["127.0.0.1", `e2e-tunnel-${stamp}`, `tok-${stamp}`].filter((s) =>
  sealedBytes.includes(Buffer.from(s)),
);
check(
  "sync turns on from Settings and writes only sealed data to the folder",
  syncOn >= 0 &&
    existsSync(join(syncDir, "hopr-sync.json")) &&
    snapFiles.length === 1 &&
    sealedBytes.length > 1000 &&
    leaks.length === 0,
  `${snapFiles.length} snapshot, ${sealedBytes.length} bytes, leaks: ${leaks.join(",") || "none"}`,
);
const again = await evaluate("__hopr.syncNow()");
check(
  "sync now runs again with no changes to apply",
  again.ok && again.last?.applied === 0,
  `${again.last?.pushed ?? 0} records shared`,
);

// Joining the same folder with the wrong passphrase is refused; the right one works.
await clickText('[aria-label="Sync"] button', "Turn off");
await waitFor(`!!document.querySelector('[aria-label="Sync passphrase"]')`, 5000);
await fill("Sync folder", syncDir);
await fill("Device name", `E2E ${stamp}`);
await fill("Sync passphrase", "wrong-passphrase");
await clickText('[aria-label="Sync"] button', "Turn on sync");
const syncRefused = await waitFor(
  `(document.querySelector('[aria-label="Sync"] [role="alert"]')?.textContent ?? "").includes("passphrase")`,
  60000,
);
await fill("Sync passphrase", `sync-pass-${stamp}`);
await clickText('[aria-label="Sync"] button', "Turn on sync");
const rejoined = await waitFor(`__hopr.syncStatus().then((s) => !!s?.enabled)`, 60000);
check(
  "a wrong sync passphrase can't join the folder; the right one can",
  syncRefused >= 0 && rejoined >= 0,
);
await press("Escape");

ws.close();
const failed = results.filter((r) => !r.ok).length;
console.log(`\n${results.length - failed}/${results.length} checks passed`);
process.exit(failed ? 1 : 0);
