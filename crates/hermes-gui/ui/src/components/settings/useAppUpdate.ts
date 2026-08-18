import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { useUiStore } from "../../store/uiStore";

export type UpdatePhase =
  | { kind: "checking"; version: string }
  | { kind: "dev"; version: string }
  | { kind: "latest"; version: string }
  | { kind: "available"; version: string; next: string; notes: string }
  | { kind: "downloading"; version: string; next: string; percent: number | null }
  | { kind: "installing"; version: string; next: string }
  | { kind: "error"; version: string; next?: string; message: string };

function humanizeUpdateError(
  err: unknown,
  t: (key: "settings.updateErrNetwork" | "settings.updateErrSignature" | "settings.updateErrWrite" | "settings.updateErrGeneric") => string,
): string {
  const raw = err instanceof Error ? err.message : String(err);
  const lower = raw.toLowerCase();
  if (
    lower.includes("sign") ||
    lower.includes("signature") ||
    lower.includes("minisign") ||
    lower.includes("pubkey") ||
    lower.includes("public key")
  ) {
    return t("settings.updateErrSignature");
  }
  if (
    lower.includes("permission") ||
    lower.includes("denied") ||
    lower.includes("not allowed") ||
    lower.includes("acl") ||
    lower.includes("access") ||
    lower.includes("readonly") ||
    lower.includes("read-only")
  ) {
    return t("settings.updateErrWrite");
  }
  if (
    lower.includes("network") ||
    lower.includes("fetch") ||
    lower.includes("dns") ||
    lower.includes("timed out") ||
    lower.includes("timeout") ||
    lower.includes("connection") ||
    lower.includes("connect") ||
    lower.includes("offline") ||
    lower.includes("github") ||
    lower.includes("status code") ||
    lower.includes("404") ||
    lower.includes("could not") ||
    lower.includes("error sending")
  ) {
    return t("settings.updateErrNetwork");
  }
  return t("settings.updateErrGeneric");
}

/**
 * Version check + click-to-apply. State lives in the caller so switching
 * Settings tabs does not abort an in-flight download.
 */
export function useAppUpdate(active: boolean) {
  const t = useUiStore((s) => s.t);
  const [phase, setPhase] = useState<UpdatePhase>({ kind: "checking", version: "" });
  const pendingRef = useRef<Update | null>(null);
  const inflightRef = useRef(false);
  const versionRef = useRef("");

  useEffect(() => {
    let cancelled = false;
    void getVersion()
      .then((v) => {
        if (cancelled) return;
        versionRef.current = v;
        setPhase((p) => (p.version ? p : { ...p, version: v }));
      })
      .catch(() => {
        /* keep empty until a later check */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const inspect = useCallback(async () => {
    if (inflightRef.current) return;
    const current = versionRef.current;
    let debugBuild = import.meta.env.DEV;
    if (!debugBuild) {
      try {
        debugBuild = await invoke<boolean>("app_debug_build");
      } catch {
        debugBuild = true;
      }
    }
    if (debugBuild) {
      setPhase({ kind: "dev", version: current });
      return;
    }
    setPhase({ kind: "checking", version: current });
    pendingRef.current = null;
    try {
      const update = await check();
      const version = versionRef.current || current;
      if (!update) {
        setPhase({ kind: "latest", version });
        return;
      }
      pendingRef.current = update;
      const rawNotes = (update.body ?? "").trim();
      const notes = /github/i.test(rawNotes) ? "" : rawNotes;
      setPhase({
        kind: "available",
        version,
        next: update.version,
        notes,
      });
    } catch (err) {
      console.error("update check failed", err);
      setPhase({
        kind: "error",
        version: versionRef.current || current,
        message: humanizeUpdateError(err, t),
      });
    }
  }, [t]);

  useEffect(() => {
    if (!active) return;
    if (inflightRef.current) return;
    void inspect();
  }, [active, inspect]);

  const apply = useCallback(async () => {
    if (inflightRef.current) return;
    const current = versionRef.current;
    inflightRef.current = true;
    try {
      let update = pendingRef.current;
      if (!update) {
        setPhase({ kind: "checking", version: current });
        update = await check();
        if (!update) {
          setPhase({ kind: "latest", version: current });
          return;
        }
        pendingRef.current = update;
      }
      const next = update.version;
      let downloaded = 0;
      let total = 0;
      setPhase({ kind: "downloading", version: current, next, percent: null });
      await update.downloadAndInstall((ev: DownloadEvent) => {
        if (ev.event === "Started") {
          total = ev.data.contentLength ?? 0;
          downloaded = 0;
          setPhase({
            kind: "downloading",
            version: current,
            next,
            percent: total > 0 ? 0 : null,
          });
        } else if (ev.event === "Progress") {
          downloaded += ev.data.chunkLength;
          setPhase({
            kind: "downloading",
            version: current,
            next,
            percent: total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null,
          });
        } else if (ev.event === "Finished") {
          setPhase({ kind: "installing", version: current, next });
        }
      });
      setPhase({ kind: "installing", version: current, next });
      await relaunch();
    } catch (err) {
      setPhase({
        kind: "error",
        version: versionRef.current || current,
        next: pendingRef.current?.version,
        message: humanizeUpdateError(err, t),
      });
      pendingRef.current = null;
    } finally {
      inflightRef.current = false;
    }
  }, [t]);

  return { phase, inspect, apply };
}
