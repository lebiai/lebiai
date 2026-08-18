import type { ReactNode } from "react";
import { Button } from "../common/ui";
import { useUiStore } from "../../store/uiStore";
import { StatusRow } from "./StatusRow";
import type { UpdatePhase } from "./useAppUpdate";

export function VersionStatusRow({
  phase,
  onCheck,
  onApply,
}: {
  phase: UpdatePhase;
  onCheck: () => void;
  onApply: () => void;
}) {
  const t = useUiStore((s) => s.t);

  let tone: "ok" | "warn" | "danger" | "neutral" = "neutral";
  let title = t("settings.updateChecking");
  let subtitle: string | undefined = phase.version
    ? t("settings.updateCurrent", { version: phase.version })
    : undefined;
  let action: ReactNode | undefined;

  switch (phase.kind) {
    case "checking":
      tone = "neutral";
      title = t("settings.updateChecking");
      break;
    case "dev":
      tone = "neutral";
      title = t("settings.updateDev");
      subtitle = t("settings.updateDevHint", { version: phase.version || "—" });
      break;
    case "latest":
      tone = "ok";
      title = t("settings.updateLatest");
      subtitle = t("settings.updateCurrent", { version: phase.version });
      action = (
        <button
          type="button"
          className="text-xs text-app-primary hover:underline"
          onClick={onCheck}
        >
          {t("settings.updateCheck")}
        </button>
      );
      break;
    case "available":
      tone = "warn";
      title = t("settings.updateAvailable", { version: phase.next });
      subtitle = phase.notes || t("settings.updateCurrent", { version: phase.version });
      action = (
        <Button size="sm" onClick={onApply}>
          {t("settings.updateNow")}
        </Button>
      );
      break;
    case "downloading":
      tone = "warn";
      title = t("settings.updateDownloading");
      subtitle =
        phase.percent == null
          ? t("settings.updateInstallingHint")
          : t("settings.updateProgress", { percent: phase.percent });
      action = (
        <Button size="sm" disabled>
          {t("settings.updateNow")}
        </Button>
      );
      break;
    case "installing":
      tone = "warn";
      title = t("settings.updateInstalling");
      subtitle = t("settings.updateInstallingHint");
      action = (
        <Button size="sm" disabled>
          {t("settings.updateNow")}
        </Button>
      );
      break;
    case "error":
      tone = "danger";
      title = t("settings.updateFailed");
      subtitle = phase.message;
      action = (
        <Button size="sm" variant="secondary" onClick={onCheck}>
          {t("settings.updateRetry")}
        </Button>
      );
      break;
  }

  return <StatusRow tone={tone} title={title} subtitle={subtitle} action={action} />;
}
