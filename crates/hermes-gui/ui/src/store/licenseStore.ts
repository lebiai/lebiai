import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { useChatStore } from "./chatStore";

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
  /** 本机应显示的角色 id（自带不在码里，由后端补上）。 */
  personas: string[];
  /** 码里点名但本版本名册里没有的 id——要显示给用户，不许静默。 */
  unknownPersonas: string[];
}

interface LicenseState {
  status: LicenseStatus | null;
  loaded: boolean;
  /** Increment to scroll/focus settings license block. */
  focusRequestId: number;
  refresh: () => Promise<LicenseStatus | null>;
  applyToken: (token: string) => Promise<ApplyTokenResult>;
  markNudgeSeen: () => Promise<void>;
  requestLicenseFocus: () => void;
}

export interface ApplyTokenResult {
  status: LicenseStatus;
  /**
   * 这张码落定后开通的授权角色 id（后端已按名册 + 授权过滤；自带不在其中）。
   * 老格式码（不带名单）→ 空数组。
   */
  enabledPersonas: string[];
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
      const res = await invoke<{
        status: LicenseStatus;
        message: string;
        enabledPersonas: string[];
      }>("apply_license", { token });
      set({ status: res.status, loaded: true });
      // 新码可能带来新的角色名单（也可能撤掉）——工位列表必须跟着换，
      // 否则用户输了码却在侧栏里看不到刚开通的工位。**等它回来**再返回，
      // 调用方（表单）才能立刻按名字说「已开通哪几个工位」。
      await useChatStore.getState().fetchPersonas();
      // 桌上的人跟着授权名单变：谁缺席是同一份判据算出来的。
      await useChatStore.getState().fetchTeams();
      return {
        status: res.status,
        enabledPersonas: res.enabledPersonas ?? [],
      };
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
