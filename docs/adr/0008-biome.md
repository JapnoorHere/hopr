# ADR 0008: Biome for TS lint and format

**Status:** Accepted · 2026-10-16

## Decision
Biome handles linting and formatting for TS, JS, JSON, CSS and Svelte script blocks. svelte-check (strict) covers types. A custom `scripts/check-tokens.mjs` forbids raw colors in `.svelte` files outside `src/lib/design`.

## Notes
Tailwind v4 directives (`@theme`, `@utility`) need `--css-parse-tailwind-directives=true`, which is passed in the `lint` and `format` scripts.
