# IM 发送方白名单（channel-allowlist）

> **种类：** B 用户说明书（非权威）。冲突以 P0 v0.9 为准。

微信 / 飞书 / Telegram 机器人**默认拒绝所有人**，直到你配置白名单。

文件路径：`~/.lebi-ai/channel-allowlist.toml`（或 `$LEBI_DATA_DIR/channel-allowlist.toml`）。

修改后**重启** bot（进程内缓存一次）。

## 模板

```toml
# 空列表 = 拒绝该渠道全部发送方（默认安全）
# 显式对所有人开放（仅当你确认风险时）：
#   allowed = ["*"]

[telegram]
# Telegram chat_id（数字字符串）。私聊时等于用户 id。
allowed = ["123456789"]

[feishu]
# 飞书 open_id
allowed = ["ou_xxxxxxxx"]

[wechat]
# 微信 from_user_id；调试期可临时 "*"
allowed = ["*"]
```

## 被拒绝时

机器人会回复一段中文说明，指向本文件路径；不会进入模型回合。

## 与工具白名单的关系

即使某人在 allowlist 内，IM 仍只能使用 **只读/检索类** 工具
（`IM_TOOL_WHITELIST`）：不含 `bash` / `write` / `memory_save` / `skill_create` 等。
长期记忆与技能写入请在桌面 GUI 完成。
