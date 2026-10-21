/**
 * Vault lifecycle on the UI side: onboarding → unlock → app, plus lock (manual, idle).
 * Activity pings keep the backend's auto-lock timer fresh while the user works.
 */
import { type AppError, commands, events, inTauri, type VaultStatus } from "$lib/ipc";
import { isMainWindow } from "$lib/window";

export type Result<T> = { ok: true; data: T } | { ok: false; error: AppError };

export function wrap<T>(
  res: { status: "ok"; data: T } | { status: "error"; error: AppError },
): Result<T> {
  return res.status === "ok" ? { ok: true, data: res.data } : { ok: false, error: res.error };
}

const ACTIVITY_PING_MS = 30_000;

class VaultStore {
  /** `null` until the first status call returns. */
  status = $state<VaultStatus | null>(null);
  /** True once the app has been unlocked in this run (keeps sessions mounted behind the lock). */
  everUnlocked = $state(false);
  lockReason = $state<string | null>(null);
  /** Just created: keep onboarding up until the recovery key is acknowledged. */
  showRecovery = $state(false);

  #lastPing = 0;

  get unlocked(): boolean {
    return this.status?.unlocked ?? false;
  }

  async init(): Promise<void> {
    if (!inTauri) {
      // Plain-browser preview: behave as an unlocked, empty vault.
      this.status = {
        exists: true,
        unlocked: true,
        remembered: false,
        autoLockMinutes: 0,
        clipboardClearSecs: 30,
        biometric: false,
        biometricAvailable: false,
      };
      this.everUnlocked = true;
      return;
    }
    await events.vaultLockedEvent.listen((e) => {
      this.#setUnlocked(false);
      this.lockReason = e.payload.reason;
    });
    await this.refresh();
    // Every window runs this; only the main window prompts at startup.

    if (isMainWindow && this.status?.exists && !this.status.unlocked) {
      if (this.status.biometric) await this.unlockBiometric();
      else if (this.status.remembered) await this.unlockRemembered();
    }
    const onActivity = () => this.activity();
    window.addEventListener("keydown", onActivity, { passive: true, capture: true });
    window.addEventListener("pointerdown", onActivity, { passive: true, capture: true });
  }

  async refresh(): Promise<void> {
    const res = await commands.vaultStatus();
    if (res.status === "ok") {
      this.status = res.data;
      if (res.data.unlocked) this.everUnlocked = true;
    }
  }

  #setUnlocked(unlocked: boolean) {
    if (this.status) this.status = { ...this.status, unlocked };
    if (unlocked) {
      this.everUnlocked = true;
      this.lockReason = null;
    }
  }

  async create(password: string): Promise<Result<string>> {
    const res = wrap(await commands.vaultCreate(password));
    if (res.ok) {
      this.showRecovery = true;
      if (this.status) this.status = { ...this.status, exists: true };
      this.#setUnlocked(true);
    }
    return res;
  }

  async unlock(password: string, remember: boolean): Promise<Result<null>> {
    const res = wrap(await commands.vaultUnlock(password, remember));
    if (res.ok) {
      if (this.status) this.status = { ...this.status, remembered: remember };
      this.#setUnlocked(true);
    }
    return res;
  }

  async unlockRemembered(): Promise<Result<null>> {
    const res = wrap(await commands.vaultUnlockRemembered());
    if (res.ok) this.#setUnlocked(true);
    else if (this.status) this.status = { ...this.status, remembered: false };
    return res;
  }

  /** Windows Hello, then the key kept in Credential Manager. */
  async unlockBiometric(): Promise<Result<null>> {
    const res = wrap(await commands.vaultUnlockBiometric());
    if (res.ok) this.#setUnlocked(true);
    return res;
  }

  async setBiometric(enabled: boolean): Promise<Result<null>> {
    const res = wrap(await commands.vaultSetBiometric(enabled));
    if (res.ok && this.status) {
      this.status = { ...this.status, biometric: enabled, remembered: enabled };
    }
    return res;
  }

  async recover(recoveryKey: string, newPassword: string): Promise<Result<null>> {
    const res = wrap(await commands.vaultRecover(recoveryKey, newPassword));
    if (res.ok) {
      if (this.status) this.status = { ...this.status, remembered: false };
      this.#setUnlocked(true);
    }
    return res;
  }

  async changePassword(current: string, next: string): Promise<Result<null>> {
    return wrap(await commands.vaultChangePassword(current, next));
  }

  async setAutoLock(minutes: number): Promise<Result<null>> {
    const res = wrap(await commands.vaultSetAutoLock(minutes));
    if (res.ok && this.status) this.status = { ...this.status, autoLockMinutes: minutes };
    return res;
  }

  async setClipboardClear(seconds: number): Promise<Result<null>> {
    const res = wrap(await commands.vaultSetClipboardClear(seconds));
    if (res.ok && this.status) this.status = { ...this.status, clipboardClearSecs: seconds };
    return res;
  }

  async forgetDevice(): Promise<void> {
    await commands.vaultForgetDevice();
    if (this.status) this.status = { ...this.status, remembered: false, biometric: false };
  }

  async lock(): Promise<void> {
    if (!inTauri || !this.unlocked) return;
    await commands.vaultLock();
    this.lockReason = "manual";
    this.#setUnlocked(false);
  }

  activity() {
    const now = Date.now();
    if (!inTauri || !this.unlocked || now - this.#lastPing < ACTIVITY_PING_MS) return;
    this.#lastPing = now;
    void commands.vaultActivity();
  }
}

export const vault = new VaultStore();
