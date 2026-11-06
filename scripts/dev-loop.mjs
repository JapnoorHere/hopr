// `pnpm dev:app`: `pnpm tauri dev` that survives Hopr restarting itself (switching profile,
// restoring a backup). A plain restart ends the process `tauri dev` watches, which stops the
// dev server too; here the app leaves a marker and exits, and this loop starts it again.
import { spawnSync } from "node:child_process";
import { existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const marker = join(tmpdir(), "hopr-dev-restart");
rmSync(marker, { force: true });

for (;;) {
  const run = spawnSync("pnpm", ["tauri", "dev", ...process.argv.slice(2)], {
    stdio: "inherit",
    shell: true,
    env: { ...process.env, HOPR_DEV_LOOP: "1" },
  });
  if (!existsSync(marker)) process.exit(run.status ?? 0);
  rmSync(marker, { force: true });
  console.log("\n[dev-loop] Hopr asked to restart; starting it again…\n");
}
