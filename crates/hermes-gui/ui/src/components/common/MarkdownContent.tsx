import {
  memo,
  useState,
  type ComponentProps,
  type MouseEvent,
  type ReactNode,
} from "react";
import ReactMarkdown, { type Components, type ExtraProps } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { Check, Copy, ExternalLink } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useUiStore } from "../../store/uiStore";
import { toast } from "../../utils/toast";

function CodeBlock({ children, className }: { children: ReactNode; className?: string }) {
  const t = useUiStore((s) => s.t);
  const [copied, setCopied] = useState(false);
  const text = String(children).replace(/\n$/, "");
  const lang = /language-(\w+)/.exec(className ?? "")?.[1];

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      toast.success(t("toast.copied"));
      window.setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="relative group my-2 rounded-lg overflow-hidden border border-app-border dark:border-slate-700 bg-slate-900 text-slate-100">
      <div className="flex items-center justify-between px-3 py-1 border-b border-slate-700/80 bg-slate-800/80">
        <span className="text-xs uppercase tracking-wide text-slate-400 font-mono">
          {lang || "code"}
        </span>
        <button
          type="button"
          onClick={() => void copy()}
          className="inline-flex items-center gap-1 text-xs text-slate-300 hover:text-white px-1.5 py-0.5 rounded-md hover:bg-slate-700/80"
          aria-label={t("common.copy")}
        >
          {copied ? <Check size={12} /> : <Copy size={12} />}
          {copied ? t("common.copied") : t("common.copy")}
        </button>
      </div>
      <pre className="m-0 p-3 overflow-x-auto text-xs leading-relaxed">
        <code className={className}>{text}</code>
      </pre>
    </div>
  );
}

function safeHref(href: string | undefined): string | undefined {
  if (!href) return undefined;
  const t = href.trim();
  const lower = t.toLowerCase();
  if (
    lower.startsWith("http://") ||
    lower.startsWith("https://") ||
    lower.startsWith("mailto:")
  ) {
    return t;
  }
  // Block javascript:, data:, file:, etc.
  return undefined;
}

// remark-breaks：单换行就是换行（对话里本就该这样）。没有它，模型写下的多行清单
// 会被折成一行——2026-09-20 资讯卷按「每条三行」定死格式后暴露。
const REMARK_PLUGINS = [remarkGfm, remarkBreaks];

/**
 * 渲染器与插件**必须**是模块级常量。react-markdown 把它们当组件类型用：写成内联对象
 * 等于每次渲染换一批新类型，React 只能把整棵子树卸载重建（一份资讯卷里上百个元素）。
 * 流式期间一秒几十次——这就是「文字不出来，最后整段蹦出来」的主因之一。
 */
const MD_COMPONENTS: Components = {
  pre({ children }: ComponentProps<"pre">) {
    // Unwrap default pre — CodeBlock owns the shell.
    return <>{children}</>;
  },
  code({ className, children, ...props }: ComponentProps<"code"> & ExtraProps) {
    // Only fenced blocks with an explicit language get the copy chrome.
    // Multi-line plain text / process-ish dumps should not look like code tools.
    const hasLang = Boolean(className && /language-/.test(className));
    if (hasLang) {
      return <CodeBlock className={className}>{children}</CodeBlock>;
    }
    const text = String(children);
    if (text.includes("\n")) {
      return (
        <pre className="my-2 p-3 overflow-x-auto text-xs leading-relaxed rounded-lg border border-app-border dark:border-slate-700 bg-app-muted/40 dark:bg-slate-900/50">
          <code className={className} {...props}>
            {children}
          </code>
        </pre>
      );
    }
    return (
      <code className={className} {...props}>
        {children}
      </code>
    );
  },
  a({ href, children, ...props }: ComponentProps<"a"> & ExtraProps) {
    const safe = safeHref(href);
    if (!safe) {
      return <span {...props}>{children}</span>;
    }
    const onClick = (e: MouseEvent<HTMLAnchorElement>) => {
      // WebView 里点 <a> 什么都不会发生（用户原话：「来源的链接不能点击」）。
      // 交给系统浏览器打开，别把用户关在应用里。
      e.preventDefault();
      void openUrl(safe).catch(() => {
        // 浏览器里预览（非 Tauri 壳）时退回新窗口，不弹红字吓人。
        window.open(safe, "_blank", "noopener,noreferrer");
      });
    };
    const label = chipLabelOf(children);
    if (label === null) {
      return (
        <a href={safe} rel="noopener noreferrer" {...props} onClick={onClick}>
          {children}
        </a>
      );
    }
    return (
      <a
        href={safe}
        rel="noopener noreferrer"
        title={safe}
        {...props}
        className="source-chip"
        onClick={onClick}
      >
        <ExternalLink size={10} aria-hidden />
        <span className="source-chip-label">{label}</span>
      </a>
    );
  },
};

/**
 * 小标签该写什么字；返回 `null` = 这条链接**不该**做成小标签。
 *
 * - 「原文链接」「来源」「36氪」这类短标签 → 原样；
 * - 裸网址（模型常这么写来源）→ 只留域名：整条 URL 塞进标签会把整行撑爆；
 * - 包着一整句话的链接 → 不是来源标签，保持普通链接的样子（通篇小标签反而吵）。
 */
function chipLabelOf(children: ReactNode): string | null {
  const text =
    typeof children === "string"
      ? children
      : Array.isArray(children) && children.every((c) => typeof c === "string")
        ? children.join("")
        : null;
  if (text === null) return null;
  const t = text.trim();
  if (!t) return null;
  if (/^https?:\/\//i.test(t)) {
    try {
      const host = new URL(t).hostname.replace(/^www\./, "");
      if (host) return host;
    } catch {
      /* 解析不出来就退回原文；反正 title 里还有完整地址 */
    }
    return t;
  }
  return t.length <= 12 && !/[。，、；：!?！？]/.test(t) ? t : null;
}

/** Markdown renderer with copyable fenced code blocks. */
export const MarkdownContent = memo(function MarkdownContent({
  content,
}: {
  content: string;
}) {
  return (
    <div className="prose prose-sm prose-chat dark:prose-invert max-w-none">
      <ReactMarkdown remarkPlugins={REMARK_PLUGINS} components={MD_COMPONENTS}>
        {content}
      </ReactMarkdown>
    </div>
  );
});
