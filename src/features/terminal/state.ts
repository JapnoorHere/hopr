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
    case "handshaking":
      return { label: "Securing the connection…", tone: "busy" };
    case "opening_shell":
      return { label: "Opening a shell…", tone: "busy" };
    case "verifying_host_key":
      return state.prompt.changed
        ? { label: "Host key changed: check before connecting", tone: "down" }
        : { label: "Verify the server's identity", tone: "warn" };
    case "prompting":
      return { label: state.prompt.title, tone: "warn" };
    case "authenticating":
      return { label: `Authenticating with ${state.method}…`, tone: "busy" };
    case "ready":
      return { label: "Connected", tone: "live" };
    case "reconnecting":
      return {
        label: `Reconnecting (attempt ${state.attempt} of ${state.maxAttempts})…`,
        tone: "warn",
      };
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
  return (
    state.kind === "connecting" ||
    state.kind === "handshaking" ||
    state.kind === "authenticating" ||
    state.kind === "opening_shell"
  );
}
