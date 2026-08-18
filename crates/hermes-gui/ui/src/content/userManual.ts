import type { Language } from "../i18n";

export type ManualScene = {
  title: string;
  situation: string;
  say: string;
  then: string;
};

export type ManualSection = {
  id: string;
  title: string;
  lead?: string;
  steps?: string[];
  paragraphs?: string[];
  scenes?: ManualScene[];
  tips?: string[];
};

export type ManualDoc = {
  kicker: string;
  title: string;
  intro: string;
  sections: ManualSection[];
};

const ZH: ManualDoc = {
  kicker: "怎么用",
  title: "乐彼AI 使用手册",
  intro:
    "乐彼AI 是你的工作搭子。打开就是对话。把事说清楚，它陪你一起做完；文件丢进来，下次问还能用上。越用越懂你怎么干活。它听你的，不替你做主。",
  sections: [
    {
      id: "first",
      title: "第一次来",
      lead: "三步就能开口。",
      steps: [
        "打开乐彼AI。左边是对话列表，中间是对话。",
        "点「设置 → 对话」，选一个服务商，把密钥贴进去，保存。",
        "回到对话，直接说你要做什么。不用学口令，不用先建任何东西。",
      ],
      tips: [
        "试用期内就可以开始用。到期后，去「设置 → 更多」贴上授权码。",
        "名字、深浅色、语言在「设置 → 外观」，改完马上生效。",
      ],
    },
    {
      id: "talk",
      title: "日常怎么对话",
      paragraphs: [
        "把它当成坐在旁边一起干活的人。你说要写什么、改什么、盯什么，它就动手。",
        "做完它会把结果给你。有时会多说一两句「还可以这样」——主结果在前面，建议在后面。你定。",
        "上次做过类似的事，它会接上，而不是从头讲一遍通用课。没做过的，它不会装记得。",
        "说错了就改口。说「不是这样」「换一种」「定稿别改了」，它会跟上。",
      ],
    },
    {
      id: "files",
      title: "把文件交给它",
      lead: "Word、PDF 丢进对话，当次就能对着问、对着改，也会自动留下。下次直接问就行，不必再找文件。",
      paragraphs: [
        "第一次留下时，会提一句「下次直接问就行」，旁边可以撤销。之后再丢，就不再打断你。",
        "答的时候如果用上了你的文件，会说一句「按你那份《××》」。问无关的事，它不会提这些材料。",
        "不想留了：左边「它记得的 → 我的材料」，点丢掉。你电脑上原来的那份不会被动。",
        "如果一份文件里读不出字（比如扫描件），它会老实告诉你，你仍可以打开原件对着问。",
        "表格、纯文本拖进来当次能用。要下次还用，说「留下」，或到「我的材料」加入。Word / PDF 不用再说留下。",
        "一次可以丢进一夹文件。能读的留下，读不了的会告诉你。",
      ],
      tips: [
        "同一份再丢一次，不会变成两行。很像同一份的新版，会按新的收下，旧的还能打开。",
        "答没用上材料时，再说一次文件名，或把文件再拖一遍——当次一定能用。",
        "用错份了，说「不是这份」。",
        "问「手头有哪些」，会列出留下的名字。",
        "说「记住这个标准」，会放到「它记得的」等你点头，不会偷偷写成习惯。",
      ],
    },
    {
      id: "scenes",
      title: "几个常用场景",
      lead: "不用记步骤。照着说就行。",
      scenes: [
        {
          title: "按口径写一条对外的话",
          situation: "你有一份对外口径，要发朋友圈、通知或短稿。",
          say: "把口径文档丢进来，再说：按我们口径写一条朋友圈，语气稳一点。",
          then: "它按你那份写。下次再写同类的，直接说「再来一条」，不必重新找文件。",
        },
        {
          title: "对着合同问条款",
          situation: "合同在手里，你想先搞清违约金、逾期、付款。",
          say: "把合同丢进来，问：违约金怎么写的？下一句可以直接问：那逾期呢？",
          then: "它还在同一份里找，你不用再报文件名。两份写法不一样时，它应摊开两边，由你定。",
        },
        {
          title: "改一版旧稿",
          situation: "已经有一版，这次只要改口径或缩短。",
          say: "把旧稿丢进来：按这个改一版，开头先说结论，比现在短三分之一。",
          then: "主稿先给你。后面最多两三条可改之处，不抢正文。",
        },
        {
          title: "对照两份材料",
          situation: "新旧制度、两版报价、两份纪要，想看差在哪。",
          say: "两份都丢进来：这两份哪里不一样？我该按哪份对外说？",
          then: "它分开讲，不私自揉成一份假制度。你点头后，它按你定的那份写对外的话。",
        },
        {
          title: "记下还没做完的事",
          situation: "周五要交方案，怕聊着聊着忘了。",
          say: "帮我记下周五交方案。",
          then: "右上角「在办」会出现这一条。到期前后它会提一句。做完勾掉就好。",
        },
        {
          title: "让它越来越像你的手感",
          situation: "你改过它的标题、口气、结构，希望下次不用再说一遍。",
          say: "直接改它写的，或说：以后标题先写冲突，不要空话。",
          then: "离开这段对话后，可能会整理出几条请你点头。不点头，不会记住。点过头，下次同类事会接上。",
        },
      ],
    },
    {
      id: "knows",
      title: "「它记得的」里有什么",
      paragraphs: [
        "左边「它记得的」是后场，不是每天都要打开的地方。",
        "「它懂你」：你点头留下的习惯和标准，比如「先结论」「不要空话」。",
        "「你的做法」：可再用的一套做法。同类事第二次会更顺。",
        "「我的材料」：你留下的原文。打开原件、丢掉，都在这里。",
        "离开一段对话后，它可能整理出几条候选。每条都要你点头才会留下。关掉就是先不留。",
      ],
    },
    {
      id: "zaiban",
      title: "在办",
      paragraphs: [
        "右上角是「在办」：还欠的交差，不是日记，也不是材料库。",
        "对话里说「记下……」就会出现。可以改期、勾完、丢掉。",
        "到点了它会提一句。不想被问，也可以在回顾里告诉它少问。",
      ],
    },
    {
      id: "wechat",
      title: "在微信里找它",
      paragraphs: [
        "「设置 → 连接」，用手机微信扫码。连上之后，在你自己的微信里就能找乐彼AI。",
        "这是你自己的微信。别人默认找它，它不会答，也不会翻你留下的材料。",
        "微信里看到的是对话记录。要改设置、丢掉材料、看在办，还是回到这台电脑上的乐彼AI。",
      ],
    },
    {
      id: "settings",
      title: "设置里还有什么",
      paragraphs: [
        "概览：现在能不能对话、授权还剩多久、有没有新版本。",
        "对话：服务商和密钥。多数情况只贴密钥，下面那些一般不用动。",
        "外观：怎么叫你、中英文、深浅色。",
        "连接：微信。",
        "更多：授权码、数据放在哪、以及很少需要动的选项。",
      ],
    },
    {
      id: "stuck",
      title: "卡住时",
      tips: [
        "答得不像你的材料：再说一次文件名，或把文件再拖进对话。",
        "用错了那一份：说「不是这份」，或到「我的材料」丢掉用错的。",
        "文件没留下成：再拖一次。还不行，打开原件，对着屏幕问。",
        "不想它再按某条习惯来：到「它记得的」里删掉那一条。",
        "密钥无效或欠费：去「设置 → 对话」换密钥，或到服务商那里处理账户。",
        "试用到了：去「设置 → 更多」贴授权码。材料、记忆都还在。",
      ],
    },
  ],
};

const EN: ManualDoc = {
  kicker: "How to use",
  title: "lebi-AI handbook",
  intro:
    "lebi-AI is your work companion. You open it and talk. Say the work; it works with you. Drop a file in and it can use it next time. It gets closer to your hand the more you use it. You stay in charge.",
  sections: [
    {
      id: "first",
      title: "First time",
      lead: "Three steps, then you can talk.",
      steps: [
        "Open lebi-AI. Conversations on the left, dialogue in the middle.",
        "Go to Settings → Dialogue. Pick a provider, paste your key, save.",
        "Go back to dialogue and say what you need. No commands to learn. Nothing to set up first.",
      ],
      tips: [
        "The trial is enough to start. When it ends, paste your license under Settings → More.",
        "Your name, theme, and language live under Settings → Appearance. Changes apply at once.",
      ],
    },
    {
      id: "talk",
      title: "Everyday dialogue",
      paragraphs: [
        "Treat it like someone sitting next to you at work. Say what to write, change, or keep an eye on. It does the work.",
        "You get the result first. Sometimes one or two notes follow — “this could be sharper.” You decide.",
        "If you have done something similar, it picks up from there. If it has no reason to remember, it will not pretend.",
        "Correct it in plain words: “not like that,” “try another,” “this is final — don’t change it.”",
      ],
    },
    {
      id: "files",
      title: "Give it a file",
      lead: "Drop a Word or PDF into the conversation. It can use it right away, and it keeps it for next time. Just ask — you don’t have to find the file again.",
      paragraphs: [
        "The first keep shows a short note you can undo. After that it stays quiet.",
        "When an answer uses your file, it says so in one line, like “per your 《…》.” Unrelated questions will not mention your files.",
        "To let a file go: What it knows → My materials → delete. The copy on your computer is untouched.",
        "If the text cannot be read (a scan, for example), it will say so. You can still open the original and ask against it.",
        "Spreadsheets and plain text work for this turn too. Say “keep this” or add them under My materials to use them next time.",
        "You can drop a whole folder. What it can read is kept; what it cannot, it will say.",
      ],
      tips: [
        "Dropping the same file again will not create a second row. A near-copy is kept as the new version; the previous one can still be opened.",
        "If an answer missed the file, say the file name again, or drop it once more — this turn will always see it.",
        "Wrong file? Say “not this one.”",
        "Ask “what files do you have” to see the names.",
        "Say “remember this standard” and it waits under What it knows for your yes.",
      ],
    },
    {
      id: "scenes",
      title: "Common situations",
      lead: "You don’t need a procedure. Say it like this.",
      scenes: [
        {
          title: "Write from your talking points",
          situation: "You have a talking-points doc and need a post, notice, or short piece.",
          say: "Drop the doc in, then: write a post in our voice, keep it steady.",
          then: "It writes from that file. Next time, just say “another one.”",
        },
        {
          title: "Ask a contract",
          situation: "The contract is in hand. You want damages, late fees, payment terms.",
          say: "Drop the contract in. Ask: how are damages written? Then: what about being late?",
          then: "It stays in the same file. If two files disagree, it should lay both out. You choose.",
        },
        {
          title: "Revise a draft",
          situation: "You already have a version. This time you only want a new angle or a shorter cut.",
          say: "Drop the draft: revise this — lead with the conclusion, about a third shorter.",
          then: "The draft comes first. At most a few concrete notes after, never instead of the piece.",
        },
        {
          title: "Compare two files",
          situation: "Old and new rules, two quotes, two sets of minutes.",
          say: "Drop both: where do these differ? Which one should I speak from?",
          then: "It keeps them separate. After you choose, it writes in that voice.",
        },
        {
          title: "Park something still open",
          situation: "The proposal is due Friday. You don’t want the chat to bury it.",
          say: "Note this: proposal due Friday.",
          then: "It shows up under Open items (top right). It will nudge near the date. Check it off when done.",
        },
        {
          title: "Let it learn your hand",
          situation: "You changed a title, a tone, a structure. You don’t want to repeat yourself next time.",
          say: "Edit what it wrote, or say: titles should start with tension — no empty lines.",
          then: "When you leave the conversation, it may offer a few notes for you to accept. Nothing is kept without a yes.",
        },
      ],
    },
    {
      id: "knows",
      title: "What “What it knows” holds",
      paragraphs: [
        "This is the back room. You do not need it every day.",
        "About you: habits and standards you accepted — “lead with the point,” “no empty talk.”",
        "How you work: a method you can reuse. The second time around is smoother.",
        "My materials: the originals you kept. Open or delete them here.",
        "After a conversation it may draft a few candidates. Each one waits for your yes. Closing the list means not now.",
      ],
    },
    {
      id: "zaiban",
      title: "Open items",
      paragraphs: [
        "Top right. These are still-due pieces of work — not a diary, not a file cabinet.",
        "Say “note this…” in dialogue and it appears. You can change the date, check it off, or drop it.",
        "It will mention a due item when the time comes. You can also tell it, in Review, to ask less.",
      ],
    },
    {
      id: "wechat",
      title: "Reach it in WeChat",
      paragraphs: [
        "Settings → Connections, then scan with your phone. After that you can find lebi-AI in your own WeChat.",
        "This is your WeChat. Other people cannot use it by default, and it will not read your materials for them.",
        "WeChat shows the conversation. Settings, materials, and open items still live in this desktop app.",
      ],
    },
    {
      id: "settings",
      title: "What else is in Settings",
      paragraphs: [
        "Overview: can you talk, how long the license lasts, whether a new version is waiting.",
        "Dialogue: provider and key. Most people only paste a key.",
        "Appearance: how it addresses you, language, light or dark.",
        "Connections: WeChat.",
        "More: license code, where your data lives, and options you rarely need.",
      ],
    },
    {
      id: "stuck",
      title: "If something feels off",
      tips: [
        "The answer ignored your file: say the file name again, or drop the file once more.",
        "Wrong file: say “not this one,” or delete the wrong one under My materials.",
        "A keep didn’t take: drop it again. Still stuck? Open the original and ask against the screen.",
        "You don’t want a habit used anymore: delete that line under What it knows.",
        "The key failed or the account is unpaid: replace the key under Settings → Dialogue, or sort the account with the provider.",
        "The trial ended: paste a license under Settings → More. Your materials and notes stay.",
      ],
    },
  ],
};

export function getUserManual(language: Language): ManualDoc {
  return language === "en-US" ? EN : ZH;
}
