export type UiPhase =
  | "loading"
  | "needs_model"
  | "needs_mic"
  | "ready"
  | "monitoring"
  | "monitoring_transcribing";

export interface UiPhaseInput {
  micStatus: string;
  monitoring: boolean;
  whisperExists: boolean | null;
  sttPendingIndex: number | null;
}

export function deriveUiPhase(input: UiPhaseInput): UiPhase {
  if (input.micStatus === "" || input.whisperExists === null) {
    return "loading";
  }
  if (!input.whisperExists) {
    return "needs_model";
  }
  if (input.micStatus !== "authorized") {
    return "needs_mic";
  }
  if (input.monitoring) {
    if (input.sttPendingIndex !== null) {
      return "monitoring_transcribing";
    }
    return "monitoring";
  }
  return "ready";
}

const PHASE_LABEL: Record<UiPhase, string> = {
  loading: "Loading…",
  needs_model: "Model missing",
  needs_mic: "Microphone required",
  ready: "Ready",
  monitoring: "Monitoring",
  monitoring_transcribing: "Monitoring · STT",
};

export interface AppStatus {
  phase: UiPhase;
  label: string;
  canStartMonitoring: boolean;
  canPaste: boolean;
}

export function buildAppStatus(
  phase: UiPhase,
  opts: {
    axAuthorized: boolean;
    hasTranscript: boolean;
    pasteBusy: boolean;
  },
): AppStatus {
  const canPaste =
    opts.axAuthorized && opts.hasTranscript && !opts.pasteBusy;

  const canStartMonitoring =
    phase !== "loading" && phase !== "needs_model" && phase !== "needs_mic";

  return {
    phase,
    label: PHASE_LABEL[phase],
    canStartMonitoring,
    canPaste,
  };
}
