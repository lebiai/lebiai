/**
 * 把后端错误翻成一句用户看得懂的话。
 *
 * 后端把「可翻译的失败」序列化成 `config: <code>` / `internal: <text>`（见
 * `crates/hermes-gui/src/error.rs`）。以前界面直接 `String(e)` 把引擎英文吐给用户，
 * 于是「同一条记忆已经存过」看起来像一次故障。这里只做一件事：认识的那几个 code
 * 翻成人话，不认识的照原样显示（不遮错 —— 看不懂的原文比一句假的安慰好）。
 */
import { useUiStore } from "../store/uiStore";

const CODES: Record<string, string> = {
  memory_duplicate: "error.memoryDuplicate",
};

export function errorText(e: unknown): string {
  const t = useUiStore.getState().t;
  const raw = e instanceof Error ? e.message : String(e);
  for (const [code, key] of Object.entries(CODES)) {
    if (raw.includes(code)) return t(key as never);
  }
  return raw.replace(/^(config|internal|provider|tool|session|not found):\s*/, "");
}
