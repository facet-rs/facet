export type ObserverMetricLabelKey =
  | "service"
  | "method"
  | "side"
  | "outcome"
  | "error_kind"
  | "channel_direction";

export type ObserverMetricLabelInput = Partial<Record<ObserverMetricLabelKey, string>>;

export type ObserverMetricLabels = Partial<Record<ObserverMetricLabelKey, string>>;

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
