import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import {
  VPN_PROGRESS_EVENT,
  type VpnDirection,
  type VpnProgress,
} from "@/lib/api";

export type ConnectionPhase =
  | "idle"
  | "connecting"
  | "connected"
  | "disconnecting"
  | "error";

export interface ConnectionLifecycle {
  phase: ConnectionPhase;
  /**
   * How full the dial should read, 0–1. Disconnecting counts back down, so the
   * ring unwinds the arc it filled while connecting. Frozen where a failure hit.
   */
  progress: number;
  /** Stage wording from the runtime; empty while resting. */
  stageLabel: string;
  /** 1-based stage index and count, null while resting. */
  stageCount: [number, number] | null;
  /** Wraps a transition so the button can animate its real stages. Rethrows. */
  run: (direction: VpnDirection, task: () => Promise<void>) => Promise<void>;
}

/** Failure is a flash, not a resting state — the button falls back afterwards. */
const ERROR_HOLD_MS = 2400;

/**
 * Turns the two booleans the runtime exposes (`socks5_ok` plus an in-flight
 * call) into the five phases the hero button animates between, and follows the
 * runtime's stage events so progress reflects real work instead of a timer.
 */
export function useConnectionLifecycle(online: boolean): ConnectionLifecycle {
  const [direction, setDirection] = useState<VpnDirection | null>(null);
  const [stage, setStage] = useState<VpnProgress | null>(null);
  const [failure, setFailure] = useState<VpnProgress | null>(null);
  const [failed, setFailed] = useState(false);
  // Read inside the event listener, which is registered once and must not see
  // a stale closure over the current transition.
  const directionRef = useRef<VpnDirection | null>(null);
  const stageRef = useRef<VpnProgress | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<VpnProgress>(VPN_PROGRESS_EVENT, (event) => {
      // Rule edits and rollbacks restart the runtime without a transition of
      // their own; those stages must not drive the button.
      if (event.payload.direction !== directionRef.current) return;
      stageRef.current = event.payload;
      setStage(event.payload);
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!failed) return;
    const timer = window.setTimeout(() => setFailed(false), ERROR_HOLD_MS);
    return () => window.clearTimeout(timer);
  }, [failed]);

  const run = useCallback(
    async (next: VpnDirection, task: () => Promise<void>) => {
      directionRef.current = next;
      stageRef.current = null;
      setDirection(next);
      setStage(null);
      setFailed(false);
      try {
        await task();
      } catch (cause) {
        setFailure(stageRef.current);
        setFailed(true);
        throw cause;
      } finally {
        directionRef.current = null;
        setDirection(null);
      }
    },
    [],
  );

  const phase: ConnectionPhase =
    direction === "connect"
      ? "connecting"
      : direction === "disconnect"
        ? "disconnecting"
        : failed
          ? "error"
          : online
            ? "connected"
            : "idle";

  const current = phase === "error" ? failure : stage;
  const ratio =
    current && current.total > 0
      ? Math.min(current.step / current.total, 1)
      : 0;
  const unwinding =
    phase === "disconnecting" ||
    (phase === "error" && failure?.direction === "disconnect");

  return {
    phase,
    progress:
      phase === "connected"
        ? 1
        : phase === "idle"
          ? 0
          : unwinding
            ? 1 - ratio
            : ratio,
    stageLabel:
      phase === "error"
        ? failure
          ? `${failure.label}失败`
          : "连接失败"
        : phase === "connecting"
          ? (stage?.label ?? "正在建立连接")
          : phase === "disconnecting"
            ? (stage?.label ?? "正在断开连接")
            : "",
    stageCount:
      current && (phase === "connecting" || phase === "disconnecting")
        ? [current.step, current.total]
        : null,
    run,
  };
}
