import type { Node } from "@/lib/api";

export function countryOf(node: Node) {
  return node.name.split("/")[0] || "其他";
}

export function asNumber(value: unknown) {
  const number = Number(value);
  return Number.isFinite(number) ? number : 0;
}

export function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function formatTraffic(bytes: string) {
  const value = Number(bytes);
  if (!Number.isFinite(value)) return "—";
  const tib = value / 1024 ** 4;
  return tib >= 1 ? `${tib.toFixed(2)} TB` : `${(value / 1024 ** 3).toFixed(1)} GB`;
}

export function formatDate(timestamp: number) {
  if (!timestamp) return "—";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(timestamp * 1000));
}

export function formatSpeed(kbps: number | null | undefined) {
  if (kbps == null) return "—";
  return kbps >= 1024 ? `${(kbps / 1024).toFixed(1)} MB/s` : `${kbps} KB/s`;
}

export function latencyColor(latency: number | null | undefined) {
  if (latency == null) return "text-muted-foreground";
  if (latency <= 80) return "text-online-foreground";
  if (latency <= 180) return "text-amber";
  if (latency <= 350) return "text-orange-500 dark:text-orange-300";
  return "text-destructive";
}

export function qualityColor(successful: number, attempts: number) {
  if (!attempts) return "text-muted-foreground";
  if (successful === attempts) return "text-online-foreground";
  if (successful * 2 >= attempts) return "text-amber";
  return "text-destructive";
}