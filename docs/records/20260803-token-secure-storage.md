# 变更记录：TOKEN-STORAGE — 移动端 token 改用 flutter_secure_storage

| 字段 | 内容 |
|------|------|
| **编号** | `20260803-token-secure-storage` |
| **日期** | 2026-08-03 |
| **状态** | **待验收**（已实施；本机无 Flutter SDK，pub get/analyze/test/build 需 Flutter 环境执行） |
| **负责人** | Codex（用户委托）→ 验证需用户在 Flutter 环境执行 |
| **关联** | `docs/project-map.md` §6 P1 缺口 TOKEN-STORAGE |

---

## 0. 用户价值（必填 · 站在用户角度）

- **谁用：** Flutter 客户端用户（iOS / Android / macOS）
- **解决什么痛点：** server token 明文存 `shared_preferences`（Android 为 XML 明文、
  iOS/macOS 为 NSUserDefaults 明文 plist），本机其他进程/备份可读；token = 服务器账号
- **用完后用户多得到什么：** token 存入 OS 级安全存储（iOS/macOS Keychain、Android
  Keystore 加密），备份与越权读取面大幅缩小
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 步骤可感知、可预期（连接设置交互不变）
  - [x] 不增加无意义确认或噪音
  - [x] 高频路径比改前更快或更省心（安全增强，无交互损失）

---

## 0b. 产品经理视角（必填 · 禁止跳过）

- **场景：** 用户在手机/桌面客户端填入 `hermes serve` 的 token
- **路径变化：** 改前（token 明文存 prefs）→ 改后（token 存 Keychain/Keystore；URL 等
  非敏感设置仍存 prefs；保存/读取交互不变）
- **成功标准：** 填入 token → 重启 app → 仍能连上 server（token 从安全存储恢复）；
  设置界面不显示已存 token（与之前一致，只读不回显）
- **明确不做什么：** 不做 token 轮换/多账号；不改 server 端；URL 不迁移到安全存储
  （非敏感）；不迁移已有 prefs 里的旧 token（首次升级需重填一次，可接受）

---

## 0c. 架构师视角（必填 · 禁止修修补补）

- **根因层级：** 客户端持久化层（连接凭证明文落盘）
- **正确的长期默认路径：** 敏感凭证走 OS 安全存储：iOS/macOS Keychain、
  Android Keystore 加密（`flutter_secure_storage` 9.x）；非敏感设置（URL）保持
  `shared_preferences`；Notifier 注入从 `SharedPreferences` 换为 `FlutterSecureStorage`
- **与引擎/各入口边界：** 只动 `clients/flutter`（UI 层连接设置 + main 引导 + 平台配置）；
  引擎/服务端不动；GUI 桌面端不在此范围
- **安全影响：** 正向（凭证从明文 → 系统加密存储）；macOS 沙盒需
  `keychain-access-groups` entitlement；Android 需 minSdk ≥ 23
- **如何防复发：** token 持久化只经 `ServerTokenNotifier` 一个出口；后续任何入口
  读 token 必须走 secure storage
- **为何这不是补丁：** 把凭证持久化收敛到 OS 标准安全存储（正确默认路径），
  而非继续在明文存储上打补丁

---

## 1. 方案（Plan）

- **目标：** token 不再明文落盘
- **范围：** 做：`pubspec.yaml` 加 `flutter_secure_storage`；`main.dart` /
  `connection_providers.dart` 改造；Android minSdk=23；macOS entitlements 加
  keychain-access-groups。**不做：** iOS 额外配置（Keychain 默认可用）；URL 迁移；
  旧 token 迁移；server/GUI 改动
- **用户路径变化：** 见 0b；注意首次升级后旧 token 不迁移，需在设置里重填一次
- **技术要点：** `FlutterSecureStorage().read/write`；`ServerTokenNotifier.set` 变 async；
  `chat_drawer.dart` 调用点保持 fire-and-forget（lint 不告警）
- **风险与回滚：** 中低——无法本机验证（无 Flutter SDK）；平台配置为标准步骤；
  git 可回滚
- **方案确认：** [x] 已对照 P0/P1（含第七条）· 日期/人：2026-08-03 / Codex

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. `clients/flutter/pubspec.yaml`：dependencies 加 `flutter_secure_storage: ^9.2.4`
  2. `clients/flutter/android/app/build.gradle.kts`：`minSdk = 23`（显式覆盖）
  3. `clients/flutter/macos/Runner/DebugProfile.entitlements` + `Release.entitlements`：
     加 `com.apple.security.keychain-access-groups` = `$(AppIdentifierPrefix)com.example.hermesApp`
  4. `clients/flutter/lib/main.dart`：token 改从 `FlutterSecureStorage().read` 读取并注入
  5. `clients/flutter/lib/ui/features/connection/view_models/connection_providers.dart`：
     `ServerTokenNotifier` 改用 `FlutterSecureStorage` 持久化（`set` 变 async）；
     URL Notifier 保持 `SharedPreferences`
- **关键路径/文件：** 见上
- **偏离方案处：** 无（`pubspec.lock` 不手工改——需 `flutter pub get` 生成，见测试）

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 依赖可解析 | `flutter pub get` | 成功且 lock 更新 | ⬜ 未执行 | 本机无 Flutter SDK |
| 2 | 静态检查 | `flutter analyze` | 无告警 | ⬜ 未执行 | |
| 3 | 单元测试 | `flutter test` | 既有测试通过 | ⬜ 未执行 | |
| 4 | 保存/恢复 | 填入 token → 重启 app → 连接 | token 从 Keychain 恢复，能连 server | ⬜ 未执行 | 真机/模拟器 |
| 5 | macOS 构建 | `flutter build macos` | 成功（entitlement 生效） | ⬜ 未执行 | |
| 6 | Android 构建 | `flutter build apk` | 成功（minSdk 23） | ⬜ 未执行 | |

- **自动化：** 本机无法执行（`flutter` / `dart` 均不在 PATH）；需用户在 Flutter 环境跑 1–3
- **手工：** 用例 4–6 需设备/模拟器
- **测试结论：** [ ] 全部通过 · [x] **未执行——待 Flutter 环境验收**（本机无 Flutter SDK）

---

## 4. 验收（Accept）

对照 **质量门槛**（见仓库根 `DEVELOPMENT_RULES.md` §变更流程）：

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ✅（设计） | 凭证进 OS 安全存储 |
| 开箱即用未破坏 | ⬜ 待验证 | pub get / analyze / build |
| 本地优先未破坏 | ✅ | 仍本地存储，只是加密 |
| 测试通过 | ⬜ | 待 Flutter 环境执行 |
| 记录完整 | ✅ | 本记录四阶段齐全 |
| 产品+架构两视角齐全 | ✅ | 见 0b/0c |
| 非修修补补（默认路径正确） | ✅ | OS 标准安全存储 |
| 代码卫生（P0 第九条） | ✅ | 无死代码；依赖保留（URL 仍用 prefs） |

- **验收人：** 待用户（Flutter 环境）
- **验收日期：** 待定
- **结论：** ☐ 通过 · ☐ 驳回（原因：）——**当前：待验收**
- **遗留项：** 测试表 1–6 需 Flutter 环境执行；首次升级旧 token 不迁移（文档已注明）

---

## 5. 附注

- 本机无 `flutter`/`dart`（PATH、`~/flutter`、`/opt/homebrew`、`codeINDEx` 均未找到）
- 版本选择：`flutter_secure_storage 9.2.x`（广泛验证的稳定版；10.x 未选用以降低风险）
- macOS entitlement：`com.apple.security.keychain-access-groups` =
  `$(AppIdentifierPrefix)com.example.hermesApp`（bundle id 见 `macos/Runner/Configs/AppInfo.xcconfig`）
- Android：`flutter_secure_storage` 9.x 需 minSdk 23（`build.gradle.kts` 显式覆盖
  `flutter.minSdkVersion`）
