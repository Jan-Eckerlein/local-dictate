export type FeedbackKind = "info" | "warn" | "error";

/** Payload declared by the caller. Kind is not inferred from the message text. */
export interface AppNotice {
  message: string;
  kind: FeedbackKind;
  action?: "accessibility";
}

function isKind(value: unknown): value is FeedbackKind {
  return value === "info" || value === "warn" || value === "error";
}

function unwrap(raw: unknown): unknown {
  if (typeof raw !== "string") return raw;
  const trimmed = raw.trim();
  if (!trimmed.startsWith("{")) return raw;
  try {
    return JSON.parse(trimmed) as unknown;
  } catch {
    return raw;
  }
}

export function asNotice(raw: unknown): AppNotice {
  const value = unwrap(raw);
  if (
    typeof value === "object" &&
    value !== null &&
    "message" in value &&
    "kind" in value &&
    typeof value.message === "string" &&
    isKind(value.kind)
  ) {
    const action =
      "action" in value && value.action === "accessibility"
        ? ("accessibility" as const)
        : undefined;
    return { message: value.message, kind: value.kind, action };
  }

  const message =
    typeof raw === "string"
      ? raw.trim()
      : raw instanceof Error
        ? raw.message
        : String(raw);
  return { message, kind: "error" };
}
