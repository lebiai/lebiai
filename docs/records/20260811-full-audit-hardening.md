# 变更记录：全量审计统一加固（安全 · 契约 · 架构 · UI · UX）

| 字段 | 内容 |
|------|------|
| **编号** | `20260811-full-audit-hardening` |
| **日期** | 2026-08-11 |
| **状态** | **工程通过（含遗留闭环）**（产品手测 ⬜ 用户；禁止标「产品已验收」） |
| **负责人** | Grok Agent（按用户要求全量处理审计项，无优先级裁剪） |
| **关联** | 会话审计报告；`20260807-codebase-hygiene-30-issues`；`20260803-permission-permissive-default`；P0/P1/AGENTS |

---

## 0. 用户价值

- **谁用：** 桌面 GUI、CLI、IM、Flutter/server 全部用户与后续协作者。
- **痛点：** 开放 bash/会话路径/IM 无白名单等真实风险；GUI 与 server 行为漂移；文档宣称「1:1」误导；英文 UI 中文错误；确认框易误点；Evolve 语义与工具免确认冲突。
- **用完后：** 高危默认 fail-safe；IM 默认仅允许名单用户；各入口契约诚实一致；错误可本地化；确认交互更安全；会话路径不可越界；文档与能力矩阵对齐。

### 好用性自检

- [x] 不新增数据库 / 额外运行时
- [x] 高频路径不无意义打断（普通 write 仍可开；高危与技能写入收紧）
- [x] 安全边界默认更硬，允许配置放宽（allow 规则 + Always Allow）
- [x] 改完清旧（失效注释、假 1:1 文案、错误字段）

---

## 0b. 产品经理视角

| 场景 | 改前 | 改后 |
|------|------|------|
| 桌面对话 | bash 默认全开；skill 可静默写入 | 高危必确认；无确认通道则拒绝；skill_create 必确认；always_active 工具侧强制 false |
| IM 机器人 | 任何人可聊；可写记忆/技能 | 必须配置 allowlist；白名单去掉 durable 写工具 |
| Server/Flutter | 可删任意 path；无空 Key 门禁 | path 限制 sessions 根；空 Key 拒发 |
| 设置迁移 | user_chosen 恒 false | 正确反映 pointer |
| 文档 | 写 1:1 | 明示 GUI 全量 / server 子集 + 能力矩阵 |

**成功标准：** 审计清单每一项均有代码或文档闭环；`cargo test`/`clippy` 相关 crate 通过。  
**明确不做什么：** 不引入完整 OS 沙箱（bubblewrap/seatbelt）——记为后续；不重写 Flutter 完整 Evolve 页（文档降级为 chat-first + server 已有 REST）；不合并 GUI/server 全部 AppState 为单 crate（收敛 ContextSources 契约与共享 path/session 校验，chat 关键门禁对齐）。

---

## 0c. 架构师视角

| 根因 | 默认路径 |
|------|----------|
| 确认通道缺失仍执行 | `hermes-turn`：需确认且 `confirm_tx=None` → **拒绝**；`Permission::Allow` 仍跑绝对高危检测 |
| 权限 key 字段错 | `permissions::extract_key_arg` 对齐 `path`/`file_path` |
| 会话 path 信任客户端 | `hermes_store`/`core` 统一 `ensure_under_sessions_root` |
| IM 开放 | channel 配置 allowlist + 白名单收敛 |
| SSRF | `web_fetch` URL 校验（scheme + 解析后私网拒绝） |
| 三份 ContextSources | GUI/server 对齐 channel 语义（palace/profile/always_active）；删除简化分叉中的关键差异（memory 热刷新、空 Key） |
| 文档契约 | P0/P1/AGENTS/project-map 改为「子集 + 矩阵」 |

**防复发：** 新增无 UI 表面必须 fail-closed；新增工具进 IM 白名单必须无 durable side-effect；session API 禁止裸 path。

---

## 1. 方案清单（审计项 → 处理）

### 安全

| ID | 项 | 处理 |
|----|----|------|
| S1 | bash 开放 RCE 面 | 绝对高危不可被 allow 跳过；无 confirm 通道拒绝高危；扩展检测；文档诚实 |
| S2 | IM 无发送方白名单 | `allowed_user_ids` / `allowed_chat_ids` 配置；默认空=拒绝全部（需显式 `*` 才开放） |
| S3 | token 打日志 | 仅指纹 |
| H1 | confirm_tx None 放行 | fail-closed |
| H2 | web_fetch SSRF | 私网/metadata 拦截 |
| H3 | session 任意 path | 强制 sessions 根下 |
| H4 | WS ?token= | 文档警告 + 日志不落 token；保留兼容（浏览器限制） |
| H5 | allow 绕过 danger | Allow 后仍 assess 绝对高危 |
| H6 | write/edit 字段 | path + file_path |
| H7 | skill_create always_active | 确认 + 强制 false |
| M | git 参数 | 拒绝危险 flag |
| M | Tauri CSP | 启用严格 CSP |
| M | Markdown 链接 | 仅 http(s)/mailto |

### 产品 / UX / UI

| ID | 项 | 处理 |
|----|----|------|
| P | server 空 Key | 与 GUI 对齐 |
| P | humanize 多语言 | `humanize_error(raw, lang)` |
| P | user_chosen | 导出 pointer 查询 |
| P | 确认弹窗 | Esc=Deny；默认焦点 Cancel；焦点陷阱 |
| P | stream 生命周期 | 终态保证 + sessionId 绑定 |
| P | Evolve 语义 | P0 精确化；skill 确认；IM 不写记忆 |
| P | Flutter Evolve | 文档降级 chat-first |
| P | 错误 toast | 去掉工程师前缀展示（humanize） |

### 架构 / 文档

| ID | 项 | 处理 |
|----|----|------|
| A | 1:1 谎言 | 修正权威文档 + 能力矩阵 |
| A | memory 热刷新 | server begin_turn 对齐 GUI |
| A | 父 AGENTS | 对齐 lebi-AI |
| A | 验收纪律 | 本台账 §4：工程 vs 产品手测分栏 |

---

## 2. 实施（Implement）

见同次提交的代码 diff；关键文件：

- `hermes-turn`：fail-closed、Allow+高危
- `hermes-turn/permissions`：path 字段
- `hermes-tools`：bash 文档/检测、skill_create、web_fetch SSRF、git
- `hermes-channel`：白名单、IM allowlist 钩子
- `hermes-cli` telegram/feishu/wechat：allowlist
- `hermes-store` / gui / server session：path 约束
- `hermes-server`：token 指纹、空 Key、memory 刷新
- `hermes-llm`：humanize 双语
- `hermes-core`：`is_user_chosen_data_root`
- GUI ConfirmModal / chatStore / Markdown
- 文档：P0/P1/AGENTS/project-map/REMOTE_ACCESS/records

---

## 3. 测试（Test）

| # | 用例 | 结果 |
|---|------|------|
| 1 | `cargo test -p hermes-turn --lib` | ✅ 24 passed |
| 2 | `cargo test -p hermes-tools --lib` | ✅ 53 passed |
| 3 | `cargo test -p hermes-store --lib` | ✅ path_guard 含越界拒绝 |
| 4 | `cargo test -p hermes-channel --lib` | ✅ allowlist 单测 |
| 5 | `cargo test -p hermes-llm --lib` | ✅ humanize 双语 |
| 6 | `cargo test -p hermes-server --test auth` | ✅ 7 passed |
| 7 | `cargo check -p hermes-gui -p hermes-server -p hermes-cli` | ✅ |
| 8 | `npx tsc --noEmit`（GUI ui） | ✅ |
| 9 | 产品手测 GUI / IM | ⬜ 用户 |

---

## 4. 验收（Accept）

| 门槛 | 状态 | 说明 |
|------|------|------|
| 审计项代码闭环 | ✅ | 见 §1 与 diff |
| 工程测试 | ✅ | §3 |
| 产品手测 GUI | ⬜ 用户 | 确认框 Esc=拒绝、Allow 非默认焦点、Key 门禁、会话 path |
| 产品手测 IM | ⬜ 用户 | 无 allowlist 拒消息；`allowed=["*"]` 才全开 |
| 禁止自标「产品已验收」 | ✅ | 本台账仅「工程通过」 |

---

## 5. 遗留闭环（第二刀 · 同日）

见 [`20260811-leftover-completion.md`](./20260811-leftover-completion.md)：

- ✅ bash seatbelt / bwrap 沙箱
- ✅ WS short-lived ticket
- ✅ CompanionContextSources 单源
- ✅ Flutter 进化收件箱
- ⏳ GUI/server AppState **整 crate 合并**仍为结构并行（行为已对齐；物理 monomorphize 成本高，非安全缺口）

## 6. 用户手测清单

见本回复「统一测试建议」。
