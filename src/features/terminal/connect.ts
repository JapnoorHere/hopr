/**
 * The connecting card's model: where a connection is on its route (this computer → jump
 * hosts → server) and at which stage, built up from the session states the backend sends.
 * Pure, so the card's logic tests without a DOM.
 */
import type { SessionState } from "$lib/ipc";

/** reach: opening the connection · secure: SSH handshake · verify: host key check ·
 *  login: authenticating · shell: asking for a terminal · ready: connected. */
export type Stage = "reach" | "secure" | "verify" | "login" | "shell" | "ready";

export interface Progress {
  /** The hop being worked on, 1-based. */
  hop: number;
  total: number;
  stage: Stage;
  /** Time to open each hop's connection (≈ one round trip), by hop, once known. */
  rtts: (number | null)[];
  /** The login method in use, as the backend names it. */
  method: string | null;
  /** The connection failed at `hop` / `stage`. */
  failed: boolean;
}

export const START: Progress = {
  hop: 1,
  total: 1,
  stage: "reach",
  rtts: [null],
  method: null,
  failed: false,
};

function sized(rtts: (number | null)[], total: number): (number | null)[] {
  return Array.from({ length: Math.max(1, total) }, (_, i) => rtts.at(i) ?? null);
}

/** The progress after `state`; states that say nothing about progress leave it as is. */
export function advance(p: Progress, state: SessionState): Progress {
  switch (state.kind) {
    case "connecting":
      return {
        ...p,
        hop: state.hop,
        total: state.total,
        stage: "reach",
        rtts: sized(p.rtts, state.total),
        method: null,
        failed: false,
      };
    case "handshaking": {
      const rtts = sized(p.rtts, state.total);
      rtts[state.hop - 1] = state.rttMs ?? null;
      return { ...p, hop: state.hop, total: state.total, stage: "secure", rtts, failed: false };
    }
    case "verifying_host_key":
      return { ...p, stage: "verify" };
    case "prompting":
      return { ...p, stage: "login" };
    case "authenticating":
      return { ...p, stage: "login", method: state.method };
    case "opening_shell":
      return { ...p, hop: p.total, stage: "shell" };
    case "ready":
      return { ...p, hop: p.total, stage: "ready", failed: false };
    case "failed":
      return { ...p, failed: true };
    default:
      return p;
  }
}

export type Part = "done" | "active" | "broken" | "idle";

/** The link leading into node `i` (1-based; node 0 is this computer). */
export function linkState(p: Progress, i: number): Part {
  if (p.stage === "ready" || i < p.hop) return "done";
  if (i > p.hop) return "idle";
  if (p.stage === "reach") return p.failed ? "broken" : "active";
  return "done";
}

/** Node `i` (1-based hop). */
export function nodeState(p: Progress, i: number): Part {
  if (p.stage === "ready" || i < p.hop) return "done";
  if (i > p.hop) return "idle";
  if (p.failed) return p.stage === "reach" ? "idle" : "broken";
  return p.stage === "reach" ? "idle" : "active";
}

const METHODS: Record<string, string> = {
  publickey: "your key",
  certificate: "your certificate",
  agent: "the SSH agent",
  password: "a password",
  "keyboard-interactive": "a code",
};

/** What the card says, in plain words. `names` = the hop names, outermost first. */
export function stageLabel(p: Progress, names: string[]): string {
  const here = names.at(p.hop - 1) ?? "the server";
  const via = p.hop > 1 ? names.at(p.hop - 2) : undefined;
  if (p.failed) return `Couldn't finish connecting to ${here}`;
  switch (p.stage) {
    case "reach":
      return via ? `Reaching ${here} through ${via}` : `Reaching ${here}`;
    case "secure":
      return `Securing the connection to ${here}`;
    case "verify":
      return `Checking ${here}'s identity`;
    case "login":
      return p.method && METHODS[p.method] ? `Logging in with ${METHODS[p.method]}` : "Logging in";
    case "shell":
      return "Opening a shell";
    case "ready":
      return "Connected";
  }
}

/** The five steps shown under the route, and how far along each is. */
export const STEPS: { stage: Stage; label: string }[] = [
  { stage: "reach", label: "Reach" },
  { stage: "secure", label: "Secure" },
  { stage: "login", label: "Log in" },
  { stage: "shell", label: "Shell" },
  { stage: "ready", label: "Ready" },
];

const ORDER: Stage[] = ["reach", "secure", "verify", "login", "shell", "ready"];

/** Steps follow the hop being worked on; the route above shows which hop that is. */
export function stepState(p: Progress, step: Stage): Part {
  const at = ORDER.indexOf(p.stage);
  const mine = ORDER.indexOf(step);
  if (mine < at) return "done";
  if (mine > at) return "idle";
  if (step === "ready") return "done";
  return p.failed ? "broken" : "active";
}
