import { expect, it, describe as suite } from "vitest";
import type { Tunnel } from "$lib/ipc";
import { describe, exposed, rate, sparkline } from "./format";

const t = (over: Partial<Tunnel>): Tunnel => ({
  id: "t",
  hostId: "h",
  label: "",
  kind: "local",
  bindAddr: "127.0.0.1",
  bindPort: 15432,
  destHost: "db",
  destPort: 5432,
  autoStart: false,
  ...over,
});

suite("tunnel format", () => {
  it("describes each kind, with the bound port when known", () => {
    expect(describe(t({}), null)).toBe("127.0.0.1:15432 → db:5432");
    expect(describe(t({ bindPort: 0 }), 50123)).toBe("127.0.0.1:50123 → db:5432");
    expect(describe(t({ bindPort: 0 }), null)).toBe("127.0.0.1:auto → db:5432");
    expect(describe(t({ kind: "dynamic", bindPort: 1080 }), null)).toBe("SOCKS5 on 127.0.0.1:1080");
    expect(
      describe(t({ kind: "remote", bindPort: 8080, destHost: "localhost", destPort: 3000 }), null),
    ).toBe("host 127.0.0.1:8080 → localhost:3000 (here)");
  });

  it("flags addresses open to the network", () => {
    expect(exposed("0.0.0.0")).toBe(true);
    expect(exposed(" :: ")).toBe(true);
    expect(exposed("127.0.0.1")).toBe(false);
  });

  it("formats rates and draws sparklines", () => {
    expect(rate(0)).toBe("0 B/s");
    expect(rate(512)).toBe("512 B/s");
    expect(rate(2048)).toBe("2.0 KB/s");
    expect(rate(3 * 1024 * 1024)).toBe("3.0 MB/s");
    expect(sparkline([5], 100, 20)).toBe("");
    const pts = sparkline([0, 10], 118, 20).split(" ");
    expect(pts).toHaveLength(2);
    expect(pts[1]).toBe("118.0,1.0");
  });
});
