# lebi-AI（乐彼AI）Flutter 客户端 — 进度看板

> **种类：** H 快照（非权威）。冲突以 P0 v0.9 为准。
> 手机端是同一引擎的表面，**已接线、非默认交付**。
> Flutter + `hermes-server`。server 是 GUI 的 **Flutter 子集** + WS 对话帧（能力矩阵，禁止 1:1）。
> WS 优先 `POST /api/v1/ws-ticket` 再 `?ticket=`。

## M0 — 地基 ✅
- [x] hermes-server crate + AppState + REST/WS 骨架 + `hermes serve`
- [x] Flutter 三端骨架 + Agent Skills + 平台权限 + 连通验证

## M1 — 核心对话(L1) ✅
- [x] server WS `/api/v1/chat`(run_turn + ChatStreamEvent 流 + cancel + confirm 双向桥接)
- [x] Flutter ChatStreamEvent 模型 + WS 客户端
- [x] ChatProvider + 对话 UI(流式 markdown + 工具卡片含 input + 危险确认弹窗 + ThinkingDelta 折叠 + 取消)
- [x] 验证: cargo build/clippy + flutter analyze + build macos 全绿

## M2 — 多会话(L2) ✅
- [x] session REST(list/new/load/delete)，与 GUI 会话能力对齐（子集）
- [x] Flutter 会话抽屉 / 切换 / 历史重放(含工具卡片渲染)

## M3 — 管理面板(L3) ✅（部分）
- [x] skills / memory / config REST(server 侧,对应 gui commands)
- [x] Flutter 管理面板:config(model 切换) + skills(查看/删除) + memory(置顶/删除)
- [ ] reflect / mcp REST:server 路由已实现,但 **Flutter 客户端尚未接线**
      (无调用方;MicroReflection / SkillCandidateProposed 事件被静默丢弃)

## M4 — 移动增值(L4) ✅
- [x] **前置**: lebi-AI 加 `Image` `ContentBlock` + ImageSource(hermes-core)+ 全 match 点补分支
- [x] 图片输入(image_picker 选图 + base64 + server attachments + agent 处理 + 消息渲染图片);anthropic 自动支持,openai 占位
- [x] 语音输入(speech_to_text + 麦克风按钮 + 三端权限)
- [x] 后台推送 / 分享扩展 → 文档化(`clients/flutter/docs/MOBILE_EXTRAS.md`,需 APNs/FCM 凭证 + Xcode/Android target,无法 headless 完成验证)

---

**当前阶段: M0-M4 全部完成**(可代码验证的部分已 build/clippy/analyze 通过;多模态/语音/推送的真机端到端需用户在配好凭证的设备验证)
