#!/usr/bin/env bash
# 把一个构建日志的「失败原因」变成可读的 CI annotation。
#
# 为什么需要它：本仓库的 job 日志下载接口要 admin 权限（本机没 token），
# 跑挂了在界面上只剩 `exit code 1`，annotation 是唯一能读到原文的通道。
# 而 `::error::` 这类 annotation 每个 check run 只显示十来条，所以这里把
# 多行内容**塞进一条** annotation（GitHub 的 workflow command 转义：
# `%` → `%25`，换行 → `%0A`）。
#
# 用法： scripts/ci-annotate-log.sh <logfile> [exit-code]
# 副作用：只往 stdout 打 `::error::` / `::warning::`，不改文件、不依赖网络。

set -euo pipefail

LOG="${1:?log file}"
CODE="${2:-1}"

if [ ! -f "$LOG" ]; then
  echo "::error::annotate: log file missing: $LOG"
  exit 0
fi

# GitHub workflow command 的转义：先 % 再换行，然后把换行拼成 %0A。
escape() {
  sed -e 's/%/%25/g' -e 's/\r$//' | awk 'BEGIN{ORS=""} {printf "%s%%0A", $0}'
}

lines="$(wc -l < "$LOG" | tr -d ' ')"

# 1) 真正的报错行优先（cargo / rustc / panic / 找不到文件）。
excerpt="$(grep -n -E 'error(\[|:)|panicked|could not compile|No such file|cannot find|fatal:|Command failed' "$LOG" 2>/dev/null \
  | head -n 40 | escape || true)"
if [ -z "$excerpt" ]; then
  excerpt="$(echo 'no error-looking lines matched; see tail' | escape)"
fi

echo "::error::log failed (exit ${CODE}, ${lines} lines) — error lines follow:%0A${excerpt}"

# 2) 日志尾部：报错的现场，经常只有这里才有「caused by」。
echo "::warning::log tail:%0A$(tail -n 25 "$LOG" | escape)"

# 3) 磁盘：runner 空间耗尽是这类构建最常见的隐形死因。
echo "::warning::disk:%0A$(df -h / | escape)"
