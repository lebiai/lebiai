import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Link2, Loader2, Play, RefreshCw, Square, Unlink } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { Button, ui } from "../common/ui";
import { toast } from "../../utils/toast";
import { WechatQrModal } from "./WechatQrModal";

interface WechatStatusView {
  state: string; // stopped | listening | token_expired | error
  botId?: string | null;
  lastError?: string | null;
  loggedIn: boolean;
  listening: boolean;
}

const STOPPED: WechatStatusView = {
  state: "stopped",
  botId: null,
  lastError: null,
  loggedIn: false,
  listening: false,
};

/**
 * WeChat connection card: status summary + actions. The QR login itself
 * lives in `WechatQrModal` (the single scan window), so this card never
 * renders an inline QR.
 */
export function WechatConnectCard() {
  const t = useUiStore((s) => s.t);
  const [status, setStatus] = useState<WechatStatusView>(STOPPED);
  const [qrOpen, setQrOpen] = useState(false);
  const [confirmLogout, setConfirmLogout] = useState(false);
  const [loginErr, setLoginErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await invoke<WechatStatusView>("wechat_status"));
    } catch (e) {
      setLoginErr(String(e));
    }
  }, []);

  useEffect(() => {
    refreshStatus();
    let unlisten: (() => void) | undefined;
    listen<WechatStatusView>("wechat-status", (ev) => setStatus(ev.payload)).then(
      (f) => (unlisten = f),
    );
    return () => unlisten?.();
  }, [refreshStatus]);

  const start = async () => {
    setBusy(true);
    try {
      await invoke<WechatStatusView>("wechat_start");
      refreshStatus();
    } catch (e) {
      setLoginErr(String(e));
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    setBusy(true);
    try {
      await invoke("wechat_stop");
      refreshStatus();
    } catch (e) {
      setLoginErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const logout = async () => {
    setConfirmLogout(false);
    setBusy(true);
    try {
      await invoke("wechat_logout");
      setQrOpen(false);
      refreshStatus();
    } catch (e) {
      setLoginErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const stateLabel =
    status.state === "stopping"
      ? t("settings.wechatStopping")
      : status.state === "listening"
      ? t("settings.wechatListening")
      : status.state === "token_expired"
        ? t("settings.wechatTokenExpired")
        : status.state === "error"
          ? t("settings.wechatError")
          : status.loggedIn
            ? t("settings.wechatStopped")
            : t("settings.wechatNotConnected");

  const dotColor =
    status.state === "stopping"
      ? "bg-amber-400"
      : status.state === "listening"
      ? "bg-emerald-500"
      : status.state === "token_expired" || status.state === "error"
        ? "bg-red-500"
        : "bg-amber-400";

  // One source of truth: backend `state` + credential file drive the buttons.
  const showConnect = !status.loggedIn && status.state !== "token_expired";
  const showRescan = status.state === "token_expired";
  const showStart = status.loggedIn && !status.listening && status.state !== "token_expired";
  const showStop = status.listening;
  const showLogout = status.loggedIn;

  return (
    <section className={`${ui.card} p-4 space-y-3`}>
      <h3 className="text-xs font-medium text-app-fg-secondary uppercase tracking-wide">
        {t("settings.wechat")}
      </h3>
      <p className="text-xs text-app-fg-secondary">{t("settings.wechatHint")}</p>

      <div className="flex items-center gap-2 text-sm">
        <span className={`inline-block h-2 w-2 rounded-full ${dotColor}`} />
        <span className="font-medium text-app-fg dark:text-slate-100">{stateLabel}</span>
        {status.botId && (
          <span className="text-xs font-mono text-app-fg-tertiary truncate">{status.botId}</span>
        )}
      </div>
      {status.lastError && (
        <p className="text-xs text-red-600 dark:text-red-300">{status.lastError}</p>
      )}
      {loginErr && <p className="text-xs text-red-600 dark:text-red-300">{loginErr}</p>}

      {status.state === "stopping" ? (
        <div className="flex items-center gap-2 text-xs text-app-fg-secondary">
          <Loader2 size={13} className="animate-spin" />
          {t("settings.wechatStopping")}
        </div>
      ) : (
      <div className="flex flex-wrap gap-2">
        {showConnect && (
          <Button size="sm" onClick={() => setQrOpen(true)} disabled={busy}>
            <Link2 size={12} />
            {t("settings.wechatConnect")}
          </Button>
        )}
        {showRescan && (
          <Button size="sm" onClick={() => setQrOpen(true)} disabled={busy}>
            <RefreshCw size={12} />
            {t("settings.wechatRescan")}
          </Button>
        )}
        {showStart && (
          <Button size="sm" onClick={start} disabled={busy}>
            <Play size={12} />
            {t("settings.wechatStart")}
          </Button>
        )}
        {showStop && (
          <Button size="sm" variant="secondary" onClick={stop} disabled={busy}>
            <Square size={12} />
            {t("settings.wechatStop")}
          </Button>
        )}
        {showLogout && (
          <Button size="sm" variant="danger" onClick={() => setConfirmLogout(true)} disabled={busy}>
            <Unlink size={12} />
            {t("settings.wechatLogout")}
          </Button>
        )}
      </div>
      )}

      <WechatQrModal open={qrOpen} onClose={() => setQrOpen(false)} onConnected={refreshStatus} />

      {confirmLogout && (
        <div
          className="fixed inset-0 z-[9000] flex items-center justify-center bg-black/45 backdrop-blur-[2px]"
          onClick={() => setConfirmLogout(false)}
          role="dialog"
          aria-modal="true"
          aria-label={t("settings.wechatLogoutTitle")}
        >
          <div
            className="w-full max-w-sm mx-4 rounded-2xl border border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-900 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="px-5 pt-5 pb-3">
              <div className="text-sm font-semibold text-app-fg dark:text-slate-100">
                {t("settings.wechatLogoutTitle")}
              </div>
              <p className="mt-1 text-xs text-app-fg-secondary dark:text-slate-400">
                {t("settings.wechatLogoutConfirm")}
              </p>
            </div>
            <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-app-border dark:border-slate-800 bg-app-muted/40 dark:bg-slate-950/40 rounded-b-2xl">
              <Button size="sm" variant="ghost" onClick={() => setConfirmLogout(false)}>
                {t("common.cancel")}
              </Button>
              <Button size="sm" variant="danger" onClick={logout}>
                {t("settings.wechatLogout")}
              </Button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
