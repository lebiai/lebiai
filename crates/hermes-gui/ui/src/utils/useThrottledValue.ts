import { useEffect, useRef, useState } from "react";

/**
 * 把一个**高频变化**的值按 `ms` 节流后再交给渲染。
 *
 * 为什么需要它：模型一秒吐几十个 token，而每渲染一次都要把整段 markdown 重新解析 +
 * 重建元素树。客户端实测（同一份资讯卷切片、`flushSync` 计时）：1k 字 2.6ms、
 * 8k 字 4.1ms、20k 字 9ms —— 每个 token 渲一次等于把主线程吃掉一半。
 *
 * 边界：
 * - 首次更新立刻生效（leading edge），用户不会等第一个字；
 * - 停更后最后一次一定落地（trailing edge），收尾那段不会丢；
 * - 只节流**显示**，不动 store 里的真值 —— 落盘/定格用的仍是完整文本。
 */
export function useThrottledValue<T>(value: T, ms: number): T {
  const [shown, setShown] = useState(value);
  const lastAt = useRef(0);

  useEffect(() => {
    if (shown === value) return;
    const wait = Math.max(0, ms - (Date.now() - lastAt.current));
    const id = window.setTimeout(() => {
      lastAt.current = Date.now();
      setShown(value);
    }, wait);
    return () => window.clearTimeout(id);
  }, [value, ms, shown]);

  return shown;
}
