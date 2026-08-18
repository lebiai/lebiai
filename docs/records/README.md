# 变更与验收记录

本目录是**唯一**的变更过程台账（位于 `docs/records/`，不在仓库根目录）。
**种类：F 台账。** 历史用词与当时方案**不构成现行法**；现行只认 P0 v0.11。变更对照只写在本目录。可分型：产品 / 工程 / 发布。

每做一次有用户可见影响（或架构/打包/共识）的修改，必须有一份记录走完：

```
方案 → 实施 → 测试 → 验收
```

未写记录、未验收通过 → **视为未完成，不得当作交付。**

模板：[`_TEMPLATE.md`](./_TEMPLATE.md)
规则全文：[`../../DEVELOPMENT_RULES.md`](../../DEVELOPMENT_RULES.md)（§变更流程）
Agent 约束：[`../../AGENTS.md`](../../AGENTS.md)
文档目录约定：[`../README.md`](../README.md)

---

## 命名

```
docs/records/YYYYMMDD-短横线-英文或拼音-slug.md
```

示例：`20260803-authoritative-docs.md`、`20260803-fix-server-auth.md`

---

## 索引（新记录加一行，最新在上）

| 编号 | 标题 | 状态 | 日期 |
|------|------|------|------|
| [20260818-release-v1.2](./20260818-release-v1.2.md) | **v1.2.0 发布**（更新器进安装包 · 概览版本行） | **实施中** | 2026-08-18 |
| [20260818-in-app-update](./20260818-in-app-update.md) | **应用内点击更新**（概览一行 · 点了才下装重启） | **测试中** | 2026-08-18 |
| [20260818-release-v1.1](./20260818-release-v1.1.md) | **v1.1.0 发布**（在办/回顾/授权 · dmg/exe 挂 GitHub） | **已验收** | 2026-08-18 |
| [20260818-audit-illusion](./20260818-audit-illusion.md) | **收 1–7 假象**（过了 Cue 真画出才记账 · 记下/换期不预填 · 角标与导航拆开） | **工程通过** · 待目视 | 2026-08-18 |
| [20260818-audit-fix](./20260818-audit-fix.md) | **审计 P0–P2 按序修复**（进化控件 · 待收下无默认期 · Cue 过了优先 · 删常驻 · 角标落待审 · 清死物 · 遮罩/芯片统一） | **工程通过** · 待目视 | 2026-08-18 |
| [20260817-product-debt](./20260817-product-debt.md) | **收口产品债**（在办 v2 冻结 · Cue/空态/已蒸过 · 快照刷新） | **工程已验收** · 桌面目视仍待用户 | 2026-08-18 |
| [20260817-zaiban-work-unify](./20260817-zaiban-work-unify.md) | **在办+回顾需求统一**（期限必填 · 周报底稿 · 改删常驻） | **工程通过** · 待目视 | 2026-08-17 |
| [20260817-review-ledger](./20260817-review-ledger.md) | **回顾页交差账与台账**（账留在页上 · 过往可点 · 去黑按钮） | **工程通过** · 待目视 | 2026-08-17 |
| [20260814-zaiban-drawer-review](./20260814-zaiban-drawer-review.md) | **在办改抽屉 · 回顾节奏规格冻结** | **规格已冻结** v1.2 · 节奏日自设 | 2026-08-14 |
| [20260814-zaiban-impl](./20260814-zaiban-impl.md) | **在办 v1 实现**（store · 工具 · 侧栏 · 离开余债） | **实施中** · 待 GUI 走查 | 2026-08-14 |
| [20260814-zaiban-commitments](./20260814-zaiban-commitments.md) | **在办（跨次还欠的交差）规格冻结** | **规格已冻结** · 实现见 impl | 2026-08-14 |
| [20260814-full-audit-fix](./20260814-full-audit-fix.md) | **全量检查：蒸馏单飞 · 搜索同题 · Office 真排版** | **工程通过** · 待产品确认 | 2026-08-14 |
| [20260814-wechat-in-gui](./20260814-wechat-in-gui.md) | **GUI 可见微信记录 · 只看不发 · 同样蒸馏** | **实施中** | 2026-08-14 |
| [20260814-distill-ledger](./20260814-distill-ledger.md) | **发送时间 + 蒸馏台账 + 整段重蒸** | **实施中** | 2026-08-14 |
| [20260814-history-care-and-process](./20260814-history-care-and-process.md) | **历史会话不露内部指令 · 过程收成一条** | **实施中** | 2026-08-14 |
| [20260814-open-and-search-truth](./20260814-open-and-search-truth.md) | **打开走引擎 · 搜索垃圾不当成功 · Word/Excel 真文件** | **工程通过** · 待产品确认 | 2026-08-14 |
| [20260814-memory-distill-living-rules](./20260814-memory-distill-living-rules.md) | **记忆改为生效规则蒸馏**（一类事一格 · 无效不进库 · 同格 supersede） | **工程通过** · 待产品确认 | 2026-08-14 |
| [20260814-do-path-from-hot-session](./20260814-do-path-from-hot-session.md) | **从一条会话抽出跨场景默认路径**（日期注入 · 桌面真出口 · 正文禁独白 · Care 收紧；Continuity 保留） | **工程通过** · 待产品确认 | 2026-08-14 |
| [20260814-fp-rebuild-surfaces](./20260814-fp-rebuild-surfaces.md) | **默认路径第二刀**（过程人话 · 它记得的 · 技能降级 · 清死面板与 Flutter 旧品牌） | **工程通过** · 待产品确认 | 2026-08-14 |
| [20260814-fp-rebuild](./20260814-fp-rebuild.md) | **按第一性原理重建默认路径**（打开即对话 · 进化实话 · 去编程剧本 · 拆 Agent 壳 · ask/run 收紧） | **工程通过** · 待产品确认 | 2026-08-14 |
| [20260814-first-principles](./20260814-first-principles.md) | **第一性原理写入权威文档**（P0 第零条 · 拒绝类比 / 拆到真 / 往上建） | **工程通过** · 待产品确认 | 2026-08-14 |
| [20260814-doc-types-ah](./20260814-doc-types-ah.md) | **文档种类 A–H 标准化**（P0 v0.9 · 目录即种类） | **工程通过** · 待产品确认 | 2026-08-14 |
| [20260814-pm-visual-ops](./20260814-pm-visual-ops.md) | **权威文纯洁性 · 产品经理对操作与视觉负责**（P0 v0.8 · P1 v0.3） | **工程通过** · 待产品确认 | 2026-08-14 |
| [20260814-authority-no-drift](./20260814-authority-no-drift.md) | **权威唯一 · 禁止偏移**（P0 v0.7 · P1 v0.2 · 身份同词 · docs 分级） | **工程通过** · 待产品确认 | 2026-08-14 |
| [20260811-license-impl](./20260811-license-impl.md) | **授权试用续期实现** | **工程通过** | 2026-08-11 |
| [20260811-settings-ia-impl](./20260811-settings-ia-impl.md) | **设置页 IA 实现**（五 Tab · 概览默认 · 保存策略 3A） | **工程通过**（目视 ⬜） | 2026-08-11 |
| [20260811-settings-ia-freeze](./20260811-settings-ia-freeze.md) | **设置页 IA 方案冻结** | **规格已冻结** · 实现见 impl | 2026-08-11 |
| [20260811-license-impl](./20260811-license-impl.md) | **授权试用续期实现**（Ed25519 · 试用3天 · 全屏锁 · 电池 · 日更提醒） | **工程通过**（目视 ⬜） | 2026-08-11 |
| [20260811-license-ux-spec](./20260811-license-ux-spec.md) | **授权与续期用户操作规格冻结** | **规格已冻结** · 实现见 license-impl | 2026-08-11 |
| [20260811-memory-skill-ux-scope](./20260811-memory-skill-ux-scope.md) | **记忆待确认高亮 · 技能人话介绍 · 范围说明** | **工程通过**（目视 ⬜） | 2026-08-11 |
| [20260811-leftover-completion](./20260811-leftover-completion.md) | **审计遗留闭环**（bash 沙箱 · WS ticket · Context 单源 · Flutter 收件箱） | **工程通过** | 2026-08-11 |
| [20260811-full-audit-hardening](./20260811-full-audit-hardening.md) | **全量审计统一加固**（安全 fail-closed · IM allowlist · SSRF · session path · 契约诚实 · UI 确认框 · humanize 双语 · 空 Key 门禁） | **工程通过（含遗留闭环）** | 2026-08-11 |
| [20260809-release-publish-fix](./20260809-release-publish-fix.md) | **Release publish 修复**（正文为空根因 · 废弃 `overwrite` 入参 · 先删后建幂等重建） | **已验收**（v1.0.0 正文+安装包复验通过） | 2026-08-09 |
| [20260809-gui-polish-round3](./20260809-gui-polish-round3.md) | **GUI 产品化第三轮**（零打扰 Key 提示 · 内置技能开箱非空 · 统一下拉 · 欢迎仪式精简 · 设置中心化 · 原生目录选择器） | **已验收**（用户确认打包） | 2026-08-09 |
| [20260809-release-v1.0](./20260809-release-v1.0.md) | **v1.0.0 正式发布**（版本统一 1.0.0 · 用户向发布说明 · Release 自动挂载 dmg/exe） | **已验收**（dmg/exe 已挂载 · 正文经 publish 修复复验） | 2026-08-09 |
| [20260809-displayname-sync](./20260809-displayname-sync.md) | **称呼全链路同步修复**（欢迎页填写的称呼 → 侧栏/设置问候/欢迎页一致 · 结构化字段 + 前端单一数据源 · 跳过路径也保存） | **已验收** | 2026-08-09 |
| [20260809-productization-round2](./20260809-productization-round2.md) | **产品化第二轮**（数据位置迁移 · Key 已配置态 · 称呼显示 · 设置分组 · 欢迎仪式内嵌配置 · 错误用户化） | **已验收**（工程全绿 · GUI 目视待复测） | 2026-08-09 |
| [20260809-default-language-zh](./20260809-default-language-zh.md) | **默认语言改为中文**（模板/默认函数/前端初始态/兜底全链路 zh-CN，可切英文） | **已验收** | 2026-08-09 |
| [20260809-windows-markitdown-bundle](./20260809-windows-markitdown-bundle.md) | **Windows 捆绑 MarkItDown**（embed python 自包含 sidecar · tauri.windows.conf.json · cmd /C spawn · 文档导入开箱即用） | **已实施**（本机绿 · Windows 实跑待 CI） | 2026-08-09 |
| [20260809-remove-license](./20260809-remove-license.md) | **删除全部执照（LICENSE）内容**（LICENSE 文件 · README 章节 · Cargo 字段 · 各文档引用；保留技能元数据字段） | **已验收** | 2026-08-09 |
| [20260809-readme-user-facing-rewrite](./20260809-readme-user-facing-rewrite.md) | **README 重写为纯用户向介绍**（是什么/场景/手感/隐私/三步使用/FAQ；删除全部开发者内容） | **已验收**（文档） | 2026-08-09 |
| [20260809-provider-preset-selector](./20260809-provider-preset-selector.md) | **模型服务预设化：选服务商 + 只填 API Key + 保存即热切换**（PROVIDER_PRESETS 单一事实源 · 设置页下拉预设 · GUI/server RwLock 热切换 · 去「重启生效」） | **已验收**（工程全绿 · 设置页目视已验证 · 真实对话热切换待复测） | 2026-08-09 |
| [20260809-default-deepseek-help-ux](./20260809-default-deepseek-help-ux.md) | **默认 DeepSeek + API Key 教程折叠 + 界面去术语**（provider 四件套 · 帮助卡片点击展开 · i18n 大白话） | **已验收**（工程全绿 · 用户目视确认中） | 2026-08-09 |
| [20260809-api-key-guide](./20260809-api-key-guide.md) | **API Key 获取教程**（GUI 内嵌帮助卡片 + 官网白名单直开 + `docs/api-key-guide.md`） | **已验收**（工程全绿 · 真机目视待复测） | 2026-08-09 |
| [20260809-guard-dialogue-without-api-key](./20260809-guard-dialogue-without-api-key.md) | **未配置 API Key 时禁止发起对话**（发送门 + onboarding 竞态 + 后端防御 + 失败不复位修复） | **已验收**（工程全绿 · 干净环境 GUI 待复测） | 2026-08-09 |
| [20260809-installers-dmg-exe-guide](./20260809-installers-dmg-exe-guide.md) | **桌面安装包（macOS DMG + Windows EXE）与用户安装指引**（tauri 平台配置 · build-exe.ps1 · release CI · docs/install.md） | **已验收**（DMG 实跑；Windows/CI 待实跑） | 2026-08-09 |
| [20260807-codebase-hygiene-30-issues](./20260807-codebase-hygiene-30-issues.md) | **问题清单 30 条全量处理**（文档诚实 · 进化链路接线 · 安全白名单 · 死代码清理 · 测试补强） | **已验收**（工程全绿 · GUI 目视/Flutter 真机待手测） | 2026-08-07 |
| [20260807-codebase-full-learning](./20260807-codebase-full-learning.md) | **全量代码学习**（权威共识 + 各子系统精读 + 问题清单落盘 `docs/codebase-learning.md`） | **已验收** | 2026-08-07 |
| [20260807-positioning-dazi](./20260807-positioning-dazi.md) | **定位定调：搭子，不是搭档**（P0 v0.5 · 全 surface 文案同步） | **已验收**（文案待用户确认） | 2026-08-07 |
| [20260807-panel-logo-lebi-ai](./20260807-panel-logo-lebi-ai.md) | **GUI 面板 logo 品牌化**（侧栏占位图标 → 乐彼AI 品牌图） | **已验收**（目视待用户确认） | 2026-08-07 |
| [20260807-dock-name-lebi-ai](./20260807-dock-name-lebi-ai.md) | **Dock 悬浮名品牌化**（二进制名 lebi-AI · 打包路径对齐） | **已验收**（Dock 目视待用户确认） | 2026-08-07 |
| [20260806-brand-lebi-ai](./20260806-brand-lebi-ai.md) | **品牌定名 乐彼AI / lebi-AI**（图标全套 · 数据目录迁移 · 全 surface 品牌统一） | **工程已验收**（GUI/dmg 视觉待用户确认） | 2026-08-06 |
| [20260806-onboarding-redesign](./20260806-onboarding-redesign.md) | **首次引导页重设计**（三屏 · 场景收集 · 欢迎页联动 · 移除微信入口） | **已实施**（工程完成 · GUI 手测待用户确认） | 2026-08-06 |
| [20260806-gui-wechat-connect-flow](./20260806-gui-wechat-connect-flow.md) | **GUI 微信连接目标路径重塑**（扫码弹窗化 · 五态状态机 · token 过期一键重扫） | **已验收** | 2026-08-06 |
| [20260806-fix-chatstore-ts](./20260806-fix-chatstore-ts.md) | **chatStore.ts TS 错误修复**（build 恢复全绿 · 遗留项②闭环） | **已验收** | 2026-08-06 |
| [20260806-gui-wechat-connect](./20260806-gui-wechat-connect.md) | **GUI 内嵌微信连接**（分发 dmg 扫码免终端 · 共享 service 循环） | **待验收**（工程通过 · GUI 真机扫码待手测） | 2026-08-06 |
| [20260806-fix-quality-gates](./20260806-fix-quality-gates.md) | **质量门槛回归修复**（GUI 测试依赖 / clippy / fmt / 环境依赖测试） | **已验收** | 2026-08-06 |
| [20260806-pending-review-inbox](./20260806-pending-review-inbox.md) | **待审收件箱**（安静进化，默认不打断离开） | **已实施** | 2026-08-06 |
| [20260806-episode-self-contained](./20260806-episode-self-contained.md) | **情节自包含**（禁见会话记录/Care 污染） | **已实施** | 2026-08-06 |
| [20260806-full-acceptance](./20260806-full-acceptance.md) | **全量验收**（定义+C-SESS+Care+有来有回） | **工程通过 · 体感待手测** | 2026-08-06 |
| [20260806-give-and-take-pushback](./20260806-give-and-take-pushback.md) | **有来有回**（理解≠赞同 · 选项 · 你定） | **已实施** | 2026-08-06 |
| [20260806-care-after-delivery](./20260806-care-after-delivery.md) | **Care 交付后改进建议**（通用工作，非垂直场景） | **已实施** | 2026-08-06 |
| [20260806-csess-work-episode-loop](./20260806-csess-work-episode-loop.md) | **C-SESS 工作情节闭环**（种子/加权/再认出） | **已实施** | 2026-08-06 |
| [20260806-product-card-v03](./20260806-product-card-v03.md) | **产品定义卡 v0.3**（酷文案 · 去生活 · P0 钉子） | **已实施** | 2026-08-06 |
| [20260806-work-companion-complete](./20260806-work-companion-complete.md) | **工作与陪伴完整方案**（蓝图 + companion 协议 + 对话化） | **已实施** | 2026-08-06 |
| [20260805-gui-ritual-system](./20260805-gui-ritual-system.md) | **全站**仪式感与视觉统一系统（不围着反思）A–E | **已验收** | 2026-08-06 |
| [20260805-gui-ritual-visibility](./20260805-gui-ritual-visibility.md) | 可见性尝试；欢迎页 Reflect CTA **已纠偏撤回** | **部分否决/修正** | 2026-08-05 |
| [20260805-gui-ritual-motion-ux](./20260805-gui-ritual-motion-ux.md) | GUI 仪式感·动效·统一视觉（欢迎/首页/落印）P0 切片 | **已并入 system 验收** | 2026-08-06 |
| [20260805-gui-micro-reflection](./20260805-gui-micro-reflection.md) | GUI micro-reflection **正确架构重做**（Event + shared micro_run） | **测试中** | 2026-08-05 |
| [20260805-memory-dedup-auto-accept](./20260805-memory-dedup-auto-accept.md) | 记忆写路径近重复门控 + AutoAccept 仅成功记日志 | **测试通过**（单测） | 2026-08-05 |
| [20260806-doc-hygiene-dead-templates](./20260806-doc-hygiene-dead-templates.md) | 文档卫生：删除已废弃模板设计/中间取消台账 | **已验收** | 2026-08-06 |
| [20260805-template-feature-removed](./20260805-template-feature-removed.md) | **移除文档模板功能**（占位符方案废弃；唯一墓碑） | **已实施** | 2026-08-05 |
| [20260803-workspace-outputs-default](./20260803-workspace-outputs-default.md) | 生成物默认目录 workspace/outputs/ | **已验收** | 2026-08-03 |
| [20260803-permission-permissive-default](./20260803-permission-permissive-default.md) | 执行权限：常态放行 · 特别危险才授权说明 | **已验收** | 2026-08-03 |
| [20260803-chat-message-canvas-ux](./20260803-chat-message-canvas-ux.md) | 聊天消息画布化：去气泡 · 过程折叠 · footer · 再生/编辑 · 虚拟列表 | **已验收** | 2026-08-03 |
| [20260803-composer-attachments-ux](./20260803-composer-attachments-ux.md) | Composer 附件体验：拖入 · 气泡外卡 · 多文件解析 | **已验收** | 2026-08-03 |
| [20260803-markitdown-release-bundle](./20260803-markitdown-release-bundle.md) | 发布捆绑 MarkItDown sidecar（客户零安装） | **待测试**（打包后再测） | 2026-08-03 |
| [20260803-document-import-compliant](./20260803-document-import-compliant.md) | 文档导入（合规）：数据目录 sidecar · 共享引擎 · GUI/Server 1:1 · 📎 · .doc | **已验收** | 2026-08-03 |
| [20260803-upload-phase-a-markitdown](./20260803-upload-phase-a-markitdown.md) | 上传 Phase A 初版（GUI 独占 + PATH 依赖） | **已否决** | 2026-08-03 |
| [20260803-session-s3-dialogue-policy](./20260803-session-s3-dialogue-policy.md) | S3：身份纪律 · 停止落盘 · 话术边界 | **已验收** | 2026-08-03 |
| [20260803-session-s2-display-thinking-usage](./20260803-session-s2-display-thinking-usage.md) | S2：tool 折叠 · thinking 落盘 · Usage 写入 · 勾选持久化修复 | **已验收** | 2026-08-03 |
| [20260803-session-s1-empty-title-workspace](./20260803-session-s1-empty-title-workspace.md) | S1：空会话清理 · 标题持久化 · workspace 律师残留隔离 · 草稿不落盘 | **已验收** | 2026-08-03 |
| [20260803-gui-shell-phase-c34](./20260803-gui-shell-phase-c34.md) | GUI Phase C3/C4：复制 · 确认 · 快捷键 · 时间分组 · ErrorBoundary | **已验收** | 2026-08-03 |
| [20260803-gui-shell-phase-c](./20260803-gui-shell-phase-c.md) | GUI Phase C：进化审阅 UI + 聊天可靠性 | **已验收** | 2026-08-03 |
| [20260803-gui-shell-phase-b](./20260803-gui-shell-phase-b.md) | GUI 壳层 Phase B：Toast · 主题 · 首启 Key · 面板对齐 | **已验收** | 2026-08-03 |
| [20260803-gui-shell-phase-a](./20260803-gui-shell-phase-a.md) | GUI 壳层 Phase A：token · 会话常驻 · Welcome · Chat 观感 | **已验收** | 2026-08-03 |
| [20260803-tb-case-data](./20260803-tb-case-data.md) | 创建 100 条结核病病案合成数据（2026 H1） | **已移除**（见 20260807-remove-tb-legacy） | 2026-08-03 |
| [20260803-product-data-isolation](./20260803-product-data-isolation.md) | 通用/律师版数据目录隔离 | **待验收** | 2026-08-03 |
| [20260803-gui-dist-default-no-white-screen](./20260803-gui-dist-default-no-white-screen.md) | GUI 默认 ui/dist 防白屏 + 权威文档 | **待验收** | 2026-08-03 |
| [20260803-gui-session-end-reflection](./20260803-gui-session-end-reflection.md) | G0：GUI 会话结束 full reflection + 候选确认 | **待验收**（真机手测） | 2026-08-03 |
| [20260803-token-secure-storage](./20260803-token-secure-storage.md) | TOKEN-STORAGE：移动端 token 改用 flutter_secure_storage | **待验收**（需 Flutter 环境） | 2026-08-03 |
| [20260803-fmt-check](./20260803-fmt-check.md) | FMT-CHK：全仓 cargo fmt + CI 加 fmt 检查 | **已验收** | 2026-08-03 |
| [20260803-telegram-offset-and-docs](./20260803-telegram-offset-and-docs.md) | TELEGRAM：offset 持久化 + README 对齐 | **已验收**（实现）· 端到端待手测 | 2026-08-03 |
| [20260803-reflect-end-manual-acceptance](./20260803-reflect-end-manual-acceptance.md) | REFLECT-END 真机手测验收（会话结束自动提炼） | **待验收** | 2026-08-03 |
| [20260803-clippy-fix-and-ci](./20260803-clippy-fix-and-ci.md) | CLIPPY-1：clippy 修复 + CI 工作流 | **已验收** | 2026-08-03 |
| [20260803-reflect-end-session-reflection](./20260803-reflect-end-session-reflection.md) | REFLECT-END：CLI 会话结束 full reflection 接线 + 清死代码 | **待验收**（真机手测） | 2026-08-03 |
| [20260803-rule-accept-default-false](./20260803-rule-accept-default-false.md) | RULE-ACCEPT：auto_accept 默认值对齐 P0 | **已验收** | 2026-08-03 |
| [20260803-pre-dev-review-rules](./20260803-pre-dev-review-rules.md) | 开发前全面审查 + 规则定稿（P1/AGENTS 增补、README/docker 修正、TODO 迁移、缺口表） | **已验收**（规则/文档） | 2026-08-03 |
| [20260803-authoritative-docs](./20260803-authoritative-docs.md) | 权威文档体系（P0/P1/P2 + docs 索引 + 台账） | **已验收**（文档） | 2026-08-03 |
