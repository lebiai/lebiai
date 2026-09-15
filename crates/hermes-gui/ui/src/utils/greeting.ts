/**
 * Light time-of-day return greeting for empty chat (phase D).
 * Not Reflect / engine jargon — just a calm “you’re back”.
 */

import type { TranslationKey } from "../i18n";
import { isOnboardingDone } from "./onboarding";

/**
 * 首页问候语的时间段 key —— 空会话首页**唯一**的文案。
 * 引导还没走完时返回 null：那时首页被欢迎仪式覆盖，不需要再摆一句。
 */
export function returnGreetingKey(): TranslationKey | null {
  if (!isOnboardingDone()) return null;
  const hour = new Date().getHours();
  if (hour >= 5 && hour < 12) return "welcome.returnMorning";
  if (hour >= 12 && hour < 18) return "welcome.returnAfternoon";
  if (hour >= 18 && hour < 23) return "welcome.returnEvening";
  return "welcome.returnLate";
}

/** Time-of-day greeting part used by the settings header (5 segments). */
export type GreetingPart = "morning" | "noon" | "afternoon" | "evening" | "late";

export function timeGreetingPart(hour = new Date().getHours()): GreetingPart {
  if (hour >= 5 && hour < 11) return "morning";
  if (hour >= 11 && hour < 13) return "noon";
  if (hour >= 13 && hour < 18) return "afternoon";
  if (hour >= 18 && hour < 23) return "evening";
  return "late";
}
