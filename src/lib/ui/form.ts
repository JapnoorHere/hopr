import { tick } from "svelte";
import { shake } from "$lib/motion";

/** Field name → problem. Empty when the form is fine. */
export type FieldErrors<K extends string = string> = Partial<Record<K, string>>;

export const hasErrors = (e: FieldErrors) => Object.values(e).some(Boolean);

/**
 * After validation flags fields: once the DOM shows the errors, scroll the first flagged
 * field into view and focus it, so the user sees what to fix.
 */
export async function focusFirstInvalid(root: ParentNode | null | undefined) {
  await tick();
  const flagged = [
    ...(root?.querySelectorAll<HTMLElement>('[aria-invalid="true"], [data-invalid="true"]') ?? []),
  ];
  const el = flagged.at(0);
  if (!el) return;
  // Each flagged field shakes once, so the problems are found at a glance.
  for (const f of flagged.slice(0, 4)) shake(f, 5);
  el.scrollIntoView({ block: "center", behavior: "smooth" });
  const target = el.matches("input, select, textarea, button")
    ? el
    : el.querySelector<HTMLElement>("input, select, textarea, button");
  target?.focus({ preventScroll: true });
}

/** A port typed into a text field, or null when it isn't 1–65535. */
export function parsePort(text: string): number | null {
  const n = Number(text.trim());
  return text.trim() && Number.isInteger(n) && n >= 1 && n <= 65535 ? n : null;
}
