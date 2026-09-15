# 人物名册：新增六个角色（王海燕 / 小谢 / 小金 / 雨天 / 小雨 / 小文）

| 字段 | 内容 |
|------|------|
| **日期** | 2026-09-15 |
| **范围** | 只做**人物定义**。skill / 授权名单 / GUI 一律不动（后续各自专项） |
| **状态** | 已实现 · 待验收（GUI 未接线，用户不可见） |
| **上游** | [`../spec/personas.md`](../spec/personas.md) · P0 v0.11 §人物 |

## 第零条：第一性原理

1. **拒绝了什么类比：** 「人物 = 一个预设提示词模板」。模板类比会把人设做成可替换的字符串，边界靠自觉。
2. **拆出的真：** 人物的「硬」来自**职责边界**（干什么 / 不干什么 / 口径 / 越界指路），不是来自口吻。
   口吻是调味，边界是结构——边界写不出具体清单，拦不住也指不了路。
3. **如何从真推出：** 每个定义必须写全四段，其中「不干什么」和「越界指路」是**可执行清单**而非抽象口号；
   人物是人物、skill 是 skill：定义只管**这个人干什么**，绑技能是另一层（本次不做）。

## 用户价值

用户在侧栏看见的工位，从 4 个扩到 10 个（3 自带 + 7 授权），且每个工位的职责互不重叠。

## 产品经理视角

- **场景：** 用户要一份林碳判断 / 一条具身智能解读 / 审稿 / 口播 / 资料入库，各自找对人。
- **怎么走完：** CLI 今天用 `hermes chat --persona <id>` 验证；GUI 侧栏属阶段 3。
- **空/载/错态：** 沿用既有（空态用人物自己的 `empty_hint`）。
- **不做什么：** 不做人物管理界面；不改 skill；不发票；不动 GUI。

## 架构师视角

- **根因：** 名册是产品设定，编译进二进制、单一真源；缺定义 = 静默不发车，所以有 `every_definition_file_is_registered_in_sources` 盯着目录与 `SOURCES` 两边。
- **默认路径：** 新增角色 = 加一个 `src/personas/<id>.md`（文件名 stem 必须等于 id）+ 登记进 `SOURCES`。
- **边界：**
  - 五个授权角色（`wang-hai-yan` / `xiao-xie` / `xiao-jin` / `yu-tian` / `xiao-yu`）：`kind: dedicated`、`builtin: false`、记忆归自己。
  - 资料员小文（`xiao-wen`）：**必备角色**，用现成 `builtin: true`——谁都有、不进授权码、其会话记忆算全局。
    不新增字段：新增会让「谁都有」出现第二个真源。
- **代价：** `builtin` 数量 2 → 3，断言与文案需同步（本次已改）。

## 改动文件

- 新增 `crates/hermes-core/src/personas/{wang-hai-yan,xiao-xie,xiao-jin,yu-tian,xiao-yu,xiao-wen}.md`
- `crates/hermes-core/src/persona.rs`：`SOURCES` 4 → 10；`builtins().count()` 断言 2 → 3；新增名册测试。

## 验收

- `cargo test -p hermes-core persona` → 14 passed / 0 failed。
- `hermes personas` 输出 10 行，小文标「自带」，五个新角色标「授权」。

## 已知未做（明确不在本次）

1. **职责重叠未清**：`lin-tan`（小张）与 `xiao-xie`（林碳小谢）同盯林碳；`xiao-wang`（小小小王）与 `wang-hai-yan`（情报王海燕）同为新闻采集。**待用户拍板删除或合并。**
2. 六个角色**均未绑 skill**（`skills: []`）——技能面收窄属阶段 4。
3. 授权码仍无 `personas` 字段（阶段 2）；今天任何码都看到全部角色。
4. GUI 侧栏与头像未做（阶段 3）。

---

# 追加：人物选择（设置）+ 会话身份（GUI 后端）

**日期：** 2026-09-15 · **范围：** 只改 `crates/hermes-gui/src/**.rs`（前端同批由另一人改，本次一行未碰 `ui/`）

## 第零条三步

- **拒绝了什么类比：** 拒绝「把人物选择当成一份 UI 偏好、只存在前端内存里」。那样重开 App 就归零，且引擎侧（提示词 / 记忆归属）看不到用户选了什么。
- **拆出的真：** 人物可见性是**用户资产的一部分**，必须落盘在数据根、与引擎同一份真源（`hermes_core::persona::all()`）。会话身份则是**文件事实**，写在 `SessionMeta.persona`。
- **如何推出：** 选择落 `personas.json`（只存非自带的 id）；引擎渲染条目时 `builtin` 恒真 + 文件里的选择；开会话时把 persona 写进 meta，系统提示词再按 meta 解出人物块。

## 产品经理视角

- 设置页：人物列表 = `list_personas()`，每条带 `enabled`；勾选 = `set_personas(ids)`，**返回盘上那一份**。
- 侧栏：按 `enabled` 过滤出工位；点工位 = `new_session({ personaId })`。
- 空/错态：`personas.json` 不存在 / 读不动 / 形状不对 → 只有三个自带（小乐 / 李现 / 小文），不报错、不白屏。
- 不做什么：不做人物管理界面、不让人物自己改人设、不动授权码。

## 架构师视角

- 落盘：`hermes_core::data_path("personas.json")`（现成取法，`LEBI_DATA_DIR` 可覆盖），形状 `{"enabled":[...]}`。
- `enabled` 语义：`builtin` 恒真；其余看文件。未知 id **读写两侧都丢**（写出口过滤，任何调用方都写不出脏文件）。
- `SessionSummary` 增 `persona: Option<String>`（camelCase → `persona`，恒发键：无人物是 `null`），`list_sessions` / `new_session` 都填。
- `new_session(persona_id)`：id 不在名册 → `NotFound`（不认识的 id 落进 `meta.persona` 会静默变成「没人设也没归属」）；空草稿**只在人物一致时复用**，换人物即丢弃旧草稿（草稿不落盘）。
- 提示词：新增 `turn_sources(...)`，`persona` 由 `session.meta.persona` 解出（不认识 → `None`，与 `memory_owner_for` 容错一致），把 `persona: None` 写死会被测试抓住。

## 改动文件

- 新增 `crates/hermes-gui/src/commands/personas.rs`（`PersonaItem` / `list_personas` / `set_personas` + 单测）
- `crates/hermes-gui/src/commands/session.rs`（`SessionSummary.persona`、`draft_summary`、`reuses_empty_draft`、`new_session(persona_id)`）
- `crates/hermes-gui/src/commands/chat.rs`（`turn_sources` 接线人物块 + 单测）
- `crates/hermes-gui/src/commands/mod.rs`、`crates/hermes-gui/src/main.rs`（注册两条命令）

## 验收

- `cargo test -p hermes-gui` → 24 + 2 + 2 passed / 0 failed（含 `every_invoked_command_is_registered`）。
- `cargo clippy -p hermes-gui --all-targets -- -D warnings` → 0 告警；`cargo fmt -p hermes-gui` 干净；`cargo check --workspace` 通过。
- 真实数据未动：`/Users/aodun/Documents/codeINDEx/test/memory-stats.jsonl` sha256 仍为 `0034fac7…d63b59`；未生成 `~/.lebi-ai/personas.json`。

## 已知未做

1. 授权码仍无 `personas` 字段 → **今天发任何码，客户端都看到全部 10 个角色**（阶段 2）。
2. 人物头像、侧栏视觉（前端同批）。
3. `lin-tan`/`xiao-xie`、`xiao-wang`/`wang-hai-yan` 职责重叠未清。

# 追加：侧栏去掉「最近对话」模块（工位成为唯一会话入口）

**日期：** 2026-09-15 · **范围：** 只改 `crates/hermes-gui/ui/`

## 第零条三步

- **拒绝了什么类比：** 拒绝「把记录列表藏起来就行」——藏掉列表却保留「新对话」按钮，等于保留一条**通向死胡同**的入口
  （造出的会话 `persona: null`，界面上再无任何地方能回到它）。
- **拆出的真：** 一段会话必须**总能被再次打开**，否则它不是「隐藏」而是「丢失」。入口与归属必须同时成立。
- **如何推出：** 侧栏唯一入口 = 工位；会话的**归属即入口**（工位 → 该人物最近一段）。因此凡是会造出无归属会话的入口，
  要么去掉，要么改指向一个真实工位。三处无参 `newSession()` 全部指向自带角色「搭子小乐」——
  「自由对话」由此从说法变成代码事实。

## 产品经理视角

- 侧栏只剩「工位」（自带三个）+ 底部导航 + 用户条；删掉的是：最近对话标题、分组列表、每条的删除按钮、搜索按钮与搜索框、「新对话」按钮。
- 「自由对话」= 搭子小乐：首次启动、⌘N、引导结束三条路都开到它，不再产生没处回去的会话。
- 已知后果（**用户拍板的取舍**）：微信/飞书渠道会话在 GUI 里**不再有入口**（唯一例外是「在办」里指向某条 sessionId 的链接）。
  文件仍在磁盘，未删。若要让它们可回看，需另设入口。

## 架构师视角

- `Sidebar.tsx` 420 → 206 行；删掉的符号：`query` / `searchOpen` / `searchInputRef` / `confirmDeletePath` / `draftMatches` /
  `filtered` / `groups` / `hasListContent` / `locale`、`sessionTitleOf` / `groupLabelKey` / `openSession` / `handleNew` /
  `openDraft` / `closeSearch`，以及只为它们服务的 import 与三条 `useEffect`。
- 布局：工位块由 `shrink-0 max-h-[34vh]` 改为 `flex-1 min-h-0 overflow-y-auto`（占满并自滚，不留空白）。
- **旧必清**：`ui/src/utils/sessionTime.ts` 零引用 → 整个文件删除。
- 保留：`openStation`（工位 → 该人物最近一段会话）、`fetchSessions`（回窗口时刷一次，渠道会话会在后台写入）。

## 验收

- `npm run build` 通过；`cargo test -p hermes-gui --test command_registration` 2 passed / 0 failed。
- 协调者实跑 `scripts/run-gui.sh` 重启 + 截图目视：侧栏只剩「工位」，无最近对话、无新对话、无搜索。

## 已知未做

1. i18n 里 `chat.recentSessions` / `chat.group*` / `chat.searchSessions` 等 key 已无引用，**待清理**（另开一轮）。
2. 渠道会话（微信/飞书）暂无 GUI 入口（见上）。
3. `lin-tan`/`xiao-xie`、`xiao-wang`/`wang-hai-yan` 职责重叠未清。
---

# 授权码带人物名单（阶段 2 · 授权链路）

> 受理：2026-09-15 · 范围：`hermes-core/src/license.rs`、`hermes-gui/src/commands/personas.rs`、
> `scripts/issue-license.py`。前端展示与两个旧人物删除由并行的任务负责，本任务未碰。

## 第零条三步

1. **拒绝的类比：** 「授权码管的是能用/不能用，人物是设置里的开关」——把两件事当成两个系统，
   于是客户端只要有设置页，谁都能给自己开出全部角色。这是把「授权」降级成「界面偏好」。
2. **拆出的真：** 谁在这儿上班，是**卖家卖的**，不是**用户挑的**。人物名单属于**授权事实**，
   与有效期同一个签名载荷；设置页的勾选只是「已授权的这几个里，我今天想看见谁」。两个闸门串联，不是一个。
3. **如何推出：** 名单必须随码签名（离线可验、改不了）→ 落在 `LicensePayload.personas`；
   客户端把它翻成 `LicenseStatus.personas`（外加不认识的另立一列）；显示口径定为
   `enabled = builtin || (授权名单含该 id && 用户勾过)`，且**这一处判断只写一遍**，读盘、写盘、渲染都问它。

**用户价值：** 卖家发一张码就能决定这个客户开哪几个工位；用户手改本机文件也变不出没买的角色；
过期时角色不消失（买过的东西不因为续费提醒而消失）。强化「Care / Continuity」。

## 产品经理视角

- **怎么走完：** 设置 → 授权 → 粘码 → 侧栏工位区**立即**多出码里的那几个；没买的角色不出现。
- 试用期（未输码）：只有三个自带（小乐 / 李现 / 小文）——试用不是名册。
- 过期后：主能力锁屏，但工位**照最后一份有效码**显示，不收回。
- 设置页的候选项里会出现「没买」的角色，`licensed: false`；前端据此显示「不在你的授权内」，
  不再给一个勾了没反应的开关（本任务只把字段备好，前端接线按阶段 3）。
- 码里写了个本版本不认识的 id：不静默——`unknownPersonas` 带出去，界面可以说「你的码比 App 新」。

## 架构师视角

- **`LicensePayload`**（无 `deny_unknown_fields`，向后兼容不变）：新增
  `#[serde(default, skip_serializing_if = "Option::is_none")] personas: Option<Vec<String>>`。
  不传 = 老格式**逐字节不变**（有测试盯着 payload 里不出现 `personas` 键）。
- **`VerifiedLicense`**：新增 `pub personas: Vec<String>`——**token 说什么就是什么**，不在这里筛。
- **`LicenseStatus`**（camelCase）新增两个键，**确切 JSON 名**：
  - `personas: string[]` — 本机应显示的角色 id（已剔除本版本不认识的；Trial 为空）。
  - `unknownPersonas: string[]` — 码里点名、本版本名册里没有的 id（排序去重）。
- 过期路径：`build_status` 的 Locked 分支本来就 `verify_token` 一次拿 `expires_at`，
  现在同一处取名单——**过期不收回角色**由这一处保证。顺手把三份重复的
  `Utc.timestamp_opt(...)` 收敛成 `fmt_unix`。
- 名单过滤只此一处：`split_personas()`（认识的留下、去重、保持码里的顺序；不认识的另立一列）。
- **GUI 闸门：** `personas::licensed_ids()` = `load_status().personas`；
  `is_selectable()` = 注册表里有 && 非自带 && 在授权名单里。**读盘与写盘都过它**，
  所以手改 `personas.json` 也变不出没授权的角色。授权文件读不动 → **报错**，
  不静默收窄名单（静默会让下一次存盘把用户的选择抹掉）。
- **发码工具：** `--personas a,b,c`（可重复）；`--list-personas` 从
  `crates/hermes-core/src/personas/*.md` 读 id，不凭记忆写；未知 id 默认拒签
  （`--allow-unknown-personas` 才放行）；自带角色写进名单会被提示并剔除。
  签名库 PyNaCl 与 cryptography 二选一（Ed25519 确定性，两边签出同一串）。

## 验收

- `cargo test -p hermes-core -p hermes-gui` → 70 + 25 + 2 + 2 passed / 0 failed。
- `cargo clippy -p hermes-core -p hermes-gui --all-targets -- -D warnings` → 0 告警；
  `cargo fmt` 对这两个 crate 干净（全仓 `--check` 的 7 处差异都在 hermes-memory / hermes-reflect /
  hermes-tools，属并行任务的在写文件）。
- `python3 scripts/issue-license.py --list-personas` → 列出 8 个（3 自带 + 5 授权），与注册表一致。
- **端到端：** 真签一张码 → 真验证器 `verify_token` 解出 `personas = ["wang-hai-yan","xiao-xie"]`。
- **突变证伪（协调者实跑，两条都见红后回滚）：**
  ① Locked 分支名单改 `Vec::new()` → `personas_survive_expiry` FAILED；
  ② `licensed` 写死 `true` → `a_trial_never_shows_a_licensed_role_even_if_the_file_asks_for_one` FAILED。
- **三张真码**（365 天，`--plan year`）已用**客户端内置公钥**独立验签通过：
  `L1-haiyan-xie` = wang-hai-yan + xiao-xie；`L2-jin-haiyan` = xiao-jin + wang-hai-yan；
  `L3-haiyan-yutian-xiaoyu` = wang-hai-yan + yu-tian + xiao-yu。
- 真数据未动：未对 `/Users/aodun/Documents/codeINDEx/test` 写入任何文件。

## 已知未做

1. **前端未用 `licensed`**：设置页目前把没买的角色也列成「关着的开关」，勾了不动（会被写盘过滤）。
   要显示「不在你的授权内」，得前端接线（阶段 3）。
2. `hermes personas --licensed`（CLI 端展示授权名单）没做——台账 §2.2 里原本列了它。
3. 三张码是 **dev seed** 签的（与现网 dmg 同一把私钥）；生产换钥流程未变。
4. 本文件第一段「已知未做」第 3 条（两个旧人物的职责重叠）已由并行任务删除
   `lin-tan` / `xiao-wang` 解决，注册表现在 8 个：3 自带 + 5 授权。

---

# 追加：删除旧角色「小张 / 小小小王」+ 阶段 2 收口验收

**日期：** 2026-09-15 · **决策人：** 用户（原话：删除 小张 和 小小小王，明确只保留我确定的人物）

## 第零条三步

- **拒绝了什么类比：** 拒绝「保留旧角色更安全，反正没人用」。留着两顶与新角色**同职责**的帽子，
  用户和模型都要面对「林碳到底找谁」二选一——这不是安全，是把歧义留给用户。
- **拆出的真：** 名册是**产品设定**，不是历史档案；一份名册里不能有两顶同职责的帽子。
  删角色**不等于删数据**——判定标准是「删了以后有没有东西变成孤儿」。
- **如何推出：** 先验证孤儿：`rg` 真实数据根（`test/memories/`、`test/sessions/`）——
  **owner 为 `lin-tan`/`xiao-wang` 的记忆 0 条、persona 为它们的会话 0 条**（正文里出现的
  `xiao-wang` 只是文字提到 skill 名）。无孤儿 → 直接删，无需迁移。

## 改动

- 删 `crates/hermes-core/src/personas/lin-tan.md`、`personas/xiao-wang.md`；`SOURCES` 10 → 8。
- 全仓清引用：`lin-tan` 0 处；人物 id `xiao-wang` 0 处。
  **保留** `crates/hermes-skills/src/store.rs` 的 4 处 `xiao-wang`——那是**技能名 fixture**，人物与技能是两层。
- 顺手修真 bug：`xiao-le.md` 的越界指路原本写着「林碳找小张，新闻稿找小小小王」——指向两个已删的人，已改为「林碳找小谢，采情报找王海燕」。
- 删测试 `xiao_wang_is_dedicated_and_binds_its_own_skill`（人物已不存在）。

## 名册定稿（8 个）

- 自带 3：`xiao-le` 搭子小乐 / `li-xian` 工具人李现 / `xiao-wen` 资料员小文（含小文：必备，`builtin: true`）
- 授权 5：`wang-hai-yan` 情报王海燕 / `xiao-xie` 林碳小谢 / `xiao-jin` 具身智能小金 / `yu-tian` 编辑雨天 / `xiao-yu` 主播小雨

## 已发出的三张码（dev 种子，365 天，exp 一致）

| 代号 | 名单 | `lic_id` |
|---|---|---|
| 授权1 | `wang-hai-yan` + `xiao-xie` | `L1-haiyan-xie` |
| 授权2 | `xiao-jin` + `wang-hai-yan` | `L2-jin-haiyan` |
| 授权3 | `wang-hai-yan` + `yu-tian` + `xiao-yu` | `L3-haiyan-yutian-xiaoyu` |

同一台机器可依次换着粘（三张 exp 相同，不会被「比当前旧」挡掉）。

## 协调者实跑验收

- `cargo test --workspace` → **580 passed / 0 failed / 3 ignored**（协调者亲跑，非实现者自证）。
- `cargo clippy --workspace --all-targets -- -D warnings` 干净；`cargo fmt --all -- --check` 干净。
- `MUTANT` 残留扫描：0 处（防子代理把突变测试留在树里）。
- `scripts/run-gui.sh` 重启 + 截图目视：**授权未带名单时，侧栏只剩三个自带**——
  「授权即名单」在真实界面上成立。
- 真实数据未动：`test/memory-stats.jsonl` sha256 = `0034fac7…d63b59`。

## 已知取舍 / 未做

1. **老格式码（无 `personas` 字段）= 只有三个自带**。本机现存那张码就是老格式，
   所以升级后角色会「消失」，粘新码才回来。**这是我们选的语义**（码说了才算），不是 bug。
2. 前端新引入 `licenseStore → chatStore` 的反向依赖（`applyToken` 成功后 `fetchPersonas()`），
   两边只在函数体内取值，模块初始化互不触碰；**登记待后续解环**。
3. 设置页目前把「没买的角色」不显示；`PersonaItem.licensed` 字段前端**尚未使用**。
4. i18n 里 `chat.recentSessions` / `chat.group*` 等已无引用的 key 待清。

## 收尾（协调者）

- 前端那处「前向兼容」的临时类型 `LicenseWithPersonas` 已删：`personas` / `unknownPersonas` 直接声明在
  `licenseStore.ts` 的 `LicenseStatus` 上（后端字段已落地，交集类型成为冗余）。
- 前端重建后重启 GUI，确认侧栏只显示三个自带（授权名单为空时的正确表现）。

---

# 追加：命名与排序修正 + GUI 记忆读面按人物隔离（B-3 修复）

**日期：** 2026-09-15

## 第零条三步

- **拒绝了什么类比：** 拒绝「GUI 的记忆过滤下一阶段再说」——读面已经按工位切了，写面没切，
  这不是「少做一步」，是**同一轮对话里两条轴不一致**：注入的记忆按人物过滤，`memory_save`
  却把内容落成全局。用户会看到「我明明在小谢这儿说的，小金也知道」。
- **拆出的真：** 隔离不是「提示词里筛一下」，而是**注入面与写面同轴**；只要两处各写一份判据，
  就一定会有一天只改一处。
- **如何推出：** 把装配收成 `hermes_memory::views_for(all, owner)` **一处**，CLI 与 GUI 共用；
  GUI 侧再抽 `visible_memories()` 让它可测。判据仍只有 `visible_to` 一处。

## 改动

1. **GUI 记忆注入按人物切**（B-3 修复）
   - `crates/hermes-memory/src/memory.rs` 新增 `views_for`（`visible_owned` + pinned 子集），`lib.rs` 导出。
   - `crates/hermes-cli/src/commands/chat/mod.rs`：删掉本地那份同名函数，改用共享实现（**不再 fork**）。
   - `crates/hermes-gui/src/commands/chat.rs`：新增 `visible_memories()`，注入面对它取值；
     同时把「全量写缓存」与「过滤后用于注入」分开（缓存仍是全量，管理页照旧看得到全部）。
   - 新增测试 `visible_memories_keeps_globals_and_own_only`；**突变证伪**：owner 写成 `None`
     （即修复前 GUI 的行为）→ FAILED，回滚后全绿。
2. **人物改名**：`工具人李现` → `工具人李现在`（id `li-xian` 不变——授权码与记忆归属引用的是 id）。
3. **左上角 slogan**：新增 `sidebar.tagline` = 「你的AI团队」/「Your AI team」，只作用侧栏。
4. **侧栏工位排序**：授权角色在上、自带在下（两组内部保持后端原顺序；后端顺序未动）。
5. **`wang-hai-yan.md` 口径自相矛盾修复**：「不停在事实上」→「不越出事实层」——原句与她「只停在事实层」
   的定位相反，且原样进提示词。

## 协调者实跑验收

- `cargo test --workspace` → 39 组全 ok、0 failed；`clippy -D warnings` 0 告警；`fmt --check` 干净。
- `MUTANT` 残留扫描 0 处。
- 截图目视：左上角「你的AI团队」；工位显示「工具人李现在」。

## 已知欠账（本轮**未**做，按优先级）

1. **GUI 写面仍未按人物隔离**（与上面第 1 条同一根轴的另一半）：GUI 用普通 `BuiltinToolHost`，
   `memory` 恒 `owner=None`，`PersonaToolHost` 只在 CLI / IM 接线。
   **后果**：GUI 里在小谢会话说「记住…」，这条落全局，别的人物也看得见——**读写仍不一致**。
   这条是下一轮第一优先。
2. **技能面仍全局**：任何工位都看得到全部技能索引与触发词（`小王干活` 在小谢会话里也可能被激活），属阶段 4。
3. 授权锁屏页仍用旧标语「越用越像你的手感」（`app.tagline`），未与侧栏对齐。
4. `wang-hai-yan.md` 正文写「落盘到 `outputs/`」，项目默认是 `outputs/<YYYY-MM-DD>/`，口径待统一。
5. 设置页把「没买的角色」列成关着的开关，勾不动且无反馈。

---

# 追加：落码即开通工位 + 加载动效（用户实测反馈修复）

**日期：** 2026-09-15

## 第零条三步

- **拒绝了什么类比：** 拒绝「粘了码没反应 = 用户没去设置里勾」——把产品缺陷当成用户操作问题。
- **拆出的真：** 「授权名单」决定的是**这个客户买了什么**，不是**这一屏要放什么**。
  中间再插一道手动的勾选，等于让用户替产品确认它已经收过钱的事。
- **如何推出：** 落码 = 开通。名单里的角色**自动并入**本机选择（并集，不覆盖用户已有的勾选）；
  用户在设置里的勾选仍然是「买了这么多，这屏先摆哪几个」的裁剪，不是开通的前置条件。

## 改动

- `commands/personas.rs`：抽出唯一写盘口 `commit()` + `enable_licensed_at()`（并入语义；未知 id / 自带 id 一律丢弃；老格式码不碰文件）。
- `commands/license.rs`：`apply_license()` 落码后自动开通；新增 `apply_license_at(license_path, prefs_path, token)` 让测试用 tempdir 走**同一条真实路径**。
- `ApplyLicenseResult` 增 `enabledPersonas: string[]`（口径：落码后处于 enabled 的授权角色 id；自带不在其中；老格式码为空）。
- 前端：`LicenseForm` 提交中转 `Loader2` + 「正在加载你的工位…」+ 双禁用；`LicenseSettingsCard` 在卡片层显示
  「已开通 N 个工位：…」（表单成功即折叠，反馈不能跟着消失）；`applyToken` 返回 `{status, enabledPersonas}` 且**等工位刷新完再返回**。
- **顺手补的洞**：同一张码重粘，原先是 `SameAsCurrent` **报错**——于是「粘过但没生效」这种情况永远救不回来。
  现在重粘走「把名单再开通一遍」，正常返回。

## 验收

- `cargo test -p hermes-gui` → 31 + 2 + 2 passed / 0 failed（新增 5 条：自动开通、重粘、老格式码、未知 id、换码换名单）。
- 两条突变见红后回滚（重粘分支改回报错 / 落码开通改 no-op）；`MUTANT` 残留 0 处。
- 真机端到端（临时数据根，事后删除）：授权1 → 侧栏直接多出「情报王海燕 / 林碳小谢」并排在三个自带**上面**；
  再粘授权2 → 换成「情报王海燕 / 具身智能小金」，卡片显示「已开通 2 个工位：情报王海燕、具身智能小金」。
- **未截到的证据（如实记）**：加载动效（spinner + 「正在加载你的工位…」）**代码已实现但未目视验证**——
  本地往返约 10ms，注入延迟截图时 macOS 收回了辅助访问权限。协作方自行核验时可临时加延迟看一次。

---

# 追加：隐藏技能页 + 首页只留问候语（含一次 i18n 误删与恢复）

**日期：** 2026-09-15

## 第零条三步

- **拒绝了什么类比：** 拒绝「首页是产品说明书」——用四张场景卡、大标题、标语替用户先想好他要干什么。
- **拆出的真：** 用户已经知道自己要干什么；首页的职责只是**确认它还在**（一句问候），不是派活。
  菜单式首页会把「我不知道该说什么」变成一个必须跨过的界面。
- **如何推出：** 首页 = 时间问候一句（`returnGreetingKey()`），其余全撤；技能页 = 直接不可达（连 `KnowTab` 里的
  `"ways"` 一并删掉，不留半死入口）。

## 改动

1. `WelcomeScenes.tsx` 重写：只留问候语；删除场景卡（`SCENARIOS`）、大标题、标语、底部提示语，
   以及随之无用的 `onPick` / `disabled` props、`onboarding_seed_get` 取数、`MotionCard` / `RitualMark` 等 import。
   `ChatView.tsx` 同步去掉 `handlePickPrompt` 与 `setComposerPrefill` 绑定（它只为首页卡服务）。
2. 技能页不可达：`KnowPanel.tsx` 去掉「你的做法」tab 与 `SkillPanel` 分支；`navStore.ts` 的
   `KnowTab` 收成 `"you" | "materials"`（类型层封死，不留可传的死值）。
   **`components/skills/SkillPanel.tsx` 仍留在树里但已无入口**——待定：删除还是以后恢复。
3. 清掉因此死亡的 i18n 键 34 条（中英各 17）：`welcome.title` / `subtitle` / `hint` / `titleWithName`、
   `welcome.scene{Write,Think,Research,Track}.{title,desc,prompt}`、`know.tabWays`。
   **`welcome.return*` 四条保留**（首页唯一的文案，按时间段取）。

## ⚠️ 本次事故（如实记录）

- **发生：** 协调者在重做 i18n 清理时误用 `git checkout -- crates/hermes-gui/ui/src/i18n.ts`，
  **把本次会话里所有未提交的 i18n 文案改动一并回滚**（`persona.*`、`materials.*`、`memory.topics*`、
  `pager.*`、`sidebar.tagline`、`license.enabledPersonas` 等）。
- **恢复：** 从最近一次**成功**构建的产物 `crates/hermes-gui/ui/dist/assets/index-wd8oHaIA.js` 中
  按 key 提取中英两份字典，精确回填缺失键 **48 个**。校验：两边各 **715** 键、**无重复键**、
  **无单边键**（`set(en) ^ set(zh) == ∅`）、`tsc --noEmit` 干净、`vite build` 通过、`cargo test -p hermes-gui` 全绿。
- **残余风险：** 恢复的是**值**；插入位置按「相邻键」定位，**文件内的排序与换行可能与丢失前不同**。
  这属于格式差异，不影响 `TranslationKey` 与渲染结果。
- **教训（写给所有人）：** 工作区里这些改动**全部未提交**，任何 `git checkout -- <file>` 都是不可逆删除。
  清理 i18n 这类「按 key 删」的活，必须用 key 匹配删，**不许用行号范围**（第一次就是按行号把
  `welcome.return*` 一起删了，才引出后面这一串）。

---

# 追加：首页大色块（AmbientStage）拆除 + 问候语文案对齐

**日期：** 2026-09-15

## 用户报的问题

> 「不对啊，首页的样式呢？不好看了啊。文字底部的底层去掉啊，现在一个大色块放在哪儿。之前页面上欢迎的内容就很好啊。」

## 第零条三步

- **拒绝了什么类比：** 拒绝「把内容删掉、壳留着」——`AmbientStage(rich)` 是给**有内容的 hero** 当底的舞台，
  它的存在前提是台上有人。台子空了它不会自动收起，会读成一块浮在半空的蓝色色块。
- **拆出的真：** 装饰层和内容是**共生**的，不是可独立开关的两件事。装饰层的尺寸写死在 CSS 里
  （`h-[78%] w-[88%]` 的 `bg-app-primary-soft/90` + 第二团 blob + vignette），它不随内容多少变化。
- **如何推出：** 首页只剩一句问候时，**底必须一起撤**，而不是把字号调大去填。

## 改动

1. `WelcomeScenes.tsx`：不再套 `AmbientStage`，改朴素容器（`min-h-[52vh]` + 居中 `text-2xl sm:text-3xl`），
   落回对话底色。`AmbientStage` 本体保留（`OnboardingRitual` 还在用，那里台上有人）。
2. 问候语文案对齐：`welcome.returnAfternoon / returnEvening` 原写「选个起点即可 / pick a place to start」，
   而**起点（四张场景卡）已经不在**——留着就是一句指着不存在控件的话。中英各四条统一改成
   「说一句就开始 / say the word and we start」（`returnMorning` / `returnLate` 同批对齐）。
3. `utils/greeting.ts` 注释尸块清理：原文写「first-run relies on welcome.subtitle alone」，而
   `welcome.subtitle` 已随上一批删除。

## 验收

- `npx tsc --noEmit` 干净；`npm run build` 通过；GUI 重启后**目视确认色块消失**（1200×737 窗口实拍）。
- 首页现在：一句问候，居中，无背板、无卡片、无标语。

## 待定（本轮不动，等用户决定）

- `components/motion/MotionCard.tsx`：**已无任何引用**（唯一用户是首页场景卡）。
- `components/skills/SkillPanel.tsx`：**已无任何引用**（上一批「隐藏技能页」的结果）。
  两个文件都仍是 git 跟踪状态（未提交但可 `git show HEAD:path` 取回），删还是留需用户拍板。

---

# 追加：空首页的「一句话」美学（逐字浮入 + 层次 + 收尾一笔）

**日期：** 2026-09-15

## 用户报的问题

> 「好看的样式 和 文字特效没了啊。哎。你帮我构思一个好看的效果，然后坐上去吧。现在的没有一点美感。」

## 第零条三步

- **拒绝了什么类比：**
  ① 拒绝「空页面就是空」——摆一句话交差，那不叫极简，那叫没做完。
  ② 拒绝「靠加东西来撑场面」——卡片、插画、徽标、大色块，用**数量**换**质感**。
  ③ 拒绝「把字号调大就叫设计」。
- **拆出的真：** 首页此刻只有一句话，那么**这一句话就是整个界面**。它的美感只能来自
  「这句话怎么出现」+「这句话内部怎么分层」，来自**时间**与**层次**，不来自像素面积。
  上一版真正的问题是：删掉了内容，却把**装饰层**留下了——装饰层的存在前提是台上有内容。
- **如何推出：** 四件事构成首页，全部落在同一句话上——
  1. **逐字浮入**：每个字从 `blur(10px) + 下移 0.26em` 落到清晰，按 20ms 步进错开
     → 读起来是「这句话正在被写出来」，不是「一个块出现」。
  2. **两段层次**：问候语（深、600）／破折号（收小压扁、降一档色）／动作提示（浅、500）
     → 一句话自带主次，不用再加小标题。
  3. **落定后自己画出的一笔**：88px 渐变细线从中间展开（`scaleX .2 → 1`），
     延迟 = 入场总时长 → 给整组一个「闭」的动作，视线有落点。
  4. **不设任何背板**：没有 `AmbientStage`、没有卡片、没有徽标、没有插画。
- **可推演性：** 任何一条都能单独推翻（去掉线、改步进、改层级），不需要重做界面——
  这正是「从真推出」的标志。

## 改动

- `ui/src/index.css`：新增 `--motion-char-step: 20ms` 进 `@theme`（动效时长仍单一来源，
  页面不自造时长）；新增 `.greet-line / .greet-ch / .greet-sep / .greet-tail / .greet-rule`
  与 `greet-ch-in / greet-rule-in` 两个 keyframes；补齐 `html.dark` 四组配色与
  `prefers-reduced-motion` 降级（关动效后直接给落定态）。
- `ui/src/components/chat/WelcomeScenes.tsx`：按中英两种破折号把问候语切成
  「问候 / 分隔 / 动作」三段，逐字渲染并带上 `--greet-i` 索引；`sr-only` 放整句、
  逐字那层标 `aria-hidden`（否则屏幕阅读器会一个字一个字念）；容器 `min-h` 52vh → 66vh，
  把整组提到空白区的视觉中心。

## 验收

- `npx tsc --noEmit` 干净；`npm run build` 通过；GUI 重启后实拍。
- **动效目视确认**：连拍 6 帧，第 1 帧只有前 4 字处于模糊中、第 2 帧前 4 字实、后 6 字正在化开、
  第 3 帧起全部落定 → 错开确实是**左→右**，不是整块闪现。
- **几何目视确认**：整行中心 = 1471px、内容区中心 = 1471px（等宽，居中成立）；
  收尾细线实测落在 `y 743–746`（2px 高），与文字留白 26px。
- **未目视验证（如实记）**：暗色配色只有代码、没有实拍——切 macOS 系统外观未带动本应用
  （应用主题不是 `system`），改从「设置 → 外观」点选时坐标点偏落到了「使用手册」。
  深浅两套色值照抄旧版 Tailwind 的 `dark:text-white / slate-400 / slate-500` 口径，协作方复核一次即可。

---

# 追加：顶栏写「在跟谁干活」 + 新增自带角色「大导演」

**日期：** 2026-09-15

## 用户要的两件事

> 1 去掉 主页左上角的新对话，改成 选择哪个人物 就和哪个人物交流工作。
> 2 再默认增加一个人物：大导演……让用户在这里完成个人成长……人生就像一出戏，每个人都是自己人生的导演。

## 第零条三步

### A. 顶栏

- **拒绝了什么类比：** 拒绝「顶栏是会话标题栏」。会话标题是**这次聊了啥**，是历史；顶栏要回答的是**现在在跟谁干**，是当下。
  两者混在一行，换工位时顶栏一动不动，用户就分不清"我点的是不是这个人"。
- **拆出的真：** 用户不建会话、不挑标题（工位即会话）。既然"谁"是唯一稳定的东西，
  顶栏就该钉**工位**，而不是钉一段会被下一句话改写的标题。
- **如何推出：** 顶栏 = `人物名 · 职责`；换工位立刻换。没有工位的会话（人物落地前的老会话、
  别的渠道进来的会话）退回**这一段对话的主题**——主题还是默认值时给产品名。
  **任何分支都不回「新对话」**：那三个字没有信息量，正是用户要拿掉的东西。
  规格 §2.2「我是谁 → 顶部一行」原本就写着这条，这次是把它从纸面落地。
- **不做**：顶栏不做人物选择器（侧栏「工位」就是选择器，再放一个是重复入口）。

### B. 大导演

- **拒绝了什么类比：** 拒绝「再开一个通用聊天」——那和搭子小乐没区别，只是多一个名字。
- **拆出的真：** 用户要的是**积累**：人生经验、工作经验、想法，要有地方**落下来并串起来**，
  而不是聊完就散。所以这个工位的产出是**你自己说过的话**，不是它替你写的稿。
- **如何推出：** 它属于「地基」而不是「专业分工」——人人都得有一份，所以 `builtin: true`（谁都有、不进授权码）。
  职责边界写成「你的经历/教训/念头」，明确不写稿、不做行业判断、不整理文件（那些各有工位），
  并且**不替你总结人生**：把你自己说过的话摆出来，结论你定。

## 改动

1. `crates/hermes-core/src/personas/da-dao-yan.md`（新）：`kind: dedicated`、`builtin: true`、`role: 攒你的经历和想法`。
2. `persona.rs`：`SOURCES` 8 → 9 并**重排成「自带四个在前、授权五个在后」**——顺序即侧栏分组顺序，
   排在一起后测试里的期望值也能写成「自带 + 授权」两段，不再交叉。注册表测试同步钉住大导演。
3. `ChatView.tsx`：顶栏由「会话标题 + 本地 · 工作搭子」改为「工位名 + 职责」；新增无工位时的主题兜底。
4. `i18n.ts`：删掉因此死亡的 `chat.defaultTitle`、`chat.headerSubShort`（中英各 1）。
5. GUI 两处测试里写死的自带名单 `[&str; 3]` → `[&str; 4]`，期望向量与文案同步。
6. `docs/spec/personas.md` 升 v1.2：§2.1 侧栏示意换成**真实名册**（旧的还画着早已删除的「小张/小小小王」），
   §2.2「我是谁」标为已落地，§2.4 类型表、§4.2「四个自带」、§5.2 记忆例外、§12 回滚口径一并纠正，
   新增 §2.6 说明顶栏口径。

## 验收

- `cargo test --workspace` 相关包全绿：`hermes-core` 70 passed / `hermes-gui` 31 + 2 + 2 passed。
- `npx tsc --noEmit` 干净；`npm run build` 通过。
- 真机目视（1200×737 实拍）：
  - 冷启动顶栏 = `搭子小乐 · 什么都接`；
  - 点侧栏「主播小雨」→ 顶栏立刻变 `主播小雨 · 出口播稿`，对话区回空首页；
  - 侧栏出现 `大导演 · 攒你的经历和想法`，排在自带那一组最后。
- i18n 一致性：中英各 **696** 键、逐 key 对称、无单边键；源码里 **544** 处 `t("…")` 全部可解析。
  与 HEAD（683）的差 = 本会话删 19 + 新增 32，两边都对得上（删的 19 条逐条都在台账里列过）。

## 已知取舍 / 待办

1. **大导演的记忆是全局的**（`builtin: true` → `memory_owner()` 返回 `None`）。
   与既有规格一致（自带角色是地基），但它攒的是很私人的东西，会在所有工位可见。
   要不要给自带角色也开「私有」口子，需要产品拍板——**这条我没有自己决定**。
2. §2.2 里「输入框提示语跟着人物变」**仍未做**；顶栏这条做完后，它是同一条体验里剩下的一半。
3. 会话标题现在 GUI 里**没有地方显示**（侧栏不列会话、顶栏改成工位）。
   标题仍在生成与落盘，别的入口（IM/server）还在用。要不要给它一个新入口，等你定。

---

## C. 侧栏小字改成「他是谁」 · 搭子小乐暂时下线 · 冷启动不自动开会话

**日期：** 2026-09-15 · **类型：** 产品（可见面）+ 工程 · **状态：** 工程通过 · 待目视

### 第零条三步

- **拒绝了什么类比：** 拒绝「换一批更漂亮的形容词」。文案改名如果只换词不换**这句话在回答什么问题**，
  下一版还会被推翻——所以先钉住问题本身：这行小字回答的是**他是谁**，不是**他干什么**。
  也拒绝「再开一个通用搭子」当兜底（见 §2「搭子小乐」）。
- **拆出的真：**
  1. 侧栏已经是「我的团队」这一层的入口（`你的AI团队`）。一行里，**名字**承担职能（情报王海燕、编辑雨天、
     主播小雨），**正文**承担边界（我干什么 / 我不干什么）。夹在中间的那行小字如果也写职能，
     就是用三种说法重复同一件事，而且**越写越像岗位说明书**。
  2. 用户上一版给的判词是「怪怪的」。逐条复盘后确认病因是**句式**：八条全是「X，不 Y」的对仗，
     读起来是挂在墙上的价值观；其中有几条还借了不搭的词（「不翻旧账」本是夫妻吵架的话）。
     第一版为了够"哲理"把语气端起来了，与 P0 的「搭子、直、暖、不端着」直接冲突。
  3. 一个工位如果**没有边界**（什么都接），它就没有越界可言，也就**拦不住**。
     兜底角色在定义上是「不拦」的，这在只有它一个人的时候是优点，在团队里是漏洞。
- **如何推出：**
  1. `role` 明文改成**一句自述**（他就是谁），措辞按用户挑定的 A 组；`block()` 里那句提示词
     从「工位是「X」」改成「你是 **X**——X」，并补一句「职责边界以下面『我干什么 / 我不干什么』为准」，
     免得正文和这行小字在模型眼里互相打架。
  2. 小乐**暂时**下线，机制是定义文件里一行 `retired: true` —— 不是删文件。理由是"暂时"必须真的可逆：
     工作区未提交，删文件 = 恢复要翻历史。
  3. 既然「用户不自己建会话、每个会话都绑一个人物」，那**冷启动就不该自动替他开一段对话**。
     没有会话时对话区给一句问候（沿用 `WelcomeScenes`），不给「正在打开对话…」那种空转文案。

### 方案（产品经理视角）

| | 改前 | 改后 |
|---|---|---|
| 侧栏每行 | `情报王海燕 · 采当天情报` | `情报王海燕 · 今天的事，今天给你` |
| 冷启动 | 自动开一段搭子小乐的会话 | **不开**；对话区一句问候，等用户点侧栏 |
| ⌘N | 新建搭子小乐会话 | **撤掉**（用户不建会话；还留着的三个字级入口只会制造没处回去的会话） |
| 侧栏工位数 | 9（4 自带） | 8（3 自带；小乐不出现） |

- **走完一遍：** 打开 App → 侧栏 8 行 → 点某个人物 → 顶栏立刻变 `人物名 · 那句自述` → 说话。
  没点之前，对话区是一句问候，不制造"已经开了一段"的错觉。
- **空 / 载 / 错态：** 没有会话 = 问候（不是转圈）；`personas.json` 缺失/坏 → 只剩三个自带；
  已下线的 id 不出现在任何列表、也发不出授权码。
- **不做什么：** 不改人物定义正文的边界；不动记忆归属；不给下线角色留"灰着但能点"的入口。

### 方案（架构师视角）

- **根因：** ① `role` 一个字段被两处消费（侧栏小字 + 提示词里的工位名），所以它的措辞直接被产品目标左右；
  ② 名册只有一个真源（`persona::SOURCES`），所以"暂时下线"必须也放在同一个真源里，不能在前端各加一个 filter；
  ③ 三个 `newSession("xiao-le")` 是硬编码默认人物，小乐一下线它们就指向一个不存在的 id。
- **默认路径：** `retired: true` → `persona::all()` 滤掉 → `ids()` / `builtins()` / GUI 的 `build_items`
  全部跟着消失（它们都从 `all()` 走）。**没有第二处 filter。**
- **边界：** 定义文件与 `SOURCES` 登记都保留（回滚 = 删一行）；`kind: fallback` 保留（口子不拆）；
  旧会话文件不动（人物落地前的会话仍然读得出来，只是顶栏退回主题）。

### 改动

1. `crates/hermes-core/src/personas/{li-xian,xiao-wen,da-dao-yan,wang-hai-yan,xiao-xie,xiao-jin,yu-tian,xiao-yu}.md`：
   `role` 换成 A 组一句自述。
2. `crates/hermes-core/src/personas/xiao-le.md`：加 `retired: true`（带两行注释说明为什么和怎么恢复）。
3. `crates/hermes-core/src/persona.rs`：新增 `retired` 字段；`all()` 过滤；`block()` 改成
   「你是 **X**——<自述>」并写明边界以正文为准；新增「下线角色不在名册、但定义还在」的测试；
   自带角色相关测试的样本从 `xiao-le` 换成在册的 `li-xian` / `xiao-wen`。
4. `crates/hermes-gui/src/commands/{personas,license}.rs`：`BUILTINS` `[&str; 4]` → `[&str; 3]`，期望向量与文案同步。
5. `crates/hermes-cli/src/commands/{personas,chat/mod}.rs`、`crates/hermes-channel/src/{context,companion_context}.rs`、
   `crates/hermes-memory/src/scoped.rs`、`crates/hermes-reflect/src/{prompt,micro_apply}.rs`：测试样本与
   过时注释（「小乐 / 李现」）同步纠正。
6. `crates/hermes-gui/ui/src/App.tsx`：删掉三处 `newSession("xiao-le")`（冷启动 / ⌘N / 引导结束）。
7. `crates/hermes-gui/ui/src/components/chat/ChatView.tsx`：header 提成一个节点两条路共用；
   无会话时渲染问候而不是「正在打开对话…」。
8. `crates/hermes-gui/ui/src/i18n.ts`：删掉因此死亡的 `chat.opening`（中英各 1）。
9. `crates/hermes-gui/ui/src/store/chatStore.ts`：`persona: null` 的注释改成「没有工位」。
10. `scripts/issue-license.py`：`--list-personas` 认 `retired`，下线的角色标「已下线」并**从可写名单里剔除**；
    真有人把下线 id 写进 `--personas`，会像未知 id 一样被挡下——否则客户端只会当成不认识的 id 报 `unknownPersonas`。
11. `docs/spec/personas.md` 升 v1.3：§2.1 侧栏示意换新小字并把小乐移出、每行说明改成「一句话身份」、
    §2.2 / §2.6 示例、§2.4 类型表（兜底今天为空）、§4.2「三个自带」、§5.2 记忆例外、§11 验收 1、§12 回滚、
    §9.3 作废「搭子有名字『小乐』」一条。

### 验收

- `cargo test --workspace` 全绿；`cargo clippy --workspace --all-targets -- -D warnings` 无告警。
- `npx tsc --noEmit` 干净；`npm run build` 通过。
- i18n 中英逐 key 对称（删掉 `chat.opening` 后仍对称）。
- 待真机目视：侧栏 8 行、无小乐；点人物顶栏跟着变；冷启动是一句问候。

### 已知取舍 / 待办

1. **小乐的历史会话在 GUI 里没有入口了**（侧栏不列会话）。文件一条没删，别的入口也还读得到。
   要不要给它一个入口，等你定。
2. `kind: fallback` 这类今天是空的。若以后要恢复兜底角色，`retired` 一行即可回来。
3. 上一轮「小王误指路」（模型把**技能名**当成工位）的修复方案 A/B/C **仍未拍板**：
   A 注入真实工位名册 / B 技能面按人物收窄 / C 技能名与人物名解耦。
   本轮把 `role` 改成自述后，A 里那份名册可以直接用侧栏的样子（`名字 · 那句自述`），和用户看到的一致。

---

## D. 指路名册 + 技能面收窄（修「技能名被当成工位」）

**日期：** 2026-09-15 · **类型：** 工程 + 产品（提示词可见面） · **状态：** 工程通过 · 待真机对话

### 第零条三步

- **拒绝了什么类比：** 拒绝「给技能改个名就完了」。改名只让这一批名字不再撞车，模型下次照样会从
  手边**唯一看得见的那批名字**里挑一个顶上——只要它手上没有真名单，这个病就会以别的名字复发。
- **拆出的真：**
  1. **指路需要名单，不是需要更聪明的模型。** 人物块原来只写「你是谁」，一个字都没写「本机还有谁」。
     模型被要求「指给对应工位」，却没有名单可查，于是把技能索引里唯一像人名的 `xiao-wang` 当成了工位
     （实测原话：「这个问题归小王那个工位」）。
  2. **技能名与工位名在同一个视野里，必须区分。** 用户自己装的技能就取着人名（`xiao-wang`、
     `xiao-wang-2`、`xiao-wen`），索引段不点明「这是技能不是工位」，模型没有理由不把它们当人。
  3. **广告 ≠ 能力。** 让某个工位「看不见」别人的技能，指的是不往它的提示词里塞那行简介；
     技能还在磁盘上、还是用户的东西，删改用都不受影响。
  4. **「没声明」和「声明为空」必须分开。** 规格原文是「未绑定的一律不广告」。若照字面实现，
     今天 9 个人物的 `skills` 全是空的 → 上线当天所有技能一起消失（含「小王干活」）。
     所以用 `Option<Vec<String>>`：没写 = 不收窄，写了空名单 = 只剩元技能。
- **如何推出：** ① 人物块里加名册（本机**开着的**其他工位，与侧栏逐字一致）+ 一句「不许编名字」；
  ② 技能索引段加一句「这些是技能，不是工位」；③ 把「技能面收窄」实现出来，但**默认不收窄**，
  只对确实该收窄的工位开；④ 判据各写一份 → 立刻会漂，所以两条路都从同一个函数取。

### 方案（产品经理视角）

- **场景：** 用户在工具人李现在那儿问「今天的财经情报」。改前：李现在把活推给「小王那个工位」——
  用户去找，找不到（没有这个人）。改后：他推给「情报王海燕」，用户按名字在侧栏就能看见。
- **看起来怎么样：** 提示词里他手边是 `- 情报王海燕 · 今天的事，今天给你`，跟侧栏那一行**逐字一样**。
  技能够不着「小王」这个名字（他的技能索引只剩 3 个元技能）。
- **空 / 载 / 错态：** 名册为空（只有自己一个工位）→ 写「这台机器上目前只有你这一个工位」，
  并仍然禁止编名字；授权文件读不出来 → 只剩自带，与侧栏一致。
- **不做：** 不给技能改名字（动用户的数据目录）；不做黑名单；不改既有 7 个工位今天的行为。

### 方案（架构师视角）

- **根因：** 人物块只有「我是谁」，没有「还有谁」；技能索引全量注入且没说清自己不是工位。
- **默认路径：**
  - `hermes_core::persona::open()` —— 本机开着的工位 = 自带 ∪（授权 ∩ 勾选），读口与侧栏同一个
    `personas.json`、同一个判据（`is_selectable` / `selected_ids_at` 从 GUI 移到 core，GUI 委托）。
  - `persona::block(p, others)` —— `others` 由 `ContextSources.roster` 传入，调用方用 `open()` 去掉自己。
  - `hermes_channel::persona_scope::visible_skills(persona, all)` —— 收窄的**唯一判据**；
    `others()` 也在同一模块。GUI 与 CLI/IM 都从这里取。
- **边界：** `roster` 是必填字段，漏接线**不会静默**——`persona` 有值而名册为空时，提示词会明说
  「只有你这一个工位」；`persona: None` 的入口（CLI 批处理 / IM / server）传 `&[]`，整段不发。

### 改动

1. `crates/hermes-core/src/persona.rs`：`Persona.skills` 改 `Option<Vec<String>>`；新增 `prefs_path` /
   `is_selectable` / `selected_ids_at` / `open_at` / `open`（读口与判据从 GUI 搬来，只此一份）；
   `block(p, others)` 加名册段与「不许编名字」；新增 `parse_for_test` 供别的 crate 造样本；4 条新测试。
2. `crates/hermes-core/src/personas/*.md`：去掉全部 `skills: []`（= 回到「不收窄」）；
   只给 `li-xian` / `da-dao-yan` 显式写 `skills: []`（= 只剩元技能），并注释说明为什么。
3. `crates/hermes-channel/src/persona_scope.rs`（新）：`others` + `visible_skills` + 索引那句
   `SKILLS_ARE_NOT_STATIONS`，附 4 条单元测试。
4. `crates/hermes-channel/src/{context,companion_context}.rs`：`ContextSources` 加 `roster` 字段；
   人物块传名册；技能索引用 `visible_skills` 过滤（含「整段注入」的 always_active 一并在内）；
   索引段加上那句「不是工位」。新增一条端到端测试，钉住「名册进提示词 / 看不见 `xiao-wang` /
   元技能还在 / 自己不在名单里」。
5. `crates/hermes-gui/src/commands/chat.rs`：`turn_sources` 透传 `open()`；测试助手带名册并断言。
6. `crates/hermes-gui/src/commands/personas.rs`：`prefs_path` / `is_selectable` / `read_enabled` 全部委托 core。
7. `crates/hermes-cli/src/commands/{chat/mod,chat/commands,agent}.rs`、`crates/hermes-channel/src/channel.rs`、
   `crates/hermes-server/src/routes/chat.rs`：补 `roster`（有工位的传 `open()`，其余 `&[]`）。
8. `docs/spec/personas.md` 升 v1.4：§2.3 名册落地、§6.1② 收窄落地并写明「没写 ≠ 写了空的」、§12 补两条风险。

### 验收

- `cargo test --workspace` 全绿；`cargo clippy --workspace --all-targets -- -D warnings` 无告警。
- **真机实测**（`hermes chat --persona <id>` + `/context` 打出完整系统提示词，用真实数据目录）：
  - 李现在的提示词里名册 = `资料员小文 / 大导演 / 情报王海燕 / 编辑雨天 / 主播小雨`（自己不在里面），
    并带「不许自己编一个工位名字」；
  - 他的技能段只剩 `find-skills` / `memory-palace` / `skill-creator`，全文 **`xiao-wang` 出现 0 次**；
  - 情报王海燕（没写 `skills`）13 个技能一个不少 —— 没开到的人行为逐字节不变。

### 已知取舍 / 待办

1. **绑定表还没定。** 今天只有李现在、大导演开了收窄。其余工位要收窄，得先决定
   「小王 / 小王2 / 小文 / web-data-collect / wechat-article …」各归谁——**这个我不替产品拍**。
2. **C（技能改名）没做**，按约定默认不动 `test/skills/`。今天靠「技能不是工位」那句 + 名册兜住。
3. `persona_scope::visible_skills` 收窄的是**广告**：被收窄的技能仍可被 `skill_read` 直接读到
   （工具没按人物裁）。规格 §6.1② 说工具面收窄属 `tools_deny`，v1 不做——现状一致，但别误以为
   李现在**读不到** `xiao-wang`。
4. 本轮跑 CLI `/context` 实测，在真实数据目录里留下了 3 个只有 meta 的空会话文件，并首次生成了
   `profile.md`（此前不存在；GUI 会读它）。已向用户报告，是否清理待其发话。
