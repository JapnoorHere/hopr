/**
 * One terminal session on the UI side: owns the binary output channel, buffers output until a
 * terminal is attached, acks rendered bytes (flow control), and mirrors the backend state
 * machine (including host-key and login prompts).
 */
import { Channel } from "@tauri-apps/api/core";
import { historyPrefs } from "$features/history/prefs.svelte";
import {
  type ConnectTarget,
  commands,
  type Environment,
  events,
  type HostKeyDecision,
  inTauri,
  type RawBytes,
  type SessionState,
} from "$lib/ipc";

/** What a tab connects to. Never holds secrets. */
export type TabTarget =
  | { kind: "host"; hostId: string; label: string; environment: Environment; trail: string[] }
  | { kind: "quick"; host: string; port: number; username: string }
  | { kind: "local"; shellId: string; label: string };

/** Prefill for the quick-connect form. */
export interface QuickDraft {
  host?: string;
  port?: number;
  username?: string;
}

/** What a mounted terminal provides to its session. */
export interface TerminalSink {
  write(data: Uint8Array, done: () => void): void;
  /** Buffer as plain text, optionally only the last `maxLines` (copy output, dev E2E hook). */
  snapshot(maxLines?: number): string;
  /** Open the search bar. */
  find(): void;
  /** Copy the selection to the clipboard. */
  copy(): void;
  /** Paste the clipboard (through the multi-line guard). */
  paste(): void;
  /** Screen and scrollback as escape sequences, to replay in another window. */
  serialize(): string;
  /** Scroll to the previous (-1) or next (1) prompt (shell integration). */
  jumpPrompt(dir: -1 | 1): void;
  /** Whether any prompt marks are known. */
  hasPrompts(): boolean;
  /** First buffer line shown (scroll position). */
  viewportY(): number;
  /** Scroll so buffer line `line` is at the top. */
  scrollToLine(line: number): void;
}

/** The menu for one command block (its exit chip was clicked). */
export interface BlockMenu {
  command: string;
  output: string;
  x: number;
  y: number;
}

/** A command that finished in a terminal. */
export interface CommandDone {
  command: string;
  exit: number | null;
  cwd: string | null;
  durationMs: number;
}

function formatDuration(ms: number): string {
  const s = Math.round(ms / 1000);
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
}

/** A live session handed over from another window (tear-off). */
export interface Adopted {
  id: string;
  state: SessionState;
  scrollback: string;
}

/** Acks are coalesced: one IPC per ~frame or per 512 KiB, not one per rendered batch. */
const ACK_FLUSH_MS = 16;
const ACK_FLUSH_BYTES = 512 * 1024;

const byId = new Map<string, TerminalSession>();
const earlyStates = new Map<string, SessionState>();

export class TerminalSession {
  readonly key: string = crypto.randomUUID();
  readonly target: TabTarget;
  id = $state<string | null>(null);
  state = $state<SessionState>({ kind: "connecting", hop: 1, total: 1 });
  /** When the terminal bell last rang (a background tab wiggles). */
  bellAt = $state(0);

  // A quick-connect password is kept only until the session is ready, so Retry works.
  #password: string | null;
  #sink: TerminalSink | null = null;
  #pending: Uint8Array[] = [];
  #unackedBeforeId = 0;
  #ackPending = 0;
  #ackTimer: ReturnType<typeof setTimeout> | undefined;
  #resizeTimer: ReturnType<typeof setTimeout> | undefined;
  #started = false;
  /** Last grid size the terminal asked for. */
  #size: { cols: number; rows: number } | null = null;
  /** Output bytes received (diagnostics). */
  bytesIn = 0;
  /** Input typed before the session id is known (see `write`). */
  #earlyWrites: Uint8Array[] = [];
  /** Set when this session was moved from another window instead of connected here. */
  #adopted: Adopted | null = null;
  /** Working directory reported by the shell (OSC 7). */
  cwd = $state<string | null>(null);
  /** The shell sends shell-integration marks (OSC 133). */
  integrated = $state(false);
  /** Recording file name while recording. */
  recording = $state<string | null>(null);
  /** The last command that finished (status bar, E2E). */
  lastCommand = $state<CommandDone | null>(null);

  constructor(target: TabTarget, password: string | null = null, adopted: Adopted | null = null) {
    this.target = target;
    this.#password = password;
    this.#adopted = adopted;
    if (adopted) {
      this.id = adopted.id;
      this.state = adopted.state;
    } else if (target.kind === "local") {
      this.state = { kind: "ready" };
    }
  }

  get label(): string {
    return this.target.kind === "quick" ? this.target.host : this.target.label;
  }

  /** Hops after the origin, outermost first (the jump chain for saved hosts). */
  get hops(): string[] {
    return this.target.kind === "host" ? this.target.trail : [this.label];
  }

  /** Breadcrumb origin shown before the label. */
  get origin(): string {
    return this.target.kind === "local" ? "local" : "laptop";
  }

  get environment(): Environment {
    return this.target.kind === "host" ? this.target.environment : "none";
  }

  /** `user@host:port` for quick-connect tabs. */
  get address(): string | null {
    return this.target.kind === "quick"
      ? `${this.target.username}@${this.target.host}:${this.target.port}`
      : null;
  }

  takePassword(): string | null {
    const pw = this.#password;
    this.#password = null;
    return pw;
  }

  #connectTarget(): ConnectTarget {
    const t = this.target;
    switch (t.kind) {
      case "host":
        return { kind: "host", hostId: t.hostId };
      case "local":
        return { kind: "local", shellId: t.shellId };
      case "quick":
        return {
          kind: "quick",
          host: t.host,
          port: t.port,
          username: t.username,
          password: this.#password,
        };
    }
  }

  /** Called by the terminal once it knows its grid size. */
  async start(cols: number, rows: number): Promise<void> {
    if (this.#started) return;
    this.#started = true;
    this.#size = { cols, rows };
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
    const adopted = this.#adopted;
    if (adopted) {
      // Already connected: route its output here, then match this window's size.
      byId.set(adopted.id, this);
      const res = await commands.sessionAttach(adopted.id, channel as unknown as Channel<RawBytes>);
      if (res.status === "error") {
        this.setState({ kind: "failed", error: res.error });
        return;
      }
      if (this.state.kind === "ready") void commands.sessionResize(adopted.id, cols, rows);
      this.recording = await commands.recordingStatus(adopted.id);
      return;
    }
    const res = await commands.sessionOpen(
      this.#connectTarget(),
      cols,
      rows,
      // Raw IPC bodies arrive as ArrayBuffer; specta can only describe them as number[].
      channel as unknown as Channel<RawBytes>,
    );
    if (res.status === "error") {
      this.setState({ kind: "failed", error: res.error });
      return;
    }
    this.id = res.data;
    byId.set(res.data, this);
    for (const data of this.#earlyWrites.splice(0)) {
      void commands.sessionWrite(res.data, Array.from(data));
    }
    // Size changes made while the id was unknown were never sent: send the size now.
    const size = this.#size ?? { cols, rows };
    if (this.target.kind === "local") {
      // A fresh ConPTY may not paint its first frame (the prompt) until it is resized, so
      // nudge it by a row and back.
      void commands.sessionResize(res.data, size.cols, size.rows + 1);
      void commands.sessionResize(res.data, size.cols, size.rows);
    } else if (size.cols !== cols || size.rows !== rows) {
      void commands.sessionResize(res.data, size.cols, size.rows);
    }
    const early = earlyStates.get(res.data);
    if (early) {
      earlyStates.delete(res.data);
      this.setState(early);
    }
    if (this.#unackedBeforeId > 0) {
      void commands.sessionAck(res.data, this.#unackedBeforeId);
      this.#unackedBeforeId = 0;
    }
    // "Record every session" starts recording in the backend.
    this.recording = await commands.recordingStatus(res.data);
  }

  /** Start or stop recording this session. Returns an error message, or null. */
  async toggleRecording(): Promise<string | null> {
    if (!this.id) return "Not connected yet";
    if (this.recording) {
      const r = await commands.recordingStop(this.id);
      if (r.status === "error") return r.error.message;
      this.recording = null;
      return null;
    }
    const r = await commands.recordingStart(this.id);
    if (r.status === "error") return r.error.message;
    this.recording = r.data;
    return null;
  }

  /** A command finished: save it to history, and notify if it ran long in the background. */
  commandFinished(done: CommandDone, exact: boolean) {
    this.lastCommand = done;
    if (!this.id || !inTauri) return;
    void commands.historyAdd({
      sessionId: this.id,
      command: done.command,
      cwd: done.cwd,
      exitCode: done.exit,
      exact,
      durationMs: exact ? Math.min(done.durationMs, 0xffffffff) : null,
    });
    const after = historyPrefs.current.notifyAfterSecs;
    const background = document.hidden || !document.hasFocus() || !inFront(this);
    if (exact && after > 0 && done.durationMs >= after * 1000 && background) {
      const status =
        done.exit === null
          ? "finished"
          : done.exit === 0
            ? "succeeded"
            : `failed (exit ${done.exit})`;
      void commands.notify(
        `${done.command.slice(0, 60)} ${status}`,
        `${this.label} · ran ${formatDuration(done.durationMs)}`,
      );
    }
  }

  jumpPrompt(dir: -1 | 1) {
    this.#sink?.jumpPrompt(dir);
  }

  get hasPrompts(): boolean {
    return this.#sink?.hasPrompts() ?? false;
  }

  get viewportY(): number {
    return this.#sink?.viewportY() ?? -1;
  }

  /** While scrolled back into a command's output, that command (pinned at the top). */
  sticky = $state<{ command: string; exit: number | null; line: number } | null>(null);
  /** The open block menu, if any. */
  blockMenu = $state<BlockMenu | null>(null);

  scrollToLine(line: number) {
    this.#sink?.scrollToLine(line);
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
    // Replayed scrollback is drawn first and never acknowledged (the backend sent it long ago).
    const replay = this.#adopted?.scrollback;
    if (replay && this.#adopted) {
      this.#adopted.scrollback = "";
      sink.write(new TextEncoder().encode(replay), () => {});
    }
    for (const chunk of this.#pending.splice(0)) this.#render(chunk);
  }

  serialize(): string {
    return this.#sink?.serialize() ?? "";
  }

  /** Take this session's output back after a failed move to another window. */
  async reattach(): Promise<void> {
    if (!this.id) return;
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (buf) => this.#onOutput(new Uint8Array(buf));
    await commands.sessionAttach(this.id, channel as unknown as Channel<RawBytes>);
  }

  /** Flush pending acks now (before the session moves to another window). */
  flushAcks() {
    this.#flushAcks();
  }

  detach() {
    this.#sink = null;
  }

  snapshot(maxLines?: number): string {
    return this.#sink?.snapshot(maxLines) ?? "";
  }

  find() {
    this.#sink?.find();
  }

  copy() {
    this.#sink?.copy();
  }

  paste() {
    this.#sink?.paste();
  }

  write(data: Uint8Array) {
    if (this.state.kind !== "ready") return;
    if (this.id) {
      void commands.sessionWrite(this.id, Array.from(data));
    } else if (this.#earlyWrites.length < 64) {
      // A local shell is ready before its id comes back, and output can arrive first:
      // xterm's answer to ConPTY's opening cursor query must not be lost, or the shell
      // waits for it forever.
      this.#earlyWrites.push(data);
    }
  }

  resize(cols: number, rows: number) {
    this.#size = { cols, rows };
    clearTimeout(this.#resizeTimer);
    this.#resizeTimer = setTimeout(() => {
      if (this.id && this.state.kind === "ready") void commands.sessionResize(this.id, cols, rows);
    }, 50);
  }

  answerHostKey(decision: HostKeyDecision) {
    if (this.id) void commands.sessionHostKeyDecision(this.id, decision);
  }

  /** `null` cancels the login. */
  answerPrompt(responses: string[] | null, remember = false) {
    if (!this.id) return;
    void commands.sessionPromptResponse(this.id, responses ? { responses, remember } : null);
  }

  async close(): Promise<void> {
    clearTimeout(this.#resizeTimer);
    if (this.id) await commands.sessionClose(this.id);
  }

  #onOutput(chunk: Uint8Array) {
    this.bytesIn += chunk.byteLength;
    if (this.#sink) this.#render(chunk);
    else this.#pending.push(chunk);
  }

  #render(chunk: Uint8Array) {
    const sink = this.#sink;
    if (!sink) return;
    sink.write(chunk, () => this.#queueAck(chunk.byteLength));
  }

  #queueAck(bytes: number) {
    this.#ackPending += bytes;
    if (this.#ackPending >= ACK_FLUSH_BYTES) this.#flushAcks();
    else this.#ackTimer ??= setTimeout(() => this.#flushAcks(), ACK_FLUSH_MS);
  }

  #flushAcks() {
    clearTimeout(this.#ackTimer);
    this.#ackTimer = undefined;
    const bytes = this.#ackPending;
    this.#ackPending = 0;
    if (bytes === 0) return;
    if (this.id) void commands.sessionAck(this.id, bytes);
    else this.#unackedBeforeId += bytes;
  }
}

/** Whether a session is the one in front. The store sets this, so neither module imports the other. */
let inFront: (s: TerminalSession) => boolean = () => false;
export function setInFront(check: (s: TerminalSession) => boolean) {
  inFront = check;
}

/** A session that went away (closed, torn off) stops receiving state events. */
export function forgetSession(id: string) {
  byId.delete(id);
}

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
