import type { SessionState } from "$lib/ipc";

/** How a state is shown as light (HOPR_PLAN §1.3): live, busy, warn, down, idle. */
export type Tone = "live" | "busy" | "warn" | "down" | "idle";

export function describeState(state: SessionState): { label: string; tone: Tone } {
  switch (state.kind) {
    case "connecting":
      return {
        label: state.total > 1 ? `Connecting (hop ${state.hop} of ${state.total})…` : "Connecting…",
        tone: "busy",
      };
    case "authenticating":
      return { label: `Authenticating with ${state.method}…`, tone: "busy" };
    case "ready":
      return { label: "Connected", tone: "live" };
    case "reconnecting":
      return { label: `Reconnecting (attempt ${state.attempt})…`, tone: "warn" };
    case "closed":
      switch (state.reason) {
        case "user_closed":
          return { label: "Closed", tone: "idle" };
        case "remote_exited":
          return { label: "Session ended", tone: "idle" };
        case "connection_lost":
          return { label: "Connection lost", tone: "down" };
      }
      break;
    case "failed":
      return { label: state.error.message, tone: "down" };
  }
  return { label: "Unknown", tone: "idle" };
}

export function isBusy(state: SessionState): boolean {
  return state.kind === "connecting" || state.kind === "authenticating";
}
