/**
 * English messages: the source of truth for message keys. `{name}` placeholders are
 * filled by `t()`. Keep messages short and in sentence case (HOPR_PLAN §10).
 */
export const en = {
  "rail.primary": "Primary",
  "rail.hosts": "Hosts",
  "rail.files": "Files (SFTP)",
  "rail.history": "History",
  "rail.tunnels": "Tunnels",
  "rail.snippets": "Snippets",
  "rail.keys": "Keys",
  "rail.palette": "Command palette",
  "rail.lock": "Lock",
  "rail.settings": "Settings",

  "settings.title": "Settings",
  "settings.theme": "Theme",
  "settings.density": "Density",
  "settings.language": "Language",
  "settings.languageHint": "Translations cover the main screens so far; the rest is in English.",
  "settings.languageSystem": "System ({name})",

  "density.compact": "Compact",
  "density.comfortable": "Comfortable",
  "density.spacious": "Spacious",
  "settings.motion": "Motion",
  "settings.motionHint":
    "System follows Windows' animation setting. Reduced keeps short fades and stops movement.",
  "motion.system": "System",
  "motion.full": "Full",
  "motion.reduced": "Reduced",
  "motion.off": "Off",
  "settings.backdrop": "Window backdrop",
  "settings.backdropHint":
    "Mica tints the frame with your desktop colour; Acrylic blurs whatever is behind the window (Windows 11). Lower the terminal's opacity in Terminal settings to see through it too.",
  "backdrop.solid": "Solid",
  "backdrop.mica": "Mica",
  "backdrop.acrylic": "Acrylic",

  "import.title": "Import",
  "import.fromConfig": "From ~/.ssh/config",
  "import.fromApp": "From another app",
  "import.fromKnownHosts": "From ~/.ssh/known_hosts",
  "import.knownHint": "Trust the host keys you already accepted with OpenSSH.",
  "import.knownButton": "Import trusted keys",
  "import.readComputer": "Read from this computer",
  "import.chooseFile": "Choose file…",
  "import.chooseFolder": "Choose folder…",
  "import.sessions": "Import {count} sessions",
  "import.sessionsOne": "Import 1 session",
  "import.noPasswords": "Passwords aren't imported; Hopr asks on first connect.",
  "import.none": "No sessions found.",

  "history.title": "History",
  "history.commands": "Commands",
  "history.connections": "Connections",
  "history.recordings": "Recordings",
  "history.search": "Search commands",
  "history.allHosts": "All hosts",
  "history.clear": "Clear history…",
} as const;

export type MessageKey = keyof typeof en;
export type Messages = Record<MessageKey, string>;
