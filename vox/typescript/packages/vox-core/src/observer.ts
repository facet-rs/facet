export type ObserverMetricLabelKey =
  | "service"
  | "method"
  | "side"
  | "outcome"
  | "error_kind"
  | "channel_direction";

export type ObserverMetricLabelInput = Partial<Record<ObserverMetricLabelKey, string>>;

export type ObserverMetricLabels = Partial<Record<ObserverMetricLabelKey, string>>;

export type EstablishmentRole = "initiator" | "acceptor";

export type EstablishmentPhase =
  | "transport-prologue"
  | "connection-handshake"
  | "identity-resolution"
  | "connection-policy"
  | "schema-decode-plan"
  | "service-lane-open"
  | "lane-authorization"
  | "lane-grant"
  | "lane-grant-revocation";

export type EstablishmentOutcome =
  | "ok"
  | "rejected"
  | "error";

export interface EstablishmentContext {
  role: EstablishmentRole;
  phase: EstablishmentPhase;
  laneId?: bigint;
}

export type EstablishmentEvent =
  | {
      kind: "started";
      context: EstablishmentContext;
    }
  | {
      kind: "finished";
      context: EstablishmentContext;
      outcome: EstablishmentOutcome;
      elapsedMs: number;
      error?: string;
    };

export interface VoxObserver {
  establishment?(event: EstablishmentEvent): void;
}

const OBSERVER_METRIC_LABEL_KEYS: ObserverMetricLabelKey[] = [
  "service",
  "method",
  "side",
  "outcome",
  "error_kind",
  "channel_direction",
];

// r[impl rpc.observability.low-cardinality]
export function observerMetricLabels(input: ObserverMetricLabelInput): ObserverMetricLabels {
  const labels: ObserverMetricLabels = {};
  for (const key of OBSERVER_METRIC_LABEL_KEYS) {
    const value = input[key];
    if (value !== undefined && value.length > 0) {
      labels[key] = value;
    }
  }
  return labels;
}

export function splitQualifiedMethodName(name: string): { service?: string; method: string } {
  const lastDot = name.lastIndexOf(".");
  if (lastDot < 0) {
    return { method: name };
  }
  return {
    service: name.slice(0, lastDot),
    method: name.slice(lastDot + 1),
  };
}

// r[impl rpc.observability.establishment]
export function observeEstablishmentStarted(
  observer: VoxObserver | undefined,
  context: EstablishmentContext,
): number {
  const startedAt = Date.now();
  observer?.establishment?.({
    kind: "started",
    context,
  });
  return startedAt;
}

// r[impl rpc.observability.establishment]
export function observeEstablishmentFinished(
  observer: VoxObserver | undefined,
  context: EstablishmentContext,
  startedAt: number,
  outcome: EstablishmentOutcome,
  error?: unknown,
): void {
  const message = error instanceof Error
    ? error.message
    : error === undefined
      ? undefined
      : String(error);
  observer?.establishment?.({
    kind: "finished",
    context,
    outcome,
    elapsedMs: Math.max(0, Date.now() - startedAt),
    ...(message ? { error: message } : {}),
  });
}
