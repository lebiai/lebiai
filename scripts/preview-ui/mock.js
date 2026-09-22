/**
 * 预览用的 Tauri IPC 桩（**只给开发看界面用，不是产品的一部分**）。
 *
 * 桌面 GUI 是 Tauri 壳：前端所有数据都走 `window.__TAURI_INTERNALS__.invoke`，
 * 所以把 `ui/dist` 直接用浏览器打开只会是个死壳。这个脚本在应用包之前注入，
 * 用**假数据**顶掉 IPC，让浏览器里能真实渲染组件、看布局与空/载/错态。
 *
 * 它不连后端、不写任何文件：数字与内容都是这里写死的，别拿它当功能验证。
 * **每条桩的形状必须照 Rust 侧的真身写**——形状写错，预览里会看到的是
 * 渲染崩溃，而不是界面。
 */
(function () {
  // 引导页只看本机这个标记（`utils/onboarding.ts`），预览里直接标成「已走过」。
  try {
    localStorage.setItem("hermes.onboarding.v1.done", "1");
  } catch (_) {
    /* ignore */
  }

  const text = (t) => [{ type: "text", text: t }];

  // ── 窗口里的那几条（最近两天）────────────────────────────────
  const windowed = [
    { role: "user", content: text("今天开工") },
    {
      role: "assistant",
      content: [
        { type: "thinking", thinking: "先看料到了几栏，再核数字。" },
        { type: "text", text: "开工。料到了三栏，我先去核数字。" },
        {
          type: "toolUse",
          id: "t-recall-1",
          name: "conversation_recall",
          input: { query: "公告 条数" },
        },
        {
          type: "toolResult",
          toolUseId: "t-recall-1",
          content: "9 月 16 日：公告别给两条，一条太薄。",
          isError: false,
        },
        { type: "text", text: "翻到 9 月 16 日：公告只排一条。我按这个来。" },
      ],
    },
    { role: "user", content: text("选题单先出，别急着写稿") },
    {
      role: "assistant",
      content: text("收到。选题单在这儿，你点头我再往下走。"),
    },
  ];

  // 点开「更早的那天」时回来的那一段（**只读**）。
  const olderDay = [
    { role: "user", content: text("今天的料齐了，先出一版看看") },
    { role: "assistant", content: text("收到。我按老规矩先过一遍三道门，再报题单。") },
    { role: "user", content: text("公告别给两条，一条太薄") },
    { role: "assistant", content: text("记下了——公告吃 1/3，排一条。") },
  ];

  const days = [
    { day: "2026-09-17", label: "9 月 17 日", turns: 41, messages: 195 },
    { day: "2026-09-16", label: "9 月 16 日", turns: 33, messages: 120 },
    { day: null, label: "更早", turns: 6, messages: 12 },
  ];

  const session = {
    id: "preview",
    title: "你能做什么",
    createdAt: new Date(Date.now() - 3 * 86400000).toISOString(),
    updatedAt: new Date().toISOString(),
    path: "/preview/session.jsonl",
    persona: "wang-hai-yan",
    team: null,
  };

  // 形状照 `commands/personas.rs::PersonaItem`：**一个数组**（不是 {personas,builtins}）。
  const personas = [
    { id: "li-xian", name: "工具人李现在", role: "你卡在哪，我带你走一遍", builtin: true, licensed: true, enabled: true },
    { id: "xiao-wen", name: "资料员小文", role: "东西给我，先存住", builtin: true, licensed: true, enabled: true },
    { id: "da-dao-yan", name: "大导演", role: "你走过的路，我帮你记", builtin: true, licensed: true, enabled: true },
    { id: "wang-hai-yan", name: "情报王海燕", role: "今天的事，今天给你", builtin: false, licensed: true, enabled: true },
    { id: "lv-lao-shi", name: "主编吕老师", role: "播哪几条，我定调", builtin: false, licensed: true, enabled: true },
    { id: "xiao-song", name: "记者小宋", role: "料给我，我成稿", builtin: false, licensed: true, enabled: true },
    { id: "yu-tian", name: "编辑雨天", role: "错在哪，我指给你", builtin: false, licensed: true, enabled: true },
    { id: "zhu-bo-xiao-yu", name: "主播小雨", role: "稿子给我，我说给你听", builtin: false, licensed: false, enabled: false },
  ];

  // 形状照 `commands/teams.rs::TeamItem`：也是**一个数组**。
  const teams = [
    {
      id: "caifu-zaozhidao",
      name: "财富早知道",
      role: "一天一期，先报题再出稿",
      members: [
        { id: "wang-hai-yan", name: "情报王海燕", role: "采料", present: true },
        { id: "lv-lao-shi", name: "主编吕老师", role: "选题定调", present: true },
        { id: "xiao-song", name: "记者小宋", role: "梳理成稿", present: false },
      ],
      missing: 1,
      canRun: true,
      speakerId: "wang-hai-yan",
      hint: "缺 1 人 · 需要「记者小宋」",
    },
  ];

  // 形状照 `store/zaibanStore.ts::ZaibanList`（`items` 挂在对象上）。
  const zaiban = {
    items: [],
    owedCount: 0,
    overdueCount: 0,
    crowded: false,
    recentDone: [],
    mergeHint: null,
  };

  // 形状照 `commands/review.rs::ReviewPrefsView`。
  const reviewPrefs = {
    weekday: 5,
    defaultSpan: "week",
    inviteDue: false,
    reviewed: false,
    from: "2026-09-14",
    to: "2026-09-18",
  };

  // 形状照 `commands/memory.rs::MemoryItem`。「它记得的」那一页要的就是它。
  const memories = [
    {
      id: "m1",
      body: "选题单先出，我点头你才写稿——顺序不许反。",
      scope: "project",
      pinned: false,
      confidence: "high",
      tags: ["流程"],
      zone: "semantic",
      createdAt: new Date(Date.now() - 3600000).toISOString(),
      source: "对话",
      owner: "caifu-zaozhidao",
      ownerName: "财富早知道",
      because: "你连着两期都先要题单，再让动手。",
      supersedes: [],
      version: 2,
    },
    {
      id: "m2",
      body: "公告只排一条：两条太薄，一条又显得轻——一条 + 一句解读。",
      scope: "project",
      pinned: true,
      confidence: "high",
      tags: ["采料"],
      zone: "semantic",
      createdAt: new Date(Date.now() - 6 * 3600000).toISOString(),
      source: "对话",
      owner: "caifu-zaozhidao",
      ownerName: "财富早知道",
      because: "你说过「公告别给两条，一条太薄」。",
      supersedes: ["m0"],
      version: 2,
    },
    {
      id: "m3",
      body: "核数字要给出处，不许只给结论。",
      scope: "persona",
      pinned: false,
      confidence: "medium",
      tags: ["纪律"],
      zone: "semantic",
      createdAt: new Date(Date.now() - 30 * 3600000).toISOString(),
      source: "对话",
      owner: "wang-hai-yan",
      ownerName: "情报王海燕",
      because: "你退回了我一版没写出处的稿。",
      supersedes: [],
      version: 1,
    },
    {
      id: "m4",
      body: "每周一早上先对齐一次本周要推的事。",
      scope: "global",
      pinned: false,
      confidence: "medium",
      tags: ["节奏"],
      zone: "semantic",
      createdAt: new Date(Date.now() - 72 * 3600000).toISOString(),
      source: "对话",
      owner: null,
      ownerName: "全局",
      because: null,
      supersedes: [],
      version: 1,
    },
  ];

  // 形状照 `commands/inbox.rs::InboxItemView`（含本期新加的 ownerId / ownerName）。
  const pending = [
    {
      id: "p1",
      createdAt: new Date(Date.now() - 600000).toISOString(),
      source: "session_end",
      kind: "memory",
      title: "往期公告都吃 1/3",
      body: "往期公告都吃 1/3。",
      zone: "standards",
      tags: ["采料"],
      confidence: "High",
      rationale: "你连着两期都这么裁公告。",
      skillName: null,
      skillDescription: null,
      skillTriggers: null,
      ownerId: "caifu-zaozhidao",
      ownerName: "财富早知道",
    },
    {
      id: "p2",
      createdAt: new Date(Date.now() - 900000).toISOString(),
      source: "micro",
      kind: "memory",
      title: "周末不看盘",
      body: "周末不看盘，周一一早再对数字。",
      zone: "preferences",
      tags: [],
      confidence: "Medium",
      rationale: "你周日把行情问题岔开了。",
      skillName: null,
      skillDescription: null,
      skillTriggers: null,
    },
  ];

  // 形状照 `commands/source.rs::SourceListItem` / `main` 里的 `list_outputs`。
  const sources = [
    {
      id: "s1",
      title: "2026 年 9 月宏观数据一览",
      originalName: "宏观数据.xlsx",
      ext: "xlsx",
      createdAt: new Date(Date.now() - 5 * 3600000).toISOString(),
      readable: true,
      chars: 18240,
    },
    {
      id: "s2",
      title: "上一期公告原文",
      originalName: "公告.pdf",
      ext: "pdf",
      createdAt: new Date(Date.now() - 29 * 3600000).toISOString(),
      readable: true,
      chars: 6410,
    },
  ];

  const outputs = [
    {
      day: new Intl.DateTimeFormat("en-CA").format(new Date(Date.now() - 86400000)),
      items: [
        {
          relPath: "out/财富早知道-0918.md",
          name: "财富早知道-0918.md",
          ext: "md",
          bytes: 8420,
          modified: new Date(Date.now() - 30 * 3600000).toISOString(),
        },
      ],
    },
  ];

  const handlers = {
    list_sessions: () => [session],
    list_memories: () => memories,
    list_sources: () => sources,
    list_outputs: () => outputs,
    // 点一个还没有会话的工位/项目组时走这条：**只回一张牌，不落盘**。
    new_session: (a) => ({
      id: `preview-${a?.personaId ?? a?.teamId ?? "free"}`,
      title: "",
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      path: `/preview/${a?.personaId ?? a?.teamId ?? "free"}.jsonl`,
      persona: a?.personaId ?? null,
      team: a?.teamId ?? null,
      readOnly: false,
    }),
    list_personas: () => personas,
    list_teams: () => teams,
    count_pending_review: () => pending.length,
    list_pending_review: () => pending,
    list_commitments: () => zaiban,
    get_review_prefs: () => reviewPrefs,
    get_license_status: () => ({
      status: "active",
      daysLeft: 30,
      trial: false,
      personas: ["wang-hai-yan", "yu-tian"],
    }),
    mark_license_nudge_seen: () => null,
    drain_pending_leave: () => null,
    onboarding_seed_get: () => ({ displayName: null }),
    // 形状照 `commands/config.rs::ConfigView`——设置页整页都读它。
    get_config: () => ({
      defaultProvider: "deepseek",
      model: "deepseek-chat",
      maxTokens: 8192,
      baseUrl: "https://api.deepseek.com",
      providers: [
        {
          key: "deepseek",
          model: "deepseek-chat",
          maxTokens: 8192,
          baseUrl: "https://api.deepseek.com",
          apiKeyMasked: "sk-****preview",
          hasApiKey: true,
        },
        {
          key: "anthropic",
          model: "claude-sonnet-4",
          maxTokens: 8192,
          baseUrl: "https://api.anthropic.com",
          apiKeyMasked: "",
          hasApiKey: false,
        },
      ],
      reflectMinTurns: 8,
      reflectAutoAcceptMemories: true,
      contextModelLimit: 128000,
      permissionsAllow: [],
      permissionsDeny: [],
      workspaceRoot: "/preview/workspace",
      dataDir: "/preview",
      uiLanguage: "zh-CN",
      uiTheme: "light",
      persistThinking: false,
      hasApiKey: true,
    }),
    session_needs_distill: () => false,
    // 形状照 `types::EpisodeView`——「这一期」那条 + 名册要用它。
    list_episode: () => ({
      day: new Date().toISOString().slice(0, 10),
      holderId: "yu-tian",
      holderName: "编辑雨天",
      handoffs: [
        {
          fromName: "情报王海燕",
          toName: "编辑雨天",
          toId: "yu-tian",
          at: new Date(Date.now() - 5400000).toISOString(),
        },
      ],
      items: [
        {
          relPath: "out/topics.md",
          name: "选题单.md",
          ext: "md",
          modified: new Date(Date.now() - 7200000).toISOString(),
        },
      ],
      pendingDecision: true,
    }),
    data_dir_pick: () => null,
    license_dev_has_backup: () => false,
    license_dev_tools_enabled: () => false,
    load_session: () => ({
      ...session,
      messages: windowed,
      inputTokens: 1200,
      outputTokens: 800,
      readOnly: false,
      // 折起来的那几段在窗口之前：**载荷要带 baseOffset**，编辑重发才不会错位。
      baseOffset: 327,
      // `?days=1` 只留一天，用来走「只有一天时汇总行就是那一天」那个分支。
      days: new URLSearchParams(location.search).get("days") === "1" ? days.slice(0, 1) : days,
    }),
    // 故意慢一点：好让「正在翻旧账…」那一态**看得见**（真机上它一闪而过）。
    load_session_day: async () => {
      await new Promise((r) => setTimeout(r, 900));
      return olderDay;
    },
  };

  let callbackId = 0;
  const callbacks = new Map();

  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
    },
    transformCallback: (cb) => {
      const id = ++callbackId;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id) => callbacks.delete(id),
    convertFileSrc: (p) => p,
    invoke: async (cmd, args) => {
      if (cmd.startsWith("plugin:")) return null; // 事件 / 窗口插件：预览里静默
      const h = handlers[cmd];
      if (!h) {
        console.warn("[preview] 没有桩的命令:", cmd, args);
        return null;
      }
      return h(args);
    },
  };
})();
