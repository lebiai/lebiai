import { useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Check, Copy } from "lucide-react";
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
        <span className="text-[10px] uppercase tracking-wide text-slate-400 font-mono">
          {lang || "code"}
        </span>
        <button
          type="button"
          onClick={() => void copy()}
          className="inline-flex items-center gap-1 text-[11px] text-slate-300 hover:text-white px-1.5 py-0.5 rounded-md hover:bg-slate-700/80"
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

/** Markdown renderer with copyable fenced code blocks. */
export function MarkdownContent({ content }: { content: string }) {
  return (
    <div className="prose prose-sm prose-chat dark:prose-invert max-w-none">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          pre({ children }) {
            // Unwrap default pre — CodeBlock owns the shell.
            return <>{children}</>;
          },
          code({ className, children, ...props }) {
            const isBlock = Boolean(className) || String(children).includes("\n");
            if (isBlock) {
              return <CodeBlock className={className}>{children}</CodeBlock>;
            }
            return (
              <code className={className} {...props}>
                {children}
              </code>
            );
          },
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
