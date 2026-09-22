# 变更记录：授权收口 —— 轮换出厂密钥、门禁收到装配层、数据根挡在版本库外

| 字段 | 内容 |
|------|------|
| **编号** | `20260918-license-hardening` |
| **日期** | 2026-09-18（收尾 09-19） |
| **状态** | **已验收**（工程 + 真机；旧码已确认作废） |
| **负责人** | Codex（用户拍板：第一批 · 发布阻断，来源 [`20260918-reaudit`](./20260918-reaudit.md)） |
| **关联** | 上一轮复审 P0-1 / P0-2 / P0-4（+ P1-6 的处置） |

---

## 0-fp. 第一性原理（P0 第零条）

- **拒绝的类比：**
  1. 不是「把 seed 从工作树删掉就算完」——它在提交历史里，任何人 `git show` 就能拿到。
  2. 不是「在每个业务命令前面再补一次 `if !can_use_main()`」——那是逐条堵，下一条新命令又会漏。
  3. 不是「把过期用户的人物收走」——用户早前明确决定「授权过期后人物还在」。要修的是**能力**，不是**名单**。
- **拆出的真：**
  1. 授权码的安全性 = 「能签出这张码的那把钥匙」的保密性。钥匙一旦公开，任何校验逻辑都拦不住伪造者；**换锁**是唯一出路，删痕迹只是打扫现场。
  2. 所有能烧用户 API Key 的路径，最后都必须经过一个**provider**。要拦的不是命令，是「拿到 provider 这件事」。
  3. 试用期 / 已购 / 过期是三种状态，用户可见的「谁在工位上」与「能不能干活」是两件事，必须分开管。
- **如何从真推出：**
   - 生成新密钥对，私钥只落在签发机（`~/.lebi-ai-issuer/seed.hex`，0600），仓库里只留新公钥；加一条**扫描全仓、任何 seed 能推出出厂公钥就报错**的回归测试，防复发。
   - 门禁做成 provider 装饰器，在 `Config::build_active_provider()` 一处套上 ⇒ GUI / server / CLI / IM 一次覆盖，且 GUI 能照常启动去输入授权码。
   - 过期仍显示工位（按用户决定），但模型调用被装饰器拒 ⇒ 「锁主能力」名副其实且无处可绕。

---

## 0. 用户价值

- **谁用：** 产品所有者（发码）+ 已安装的客户（用码）。
- **解决什么痛点：** 现在任何人拿到源码即可自签授权码（等于白送工位）；客户装了 `hermes-server` / 用命令行 / 接微信，都能绕过过期限制白用 API Key。
- **用完后用户多得到什么：** 授权码重新变成「只有你能发」；过期这件事在所有入口一致生效；真实数据（含 API Key）不再躺在 git 工作树里等着被 `git add` 提交。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库（密钥是一个 0600 文本文件）
  - [x] 步骤可感知：过期用户在 GUI 仍能看到工位与升级入口，只是发不出话
  - [x] 不增加无意义确认：正常期内用户完全无感
  - [x] 空/载/错态完整：GUI 保留既有 `license_locked` 提示；其他入口回一句人话错误
  - [x] 高频路径不变

---

## 0b. 产品经理视角

- **场景：** ①所有者要发新码给客户；②客户试用到期或买了新工位，输入授权码；③过期客户打开桌面端，或用命令行 / 手机 / 微信继续用。
- **怎么走完：** 所有者在签发机跑 `scripts/issue-license.py`（读本机 seed 文件）→ 得到 `LEBI1.…` 码 → 发给客户；客户在设置里粘贴 → 看到「校验中…」→ 工位刷新。过期用户：界面照旧，但发送后得到一句「授权已到期」的提示（不是静默失败）。
- **看起来怎么样：** 不新增界面。唯一新增可见面是过期时非 GUI 入口的错误文案（中文一句话）。
- **好走 / 好看：** 步骤与今天一致；所有者侧少了一个「别把 seed 提交上去」的人肉纪律。
- **成功标准：** ① 旧码在换锁后**一律失效**，新码可正常激活；② 过期状态下 GUI / CLI / server / IM 四个入口都发不出模型请求；③ 仓库里搜不到任何能推出出厂公钥的私钥。
- **明确不做什么：** 不做机器绑定与吊销表（改 token 语义、需要换机流程，留第二批）；不收回过期用户的工位显示（用户已拍板「人物还在」）；不动试用期可重置问题（P0-5，另行处理）。

## 0c. 架构师视角

- **根因层级：** 密钥管理（私钥进仓库 + 进历史）+ 授权放置层级（门禁写在业务命令里，而不是装配层）。
- **正确的长期默认路径：** 私钥只存在于签发机的一个 0600 文件；仓库只有公钥；门禁是 `hermes-core` 里的 provider 装饰器，由 `hermes-llm::Config::build_active_provider()` 统一套上——能力的唯一入口。
- **与引擎/各入口边界：** 装饰器放 `hermes-core`（trait 与授权同在），装配点在 `hermes-llm`（四个入口共用的 provider 构造）⇒ 不为某个入口 fork 逻辑。
- **安全影响：** 只增不减：新私钥不进仓库（0600）；门禁走**只读**授权检查（不写 `license.json`、不刷 `last_seen`）。
- **如何防复发：** ① 全仓扫描回归测试（seed → 公钥比对）；② 门禁测试（锁定时 `complete` 不发请求）；③ 过期状态测试（`can_use_main = false`，名单仍在）。
- **为何这不是补丁：** 换锁解决「伪造」这个根；装饰器解决「漏入口」这个根。两处都是把判据放回它该在的层，而不是给症状加 if。


---

## 1. 方案（Plan）

- **目标：** ① 换锁（新密钥对，私钥只留签发机）+ 仓库与历史里不再有可用 seed；② 门禁从「两处业务命令」收到「provider 装配层」，四个入口一次覆盖；③ 数据根挡在 git 外。
- **范围：**
  - **做：** 轮换密钥对；`sign_token_with_seed` 入口改成注入式（库里不再有「拿内置 seed 签名」这条路）；全仓扫描守卫测试（防复发）；`LicenseGatedProvider` + 只读授权检查；GUI/CLI/server/IM 四入口共用装配点；`scripts/*` 两处硬编码旧 seed 清理；`Hermes/codeINDEx` 外层 `.gitignore` 挡 `test/`。
  - **不做：** 机器绑定、吊销表（改 token 语义、需要换机流程 → 第二批）；不收回过期用户的工位显示；不动试用可重置（P0-5）。
- **用户路径变化：**
  - 所有者发码：`python3 scripts/issue-license.py …`（读签发机 seed 文件）→ **不变**，但旧码全部作废、必须重发。
  - 客户：试用 → 到期 → 输码激活 → **不变**；过期时在 GUI 之外（CLI/server/IM）多了一句人话错误，而不是静默能用。
- **技术要点：** `hermes-core/{license.rs,provider.rs,lib.rs}` · `hermes-llm/src/config.rs` · `hermes-gui/src/commands/{license.rs,chat.rs,review.rs}` · `scripts/{issue-license.py,license-issuer.html}` · 外层 `.gitignore`。
- **风险与回滚：** 风险 = 旧码失效（**这是目的**，但意味着已发出的 3 张发布码要重发）。回滚 = 恢复旧的 `PUBLIC_KEY_BYTES` 与旧 seed —— 只有在确认新码发不出去时才用，且要重开 P0-1。
- **方案确认：** [x] 已对照 P0/P1 · 2026-09-18 用户「第一批」拍板

---

## 2. 实施（Implement）

- **实际改动摘要：**
  1. **换锁。** 新密钥对：私钥 `~/.lebi-ai-issuer/seed.hex`（0600，目录 0700，**只在签发机**）；`PUBLIC_KEY_BYTES` 换成新公钥（`2ed6d2d6…4b94`）。`license.rs` 删掉 `DEV_SEED` 与 `sign_token_with_seed`，改为 `sign_token_with_key(&SigningKey, …)`；验签/落码/状态全部走 `*_with_key` 变体，跨 crate 只暴露「注入公钥」的入口。
  2. **防复发守卫。** 新增 `the_signing_seed_for_the_shipped_key_is_not_in_the_repo`：遍历全仓（跳过 `target/.git/node_modules/dist/.trash`），从文本里抠「可能是 seed」的候选——任意 ≥32 个连续字节值列表（`0x..` 或十进制，**不依赖 `[u8; 32]` 标记**）与 64 位 hex 串——任何一个能推出出厂公钥即 panic。配 `the_leak_scanner_finds_a_planted_seed` 自测（含无标记列表写法）+ 遍历器必须走到本文件的断言，防「守卫自己假绿」。
  3. **门禁收到装配层。** `hermes-core` 新增 `LicenseGatedProvider`（装饰器，锁定时 `complete`/`stream` 直接返回 `license_locked`，`capabilities`/`name` 照透传）；`can_use_main_readonly()` 走 `load_file + build_status`，**不写盘**（热路径不能因为检查而刷 `last_seen`）。`hermes-llm::Config::build_active_provider()` 是四个入口唯一的 provider 装配点，统一套上 ⇒ GUI / server / CLI / IM 一次覆盖。GUI 里原有的两处 `license_locked` 前检保留（用户能看懂的报错），改走只读版，去掉每条消息一次写盘。
  4. **清旧 seed。** `scripts/issue-license.py` 不再有 `DEFAULT_SEED_HEX`，改为 `LEBI_LICENSE_SEED_HEX` → `LEBI_ISSUER_SEED_FILE` → `~/.lebi-ai-issuer/seed.hex` 三级解析，取不到直接报错退出；`scripts/license-issuer.html` 的 seed 输入框清空，页面不再预置密钥。
  5. **数据根挡在 git 外。** `/Users/aodun/Documents/codeINDEx/.gitignore` 加 `test/` 与 `.DS_Store`；`git check-ignore -v test/config.toml` 命中。
- **关键路径/文件：** 见上「技术要点」；签发机自检测试 `issuer_seed_matches_the_shipped_public_key_when_provided`（无 seed 的机器自动跳过）。
- **关于「源码与历史里的 seed」——本轮只做了轮换，没重写历史（这是有意的）：**
  - 事实（亲验）：旧私钥确实在**提交历史**里。`git log --all -S DEV_SEED` 命中 `c7a4897`（v1.4.0）
    与 `9f64eed`（v1.1.0）；`git show HEAD:crates/hermes-core/src/license.rs` 里 `DEV_SEED` 是那个
    `[u8; 32]` 字节数组（`0xb3, 0x6d, …`），远端是 `git@github.com:lebiai/lebiai.git`。
  - 判断：**换锁之后，历史里的旧私钥已经签不出任何客户端认的码**（`verify_token` 用新公钥，
    新公钥与旧私钥不对偶）⇒ 历史泄漏从「致命」降级为「已作废的旧钥匙」。重写历史不会多挡住任何攻击，
    却会打断所有 clone，所以不做。
  - **但必须说清的后果：** 公钥在内核里，改不了已经发出去的包。
    **已发布的 v1.4.0 dmg/exe 仍然认旧私钥签的码**（也就是那三张 `lic-1/2/3` 在旧包上继续能用，
    而新私钥签的码在旧包上激活不了）。轮换真正生效要等**下一版安装包**（v1.4.1+）发出去、客户升级。
- **偏离方案处：**
  - 原打算「把 `can_use_main` 全删、只留只读版」——实际保留 `can_use_main`（它是「顺带把试用开起来」的语义入口，`load_status` 的薄封装），另加只读兄弟。低频路径仍可用它。
  - 原扫描器依赖 `[u8; 32]` 标记 + `marker + 900` 固定窗口切片。**全量测试时暴露了真 bug**：窗口按字节切，撞上中文字符边界直接 panic（`end byte index 40262 is not a char boundary`）。已重写为「标记无关的连续字节值扫描 + 字符边界安全的切词」，同时覆盖了原先抓不到的 `vec![]` / Python 列表写法。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 | 备注 |
|---|------------------|------|------|------|------|
| 1 | 用新私钥签的码能激活 | `issue-license.py`（读签发机 seed）签 `wang-hai-yan,lv-lao-shi` → 真验签路径 `verify_token` + `apply_token_at` | 验签通过、名单正确、`phase=Licensed`、`can_use_main=true` | 通过 | 临时集成测试跑在真实代码路径上 |
| 2 | 换锁前的旧码一律失效 | 用事故里的旧 seed 签同样 payload → `verify_token` + `apply_token_at` | 两处都被拒；**且不落盘**（不写 `license.json`） | 通过 | 同上 |
| 3 | 签发机上的钥匙就是这台构建认的锁 | 签发机跑 `cargo test … license::tests::issuer_seed` | 私钥派生公钥 == 出厂公钥；签→验闭环通过 | 通过 | 另用 `cryptography` 独立复算公钥，同值 |
| 4 | 仓库里搜不到能推出出厂公钥的私钥 | `cargo test … license::tests::the_signing_seed_for_the_shipped_key_is_not_in_the_repo` | 全仓扫描无命中（含 `scripts/` 两处历史硬编码已清） | 通过 | 自测证明扫描器抓得到植入样本 |
| 5 | 过期时发不出模型请求 | `locked_license_never_reaches_the_provider`：注入「未授权」检查 → `complete` / `stream` | 两个方法都报 `license_locked`，**内层 provider 调用次数为 0** | 通过 | 门禁不是「返回假成功」 |
| 6 | 授权正常时用户无感 | `licensed_license_passes_through_untouched` | 请求透传、返回原文、`name`/`capabilities` 不受影响 | 通过 | |
| 7 | 只读检查不写盘 | `can_use_main_readonly_never_writes_and_matches_the_trial_window` | 新机（无文件）放行且不建文件；试用中放行且 `license.json` 字节不变；过期拦住 | 通过 | 热路径要求 |
| 8 | 数据根不在版本库里 | `git check-ignore -v test/config.toml` | 命中外层 `.gitignore:3:test/` | 通过 | |
| 9 | **真机：换锁后旧码被拒、新码可激活** | 用户在 GUI 过期全屏粘贴新发的 `L3-full` | 全屏消失；盘上 `lic_id=L3-full`、名单 6 人、`personas.json.enabled` 同步 6 人 | 通过 | 2026-09-19 10:31 真机 |
| 10 | **真机：过期时 CLI 入口发不出请求** | `LEBI_DATA_DIR=<临时>`（过期试用 + 无 token）跑 `hermes ask "你好"` | 报 `license_locked`；因 base_url 指向关闭端口，若门禁失效会是连接错误 ⇒ **0 次 HTTP 尝试** | 通过 | 端到端，非单测 |
| 11 | 有效授权时门禁不误伤（对照组） | 同一临时数据根换上刚激活的 `license.json` 再跑 | 不再是 `license_locked`，而是真的去连（连接失败）⇒ 门禁会正确放行 | 通过 | 正反两向都验过 |

- **自动化：** `cargo test --workspace`（全绿）· `cargo clippy --workspace --all-targets -- -D warnings`（全绿）· `cargo fmt --all -- --check`（通过）· hermes-core license/provider 共 21 例。
- **手工（真机，2026-09-19）：**
  - 换锁后启动 GUI：旧码被拒 + 试用已于 2026-08-09 过期 ⇒ 落到「已过期」全屏（人物仍在）。
  - 粘 `L3-full` 新码：全屏消失，`license.json` 里 `lic_id=L3-full`、名单 = 王海燕/扫地僧/吕老师/小宋/雨天/小雨；
    `personas.json` 的 `enabled` 同步成这 6 个 ⇒「落码即开通工位」在真机上成立。
  - CLI 入口按用例 10/11 正反两向实测；GUI / server / IM 与 CLI 共用
    `Config::build_active_provider()` 这一个装配点，且装饰器**每次调用**重新判定
    （不是启动时判一次），所以运行中到期也会立刻拦住，不用重启。
- **测试结论：** [x] 全部通过

**门禁实测（2026-09-19，当前工作树）：**

| 命令 | 结果 |
|------|------|
| `cargo fmt --all -- --check` | 通过（无输出） |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过（`Finished dev profile`，0 告警） |
| `cargo test --workspace` | **EXIT=0**，`39` 个 suite 全 ok，**670 passed / 0 failed**，无 panic |

---

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 授权码重新变成「只有所有者能发」；过期在所有入口一致生效 |
| 开箱即用未破坏 | ☑ | 所有者侧多一个「seed 文件在哪」的事实，发码命令不变 |
| 本地优先未破坏 | ☑ | 密钥是一个本地 0600 文件，无新服务、无网络依赖 |
| 测试通过 | ☑ | 见 §3 |
| 记录完整 | ☑ | 本文件 + README 索引 + 两处开发者文档 |
| 产品+架构两视角齐全 | ☑ | §0b / §0c |
| 非修修补补（默认路径正确） | ☑ | 门禁在装配点而非业务命令；私钥靠密钥管理而非自觉 |
| 代码卫生 | ☑ | `DEV_SEED` / `sign_token_with_seed` / 两处硬编码 seed / 旧扫描器全部删除，无新旧两套并存 |
| 操作与视觉 | ☑ | 不新增可见面；唯一新增可见面是过期时非 GUI 入口的人话错误 |
| 第一性原理三步写全 | ☑ | §0-fp |

- **验收人：** 用户（真机粘码）· 工程侧自证见 §3
- **验收日期：** 2026-09-19
- **结论：** ☑ 通过（真机用例 9/10/11 现场跑过）
- **遗留项：**
  1. **要发下一版安装包，轮换才真正生效。** 已发布的 v1.4.0 公钥在二进制里，改不掉：旧码在旧包上仍能用，
     新码在旧包上激活不了。给客户的新码必须配新包（v1.4.1+）。
  2. **已发出的授权码（`lic-1/2/3` 等）对新包一律作废，必须重发。**
  3. 签发机 `seed.hex` 的备份 / 换机流程未写进手册（只写了位置与自检命令）。
  4. 非 GUI 入口过期时报的是 `license_locked` 这个 token（CLI/IM 没有像 GUI 那样的中文人话），
     用户可见面待补一句中文文案。
  5. P0-3（P0 未收编工位项目组）/ P0-5（试用可本地重置）/ 其余 P1 未在本批处理。

---

## 5. 附注

- 新码（本轮现发，附签发时间 2026-09-19，均为一年期 → 2027-09-18）：见 §3 用例 1 的口径复验记录；
  码本身不进仓库（与 v1.4.0 同一纪律）。
- 换锁后本机 `test/` 数据目录里那张 `L3-caifu-quantao`（旧私钥签的、2027-09-17 到期）会失效，
  启动后回落到 3 天试用 —— 这是预期的换锁后果，不是 bug。
