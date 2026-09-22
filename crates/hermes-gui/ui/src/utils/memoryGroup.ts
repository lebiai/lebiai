import type { TranslationKey } from "../i18n";

/**
 * 「这条记忆算哪一类」——**判据只有这里一处**。
 *
 * 记忆页用它分组、待确认卡用它显示候选的归类、筛选用它过滤。三处各判一遍，
 * 就会出现「侧栏说它是标准、卡片说它是其他」。
 */
export type CompanionGroup = "preferences" | "standards" | "work" | "other";

/** 只看 `zone` 与 `tags`；归属（`owner`）是另一条轴，不在这里判。 */
export function companionGroup(mem: {
  zone?: string | null;
  tags?: string[] | null;
}): CompanionGroup {
  const z = (mem.zone || "").toLowerCase();
  const tags = (mem.tags ?? []).map((tag) => tag.toLowerCase());
  if (z === "preferences" || z === "preference" || z === "core" || tags.includes("preference")) {
    return "preferences";
  }
  if (z === "standards" || z === "standard" || tags.includes("standard")) {
    return "standards";
  }
  if (z === "work" || z === "episode" || z === "work-episode" || tags.includes("work-episode")) {
    return "work";
  }
  return "other";
}

/** 分类给人看的名字。认不出来的原始 `zone` 原样返回，不假装认识。 */
export function memoryGroupLabel(
  t: (key: TranslationKey, params?: Record<string, string | number>) => string,
  id: string
): string {
  switch (id) {
    case "preferences":
      return t("memory.groupPreferences");
    case "standards":
      return t("memory.groupStandards");
    case "work":
      return t("memory.groupWork");
    case "other":
      return t("memory.groupOther");
    default:
      return id;
  }
}
