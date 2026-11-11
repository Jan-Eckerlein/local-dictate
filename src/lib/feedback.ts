export type FeedbackKind = "error" | "info" | "alert";

export interface UserFeedback {
  message: string;
  kind: FeedbackKind;
}

function isPasteInfoMessage(message: string): boolean {
  const lower = message.toLowerCase();
  return lower.includes("no new segments since last batch paste");
}

function isAlertMessage(message: string): boolean {
  const lower = message.toLowerCase();
  return (
    lower.includes("accessibility") ||
    lower.includes("not trusted") ||
    lower.includes("no transcript yet") ||
    lower.includes("microphone") ||
    lower.includes("nothing to paste")
  );
}

export function feedbackFromRaw(raw: string): UserFeedback {
  const message = raw.trim();
  if (isPasteInfoMessage(message)) {
    return { message, kind: "info" };
  }
  if (isAlertMessage(message)) {
    return { message, kind: "alert" };
  }
  return { message, kind: "error" };
}
