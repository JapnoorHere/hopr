/** Rough master-password strength, for a hint only (the vault accepts any 8+ chars). */
export type Strength = { score: 0 | 1 | 2 | 3; label: string };

export function passwordStrength(pw: string): Strength {
  if (pw.length < 8) return { score: 0, label: "Too short" };
  if (/^(.)\1+$/.test(pw) || /^(password|12345678|qwerty)/i.test(pw)) {
    return { score: 0, label: "Too easy to guess" };
  }
  const classes = [/[a-z]/, /[A-Z]/, /\d/, /[^A-Za-z0-9]/].filter((r) => r.test(pw)).length;
  const words = pw.trim().split(/\s+/).length;
  let points = (pw.length >= 12 ? 1 : 0) + (pw.length >= 16 ? 1 : 0) + (classes >= 3 ? 1 : 0);
  if (words >= 4) points += 1; // passphrases are strong even in one character class
  if (points >= 3) return { score: 3, label: "Strong" };
  if (points === 2) return { score: 2, label: "Good" };
  return { score: 1, label: "Weak" };
}
