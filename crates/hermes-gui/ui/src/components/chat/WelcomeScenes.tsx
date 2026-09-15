/**
 * 空会话的首页：**只有一句问候**。
 *
 * 产品决定（2026-09-15）：首页原先那套（场景卡、大标题、标语、底部提示语）
 * 已经不适用，全部撤掉。首页是一句问候，不是一张菜单——要干什么，用户说出来
 * 就行，不用我们先摆四个选项替他决定。
 *
 * **美感只能从这一句话来**（见 `index.css` 的 `.greet-*`）：
 * 逐字浮入（模糊→实）、前后两段层次、破折号收小当分隔。
 * 这里**不套 `AmbientStage`**、不加任何背板 —— 那层光晕是给「有内容的 hero」
 * 当底的，只剩一行字时它会读成一块空色块。也不加卡片 / 插画 / 标志去撑场面。
 *
 * 问候语按时间给（早/午/晚/深夜）；引导流程还没走完时不显示（那时首页被
 * 引导覆盖，没有「空白」的问题）。
 */
import type { CSSProperties } from "react";
import { useUiStore } from "../../store/uiStore";
import { returnGreetingKey } from "../../utils/greeting";

/** 中英两种破折号：把问候语切成「问候 / 分隔 / 动作」三段。 */
const SPLIT = /^(.*?)(——|—)(.*)$/;

function Chars({
  text,
  start,
  className = "greet-ch",
}: {
  text: string;
  start: number;
  className?: string;
}) {
  return (
    <>
      {Array.from(text).map((ch, i) => (
        <span
          key={`${start + i}-${ch}`}
          className={className}
          style={{ "--greet-i": start + i } as CSSProperties}
        >
          {ch === " " ? "\u00A0" : ch}
        </span>
      ))}
    </>
  );
}

export function WelcomeScenes() {
  const t = useUiStore((s) => s.t);
  const greetingKey = returnGreetingKey();
  if (!greetingKey) return null;

  const text = t(greetingKey);
  const parts = text.match(SPLIT);
  const head = parts ? parts[1].trimEnd() : text;
  const sep = parts ? parts[2] : "";
  const tail = parts ? parts[3].trimStart() : "";
  const charCount = head.length + sep.length + tail.length;

  return (
    <div className="flex items-center justify-center px-4 min-h-[66vh]">
      <div
        className="greet-block"
        style={{ "--greet-chars": charCount } as CSSProperties}
      >
        <p className="greet-line">
          {/* 逐字拆开后屏幕阅读器会一个字一个字念，所以给它整句、把字标成装饰。 */}
          <span className="sr-only">{text}</span>
          <span aria-hidden="true">
            <Chars text={head} start={0} />
            {sep && (
              <span
                className="greet-ch greet-sep"
                style={{ "--greet-i": head.length } as CSSProperties}
              >
                {sep}
              </span>
            )}
            <Chars
              text={tail}
              start={head.length + sep.length}
              className="greet-ch greet-tail"
            />
          </span>
        </p>
        <span className="greet-rule" aria-hidden="true" />
      </div>
    </div>
  );
}
