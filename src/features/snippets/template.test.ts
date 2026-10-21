import { describe, expect, it } from "vitest";
import { fields, render, secretsIn, today, usesClipboard } from "./template";

const B = { host: "db-1", user: "ann", date: "2026-10-18", clipboard: "copied" };

describe("snippet templates", () => {
  it("lists each variable once with defaults and choices", () => {
    const body =
      "psql -h {{host}} -U {{user:postgres}} -d {{db}} -c '{{db}}' {{env:dev|staging|prod}} {{fast|slow}}";
    expect(fields(body)).toEqual([
      { key: "user", label: "user", kind: "text", default: "postgres", options: [] },
      { key: "db", label: "db", kind: "text", default: "", options: [] },
      {
        key: "env",
        label: "env",
        kind: "choice",
        default: "dev",
        options: ["dev", "staging", "prod"],
      },
      {
        key: "choice5",
        label: "Choose",
        kind: "choice",
        default: "fast",
        options: ["fast", "slow"],
      },
    ]);
  });

  it("renders values, defaults and built-ins, keeping secrets for the backend", () => {
    const body =
      "mysql -h {{host}} -u {{ user:root }} -p{{secret: db.pass }} {{db}} # {{date}} {{clipboard}}";
    expect(render(body, { db: "app" }, B)).toBe(
      "mysql -h db-1 -u root -p{{secret:db.pass}} app # 2026-10-18 copied",
    );
    expect(render("{{user:root}}", { user: "x" }, B)).toBe("x");
    expect(render("{{a|b}}", {}, B)).toBe("a");
    expect(render("{{a|b}}", { choice0: "b" }, B)).toBe("b");
  });

  it("finds secrets and clipboard use", () => {
    expect(secretsIn("{{secret:a}} {{secret:b}} {{secret:a}}")).toEqual(["a", "b"]);
    expect(usesClipboard("echo {{clipboard}}")).toBe(true);
    expect(usesClipboard("echo {{host}}")).toBe(false);
    expect(fields("plain text, no {braces}")).toEqual([]);
  });

  it("formats today's date", () => {
    expect(today(new Date(2026, 0, 5))).toBe("2026-01-05");
  });
});
