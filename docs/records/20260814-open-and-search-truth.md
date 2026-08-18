# 变更记录：打开走引擎 · 搜索垃圾不当成功 · Word/Excel 真文件

| 字段 | 内容 |
|------|------|
| **编号** | `20260814-open-and-search-truth` |
| **日期** | 2026-08-14 |
| **状态** | 工程通过 · 待产品确认 |
| **种类** | F 台账 · 工程 |
| **关联** | 桌面 `open` 报错 -54 · 搜索 CSS 当结果 · RTF 当 .doc |

---

## 0-fp. 第一性原理

- **拒绝的类比：** 不是再让模型猜 Word/WPS/Pages；不是关沙盒；不是再堆一个搜索引擎；不是把「打开」做成聊天里的按钮秀。
- **拆出的真：** 用户要打开的是他说的文件或网页；系统默认应用会打开。搜索必须交可点可读的条目，样式表/脚本不是成功。Word/Excel 是交付物，必须是 Office 能打开的包，写到他说的位置，再用打开交给他。IM 没有确认 UI，不能打开本机文件。
- **如何推出：** `open` 工具在沙盒外调系统打开；`bash open` 改走同一条路径；搜索过滤后为空则失败并降级；`write` 遇到 `.docx/.xlsx` 在引擎里打成 OOXML，不依赖本机 Python。

## 0. 用户价值

- **谁用：** 桌面对话用户（查资料、写文档、打开结果）。
- **痛点：** 说「打开」却报 sandbox -54；搜到 CSS 当热点；桌面上的「Word」其实是假文件。
- **用完后：** 打开就是打开；搜到的是页面；Word/Excel 双击能开。
- **好用性自检：** 无新运行时；一步打开；IM 不暴露打开。

## 0b. 产品经理

- **场景：** 查完资料 → 写成 Word/Excel → 打开看；或直接打开一个视频/网页。
- **怎么走完：** 对话里说打开/写出；过程条显示「打开」而不是工具名；失败一句人话。
- **看起来：** 与写文件、查资料同级的人话过程，不是调试台。
- **成功标准：** 桌面文件是真 OOXML；`open` 不走 seatbelt；IM 没有 open。
- **不做：** 搜完自动出 Word；关掉沙盒；给 IM 打开本机文件。

## 0c. 架构师

- **根因：** 没有打开能力，只有沙盒 bash；搜索解析把资源 URL 当命中；write 只写 UTF-8。
- **默认路径：** 引擎工具 `open`；write 打包 Office；搜索可用性在解析层。
- **边界：** Dialogue 有 open；IM 白名单不含 open/write/bash。
- **安全：** 打开限工作区或用户家目录，拒 `/etc` 与密钥路径；URL 只允许 http(s) 公网。
- **防复发：** bash 里的 `open`/`xdg-open` 转发到同一工具；单测锁住垃圾 URL 与假 docx。
- **为何不是补丁：** 补的是能力与契约，不是再教模型绕沙盒。

## 1. 方案

做：open 工具、bash 转发、搜索可用性过滤、write 打包 docx/xlsx、回归测试。  
不做：自动搜完出 Word；关沙盒；IM 打开本机。

## 2. 实施

- `hermes-tools`: `open.rs` 接入 `handles` / `list_tools` / `call`
- `safety::resolve_for_open`：工作区或家目录已存在文件；拒系统路径与密钥路径
- `bash`：识别 `open`/`xdg-open`/`start` 并转发；osascript 猜 Word 直接失败
- `office_export`：最小 OOXML zip；`.doc`/`.xls` 升为 `.docx`/`.xlsx`
- `web_search`：CSS/JS/tracker 不当成功；缓存 key `v2`
- Dialogue 提示用 `open` 工具；过程文案「打开」；Flutter 同词
- IM 白名单测试：不得含 open

## 3. 测试

- `hermes-tools` 80 单测全绿；`hermes-turn` 25 全绿；`hermes-channel` 含 IM 不得暴露 `open`
- 桌面实写：`~/Desktop/乐彼AI测试-简报-*.docx` / `乐彼AI测试-表格-*.xlsx`，`unzip -t` 通过，正文含中文
- `open` 实开桌面 Word 与 `https://example.com/` 成功（不再走 seatbelt / -54）
- `hermes ask` 推理：`3+5` → `8`；带 `web_search` 查 example.com，模型按工具结果作答
- 搜索过滤：CSS / `r.bing.com` / 空标题不当成功
- 回朔另修：macOS `/var` vs `/private/var` 导致往还不存在的 `outputs/` 写入被误判逃出工作区

## 4. 验收

工程通过。待产品在新 GUI 里走一遍：打开文件/网页、写 Word/Excel、查资料。
