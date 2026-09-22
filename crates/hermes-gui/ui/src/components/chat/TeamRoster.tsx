/**
 * 组会话顶部那条名册：这张桌子上坐着谁、谁缺席、**这一棒现在在谁手上**。
 *
 * **缺席不许静默**（规格 §7.1）：不在册的人照样列出来，只是灰掉、写明「缺席 ·
 * 需要「他」」——用户得知道这活为什么跑不起来、从哪儿能把它补齐。
 * 这里不做可点的假按钮：点了没反应比没有更让人生气（2026-09-13 约定）。
 *
 * 棒跟产物走，不在这点「交给…」。用户要跳步或加料，说话点名桌上的人。
 */
import { useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  UserCheck,
  Users,
} from "lucide-react";
import type { TeamItem } from "../../types";
import { useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";

export function TeamRoster({ team }: { team: TeamItem }) {
  const t = useUiStore((s) => s.t);
  const episode = useChatStore((s) => s.episode);
  const [open, setOpen] = useState(false);
  const present = team.members.length - team.missing;
  const holderId = episode?.holderId ?? team.speakerId;
  const holderName =
    team.members.find((m) => m.id === holderId)?.name ?? team.speakerId;

  return (
    <div className="shrink-0 border-b border-app-border dark:border-slate-800 bg-app-muted/40 dark:bg-slate-900/40">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="w-full flex items-center gap-1.5 px-4 py-1.5 text-app-sub text-app-fg-secondary dark:text-slate-400 hover:text-app-fg dark:hover:text-slate-300 transition-colors duration-[var(--motion-fast)]"
      >
        {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        <Users size={12} strokeWidth={1.75} />
        <span>{t("team.table", { count: present })}</span>
        <span className="text-app-fg-secondary dark:text-slate-400">
          {t("team.holdingBaton", { name: holderName })}
        </span>
        {team.missing > 0 && (
          <span className="text-amber-600 dark:text-amber-400">
            {t("team.rosterAbsent", { count: team.missing })}
          </span>
        )}
      </button>

      {open && (
        <>
          <ul className="px-4 pb-2 pt-0.5 space-y-0.5">
            {team.members.map((m) => (
              <li
                key={m.id}
                className={`flex items-baseline gap-2 text-app-sub leading-relaxed ${
                  m.present
                    ? "text-app-fg-secondary dark:text-slate-400"
                    : "text-app-fg-tertiary dark:text-slate-600"
                }`}
              >
                <span className="w-16 shrink-0 truncate">{m.name}</span>
                <span className="min-w-0 flex-1 truncate">{m.duty}</span>
                {m.id === holderId && m.present ? (
                  <span className="inline-flex shrink-0 items-center gap-1 text-app-primary dark:text-blue-400">
                    <UserCheck size={11} strokeWidth={1.75} />
                    {t("handoff.holding")}
                  </span>
                ) : (
                  !m.present && (
                    <span className="shrink-0 text-amber-600 dark:text-amber-400">
                      {t("team.rosterNeed", { name: m.name })}
                    </span>
                  )
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
