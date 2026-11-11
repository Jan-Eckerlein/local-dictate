import { invoke } from "@tauri-apps/api/core";
import { toast, type ExternalToast } from "sonner";
import { feedbackFromRaw, type FeedbackKind } from "@/lib/feedback";

const DURATION: Record<FeedbackKind, number> = {
  info: 6_000,
  alert: 10_000,
  error: Number.POSITIVE_INFINITY,
};

const KIND_CLASS: Record<FeedbackKind, string> = {
  info:
    "!border-blue-500/45 !bg-blue-500/12 !text-foreground dark:!bg-blue-500/20 [&_[data-title]]:text-foreground",
  alert:
    "!border-amber-500/50 !bg-amber-500/14 !text-foreground dark:!bg-amber-500/18 [&_[data-title]]:text-foreground",
  error:
    "!border-destructive/55 !bg-destructive/12 !text-destructive dark:!bg-destructive/18",
};

export function showAppToast(
  message: string,
  kind: FeedbackKind,
  options?: ExternalToast,
) {
  const classNames = {
    toast: KIND_CLASS[kind],
    ...options?.classNames,
  };
  const opts: ExternalToast = {
    ...options,
    duration: options?.duration ?? DURATION[kind],
    classNames,
  };

  switch (kind) {
    case "info":
      toast.info(message, opts);
      break;
    case "alert":
      toast.warning(message, opts);
      break;
    case "error":
      toast.error(message, opts);
      break;
  }
}

function withContextActions(
  message: string,
  options?: ExternalToast,
): ExternalToast | undefined {
  const lower = message.toLowerCase();
  if (
    lower.includes("accessibility") ||
    lower.includes("not trusted")
  ) {
    return {
      ...options,
      action: {
        label: "Settings",
        onClick: () => {
          void invoke("open_accessibility_settings");
        },
      },
    };
  }
  return options;
}

export function notifyFromRaw(raw: string, options?: ExternalToast) {
  const { message, kind } = feedbackFromRaw(raw);
  showAppToast(message, kind, withContextActions(message, options));
}
