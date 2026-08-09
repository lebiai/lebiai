import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QrCode, RefreshCw, X } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { Button } from "../common/ui";
import { toast } from "../../utils/toast";

interface WechatLoginView {
  matrix: boolean[][];
}

interface WechatPollView {
  status: string; // waiting | scanned | refreshed | confirmed
  matrix?: boolean[][] | null;
  botId?: string | null;
}

/** Render a QR bool-matrix on a canvas (quiet zone included). */
function QrCanvas({ matrix }: { matrix: boolean[][] }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const cv = ref.current;
    if (!cv || matrix.length === 0) return;
    const n = matrix.length;
    const scale = 10;
    cv.width = n * scale;
    cv.height = n * scale;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, cv.width, cv.height);
    ctx.fillStyle = "#0f172a";
    matrix.forEach((row, y) => {
      row.forEach((dark, x) => {
        if (dark) ctx.fillRect(x * scale, y * scale, scale, scale);
      });
    });
  }, [matrix]);
  return (
    <canvas
      ref={ref}
      className="rounded-xl border border-app-border dark:border-slate-600 bg-white"
      style={{ width: 280, height: 280 }}
    />
  );
}

/**
 * The single QR-login window (目标路径定死：扫码 = 唯一居中弹窗).
 * Opens a login session on mount, polls it, auto-refreshes expired QRs,
 * closes itself on confirmation. Esc / backdrop / Cancel to close.
 */
export function WechatQrModal({
  open,
  onClose,
  onConnected,
}: {
  open: boolean;
  onClose: () => void;
  onConnected?: () => void;
}) {
  const t = useUiStore((s) => s.t);
  const [matrix, setMatrix] = useState<boolean[][] | null>(null);
  const [phase, setPhase] = useState<"waiting" | "scanned">("waiting");
  const [refreshed, setRefreshed] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const startLogin = useCallback(async () => {
    setBusy(true);
    setErr(null);
    setRefreshed(false);
    setPhase("waiting");
    try {
      const view = await invoke<WechatLoginView>("wechat_login_start");
      setMatrix(view.matrix);
    } catch (e) {
      setMatrix(null);
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  // Start a fresh login whenever the modal opens.
  useEffect(() => {
    if (!open) return;
    startLogin();
  }, [open, startLogin]);

  // Poll the login session while open.
  useEffect(() => {
    if (!open) return;
    const iv = setInterval(async () => {
      try {
        const p = await invoke<WechatPollView>("wechat_login_poll");
        if (p.status === "refreshed" && p.matrix) {
          setMatrix(p.matrix);
          setPhase("waiting");
          setRefreshed(true);
        } else if (p.status === "scanned") {
          setPhase("scanned");
        } else if (p.status === "confirmed") {
          onClose();
          toast.success(t("settings.wechatConnectedToast"));
          onConnected?.();
        }
      } catch (e) {
        setErr(String(e));
      }
    }, 1000);
    return () => clearInterval(iv);
  }, [open, onClose, onConnected, t]);

  // Esc closes the modal.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[9000] flex items-center justify-center bg-black/45 backdrop-blur-[2px]"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label={t("settings.wechat")}
    >
      <div
        className="w-full max-w-sm mx-4 rounded-2xl border border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-900 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-5 pb-3">
          <div className="flex items-center gap-2 text-sm font-semibold text-app-fg dark:text-slate-100">
            <QrCode size={16} className="text-app-accent dark:text-violet-300" />
            {t("settings.wechatConnect")}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg p-1 text-app-fg-tertiary hover:bg-app-muted dark:hover:bg-slate-800"
            aria-label={t("common.cancel")}
          >
            <X size={16} />
          </button>
        </div>

        <div className="px-5 pb-4 space-y-3">
          {err ? (
            <div className="space-y-3">
              <p className="text-xs text-red-600 dark:text-red-300">{err}</p>
              <Button size="sm" onClick={startLogin} disabled={busy}>
                <RefreshCw size={12} />
                {t("common.retry")}
              </Button>
            </div>
          ) : matrix ? (
            <div className="flex flex-col items-center gap-2">
              <QrCanvas matrix={matrix} />
              <p className="text-xs text-app-fg-secondary dark:text-slate-400">
                {phase === "scanned"
                  ? t("settings.wechatScanned")
                  : refreshed
                    ? t("settings.wechatRefreshed")
                    : t("settings.wechatScanHint")}
              </p>
            </div>
          ) : (
            <p className="text-xs text-app-fg-secondary">{t("common.loading")}</p>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-app-border dark:border-slate-800 bg-app-muted/40 dark:bg-slate-950/40 rounded-b-2xl">
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
        </div>
      </div>
    </div>
  );
}
