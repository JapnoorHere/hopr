/**
 * Terminal sessions on the UI side. A TerminalSession owns the binary output channel,
 * buffers output until a terminal is attached, acks rendered bytes (flow control), and
 * mirrors the backend state machine. The store tracks tabs and the active one.
 */
import { Channel } from "@tauri-apps/api/core";
import { commands, events, inTauri, type RawBytes, type SessionState } from "$lib/ipc";

export interface ConnectTarget {
  host: string;
  port: number;
  username: string;
}

/** What a mounted terminal provides to its session. */
export interface TerminalSink {
  write(data: Uint8Array, done: () => void): void;
  /** Visible buffer as plain text (used by "copy output" and the dev E2E hook). */
  snapshot(): string;
}

const byId = new Map<string, TerminalSession>();
const earlyStates = new Map<string, SessionState>();

export class TerminalSession {
  readonly key = crypto.randomUUID();
  readonly target: ConnectTarget;
  id = $state<string | null>(null);
  state = $state<SessionState>({ kind: "connecting", hop: 1, total: 1 });

  // Kept only until the session is ready so "Retry" works after a failed attempt.
  // Phase 2 moves credentials into the encrypted vault.
  #password: string | null;
  #sink: TerminalSink | null = null;
  #pending: Uint8Array[] = [];
  #unackedBeforeId = 0;
  #resizeTimer: ReturnType<typeof setTimeout> | undefined;
  #started = false;

  constructor(target: ConnectTarget, password: string) {
    this.target = target;
    this.#password = password;
  }

  get label(): string {
    return this.target.host;
  }

  get canRetry(): boolean {
    return this.#password !== null;
  }

  takePassword(): string | null {
    const pw = this.#password;
    this.#password = null;
    return pw;
  }

  /** Called by the terminal once it knows its grid size. */
  async start(cols: number, rows: number): Promise<void> {
    if (this.#started) return;
    this.#started = true;
    if (!inTauri) {
      this.setState({
        kind: "failed",
        error: {
          code: "internal",
          message: "Connections need the Hopr desktop app",
          detail: null,
          retryable: false,
        },
      });
      return;
    }
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (buf) => this.#onOutput(new Uint8Array(buf));
    const res = await commands.sessionOpen(
      { ...this.target, password: this.#password ?? "", cols, rows },
      // Raw IPC bodies arrive as ArrayBuffer; specta can only describe them as number[].
      channel as unknown as Channel<RawBytes>,
    );
    if (res.status === "error") {
      this.setState({ kind: "failed", error: res.error });
      return;
    }
    this.id = res.data;
    byId.set(res.data, this);
    const early = earlyStates.get(res.data);
    if (early) {
      earlyStates.delete(res.data);
      this.setState(early);
    }
    if (this.#unackedBeforeId > 0) {
      void commands.sessionAck(res.data, this.#unackedBeforeId);
      this.#unackedBeforeId = 0;
    }
  }

  setState(state: SessionState) {
    this.state = state;
    if (state.kind === "ready") this.#password = null;
    if (state.kind === "closed" || state.kind === "failed") {
      if (this.id) byId.delete(this.id);
    }
  }

  attach(sink: TerminalSink) {
    this.#sink = sink;
    for (const chunk of this.#pending.splice(0)) this.#render(chunk);
  }

  detach() {
    this.#sink = null;
  }

  snapshot(): string {
    return this.#sink?.snapshot() ?? "";
  }

  write(data: Uint8Array) {
    if (this.id && this.state.kind === "ready") {
      void commands.sessionWrite(this.id, Array.from(data));
    }
  }

  resize(cols: number, rows: number) {
    clearTimeout(this.#resizeTimer);
    this.#resizeTimer = setTimeout(() => {
      if (this.id && this.state.kind === "ready") void commands.sessionResize(this.id, cols, rows);
    }, 50);
  }

  async close(): Promise<void> {
    clearTimeout(this.#resizeTimer);
    if (this.id) await commands.sessionClose(this.id);
  }

  #onOutput(chunk: Uint8Array) {
    if (this.#sink) this.#render(chunk);
    else this.#pending.push(chunk);
  }

  #render(chunk: Uint8Array) {
    const sink = this.#sink;
    if (!sink) return;
    sink.write(chunk, () => {
      if (this.id) void commands.sessionAck(this.id, chunk.byteLength);
      else this.#unackedBeforeId += chunk.byteLength;
    });
  }
}

class SessionsStore {
  list = $state<TerminalSession[]>([]);
  activeKey = $state<string | null>(null);
  /** Prefill for the connect form (e.g. "Edit connection" after a failure). */
  draft = $state<Partial<ConnectTarget> | null>(null);

  get active(): TerminalSession | null {
    return this.list.find((s) => s.key === this.activeKey) ?? null;
  }

  open(target: ConnectTarget, password: string): TerminalSession {
    const session = new TerminalSession(target, password);
    this.list.push(session);
    this.activeKey = session.key;
    this.draft = null;
    return session;
  }

  activate(key: string) {
    this.activeKey = key;
  }

  showNew(prefill: Partial<ConnectTarget> | null = null) {
    this.draft = prefill;
    this.activeKey = null;
  }

  /** Retry in place when credentials are still held; otherwise reopen the form prefilled. */
  retry(old: TerminalSession) {
    const password = old.takePassword();
    const i = this.list.indexOf(old);
    void old.close();
    if (password === null || i === -1) {
      if (i !== -1) this.list.splice(i, 1);
      this.showNew(old.target);
      return;
    }
    const next = new TerminalSession(old.target, password);
    this.list[i] = next;
    this.activeKey = next.key;
  }

  async close(session: TerminalSession) {
    const i = this.list.indexOf(session);
    if (i !== -1) this.list.splice(i, 1);
    if (this.activeKey === session.key) {
      this.activeKey = this.list[Math.min(i, this.list.length - 1)]?.key ?? null;
    }
    await session.close();
  }
}

export const sessions = new SessionsStore();

/** Route backend state events to their sessions. Call once at startup. */
export async function initSessionEvents(): Promise<void> {
  if (!inTauri) return;
  await events.sessionStateEvent.listen((e) => {
    const { id, state } = e.payload;
    const session = byId.get(id);
    if (session) session.setState(state);
    else earlyStates.set(id, state); // event beat the sessionOpen reply
  });
}
