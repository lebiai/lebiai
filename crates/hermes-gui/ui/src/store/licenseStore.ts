import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export type LicensePhase = "trial" | "licensed" | "locked";
export type LicenseUrgency = "ample" | "expiring" | "expired";

export interface LicenseStatus {
  phase: LicensePhase;
  urgency: LicenseUrgency;
  canUseMain: boolean;
  showFullLock: boolean;
  shouldNudge: boolean;
  expiresAt: string | null;
  remainingSecs: number;
  remainingRatio: number;
  onTrial: boolean;
  wechat: string;
  licId?: string | null;
  plan?: string | null;
}

interface LicenseState {
  status: LicenseStatus | null;
  loaded: boolean;
  /** Increment to scroll/focus settings license block. */
  focusRequestId: number;
  refresh: () => Promise<LicenseStatus | null>;
  applyToken: (token: string) => Promise<LicenseStatus>;
  markNudgeSeen: () => Promise<void>;
  requestLicenseFocus: () => void;
}

function parseApplyError(e: unknown): string {
  const s = String(e);
  // GuiError serializes as "config: license_xxx" or bare code
  const m = s.match(/license_[a-z_]+/);
  return m ? m[0] : s;
}

export const useLicenseStore = create<LicenseState>((set, get) => ({
  status: null,
  loaded: false,
  focusRequestId: 0,

  refresh: async () => {
    try {
      const status = await invoke<LicenseStatus>("get_license_status");
      set({ status, loaded: true });
      return status;
    } catch {
      set({ loaded: true });
      return get().status;
    }
  },

  applyToken: async (token: string) => {
    try {
      const res = await invoke<{ status: LicenseStatus; message: string }>(
        "apply_license",
        { token },
      );
      set({ status: res.status, loaded: true });
      return res.status;
    } catch (e) {
      throw new Error(parseApplyError(e));
    }
  },

  markNudgeSeen: async () => {
    try {
      const status = await invoke<LicenseStatus>("mark_license_nudge_seen");
      set({ status });
    } catch {
      /* ignore */
    }
  },

  requestLicenseFocus: () => {
    set((s) => ({ focusRequestId: s.focusRequestId + 1 }));
  },
}));

export function formatRemaining(
  secs: number,
  t: (k: string, p?: Record<string, string | number>) => string,
): string {
  if (secs <= 0) return t("license.remainingNone");
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  if (days >= 2) return t("license.remainingDays", { n: days });
  if (days === 1) return t("license.remainingOneDay");
  if (hours >= 1) return t("license.remainingHours", { n: hours });
  return t("license.remainingSoon");
}

export function formatExpiresAt(iso: string | null | undefined, locale: string): string {
  if (!iso) return "—";
  try {
    const d = new Date(iso);
    return d.toLocaleDateString(locale === "zh-CN" ? "zh-CN" : "en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  } catch {
    return iso;
  }
}
