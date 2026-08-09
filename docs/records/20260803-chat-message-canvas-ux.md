# 变更记录：聊天消息画布化（去气泡 · 过程折叠 · footer · 再生/编辑 · 虚拟列表）

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-chat-message-canvas-ux` |
| **日期** | 2026-08-03 |
| **状态** | **已验收** |
| **负责人** | Grok（用户确认边界后实施） |
| **关联** | `20260803-composer-attachments-ux`、`20260803-session-s2-display-thinking-usage` |

---

## 0. 用户价值

- **谁用：** 桌面 GUI 用户（主交付面）
- **解决什么痛点：** AI 回复像 IM 气泡难读；过程/工具摊开抢视线；无法一键复制整段答案；不知本轮耗时/用量；无法重试或改问重发；长会话滚动卡
- **用完后：** 助手正文画布阅读；过程默认可折；消息底栏复制/耗时/token；末轮可再生；用户消息可编辑重发；长列表可虚拟滚动
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知
  - [x] 不增加无意义确认
  - [x] 高频路径更省心

---

## 0b. 产品经理视角

- **场景：** 读长回答、回看工具过程、复制答案、看本轮成本、改错问题重问、重跑最后一次回答
- **路径变化：**
  - 改前：助手灰气泡；过程块重；无消息级操作；仅顶栏累计 token
  - 改后：用户保留气泡；助手无壳正文；过程完成后默认折叠摘要；footer 复制+耗时+本轮 token；再生/编辑；长列表虚拟化
- **成功标准：**
  1. 助手正文无圆角卡片壳
  2. 完成后思考+多工具合成一组，默认折叠
  3. 助手 footer：复制（仅正文）、有则显示耗时与本轮 token
  4. 最后一轮可「重新生成」；用户消息可「编辑并重发」（截断其后历史）
  5. 消息较多时列表虚拟滚动不丢流式末条
- **明确不做什么：** Flutter 本期不动；不改 reflection；不引入云同步

### 已确认边界（用户 2026-08-03）

1. 仅桌面 GUI  
2. 复制 = 仅助手正文  
3. 耗时 = 本轮墙钟；历史无则不显示  
4. Token：本轮尽量显示；历史无则仅顶栏累计  
5. 过程：完成后收起 + 摘要；多 tool 一组  
6. **需要** 重新生成、编辑用户消息、虚拟列表  

---

## 0c. 架构师视角

- **根因：** 消息 UI 按 IM 气泡建模；MessageData 无 per-turn 元数据；JSONL 仅 append 缺 truncate/rewrite；send_message 强制 push user 无法 regenerate
- **正确默认路径：**
  - 展示：过程层 / 结果层 / 操作层 三层；流式与完成同构组件
  - 元数据：前端流式记录 `durationMs` + 本轮 usage；不伪造历史
  - 截断：`hermes-store` 提供 rewrite；GUI `truncate_session` 同步内存+磁盘
  - 再生：`regenerate_turn` 不 push user，在截断后的 history 上 run_turn
  - 编辑：truncate 至该 user 之前 + 普通 send_message
  - 虚拟列表：`@tanstack/react-virtual` + measureElement
- **边界：** 仅 hermes-gui + hermes-store；server 不 1:1 本期（台账注明 Flutter 另跟）
- **防复发：** 禁止再给助手正文加 card chrome；footer 为消息操作唯一入口
- **为何不是补丁：** 统一信息架构与会话变更契约，而非样式 if

---

## 1. 方案（Plan）

- **目标：** 主流 AI 画布消息体验 + 消息级操作 + 长列表性能
- **范围：** 做见成功标准；不做 Flutter/server 路由同步
- **技术要点：**
  - `hermes-store::rewrite_session`
  - GUI：`truncate_session` / `regenerate_turn`
  - UI：MessageBubble 重构、ProcessGroup、MessageFooter、ChatMessageList 虚拟化
  - store：turn 计时、本轮 token、regenerate/edit
- **风险：** rewrite 竞态（流式中禁用截断）；动态高度虚拟列表抖动（measure）
- **方案确认：** [x] 用户确认 1–6 · 2026-08-03

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. `hermes-store::rewrite_session`：截断后原子重写 JSONL
  2. GUI commands：`truncate_session` / `truncate_after_last_user` / `regenerate_turn`；`send_message` 与 regenerate 共用 `begin_turn`
  3. 前端 MessageData 增 `durationMs` / 本轮 token；`DisplayMessage.rawStart/rawEnd`
  4. 助手去气泡画布；过程 `ProcessGroup` 完成后默认折叠；footer 复制/再生/耗时/token
  5. 用户消息 hover 复制 + 编辑；编辑弹窗截断后重发
  6. ≥28 条启用 `@tanstack/react-virtual`；流式条始终在列表底部
  7. i18n 中英 key
- **关键路径/文件：** 见上 + `chatStore.ts` / `MessageBubble.tsx` / `ChatView.tsx` / `main.rs` 注册
- **偏离方案处：** 无；Flutter/server 未同步（台账已声明本期仅桌面 GUI）

---

## 3. 测试（Test）

| # | 用例 | 期望 | 结果 |
|---|------|------|------|
| 1 | 助手长文 | 无气泡壳，可读 | **通过** |
| 2 | 含 thinking+tools 完成后 | 默认折叠摘要，可展开 | **通过** |
| 3 | 复制 | 剪贴板为正文 | **通过** |
| 4 | 本轮耗时/token | footer 有；历史可无 | **通过** |
| 5 | 重新生成末轮 | 替换末助手回答 | **通过** |
| 6 | 编辑用户消息 | 其后截断并重答 | **通过** |
| 7 | 长会话滚动 | 虚拟列表可用 | **通过** |
| 8 | 自动化 | store 单测 + gui check + ui build | **通过** |

- **自动化：** `cargo test -p hermes-store rewrite_session` ✅；`cargo check -p hermes-gui` ✅；`npm run build` ✅
- **测试结论：** [x] 全部通过 · [ ] 有已知问题

---

## 4. 验收（Accept）

- **验收人：** 用户
- **验收日期：** 2026-08-03
- **结论：** ☑ 通过 · ☐ 驳回
- **遗留项：** Flutter/server 展示未同步（本期明确仅桌面 GUI）
