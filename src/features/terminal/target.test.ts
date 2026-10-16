import { describe, expect, it } from "vitest";
import { parseTarget } from "./target";

describe("parseTarget", () => {
  it.each([
    ["prod-db", { host: "prod-db" }],
    ["deploy@prod-db", { host: "prod-db", username: "deploy" }],
    ["prod-db:2222", { host: "prod-db", port: 2222 }],
    ["deploy@10.0.3.4:22", { host: "10.0.3.4", username: "deploy", port: 22 }],
    ["ssh://me@host:2200/", { host: "host", username: "me", port: 2200 }],
    ["root@[::1]:2222", { host: "::1", username: "root", port: 2222 }],
    ["fe80::1", { host: "fe80::1" }],
    ["a@b@host", { host: "host", username: "a@b" }],
  ])("parses %s", (input, expected) => {
    expect(parseTarget(input)).toEqual(expected);
  });

  it.each(["", "   ", "host:0", "host:70000", "host:abc", "@host", "two words", "user@"])(
    "rejects %j",
    (input) => {
      expect(parseTarget(input)).toBeNull();
    },
  );
});
