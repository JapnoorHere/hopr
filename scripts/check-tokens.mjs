// Fails if a .svelte file outside src/lib/design uses a raw color.
// Design tokens are the only source of color, so theming and density stay free.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";

const ROOT = "src";
const ALLOWED_DIR = join("src", "lib", "design");
const PATTERNS = [
  /(?<![{\w&])#[0-9a-fA-F]{3,8}\b/,
  /\b(?:rgb|rgba|hsl|hsla|oklch|oklab|lab|lch)\(/,
];

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (p.endsWith(".svelte")) out.push(p);
  }
  return out;
}

const problems = [];
for (const file of walk(ROOT)) {
  if (file.startsWith(ALLOWED_DIR + sep)) continue;
  readFileSync(file, "utf8")
    .split("\n")
    .forEach((line, i) => {
      if (PATTERNS.some((re) => re.test(line))) {
        problems.push(`${relative(".", file)}:${i + 1}: raw color, use a design token`);
      }
    });
}

if (problems.length) {
  console.error(problems.join("\n"));
  process.exit(1);
}
console.log("check-tokens: ok");
