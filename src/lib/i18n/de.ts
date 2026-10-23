/** German messages. Every key in `en` must be here (checked by i18n.test.ts). */
import type { Messages } from "./en";

export const de: Messages = {
  "rail.primary": "Hauptnavigation",
  "rail.hosts": "Hosts",
  "rail.files": "Dateien (SFTP)",
  "rail.history": "Verlauf",
  "rail.tunnels": "Tunnel",
  "rail.snippets": "Snippets",
  "rail.keys": "Schlüssel",
  "rail.palette": "Befehlspalette",
  "rail.lock": "Sperren",
  "rail.settings": "Einstellungen",

  "settings.title": "Einstellungen",
  "settings.theme": "Design",
  "settings.density": "Dichte",
  "settings.language": "Sprache",
  "settings.languageHint":
    "Bisher sind die wichtigsten Bereiche übersetzt; der Rest ist auf Englisch.",
  "settings.languageSystem": "System ({name})",

  "density.compact": "Kompakt",
  "density.comfortable": "Normal",
  "density.spacious": "Großzügig",
  "settings.motion": "Bewegung",
  "settings.motionHint":
    "System folgt der Windows-Einstellung für Animationen. Reduziert behält kurze Überblendungen und stoppt Bewegung.",
  "motion.system": "System",
  "motion.full": "Voll",
  "motion.reduced": "Reduziert",
  "motion.off": "Aus",
  "settings.backdrop": "Fensterhintergrund",
  "settings.backdropHint":
    "Mica färbt den Rahmen mit der Desktopfarbe; Acrylic zeichnet alles hinter dem Fenster weich (Windows 11). Mit geringerer Terminal-Deckkraft (Terminal-Einstellungen) wird auch das Terminal durchscheinend.",
  "backdrop.solid": "Deckend",
  "backdrop.mica": "Mica",
  "backdrop.acrylic": "Acrylic",

  "import.title": "Importieren",
  "import.fromConfig": "Aus ~/.ssh/config",
  "import.fromApp": "Aus einer anderen App",
  "import.fromKnownHosts": "Aus ~/.ssh/known_hosts",
  "import.knownHint": "Host-Schlüssel übernehmen, denen du in OpenSSH schon vertraut hast.",
  "import.knownButton": "Vertraute Schlüssel importieren",
  "import.readComputer": "Von diesem Computer lesen",
  "import.chooseFile": "Datei wählen…",
  "import.chooseFolder": "Ordner wählen…",
  "import.sessions": "{count} Sitzungen importieren",
  "import.sessionsOne": "1 Sitzung importieren",
  "import.noPasswords": "Passwörter werden nicht übernommen; Hopr fragt beim ersten Verbinden.",
  "import.none": "Keine Sitzungen gefunden.",

  "history.title": "Verlauf",
  "history.commands": "Befehle",
  "history.connections": "Verbindungen",
  "history.recordings": "Aufnahmen",
  "history.search": "Befehle suchen",
  "history.allHosts": "Alle Hosts",
  "history.clear": "Verlauf löschen…",
};
