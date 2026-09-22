# 变更记录：项目组 · 第 0 期 —— 人物立三、下三、指路改向

> **For agentic workers:** 本文件同时是**实施计划**。按 `- [ ]` 逐条执行；每步都能单独验证。

**Goal：** 让名册先变成《财富早知道》需要的样子——立三个新人物（产业专家扫地僧、主编吕老师、记者小宋），下线三个（林碳小谢、具身智能小金、搭子小乐），并把所有人的「指路」改到真实存在的工位上。

**Architecture：** 人物是**编译进二进制的产品设定**（`crates/hermes-core/src/personas/*.md` + `persona.rs` 的 `SOURCES` 数组）。下线用 `retired: true`（`all()` 过滤掉：侧栏没有、`ids()` 没有、授权码发不出）。**本期不碰 GUI 结构、不碰会话与记忆归属**——那是第 1 期起的事。

**Tech Stack：** Rust（`hermes-core` / `hermes-gui`）+ `serde_yaml` + Python 发码脚本。

**Spec：** [`../spec/projects.md`](../spec/projects.md)（设计源）· [`../spec/personas.md`](../spec/personas.md)（人物规格）

---

## Global Constraints（每条任务都隐含遵守）

1. 中文主词用**「对话」**，不用「聊天」；关系词用**「搭子」**，不用「搭档 / 工作伴侣」。
2. 人物定义**编译进二进制**（`include_str!`），**不落数据根**；**文件名 stem 必须等于 `id`**。
3. 授权码**只能写「在册且未下线」的 id**；下线人物写进码 = 客户端报「不认识的 id」。
4. **指路必须指向真实存在的工位**（`personas.md` §6）。
5. 根目录只允许 4 个 md；新文档必须先选种类 B–H 放进 `docs/` 对应目录。
6. 不新增依赖。
7. **不提交**——等用户明确点头再 `git commit`（本项目长期约定）。
8. 质量门槛：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 三者全绿才算完。

---

## 0-fp. 第一性原理（本期）

- **拒绝的类比：**
  1. **不是**「在提示词里多写三段人物介绍」——人物是产品设定，要在**名册、侧栏、授权码、指路**四处同源；只改提示词，等于造了一个发不出去的岗位。
  2. **不是**「直接删文件」——仓库里早就有可逆的下线开关（`retired: true`：名册里没有、码里发不出、定义留着），删文件是不可逆的。
  3. **不是**「改完跑一遍测试就完事」——**上一版留下的指路会指向已下线的人**（已实测：5 个定义的人民正文里点名了小谢/小金），这类错不会让测试变红，只会让产品在用户面前说错话。
- **拆出的真：**
  1. 人物的增删是**产品设定变更**，一次改动同时落四处：定义文件、`SOURCES` 顺序、授权码可写名单、所有人的指路文案。漏一处的表现分别是：解析不出、侧栏没有、码报错、指向不存在的人。
  2. 名册的**唯一真源**是编译期数组 `SOURCES`；`retired` 是唯一的过滤点（`all()`）。
  3. 本次**不需要数据迁移**——实测 `test/memories` 的 11 条记忆里只有 1 条带归属（王海燕），三个下线人物名下**零条**。
- **如何从真推出：**
  先立测试（名册该有谁、下线的人不许再被指路），再改定义与文案；用一条**新测试**守住「活的工位不许指向已下线的工位」，因为这个错误下一期还会再犯。

## 0. 用户价值

- **谁用：** 桌面 GUI 用户（默认路径）。
- **解决什么痛点：** 名册里还有两个已经不要的方向（林碳、具身智能），而《财富早知道》需要的三个岗位（产业判断、定选题、成稿）**一个都没有**——2026-09-15 那天用户问"谁来确定我们出什么"，答案当时是空的。
- **用完后用户多得到什么：** 侧栏里站着《财富早知道》真正需要的人；原来那三个不再出现；每个人越界时指的路**都找得到人**。
- **好用性自检：**
  - [x] 不需要额外运行时 / 数据库
  - [x] 不需要用户做任何操作（换版本即生效）
  - [x] 不增加确认或噪音
  - [x] 空 / 载 / 错态完整（老码会给一句"有 1 个角色这个版本不认识"，不静默）
  - [x] 高频路径步骤数不变

## 0b. 产品经理视角

- **场景：** 用户打开 App，看侧栏；点进某个工位说一句话。
- **怎么走完：** 什么都不用做——升级后侧栏直接是新的：**林碳小谢、具身智能小金消失**；**产业专家扫地僧、主编吕老师、记者小宋出现**（前提：授权码里有他们）。
- **看起来怎么样：** 与既有工位完全一致的一行（名字 + 那句自述）。**本期不加头像**——人物头像是既有遗留项（`records/20260915-persona-roster.md` §遗留：GUI 侧栏与头像未做），要连同侧栏视觉一起做，不塞进这一期。
- **空 / 载 / 错态：** 老授权码里含已下线的人 → 登录/授权处提示「有 1 个角色这个版本不认识」（既有机制，`personas.md` §4.3），**不静默**。
- **成功标准：** 三个新工位能被点开并正常对话；三个下线的人在任何地方都不再出现；全仓测试全绿。
- **明确不做什么：** 项目组会话、接力、教学回路、长会话（第 1–4 期）；头像。

## 0c. 架构师视角

- **根因层级：** 名册与人物身份是编译期设定，**加人/下线的成本集中在「四处同源」**；上一轮遗漏的正是第四处（指路文案）。
- **正确的长期默认路径：** 加人只改定义文件 + `SOURCES`（一处数组），其余（侧栏、授权码可写名单、名册块、记忆归属）全部由这一处派生。
- **边界：** 本期只动 `hermes-core` 的人物层、`hermes-gui` 的测试样例、定义文件与文档。**不动** `hermes-store` / 会话 / 记忆 / 授权验签逻辑。
- **如何防复发：** 新增测试「活的工位不许指向已下线的工位」；把「人下线了」这件事从"靠人记得"变成"测试守着"。
- **为何这不是补丁：** 它没有为某个具体角色写特判，而是把「下线」这条规则补全到"文案也要守规矩"的层面。

## 1. 范围

**做：** 三个定义文件 · 三个 `retired: true` · `SOURCES` 登记 · 指路改向（7 处）· 一处防回归测试 · 受影响的既有测试迁移 · `personas.md` 名册同步 · 全套测试码 · 目视。
**不做（留后续期）：** 项目组会话与侧栏分节（第 1 期）· 接力与「决定」载体（第 2 期）· 教学回路（第 3 期）· 长会话按天归档与按需读（第 4 期）· 头像。

---

## Task 1：立三个新人物（扫地僧 / 吕老师 / 小宋）

**Files：**
- Create: `crates/hermes-core/src/personas/sao-di-seng.md`
- Create: `crates/hermes-core/src/personas/lv-lao-shi.md`
- Create: `crates/hermes-core/src/personas/xiao-song.md`
- Modify: `crates/hermes-core/src/persona.rs`（`SOURCES` 数组；`the_roster_has_the_five_licensed_roles_plus_the_clerk` → 六个）
- Test: `crates/hermes-core/src/persona.rs`（同文件 `mod tests`）

**Interfaces：**
- Produces：三个稳定 id —— `sao-di-seng`、`lv-lao-shi`、`xiao-song`。后续任务、授权码、第 1 期的项目组名册都用它们。
- Consumes：`persona::get(id)` / `persona::ids()`（既有）。

- [x] **Step 1：改测试（先红）**——把 `crates/hermes-core/src/persona.rs` 里的 `the_roster_has_the_five_licensed_roles_plus_the_clerk` 整体替换为：

```rust
    #[test]
    fn the_roster_has_the_six_licensed_roles_plus_the_clerk() {
        for (id, name) in [
            ("wang-hai-yan", "情报王海燕"),
            ("sao-di-seng", "产业专家扫地僧"),
            ("lv-lao-shi", "主编吕老师"),
            ("xiao-song", "记者小宋"),
            ("yu-tian", "编辑雨天"),
            ("xiao-yu", "主播小雨"),
        ] {
            let p = get(id).unwrap_or_else(|| panic!("{id} 必须存在"));
            assert_eq!(p.name, name);
            assert_eq!(p.kind, PersonaKind::Dedicated);
            assert!(!p.builtin, "{id} 是授权角色，不进自带");
            assert_eq!(p.memory_owner(), Some(id), "{id} 的会话记忆归自己");
        }

        let wen = get("xiao-wen").expect("资料员小文必须存在");
        assert_eq!(wen.name, "资料员小文");
        assert_eq!(wen.kind, PersonaKind::Dedicated, "小文是专职，越界要拦");
        assert!(wen.builtin, "小文是必备角色：谁都有、不进授权码");
        assert_eq!(wen.memory_owner(), None, "自带的会话记忆算全局");
    }
```

- [x] **Step 2：跑测试确认失败**

Run: `cargo test -p hermes-core persona::tests::the_roster_has_the_six_licensed_roles_plus_the_clerk`
Expected: FAIL —— `panicked at 'sao-di-seng 必须存在'`

- [x] **Step 3：写 `sao-di-seng.md`**

```markdown
---
id: sao-di-seng
name: 产业专家扫地僧
role: 热闹我不看，我看门道
voice: 话少，只说分量；先给分量判断，再给依据；没有依据就不开口
kind: dedicated
builtin: false
empty_hint: 哪条消息要我称一下分量？给我原文或链接。
version: 0.1.0
---

## 我干什么
- 给产业称重：这条消息在产业里到底有多重——是噪音，还是真的变了什么。
- 说清分量从哪来：政策、供需、技术、资本，哪一头动了。
- 需要时给出行业的位置感：这一条在整个链条的哪一环。

## 我不干什么
- 不采料——那是海燕的活；不写成品稿——找小宋；不审稿——找雨天。
- 不给投资建议：不判断该买该卖、不预测点位。
- 不替别的行业下结论——不在我这一摊就说不在我这一摊。

## 我的口径
- 只讲已发生的事实与它的位置；讲不清就写「看不准」，不硬给。
- 数字先给口径（时间、来源、统计范围），再给数。
- 判断必须能指出来源；没依据的话一律标出来。

## 越界怎么拒绝并指路
- 「这不是我这摊——采当天情报找王海燕，成稿找小宋，审稿找雨天。」
- 指路必须指向真实存在的工位。
```

- [x] **Step 4：写 `lv-lao-shi.md`**

```markdown
---
id: lv-lao-shi
name: 主编吕老师
role: 出什么，我报给你点头
voice: 先给结论再给理由；不铺垫、不客套；被否了不辩解，改就是了
kind: dedicated
builtin: false
empty_hint: 今天这期想往哪个方向走？先给我料，我报题。
version: 0.1.0
---

## 我干什么
- 定选题：从当天的料里挑出该出的，砍掉不该出的。
- 定排序与配比：哪条打头、哪条压末尾、各占多少秒。
- 报题：把选择交成一张选题单，等用户点头再往下走。

## 我不干什么
- 不自己找料——料是海燕采的。
- 不写成品稿——成稿是小宋的。
- 不审自己报的题——审是雨天的。

## 我的口径
- 每条都要写清「为什么是它」；给不出理由的条目不上单。
- 用户的取舍标准优先于我的偏好；有分歧，先按用户定的走。
- 料不够就说不够，不为了凑数硬加条目。

## 越界怎么拒绝并指路
- 「这不是我这摊——采当天情报找王海燕，产业判断找扫地僧，成稿找小宋，审稿找雨天。」
- 指路必须指向真实存在的工位。
```

- [x] **Step 5：写 `xiao-song.md`**

```markdown
---
id: xiao-song
name: 记者小宋
role: 写出来要能直接念
voice: 短句、先结论；不堆形容词；数字按口播的写法给
kind: dedicated
builtin: false
empty_hint: 素材和选题单给我，我出稿。
version: 0.1.0
---

## 我干什么
- 按选题单成稿：标题、导语、正文、出处。
- 口播友好：句子短、能一口气念完；数字写成念得出来的形式。
- 把稿子落成文件，放进工作区给用户取。

## 我不干什么
- 不改选题——那是吕老师定的。
- 不审自己的稿——找雨天；不出口播终版——找小雨。
- 不采情报——找海燕。

## 我的口径
- 只写已发生的事实陈述；趋势判断必须标出依据和不确定度。
- 每个事实留一个可回溯的出处；核不到就写「这条我没核到」。
- 字数与配比照当版标准走——**项目组的标准优先于我个人的习惯**。

## 越界怎么拒绝并指路
- 「这不是我这摊——采当天情报找王海燕，产业判断找扫地僧，审稿找雨天，口播找小雨。」
- 指路必须指向真实存在的工位。
```

- [x] **Step 6：在 `SOURCES` 里登记三个（授权组顺序 = 流水线顺序）**——把授权那一段替换为：

```rust
    // 授权（进授权码的 `personas` 名单）——顺序即侧栏里这一组的顺序。
    (
        "personas/wang-hai-yan.md",
        include_str!("personas/wang-hai-yan.md"),
    ),
    (
        "personas/sao-di-seng.md",
        include_str!("personas/sao-di-seng.md"),
    ),
    (
        "personas/lv-lao-shi.md",
        include_str!("personas/lv-lao-shi.md"),
    ),
    (
        "personas/xiao-song.md",
        include_str!("personas/xiao-song.md"),
    ),
    ("personas/yu-tian.md", include_str!("personas/yu-tian.md")),
    ("personas/xiao-yu.md", include_str!("personas/xiao-yu.md")),
    // 已下线：定义**必须留在 SOURCES 里**，否则「暂时下线」就变成「再也回不来」
    // （`a_retired_persona_is_off_the_roster_but_keeps_its_definition` 盯着这一条）。
    ("personas/xiao-xie.md", include_str!("personas/xiao-xie.md")),
    ("personas/xiao-jin.md", include_str!("personas/xiao-jin.md")),
```

同时把数组长度 `const SOURCES: [(&str, &str); 9]` 改成 `; 12]`
（原 9 = 4 自带 + 5 授权；新 12 = 4 自带 + 6 授权 + 2 已下线）。

- [x] **Step 7：跑测试与静态检查**

Run: `cargo test -p hermes-core persona::` 
Expected: PASS（含 `every_definition_file_is_registered_in_sources`——目录与数组必须两边齐全）

Run: `cargo clippy -p hermes-core --all-targets -- -D warnings`
Expected: 无告警

- [x] **Step 8：收工（不提交）**——报告改动文件清单，等用户点头再定是否提交。

---

## Task 2：下线三个（小谢 / 小金 / 小乐）

**Files：**
- Modify: `crates/hermes-core/src/personas/xiao-xie.md`（frontmatter 加 `retired: true`）
- Modify: `crates/hermes-core/src/personas/xiao-jin.md`（同上）
- `crates/hermes-core/src/personas/xiao-le.md`：**已经是** `retired: true`，不动
- Modify: `crates/hermes-core/src/persona.rs`（4 个测试迁移 + 1 个新断言）
- Modify: `crates/hermes-gui/src/commands/chat.rs`（测试样例的 id）
- Modify: `crates/hermes-gui/src/commands/license.rs`（测试样例的 id + 1 个新测试）

**Interfaces：**
- Consumes：Task 1 的三个新 id。
- Produces：`xiao-xie` / `xiao-jin` / `xiao-le` 三个 id 从 `ids()` 消失；`get(这些 id)` 返回 `None`。

- [x] **Step 1：给两个定义加一行**——在 `xiao-xie.md` 与 `xiao-jin.md` 的 frontmatter 里，`builtin: false` 下面各加一行：

```yaml
retired: true        # 2026-09-16 下线：成稿岗交给记者小宋；产业判断交给扫地僧
```

- [x] **Step 2：跑全量测试，看清被打红的清单**

Run: `cargo test --workspace 2>&1 | rg "^test .* FAILED|panicked" | head -20`
Expected: 下面这些会红（一个都不许跳过）——
`persona::tests::the_roster_has_the_six_licensed_roles_plus_the_clerk`（若 Task 1 还没把旧的已下线 id 清掉）·
`persona::tests::dedicated_persona_owns_its_memories` ·
`persona::tests::session_persona_maps_to_memory_owner_only_for_known_dedicated_personas` ·
`persona::tests::the_persona_block_keeps_the_hat_and_the_relationship_apart` ·
`persona::tests::a_retired_persona_is_off_the_roster_but_keeps_its_definition`（若还没改成循环）·
`commands::chat::tests::a_bound_persona_reaches_the_turn_prompt` ·
`commands::license::tests::a_code_with_a_roster_turns_its_workstations_on` ·
`commands::license::tests::re_pasting_the_same_code_still_opens_its_workstations` ·
`commands::license::tests::a_second_code_swaps_the_roster`

- [x] **Step 3：迁移 `persona.rs` 的四个测试**

`dedicated_persona_owns_its_memories`：

```rust
    #[test]
    fn dedicated_persona_owns_its_memories() {
        let x = get("sao-di-seng").expect("产业专家扫地僧必须存在");
        assert!(!x.builtin);
        assert_eq!(x.memory_owner(), Some("sao-di-seng"));
    }
```

`session_persona_maps_to_memory_owner_only_for_known_dedicated_personas` 里把 `xiao-xie` 换成 `sao-di-seng`，并在 `nobody` 那条前面补一条：

```rust
        assert_eq!(
            memory_owner_for(Some("xiao-xie")),
            None,
            "已下线的人物不许再持有记忆（它的记忆按 personas.md §5.2 回落全局）"
        );
```

`the_persona_block_keeps_the_hat_and_the_relationship_apart`：

```rust
    #[test]
    fn the_persona_block_keeps_the_hat_and_the_relationship_apart() {
        let x = get("sao-di-seng").unwrap();
        let b = block(x, &[get("yu-tian").unwrap()]);
        assert!(b.contains("产业专家扫地僧") && b.contains("热闹我不看，我看门道"));
        assert!(b.contains("人设红线"), "必须写明口吻可变、关系基调不可变");
        assert!(
            b.contains("编辑雨天 · 错在哪，我指给你"),
            "名册要按侧栏那行的样子写（名字 · 那句自述），用户按这个找得到人：\n{b}"
        );
        assert!(!b.contains("产业专家扫地僧 · "), "自己不进「其他工位」名单");
    }
```

`a_retired_persona_is_off_the_roster_but_keeps_its_definition` 改成对三个 id 循环：

```rust
    #[test]
    fn a_retired_persona_is_off_the_roster_but_keeps_its_definition() {
        for id in ["xiao-le", "xiao-xie", "xiao-jin"] {
            assert!(get(id).is_none(), "下线的工位不许再出现在名册里：{id}");
            assert!(
                !ids().contains(&id),
                "ids() 是授权码与侧栏的真源，下线的 id 不能留在里面：{id}"
            );
            let raw = SOURCES
                .iter()
                .find(|(path, _)| *path == format!("personas/{id}.md").as_str())
                .unwrap_or_else(|| panic!("{id} 的定义文件仍在 SOURCES 里登记，否则回不来"))
                .1;
            assert!(
                raw.contains("retired: true"),
                "暂时下线靠 `retired: true`；文件不在 = 恢复要翻历史，不算「暂时」：{id}"
            );
        }
    }
```

- [x] **Step 4：迁移 `crates/hermes-gui/src/commands/chat.rs` 的测试**

`visible_memories_keeps_globals_and_own_only` 里三处 `"xiao-xie"` 换成 `"sao-di-seng"`，`"林碳报告只引 IEA"` 不变（文字是样例，无副作用）。

`system_prompt()` 辅助函数里：

```rust
        let roster = [
            hermes_core::persona::get("sao-di-seng").unwrap(),
            hermes_core::persona::get("yu-tian").unwrap(),
        ];
```

`a_bound_persona_reaches_the_turn_prompt` 里：`system_prompt(Some("xiao-xie"))` → `system_prompt(Some("sao-di-seng"))`；断言 `bound.contains("林碳小谢")` → `bound.contains("产业专家扫地僧")`。

- [x] **Step 5：迁移 `crates/hermes-gui/src/commands/license.rs` 的测试**

三处把样例 id 换成在册的两个人：

- `a_code_with_a_roster_turns_its_workstations_on`：`Some(&["wang-hai-yan", "xiao-xie"])` → `Some(&["wang-hai-yan", "sao-di-seng"])`；期望数组里 `"xiao-xie"` → `"sao-di-seng"`；落盘字符串断言同步改成 `"{\n  \"enabled\": [\n    \"wang-hai-yan\",\n    \"sao-di-seng\"\n  ]\n}"`。
- `re_pasting_the_same_code_still_opens_its_workstations`：同上（两处 `xiao-xie` → `sao-di-seng`）。
- `a_second_code_swaps_the_roster`：两张码分别改成 `["wang-hai-yan", "sao-di-seng"]` 与 `["lv-lao-shi", "wang-hai-yan"]`；断言 `enabled_personas` 与 `enabled_on_disk` 同步；那条"小谢不在新名单里，得下去"的说明改成「吕老师不在新名单里，得下去」。

新增一条（把「下线的人与陌生 id 同待遇」钉死）：

```rust
    #[test]
    fn a_retired_id_in_a_code_is_reported_unknown_and_never_shown() {
        let dir = tempfile::tempdir().unwrap();
        let (license, prefs) = (
            dir.path().join("license.json"),
            dir.path().join("personas.json"),
        );
        let res = apply_license_at(
            &license,
            &prefs,
            &token(365, Some(&["xiao-xie", "wang-hai-yan"])),
        )
        .unwrap();

        assert_eq!(res.status.unknown_personas, vec!["xiao-xie"]);
        assert_eq!(res.enabled_personas, vec!["wang-hai-yan"]);
        assert_eq!(
            enabled_on_disk(&license, &prefs),
            vec!["li-xian", "xiao-wen", "da-dao-yan", "wang-hai-yan"],
            "下线的人不许回到侧栏"
        );
    }
```

- [x] **Step 6：跑全量测试**

Run: `cargo test --workspace`
Expected: PASS（0 failed）

- [x] **Step 7：收工（不提交）**

---

## Task 3：指路改向 + 一条防回归测试

**Files：**
- Modify: `crates/hermes-core/src/persona.rs`（新增测试）
- Modify（正文，7 处）：`personas/da-dao-yan.md`、`personas/xiao-wen.md`、`personas/wang-hai-yan.md`（2 处）、`personas/xiao-yu.md`（2 处）、`personas/yu-tian.md`

**Interfaces：**
- Consumes：Task 1 的 `sao-di-seng` / `xiao-song`。
- Produces：不存在指向已下线工位的指路文案。

- [x] **Step 1：写失败测试（加在 `mod tests` 末尾）**

```rust
    /// 「指路必须指向真实存在的工位」（规格 §6）。上一版把这条留给了人记——
    /// 结果三个人下线之后，五个定义里还在指名道姓地把用户送去空地。
    /// 这条测试把它变成机器守：**活的工位不许提到已下线的名字**。
    #[test]
    fn no_live_station_points_at_a_retired_one() {
        let retired: Vec<Persona> = SOURCES
            .iter()
            .map(|&(path, raw)| parse(path, raw))
            .filter(|p| p.retired)
            .collect();
        assert!(!retired.is_empty(), "这条测试靠下线名单才有意义");

        for (path, raw) in SOURCES {
            let live = parse(path, raw);
            if live.retired {
                continue;
            }
            for gone in &retired {
                assert!(
                    !live.body.contains(&gone.name),
                    "{path} 的正文还在把用户指给已下线的人：{}。改为指向在册工位（成稿找小宋，产业判断找扫地僧）",
                    gone.name
                );
            }
        }
    }
```

- [x] **Step 2：跑测试确认失败**

Run: `cargo test -p hermes-core persona::tests::no_live_station_points_at_a_retired_one`
Expected: FAIL，指名 5 个文件里的 7 处（`wang-hai-yan.md` 2 处、`xiao-yu.md` 2 处、`da-dao-yan.md`、`xiao-wen.md`、`yu-tian.md` 各 1 处）

- [x] **Step 3：改这 7 处（逐条 before → after，其它一字不动）**

| 文件 | 改前 | 改后 |
|------|------|------|
| `wang-hai-yan.md` | `- 不做行业判断——林碳找小谢，具身智能找小金。` | `- 不做行业判断——产业分量找扫地僧。` |
| `wang-hai-yan.md` | `- 「这不是我这摊——我只采当天的事实，不做判断。要观点找小谢或小金。」` | `- 「这不是我这摊——我只采当天的事实，不做判断。要判断找扫地僧，要成稿找小宋。」` |
| `yu-tian.md` | `- 「这不是我这摊——我只审稿。要写稿找小谢或小金，要口播找小雨。」` | `- 「这不是我这摊——我只审稿。要写稿找小宋，要口播找小雨。」` |
| `xiao-yu.md` | `- 不审稿——找雨天；不做行业判断——找小谢或小金。` | `- 不审稿——找雨天；不做产业判断——找扫地僧。` |
| `xiao-yu.md` | `- 「这不是我这摊——我只把现成内容转成口播。要写稿找小谢或小金，要审稿找雨天。」` | `- 「这不是我这摊——我只把现成内容转成口播。要写稿找小宋，产业判断找扫地僧，要审稿找雨天。」` |
| `xiao-wen.md` | `- 「这不是我这摊——我只整理和入库。写稿找小谢或小金，审稿找雨天。」` | `- 「这不是我这摊——我只整理和入库。写稿找小宋，产业判断找扫地僧，审稿找雨天。」` |
| `da-dao-yan.md` | `- 「这段经历我愿意听，但稿子不是我写——写稿找小金或小谢，审稿找雨天。」` | `- 「这段经历我愿意听，但稿子不是我写——写稿找小宋，审稿找雨天。」` |

**已知例外（有意保留）**：`personas/xiao-le.md` 正文里还有「林碳找小谢」一句。它是**已下线定义**（历史快照，不参与运行时），改它没有收益；上面这条测试也只查**在册**工位，特意不查下线定义。

- [x] **Step 4：跑测试**

Run: `cargo test -p hermes-core persona::` 
Expected: PASS

- [x] **Step 5：收工（不提交）**

---

## Task 4：文档同步

**Files：**
- Modify: `docs/spec/personas.md`（§2.4 名册、§3.2 字段示例如需）
- Modify: `docs/records/README.md`（加本记录一行）
- Modify: `docs/spec/projects.md`（若实现与规格有出入，以实际为准回填）

- [x] **Step 1：`personas.md` §2.4 那张表的「谁」一栏**改为：

```markdown
| **专职** | 全部在册工位（情报王海燕、产业专家扫地僧、主编吕老师、记者小宋、编辑雨天、主播小雨、资料员小文、工具人李现在、大导演） | **硬拦** + 指路 + 可一键切 |
```

并在该节末尾补一句（下线三人）：

```markdown
**已下线（定义留在仓库，`retired: true`）**：林碳小谢、具身智能小金（2026-09-16）、搭子小乐（2026-09-15）。
下线含义：不进名册、不进授权码可写名单、名下记忆回落全局；**指路也不许再指向他们**（有测试守着）。
```

- [x] **Step 2：`docs/records/README.md` 加一行**（放在 20260916 那条上下文压缩记录的下面）：

```markdown
| [20260916-projects-group-phase0](./20260916-projects-group-phase0.md) | 项目组 第 0 期：立三（扫地僧/吕老师/小宋）· 下三（小谢/小金/小乐）· 指路改向 + 防回归测试 | 实施中 | 2026-09-16 |
```

- [x] **Step 3：核对 `docs/spec/projects.md`**：§3 名册、§3.1、§3.2、§7 与实现一致；有出入以实际为准改规格（规格服从实现，不反过来）。

- [x] **Step 4：本记录的 §3 测试 / §4 验收**在跑完之后回填（见文件末尾）。

---

## Task 5：发「财富早知道全套」测试码 + 目视验收

**Files：**
- 产出：一枚授权码（发给用户，不落仓库）

- [x] **Step 1：先看可写名单**

Run: `python3 scripts/issue-license.py --list-personas`
Expected: 六个「授权」= `sao-di-seng,lv-lao-shi,wang-hai-yan,xiao-song,yu-tian,xiao-yu`；三个「已下线」= `xiao-jin,xiao-le,xiao-xie`（**不许出现在可写行里**）

- [x] **Step 2：发码**

Run:

```bash
python3 scripts/issue-license.py --days 365 --plan year \
  --lic-id L3-caifu-quantao \
  --personas wang-hai-yan,sao-di-seng,lv-lao-shi,xiao-song,yu-tian,xiao-yu
```

Expected: 一枚 `LEBI1.…` 码。**先落地定义再发码**（规格 §7.1：否则码里全是不认识的 id）。

- [ ] **Step 3：粘码目视**

1. 打开 GUI（`scripts/run-gui.sh`）→ 设置 → 授权 → 粘贴码。
2. 侧栏「工位」应出现：三个自带（工具人李现在 / 资料员小文 / 大导演）+ 六个授权，顺序按 `SOURCES`。
3. **林碳小谢、具身智能小金不再出现。**
4. 分别点开扫地僧、吕老师、小宋说一句话：能正常对话，且越界时指的路**都能在侧栏找到人**。

- [ ] **Step 4：老码的空/错态**

用旧码 `L2-jin-haiyan`（含已下线的小金）粘贴一次。
Expected: 提示「有 1 个角色这个版本不认识」（不静默），其余人物正常开通。

---

## 2. 实施（偏差与实测）

计划是照着「改动只有四处」写的；实测比计划多出三件事，都不是可跳过的：

1. **被打红的测试比清单多 25 个。** 计划只点了 `persona.rs`（4）· `hermes-gui/chat.rs`（2）· `hermes-gui/license.rs`（3）。
   实际还有：`hermes-gui/commands/personas.rs`（3）· `hermes-channel/{context,companion_context}.rs`（6）·
   `hermes-cli/{personas,chat/mod}.rs`（7）· `hermes-core/license.rs`（3）· `hermes-reflect/prompt.rs`（1）。
   原因：这些用例都拿一个**在册 id** 当样例（`get("xiao-xie")` 或裸字符串 `"xiao-xie"`），下线就会红。
   已全部迁到 `sao-di-seng`（或顺序一致的一组在册 id）。**没有一个被跳过或删掉。**
   顺带：`hermes-memory` / `hermes-tools` 里的 `"xiao-xie"` 只是**记忆归属的任意字符串**，
   不查人物表、也不受下线影响，**没有动**（改了才是噪音）。
2. **计划里那条防回归测试是空转的——已改强。** 计划写成比对 `gone.name`（全名「林碳小谢」），
   而定义正文里写的是简称「小谢」：测试**全绿**，7 处坏指路一处都没抓到。
   现在给人物定义加了 `aka`（别人怎么称呼它），下线之后这份名单就是禁区；并且**下线时必须写 `aka`**，
   否则测试自己会红（"这条测试守的是全名，而指路写的全是简称"）。
   这是本期唯一的**字段新增**，理由写在 [`../spec/personas.md`](../spec/personas.md) §3.2。
3. **发码脚本漏了一层。** `scripts/issue-license.py` 自己解析 frontmatter、不剥行内注释：
   `retired: true  # 2026-09-16 下线…` 被读成 `False`，于是**已下线的两个人又被列进可写名单**
   （`--list-personas` 实测：小谢、小金仍标「授权」）。客户端收到只会报「不认识的 id」。
   已在根因处修（`strip_comment`），并把「已下线」从 `unknown` 里分出来单独报——
   原来的 `gone` 分支在默认路径上永远到不了（`unknown` 先 return 了）。
4. **顺手清掉的旧路**：`SOURCES` 里 `xiao-le` 原本挂在「自带」那一段的注释下（它早已 `retired`），
   已挪到「已下线」那一段；`personas.md` §2.3 的越界示例、§3.2 的字段样例也都换成了在册的人。

---

## 3. 测试（Test）

| # | 用例（用户语言） | 步骤 | 期望 | 结果 |
|---|------------------|------|------|------|
| 1 | 三个新工位能看见、能对话 | 发码 → 侧栏 | 扫地僧/吕老师/小宋在，能正常回话 | 待目视 |
| 2 | 不要的那三个不再出现 | 看侧栏 + 搜名册 | 小谢/小金/小乐都没有 | 待目视 |
| 3 | 越界指路都指得到人 | 对每个新工位说一件不属于它的活 | 指到的名字都在侧栏里 | 待目视 |
| 4 | 老码只报一声，不崩 | 粘 `L2-jin-haiyan` | 提示一个角色不认识，其余照常 | 待目视 |
| 5 | 回归不破 | `cargo test --workspace` | 全绿 | **通过**（0 failed） |
| 6 | 已下线的 id 发不出码 | `issue-license.py --personas xiao-xie` | 报「已下线」并退出 1 | **通过** |
| 7 | 活工位不许指向下线的人 | `cargo test -p hermes-core persona::tests::no_live` | 改文案前红、改完全绿 | **通过** |

- **自动化：** `cargo fmt --all -- --check` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test --workspace`
- **手工：** 上面第 1–4 条（GUI）
- **测试结论：** [ ] 全部通过

## 4. 验收（Accept）

| 门槛 | 是否达标 | 说明 |
|------|----------|------|
| 用户价值成立 | ☑ | 三个新岗位可被授权、可被点名；两个不要的方向不再出现在任何地方 |
| 开箱即用未破坏 | ☑ | 用户不需要做任何操作（换版本即生效）；老码只多一句提示 |
| 本地优先未破坏 | ☑ | 人物定义仍编译进二进制；未新增任何落盘文件 |
| 测试通过 | ☑ | `fmt` / `clippy -D warnings` / `test --workspace` 全绿（0 failed） |
| 记录完整 | ☑ | 本文件 + `docs/records/README.md` 索引 + `personas.md` v1.5 |
| 产品+架构两视角齐全 | ☑ | §0b / §0c |
| 非修修补补 | ☑ | 四处同源（定义 / SOURCES / 授权码可写名单 / 指路），并把「指路不许指向下线的人」变成测试 |
| 代码卫生 | ☑ | 迁移后的测试样例不留已下线 id；`xiao-le` 归位到「已下线」段；发码脚本的死分支清掉 |
| 操作与视觉 | ☐ | 侧栏目视（§3 第 1–4 条，待用户） |
| 第一性原理三步写全 | ☑ | 见 §0-fp |

- **验收人：** 待用户目视
- **结论：** ☐ 通过（工程全绿；侧栏与粘码待目视）
- **遗留项：** 第 1 期（项目组会话与侧栏分节）· 第 2 期（接力与「决定」载体）· 第 3 期（教学回路）· 第 4 期（长会话：按天归档 + 后端按需读）· 人物头像（并入侧栏视觉）
