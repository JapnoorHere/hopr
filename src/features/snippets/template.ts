/**
 * Snippet templates (§9.12): `{{name}}`, `{{name:default}}`, `{{name:a|b|c}}` (a choice),
 * `{{a|b|c}}` (an unnamed choice), built-ins `{{host}} {{user}} {{date}} {{clipboard}}`,
 * and `{{secret:x}}`, which is left as is: the backend fills it in while typing it, so the
 * value never reaches the UI.
 */

export const BUILTINS = ["host", "user", "date", "clipboard"] as const;
export type Builtins = Record<(typeof BUILTINS)[number], string>;

export interface Field {
  /** Key in the values passed to `render`. */
  key: string;
  /** Label shown in the form. */
  label: string;
  kind: "text" | "choice";
  default: string;
  options: string[];
}

const TOKEN = /\{\{\s*([^{}]+?)\s*\}\}/g;

interface Parsed {
  key: string;
  field: Field | null;
  builtin: string | null;
  secret: string | null;
}

function parse(inner: string, index: number): Parsed {
  if (inner.startsWith("secret:")) {
    return { key: "", field: null, builtin: null, secret: inner.slice(7).trim() };
  }
  if ((BUILTINS as readonly string[]).includes(inner)) {
    return { key: inner, field: null, builtin: inner, secret: null };
  }
  const colon = inner.indexOf(":");
  if (colon === -1 && inner.includes("|")) {
    const options = inner.split("|").map((o) => o.trim());
    const key = `choice${index}`;
    return {
      key,
      field: { key, label: "Choose", kind: "choice", default: options[0] ?? "", options },
      builtin: null,
      secret: null,
    };
  }
  const name = (colon === -1 ? inner : inner.slice(0, colon)).trim();
  const rest = colon === -1 ? "" : inner.slice(colon + 1);
  if (rest.includes("|")) {
    const options = rest.split("|").map((o) => o.trim());
    return {
      key: name,
      field: { key: name, label: name, kind: "choice", default: options[0] ?? "", options },
      builtin: null,
      secret: null,
    };
  }
  return {
    key: name,
    field: { key: name, label: name, kind: "text", default: rest.trim(), options: [] },
    builtin: null,
    secret: null,
  };
}

function tokens(body: string): Parsed[] {
  return [...body.matchAll(TOKEN)].map((m, i) => parse(m[1] ?? "", i));
}

/** What to ask before running: each variable once, in order of first use. */
export function fields(body: string): Field[] {
  const seen = new Set<string>();
  const out: Field[] = [];
  for (const t of tokens(body)) {
    if (!t.field || seen.has(t.field.key)) continue;
    seen.add(t.field.key);
    out.push(t.field);
  }
  return out;
}

/** Secret names the snippet uses. */
export function secretsIn(body: string): string[] {
  return [...new Set(tokens(body).flatMap((t) => (t.secret ? [t.secret] : [])))];
}

/** Whether the body needs the clipboard (read it only then). */
export function usesClipboard(body: string): boolean {
  return tokens(body).some((t) => t.builtin === "clipboard");
}

/** Fill variables and built-ins; `{{secret:…}}` stays for the backend. */
export function render(body: string, values: Record<string, string>, builtins: Builtins): string {
  let i = 0;
  return body.replace(TOKEN, (raw, inner: string) => {
    const t = parse(inner, i++);
    if (t.secret !== null) return `{{secret:${t.secret}}}`;
    if (t.builtin) return builtins[t.builtin as keyof Builtins] ?? raw;
    return values[t.key] ?? t.field?.default ?? "";
  });
}

/** Today as YYYY-MM-DD (local time). */
export function today(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
