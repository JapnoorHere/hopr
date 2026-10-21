/**
 * Every app action as a palette command with a default shortcut (HOPR_PLAN §11, adapted:
 * app chords use Ctrl+Shift so plain Ctrl stays with the shell).
 */
import { broadcast } from "$features/broadcast/broadcast.svelte";
import { grouprun } from "$features/grouprun/grouprun.svelte";
import { history } from "$features/history/history.svelte";
import { library } from "$features/hosts/library.svelte";
import { palette } from "$features/palette/palette-state.svelte";
import { files } from "$features/sftp/files.svelte";
import { shell } from "$features/shell/shell.state.svelte";
import { prefs } from "$features/terminal/prefs.svelte";
import { sessions } from "$features/terminal/session.svelte";
import { SHELL_NAMES, type ShellKind, SNIPPETS } from "$features/terminal/shellint";
import { vault } from "$features/vault/vault.svelte";
import { appearance } from "$lib/design/appearance.svelte";
import { THEMES } from "$lib/design/theme";
import { saveText } from "$lib/download";
import { inTauri, commands as ipc, type ShellProfile } from "$lib/ipc";
import { type Command, shortcuts } from "$lib/shortcuts";
import { isMainWindow } from "$lib/window";
import { workspaces } from "./workspaces.svelte";

let shellCache: ShellProfile[] | null = null;

async function openLocalShell() {
  if (!inTauri) return;
  if (!shellCache) {
    const r = await ipc.shellsList();
    shellCache = r.status === "ok" ? r.data : [];
  }
  const s = shellCache[0];
  if (s) sessions.open({ kind: "local", shellId: s.id, label: s.name });
}

const hasTab = () => sessions.activeTab !== null;
const hasSplit = () =>
  sessions.activeTab ? sessions.panesOf(sessions.activeTab).length > 1 : false;

export function registerAppCommands(): () => void {
  const list: Command[] = [
    // General
    {
      id: "palette",
      title: "Command palette",
      group: "General",
      keys: ["Ctrl+Shift+P"],
      run: () => palette.show(),
    },
    {
      id: "hosts",
      title: "Hosts",
      group: "General",
      keys: ["Ctrl+Shift+N", "Ctrl+Shift+H"],
      run: () => sessions.showHome(),
    },
    { id: "host.new", title: "New host", group: "General", run: () => (library.editing = "new") },
    {
      id: "keys",
      title: "Keys",
      group: "General",
      keys: ["Ctrl+Shift+K"],
      run: () => shell.toggle("keys"),
    },
    {
      id: "import",
      title: "Import from OpenSSH",
      group: "General",
      run: () => shell.toggle("import"),
    },
    {
      id: "hosts.export",
      title: "Export hosts as SSH config",
      group: "General",
      run: async () => {
        const r = await library.exportHosts("ssh_config", []);
        if (r.ok) saveText("hopr_ssh_config", r.data);
      },
    },
    {
      id: "key.deploy",
      title: "Deploy key to a host…",
      group: "General",
      run: () => {
        const t = sessions.active?.target;
        library.deploying = { hostId: t?.kind === "host" ? t.hostId : null, keyId: null };
      },
    },
    {
      id: "files",
      title: "Browse files (SFTP)",
      group: "General",
      keys: ["Ctrl+Shift+O"],
      when: () => inTauri,
      run: () => {
        const s = sessions.active;
        shell.drawer = "files";
        // Start where the terminal is (shell integration reports its folder).
        if (s?.target.kind === "host") void files.open(s.target.hostId, s.cwd);
      },
    },
    {
      id: "snippets",
      title: "Snippets",
      group: "General",
      keys: ["Ctrl+Shift+S"],
      when: () => inTauri,
      run: () => shell.toggle("snippets"),
    },
    {
      id: "tunnels",
      title: "Tunnels",
      group: "General",
      keys: ["Ctrl+Shift+U"],
      when: () => inTauri,
      run: () => shell.toggle("tunnels"),
    },
    {
      id: "history",
      title: "History",
      group: "General",
      keys: ["Ctrl+Shift+Y"],
      when: () => inTauri,
      run: () => shell.toggle("history"),
    },
    {
      id: "recordings",
      title: "Recordings",
      group: "General",
      when: () => inTauri,
      run: () => {
        shell.drawer = "history";
        void history.show("recordings");
      },
    },
    {
      id: "audit",
      title: "Audit log",
      group: "General",
      run: () => shell.toggle("audit"),
    },
    {
      id: "tab.tearoff",
      title: "Move tab to new window",
      group: "Tabs",
      keys: ["Ctrl+Shift+M"],
      when: () => inTauri && !!sessions.activeTab,
      run: () => {
        const tab = sessions.activeTab;
        if (tab) void sessions.tearOff(tab);
      },
    },
    {
      id: "workspace.restore",
      title: "Restore last session",
      group: "Workspaces",
      keys: ["Ctrl+Shift+R"],
      when: () => isMainWindow,
      run: () => {
        workspaces.restoreLast();
      },
    },
    {
      id: "workspace.save",
      title: "Save workspace…",
      group: "Workspaces",
      when: () => isMainWindow && sessions.tabs.length > 0,
      run: () => {
        shell.drawer = "settings";
        queueMicrotask(() =>
          document.querySelector<HTMLInputElement>('[aria-label="Workspace name"]')?.focus(),
        );
      },
    },
    {
      id: "quick.toggle",
      title: "Quick terminal",
      group: "General",
      when: () => inTauri,
      run: () => void ipc.quickToggle(),
    },
    {
      id: "settings",
      title: "Settings",
      group: "General",
      keys: ["Ctrl+,"],
      run: () => shell.toggle("settings"),
    },
    {
      id: "lock",
      title: "Lock Hopr",
      group: "General",
      keys: ["Ctrl+Shift+L"],
      run: () => void vault.lock(),
    },
    {
      id: "local",
      title: "New local terminal",
      group: "General",
      keys: ["Ctrl+Shift+`"],
      run: () => void openLocalShell(),
    },
    // Tabs
    {
      id: "tab.new",
      title: "New tab",
      group: "Tabs",
      keys: ["Ctrl+Shift+T"],
      run: () => sessions.showHome(),
    },
    {
      id: "tab.duplicate",
      title: "Duplicate tab",
      group: "Tabs",
      when: hasTab,
      run: () => {
        const s = sessions.active;
        if (s) sessions.open(s.target);
      },
    },
    {
      id: "pane.close",
      title: "Close pane",
      group: "Tabs",
      keys: ["Ctrl+Shift+W"],
      when: hasTab,
      run: () => {
        const s = sessions.active;
        if (s) void sessions.close(s);
      },
    },
    {
      id: "tab.close",
      title: "Close tab",
      group: "Tabs",
      when: hasTab,
      run: () => {
        const t = sessions.activeTab;
        if (t) void sessions.closeTab(t);
      },
    },
    {
      id: "tab.reopen",
      title: "Reopen closed tab",
      group: "Tabs",
      keys: ["Ctrl+Shift+Alt+T"],
      when: () => sessions.canReopen,
      run: () => sessions.reopenClosed(),
    },
    {
      id: "tab.next",
      title: "Next tab",
      group: "Tabs",
      keys: ["Ctrl+Tab"],
      run: () => sessions.cycle(1),
    },
    {
      id: "tab.prev",
      title: "Previous tab",
      group: "Tabs",
      keys: ["Ctrl+Shift+Tab"],
      run: () => sessions.cycle(-1),
    },
    ...Array.from({ length: 9 }, (_, i) => ({
      id: `tab.goto.${i + 1}`,
      title: i === 8 ? "Go to last tab" : `Go to tab ${i + 1}`,
      group: "Tabs",
      keys: [`Ctrl+${i + 1}`],
      when: () => sessions.tabs.length > 0,
      run: () => sessions.goto(i + 1),
    })),
    // Panes
    {
      id: "pane.split.right",
      title: "Split pane right",
      group: "Panes",
      keys: ["Ctrl+Shift+D"],
      when: hasTab,
      run: () => sessions.split("row"),
    },
    {
      id: "pane.split.down",
      title: "Split pane down",
      group: "Panes",
      keys: ["Ctrl+Shift+E"],
      when: hasTab,
      run: () => sessions.split("column"),
    },
    {
      id: "pane.focus.left",
      title: "Focus pane left",
      group: "Panes",
      keys: ["Ctrl+Alt+ArrowLeft"],
      when: hasSplit,
      run: () => sessions.move("left"),
    },
    {
      id: "pane.focus.right",
      title: "Focus pane right",
      group: "Panes",
      keys: ["Ctrl+Alt+ArrowRight"],
      when: hasSplit,
      run: () => sessions.move("right"),
    },
    {
      id: "pane.focus.up",
      title: "Focus pane up",
      group: "Panes",
      keys: ["Ctrl+Alt+ArrowUp"],
      when: hasSplit,
      run: () => sessions.move("up"),
    },
    {
      id: "pane.focus.down",
      title: "Focus pane down",
      group: "Panes",
      keys: ["Ctrl+Alt+ArrowDown"],
      when: hasSplit,
      run: () => sessions.move("down"),
    },
    {
      id: "pane.zoom",
      title: "Zoom pane",
      group: "Panes",
      keys: ["Ctrl+Shift+Enter"],
      when: hasSplit,
      run: () => sessions.toggleZoom(),
    },
    // Terminal
    {
      id: "term.find",
      title: "Find in output",
      group: "Terminal",
      keys: ["Ctrl+Shift+F"],
      when: hasTab,
      run: () => sessions.active?.find(),
    },
    {
      id: "term.copy",
      title: "Copy selection",
      group: "Terminal",
      keys: ["Ctrl+Shift+C"],
      when: hasTab,
      run: () => sessions.active?.copy(),
    },
    {
      id: "term.paste",
      title: "Paste",
      group: "Terminal",
      keys: ["Ctrl+Shift+V"],
      when: hasTab,
      run: () => sessions.active?.paste(),
    },
    {
      id: "font.bigger",
      title: "Bigger font",
      group: "Terminal",
      keys: ["Ctrl+=", "Ctrl+Shift+="],
      run: () => prefs.zoom(1),
    },
    {
      id: "font.smaller",
      title: "Smaller font",
      group: "Terminal",
      keys: ["Ctrl+-"],
      run: () => prefs.zoom(-1),
    },
    {
      id: "font.reset",
      title: "Reset font size",
      group: "Terminal",
      keys: ["Ctrl+0"],
      run: () => prefs.zoom(0),
    },
    {
      id: "term.prompt.prev",
      title: "Previous prompt",
      group: "Terminal",
      keys: ["Ctrl+Shift+ArrowUp"],
      // Without shell integration the keys stay with the shell (word selection).
      when: () => !!sessions.active?.hasPrompts,
      run: () => sessions.active?.jumpPrompt(-1),
    },
    {
      id: "term.prompt.next",
      title: "Next prompt",
      group: "Terminal",
      keys: ["Ctrl+Shift+ArrowDown"],
      when: () => !!sessions.active?.hasPrompts,
      run: () => sessions.active?.jumpPrompt(1),
    },
    ...(Object.keys(SNIPPETS) as ShellKind[]).map((kind) => ({
      id: `term.integrate.${kind}`,
      title: `Enable shell integration here (${SHELL_NAMES[kind]})`,
      group: "Terminal",
      when: () => sessions.active?.state.kind === "ready" && !sessions.active.integrated,
      run: () => {
        // A leading space keeps the line out of shell history (and Hopr's).
        sessions.active?.write(new TextEncoder().encode(` ${SNIPPETS[kind]}\r`));
      },
    })),
    // Broadcast & run on hosts
    {
      id: "broadcast.tab",
      title: "Broadcast typing to this tab's panes",
      group: "Broadcast",
      keys: ["Ctrl+Shift+B"],
      when: () => !broadcast.active && hasSplit(),
      run: () => void broadcast.startTab(),
    },
    {
      id: "broadcast.all",
      title: "Broadcast typing to every session",
      group: "Broadcast",
      when: () => !broadcast.active && sessions.list.length > 1,
      run: () => void broadcast.startAll(),
    },
    {
      id: "broadcast.stop",
      title: "Stop broadcasting",
      group: "Broadcast",
      keys: ["Ctrl+Shift+B"],
      when: () => broadcast.active,
      run: () => broadcast.stop(),
    },
    {
      id: "grouprun",
      title: "Run a command on hosts…",
      group: "Broadcast",
      when: () => inTauri,
      run: () => grouprun.show(),
    },
    // Session
    {
      id: "session.record",
      title: "Record this session",
      group: "Session",
      when: () => inTauri && !!sessions.active?.id && !sessions.active.recording,
      run: () => void sessions.active?.toggleRecording(),
    },
    {
      id: "session.record.stop",
      title: "Stop recording",
      group: "Session",
      when: () => !!sessions.active?.recording,
      run: () => void sessions.active?.toggleRecording(),
    },
    {
      id: "session.reconnect",
      title: "Reconnect",
      group: "Session",
      when: () => {
        const k = sessions.active?.state.kind;
        return k === "closed" || k === "failed";
      },
      run: () => {
        const s = sessions.active;
        if (s) sessions.retry(s);
      },
    },
    // Appearance
    ...THEMES.map((t) => ({
      id: `theme.${t.id}`,
      title: `Theme: ${t.name}`,
      group: "Appearance",
      run: () => appearance.update({ theme: t.id }),
    })),
  ];
  return shortcuts.register(list);
}
