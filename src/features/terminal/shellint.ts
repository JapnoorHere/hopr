/**
 * Shell integration (§9.6): the marks a shell prints around its prompt and commands.
 * OSC 133 (FinalTerm): A prompt start, B input start, C command executed, D;exit done.
 * OSC 633 (VS Code) adds E;<command line>. OSC 7 reports the working directory.
 */

export type Mark =
  | { kind: "prompt" }
  | { kind: "input" }
  | { kind: "exec" }
  | { kind: "done"; exit: number | null }
  | { kind: "cmdline"; command: string }
  | { kind: "cwd"; path: string };

function exitCode(s: string | undefined): number | null {
  if (s === undefined || s === "") return null;
  const n = Number(s);
  return Number.isInteger(n) ? n : null;
}

/** OSC 133 payload, e.g. "A", "B", "C", "D;0", "A;cl=m;aid=1". */
export function parse133(data: string): Mark | null {
  const [code, arg] = data.split(";");
  switch (code) {
    case "A":
      return { kind: "prompt" };
    case "B":
      return { kind: "input" };
    case "C":
      return { kind: "exec" };
    case "D":
      return { kind: "done", exit: arg?.includes("=") ? null : exitCode(arg) };
    default:
      return null;
  }
}

/** VS Code's escaping in OSC 633: `\\` is a backslash, `\xNN` a byte (`;` is `\x3b`). */
export function unescape633(s: string): string {
  return s.replace(/\\(\\|x([0-9a-fA-F]{2}))/g, (_, all: string, hex?: string) =>
    hex ? String.fromCharCode(Number.parseInt(hex, 16)) : all,
  );
}

/** OSC 633 payload: the 133 marks plus E (command line) and P;Cwd=… */
export function parse633(data: string): Mark | null {
  const i = data.indexOf(";");
  const code = i === -1 ? data : data.slice(0, i);
  const rest = i === -1 ? "" : data.slice(i + 1);
  if (code === "E") {
    // "E;<command>[;<nonce>]": the command is escaped, so its own `;` never splits it.
    return { kind: "cmdline", command: unescape633(rest.split(";")[0] ?? "") };
  }
  if (code === "P") {
    const m = /^Cwd=(.*)$/.exec(rest);
    return m?.[1] ? { kind: "cwd", path: unescape633(m[1]) } : null;
  }
  return parse133(data);
}

/** OSC 7 payload `file://host/path` → the path ("/C:/x" on Windows becomes "C:/x"). */
export function parseOsc7(data: string): string | null {
  const m = /^file:\/\/[^/]*(\/.*)$/.exec(data.trim());
  if (!m?.[1]) return null;
  let path: string;
  try {
    path = decodeURIComponent(m[1]);
  } catch {
    path = m[1];
  }
  return /^\/[A-Za-z]:\//.test(path) ? path.slice(1) : path;
}

/** A command that finished, as shell integration reported it. */
export interface Finished {
  command: string;
  exit: number | null;
  cwd: string | null;
  durationMs: number;
}

/**
 * Turns marks into finished commands. The command line comes from OSC 633;E when the
 * shell sends it, else from `readInput` (the text typed between B and C).
 */
export class CommandTracker {
  /** Set once the shell sent any prompt mark. */
  integrated = false;
  cwd: string | null = null;
  #phase: "idle" | "prompt" | "input" | "running" = "idle";
  #cmdline: string | null = null;
  #command = "";
  #started = 0;
  readonly #readInput: () => string;
  readonly #now: () => number;

  constructor(readInput: () => string, now: () => number = Date.now) {
    this.#readInput = readInput;
    this.#now = now;
  }

  get running(): boolean {
    return this.#phase === "running";
  }

  mark(m: Mark): Finished | null {
    switch (m.kind) {
      case "cwd":
        this.cwd = m.path;
        return null;
      case "prompt":
      case "input":
        this.integrated = true;
        if (this.#phase === "running") return null; // a prompt mark inside output: ignore
        this.#phase = m.kind;
        return null;
      case "cmdline":
        this.#cmdline = m.command;
        return null;
      case "exec":
        if (this.#phase === "running") return null; // repeated C (e.g. bash's DEBUG trap)
        this.integrated = true;
        this.#command = (this.#cmdline ?? this.#readInput()).trim();
        this.#cmdline = null;
        this.#phase = "running";
        this.#started = this.#now();
        return null;
      case "done": {
        const was = this.#phase;
        this.#phase = "idle";
        if (was !== "running") return null; // D for the command before integration started
        return {
          command: this.#command,
          exit: m.exit,
          cwd: this.cwd,
          durationMs: Math.max(0, this.#now() - this.#started),
        };
      }
    }
  }
}

// `${` would be a template substitution, so bash's parameter expansion is spliced in.
const D = "$";

/** Shell integration snippets: one line each, so they can be typed into a session. */
export const SNIPPETS = {
  bash: String.raw`__hopr_p(){ local e=$?; printf '\e]133;D;%s\a\e]7;file://%s%s\a' "$e" "$HOSTNAME" "$PWD"; }; PROMPT_COMMAND="__hopr_p${D}{PROMPT_COMMAND:+;$PROMPT_COMMAND}"; PS1="\[\e]133;A\a\]$PS1\[\e]133;B\a\]"; trap '[[ $BASH_COMMAND != __hopr_p* ]] && printf "\e]133;C\a"' DEBUG`,
  zsh: String.raw`__hopr_pre(){ local e=$?; printf '\e]133;D;%s\a\e]7;file://%s%s\a' "$e" "$HOST" "$PWD"; }; __hopr_exec(){ printf '\e]133;C\a'; }; autoload -Uz add-zsh-hook; add-zsh-hook precmd __hopr_pre; add-zsh-hook preexec __hopr_exec; PS1=$'%{\e]133;A\a%}'"$PS1"$'%{\e]133;B\a%}'`,
  fish: String.raw`function __hopr_p --on-event fish_prompt; printf '\e]133;D;%s\a\e]7;file://%s%s\a\e]133;A\a' $status (hostname) $PWD; end; function __hopr_x --on-event fish_preexec; printf '\e]133;C\a'; end`,
  pwsh: String.raw`if (-not $global:__hp) { $global:__hp=$function:prompt; function global:prompt { $x = if ($?) { 0 } elseif ($LASTEXITCODE) { $LASTEXITCODE } else { 1 }; $q=[char]27; $b=[char]7; $p = & $global:__hp; "$q]133;D;$x$b$q]7;file://localhost/$($PWD.ProviderPath -replace '\\','/')$b$q]133;A$b$p$q]133;B$b" }; if (Test-Path Function:PSConsoleHostReadLine) { $global:__hr=$function:PSConsoleHostReadLine; function global:PSConsoleHostReadLine { $l = & $global:__hr; $q=[char]27; $b=[char]7; [Console]::Write("$q]633;E;$($l -replace '\\','\\' -replace ';','\x3b' -replace "\n",'\x0a')$b$q]133;C$b"); $l } } }`,
} as const;

export type ShellKind = keyof typeof SNIPPETS;

export const SHELL_NAMES: Record<ShellKind, string> = {
  bash: "bash",
  zsh: "zsh",
  fish: "fish",
  pwsh: "PowerShell",
};

/** Lines to add to a shell's startup file so integration is always on. */
export const STARTUP_FILES: Record<ShellKind, string> = {
  bash: "~/.bashrc",
  zsh: "~/.zshrc",
  fish: "~/.config/fish/config.fish",
  pwsh: "$PROFILE",
};
