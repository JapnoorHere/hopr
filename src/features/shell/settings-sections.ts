/** The Settings sections, in order. `keywords` lets search find a section by words it doesn't show. */
import {
  AppWindow,
  Archive,
  ArrowsClockwise,
  ClockCounterClockwise,
  FolderSimple,
  Keyboard,
  Palette,
  ShieldCheck,
  SquaresFour,
  TerminalWindow,
  UserCircle,
} from "phosphor-svelte";

export const SECTIONS = [
  {
    id: "appearance",
    label: "Appearance",
    icon: Palette,
    keywords:
      "theme dark light colour color custom terminal palette ansi density motion animation language",
  },
  {
    id: "profiles",
    label: "Profiles",
    icon: UserCircle,
    keywords: "profile profiles switch separate vault account picker startup",
  },
  {
    id: "terminal",
    label: "Terminal",
    icon: TerminalWindow,
    keywords: "font size cursor scrollback copy paste bell blocks",
  },
  {
    id: "keyboard",
    label: "Keyboard",
    icon: Keyboard,
    keywords: "shortcuts keys bindings hotkeys",
  },
  {
    id: "workspaces",
    label: "Workspaces",
    icon: SquaresFour,
    keywords: "layout tabs restore session",
  },
  {
    id: "transfers",
    label: "Files & transfers",
    icon: FolderSimple,
    keywords: "sftp upload download speed limit parallel",
  },
  {
    id: "history",
    label: "History",
    icon: ClockCounterClockwise,
    keywords: "commands recording shell integration exclude",
  },
  {
    id: "sync",
    label: "Sync",
    icon: ArrowsClockwise,
    keywords: "devices folder onedrive dropbox passphrase",
  },
  {
    id: "security",
    label: "Security",
    icon: ShieldCheck,
    keywords: "vault lock password clipboard remember device audit",
  },
  {
    id: "backups",
    label: "Backups",
    icon: Archive,
    keywords: "restore export import settings file",
  },
  {
    id: "app",
    label: "App & updates",
    icon: AppWindow,
    keywords: "tray startup quick terminal hotkey update version about",
  },
] as const;

export type SectionId = (typeof SECTIONS)[number]["id"];
