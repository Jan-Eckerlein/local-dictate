import { invoke } from "@tauri-apps/api/core";
import { toast, type ExternalToast } from "sonner";
import { asNotice, type FeedbackKind } from "@/lib/feedback";

export const TOAST_ID = {
  mic: "permission-mic",
  ax: "permission-ax",
  model: "whisper-model",
} as const;

const DURATION: Record<FeedbackKind, number> = {
  info: 6_000,
  warn: 10_000,
  error: Number.POSITIVE_INFINITY,
};

const KIND_CLASS: Record<FeedbackKind, string> = {
  info:
    "!border-blue-500/45 !bg-blue-500/12 !text-foreground dark:!bg-blue-500/20 [&_[data-title]]:text-foreground",
  warn:
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
    case "warn":
      toast.warning(message, opts);
      break;
    case "error":
      toast.error(message, opts);
      break;
  }
}

export function dismissAppToast(id: string) {
  toast.dismiss(id);
}

export function showNotice(raw: unknown, options?: ExternalToast) {
  const notice = asNotice(raw);
  showAppToast(notice.message, notice.kind, {
    ...options,
    ...(notice.action === "accessibility"
      ? {
          id: TOAST_ID.ax,
          action: {
            label: "Settings",
            onClick: () => {
              void invoke("open_accessibility_settings");
            },
          },
        }
      : {}),
  });
}
