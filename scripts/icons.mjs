#!/usr/bin/env node
// Rebuilds every app icon from the two masters in src-tauri/icons/source:
//   hopr-icon.svg        the full icon (48px and up)
//   hopr-icon-small.svg  a heavier cut for 16–32px (tray, taskbar at 100%)
// `tauri icon` makes all sizes from the full master; icon.ico and 32x32.png are then rebuilt so
// the small sizes come from the small cut. Usage: node scripts/icons.mjs
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const icons = join(root, "src-tauri", "icons");
// Relative to the repo: the Windows shell would split an absolute path at a space.
const full = "src-tauri/icons/source/hopr-icon.svg";
const small = "src-tauri/icons/source/hopr-icon-small.svg";
const SMALL_SIZES = [16, 24, 32];
const FULL_SIZES = [48, 64, 256];

const tauriIcon = (args) =>
  execFileSync("pnpm", ["tauri", "icon", ...args], {
    cwd: root,
    stdio: "inherit",
    shell: process.platform === "win32",
  });

/** PNGs of one master at the given sizes, as { size: Buffer }. */
function render(master, sizes) {
  const out = mkdtempSync(join(tmpdir(), "hopr-icons-"));
  try {
    tauriIcon([master, "-o", out, "-p", sizes.join(",")]);
    return Object.fromEntries(sizes.map((s) => [s, readFileSync(join(out, `${s}x${s}.png`))]));
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}

/** A Windows .ico holding PNG images (supported since Vista), smallest first. */
function ico(images) {
  const sizes = Object.keys(images)
    .map(Number)
    .sort((a, b) => a - b);
  const header = Buffer.alloc(6 + 16 * sizes.length);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(sizes.length, 4);
  let offset = header.length;
  sizes.forEach((size, i) => {
    const png = images[size];
    const at = 6 + 16 * i;
    header.writeUInt8(size >= 256 ? 0 : size, at);
    header.writeUInt8(size >= 256 ? 0 : size, at + 1);
    header.writeUInt8(0, at + 2);
    header.writeUInt8(0, at + 3);
    header.writeUInt16LE(1, at + 4);
    header.writeUInt16LE(32, at + 6);
    header.writeUInt32LE(png.length, at + 8);
    header.writeUInt32LE(offset, at + 12);
    offset += png.length;
  });
  return Buffer.concat([header, ...sizes.map((s) => images[s])]);
}

// 1. Every size and format from the full master (Windows, macOS, Store logos).
tauriIcon([full]);
// `tauri icon` also writes mobile folders; Hopr is desktop-only.
for (const dir of ["android", "ios"]) rmSync(join(icons, dir), { recursive: true, force: true });

// 2. The small sizes from the small cut.
const smallPngs = render(small, SMALL_SIZES);
const fullPngs = render(full, FULL_SIZES);
writeFileSync(join(icons, "icon.ico"), ico({ ...smallPngs, ...fullPngs }));
writeFileSync(join(icons, "32x32.png"), smallPngs[32]);

console.log(
  `icons rebuilt in ${icons}${existsSync(join(icons, "icon.ico")) ? " (icon.ico: 16, 24, 32 small cut; 48, 64, 256 full)" : ""}`,
);
