import { describe, expect, it } from "vitest";
import { passwordStrength } from "./strength";

describe("passwordStrength", () => {
  it("grades by length, variety and passphrases", () => {
    expect(passwordStrength("short").score).toBe(0);
    expect(passwordStrength("abcdefgh").score).toBe(1);
    expect(passwordStrength("Abcdefgh1234").score).toBe(2);
    expect(passwordStrength("Abcdefgh1234!xyz").score).toBe(3);
    expect(passwordStrength("correct horse battery staple").score).toBe(3);
  });

  it("rejects obvious patterns", () => {
    expect(passwordStrength("aaaaaaaaaaaaaaaaaaa").score).toBe(0);
    expect(passwordStrength("password12345678!A").score).toBe(0);
  });
});
