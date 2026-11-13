import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  motion,
  useAnimationFrame,
  useMotionValue,
  useReducedMotion,
  type Variants,
} from "motion/react";
import { Power } from "lucide-react";

import { cn } from "@/lib/utils";
import type { ConnectionPhase } from "@/lib/connection";
import type {
  CaptureMode,
  MarsConfig,
  Node,
  RouteMode,
} from "@/lib/api";

const DIAL = 110;
const RING = 180;
const ORBIT = 218;
const STROKE = 2.5;
const RADIUS = (RING - STROKE) / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/**
 * The card clips its overflow, so hard-edged rings have to finish inside it:
 * 110px × 2.2 stays under the 270px card height.
 */
const MAX_REACH = 2.2;

/** Degrees per millisecond: a revolution every 28s at rest, every 3s working. */
const RESTING_SPIN = 360 / 28000;
const WORKING_SPIN = 360 / 3000;
/** Long frames after the window is restored must not fling the orbit around. */
const MAX_FRAME_MS = 64;

const EXPO_OUT: [number, number, number, number] = [0.16, 1, 0.3, 1];
const SOFT_OUT: [number, number, number, number] = [0.22, 1, 0.36, 1];

const PINNED = "pointer-events-none absolute top-1/2 left-1/2";

const CAPTURE_OPTIONS: Array<{
  value: CaptureMode;
  label: string;
  hint: string;
}> = [
  { value: "local", label: "仅本地端口", hint: "不修改 Windows 网络设置" },
  { value: "system", label: "系统代理", hint: "接管支持系统代理的应用" },
  { value: "tun", label: "TUN 网卡", hint: "接管更多应用及 DNS 流量" },
];

const ROUTE_OPTIONS: Array<{
  value: RouteMode;
  label: string;
  hint: string;
}> = [
  { value: "smart", label: "智能代理", hint: "国内与局域网直连，其余按规则代理" },
  {
    value: "global",
    label: "全局代理",
    hint: "除局域网和系统基础服务外全部走代理",
  },
  { value: "direct", label: "全部直连", hint: "临时绕过代理链路" },
];

const CHIP_GROUP: Variants = {
  hidden: {},
  show: { transition: { staggerChildren: 0.07, delayChildren: 0.06 } },
};

const BREATH = {
  duration: 2.1,
  repeat: Infinity,
  ease: "easeInOut",
} as const;

interface ConnectionHeroProps {
  phase: ConnectionPhase;
  progress: number;
  stageLabel: string;
  stageCount: [number, number] | null;
  busy: boolean;
  config: MarsConfig | null;
  currentNode: Node | undefined;
  onToggle: () => void;
  onModesChange: (captureMode: CaptureMode, routeMode: RouteMode) => void;
}

function MetaChip({
  label,
  value,
  variants,
}: {
  label: string;
  value: string;
  variants: Variants;
}) {
  return (
    <motion.span
      variants={variants}
      className="flex items-center gap-2 rounded-[9px] border bg-surface-2 px-3 py-2 font-mono text-[11px] text-card-foreground"
    >
      <i className="text-[8px] font-extrabold not-italic text-muted-foreground">
        {label}
      </i>
      {value}
    </motion.span>
  );
}

/**
 * Rate-driven rather than duration-driven: changing an `animation-duration`
 * restarts the rotation and visibly jumps, so the orbit eases its own speed
 * toward the target and stays continuous when work starts and finishes.
 */
function OrbitRing({
  working,
  still,
  tone,
}: {
  working: boolean;
  still: boolean;
  tone: string;
}) {
  const rotation = useMotionValue(0);
  const speed = useRef(RESTING_SPIN);

  useAnimationFrame((_, delta) => {
    if (still) return;
    const frame = Math.min(delta, MAX_FRAME_MS);
    const target = working ? WORKING_SPIN : RESTING_SPIN;
    speed.current += (target - speed.current) * Math.min(frame / 320, 1);
    rotation.set((rotation.get() + speed.current * frame) % 360);
  });

  return (
    <motion.span
      aria-hidden
      className={cn(
        PINNED,
        "rounded-full border border-dashed transition-colors duration-500",
        tone,
      )}
      style={{
        width: ORBIT,
        height: ORBIT,
        x: "-50%",
        y: "-50%",
        rotate: rotation,
      }}
    />
  );
}

export function ConnectionHero({
  phase,
  progress,
  stageLabel,
  stageCount,
  busy,
  config,
  currentNode,
  onToggle,
  onModesChange,
}: ConnectionHeroProps) {
  const reduced = useReducedMotion() ?? false;
  const [ripples, setRipples] = useState<number[]>([]);
  const [celebration, setCelebration] = useState(0);
  const rippleId = useRef(0);
  const lastPhase = useRef(phase);

  // The one moment worth a flourish: work finished and the tunnel came up.
  useEffect(() => {
    if (lastPhase.current === "connecting" && phase === "connected") {
      setCelebration((count) => count + 1);
    }
    lastPhase.current = phase;
  }, [phase]);

  const transitioning = phase === "connecting" || phase === "disconnecting";
  const active = phase === "connected" || phase === "disconnecting";
  const locked = busy || transitioning;
  const outward = phase === "connecting";

  const captureMode = config?.capture_mode ?? "local";
  const routeMode = config?.route_mode ?? "smart";
  const captureLabel =
    CAPTURE_OPTIONS.find((option) => option.value === captureMode)?.label ??
    "仅本地端口";
  const routeLabel =
    ROUTE_OPTIONS.find((option) => option.value === routeMode)?.label ??
    "智能代理";
  const mode = `${captureLabel} · ${routeLabel}`;

  const eyebrow =
    phase === "connecting"
      ? "正在建立安全连接"
      : phase === "disconnecting"
        ? "正在断开连接"
        : phase === "error"
          ? "连接未能建立"
          : phase === "connected"
            ? "连接已受保护"
            : "当前未连接";

  const caption =
    transitioning || phase === "error"
      ? stageLabel
      : active
        ? "点击断开"
        : "点击连接";

  const chipItem: Variants = {
    hidden: { opacity: 0, y: reduced ? 0 : 9 },
    show: {
      opacity: 1,
      y: 0,
      transition: { duration: 0.34, ease: SOFT_OUT },
    },
  };

  function spawnRipple() {
    if (reduced) return;
    rippleId.current += 1;
    const id = rippleId.current;
    setRipples((current) => [...current, id]);
  }

  return (
    <section
      id="overview"
      className={cn(
        "relative grid min-h-[320px] grid-cols-[minmax(0,1fr)_280px] overflow-hidden rounded-3xl border transition-colors duration-500 shadow-[0_24px_80px_rgba(24,20,52,0.08)] dark:shadow-[0_24px_80px_rgba(0,0,0,0.2)]",
        active ? "border-online/25" : "border-primary/25",
      )}
    >
      {/* Two opaque fills crossfade because background-image cannot transition. */}
      <div
        aria-hidden
        className="absolute inset-0 bg-[radial-gradient(circle_at_83%_50%,rgba(106,75,240,0.16),transparent_31%),linear-gradient(122deg,#ffffff_0%,#f5f3fe_58%,#efeaff_100%)] dark:bg-[radial-gradient(circle_at_83%_50%,rgba(100,79,228,0.25),transparent_31%),linear-gradient(122deg,#15152a_0%,#111321_58%,#15132b_100%)]"
      />
      <motion.div
        aria-hidden
        initial={false}
        animate={{ opacity: active ? 1 : 0 }}
        transition={{ duration: 0.55, ease: SOFT_OUT }}
        className="absolute inset-0 bg-[radial-gradient(circle_at_83%_50%,rgba(18,163,106,0.16),transparent_31%),linear-gradient(122deg,#ffffff_0%,#f3f9f6_58%,#e9f7f0_100%)] dark:bg-[radial-gradient(circle_at_83%_50%,rgba(63,200,145,0.2),transparent_31%),linear-gradient(122deg,#131b27_0%,#10151f_58%,#10251f_100%)]"
      />
      <div
        aria-hidden
        className="absolute inset-0 bg-[linear-gradient(var(--hairline)_1px,transparent_1px),linear-gradient(90deg,var(--hairline)_1px,transparent_1px)] [background-size:26px_26px] [mask-image:linear-gradient(90deg,#000,transparent_76%)]"
      />

      <div className="relative z-1 self-center pt-[38px] pr-[18px] pb-[34px] pl-[38px]">
        <div
          className={cn(
            "flex items-center gap-2 text-xs font-bold tracking-[0.06em] transition-colors duration-500",
            phase === "error"
              ? "text-destructive-foreground"
              : active
                ? "text-online-foreground"
                : "text-accent-foreground",
          )}
        >
          <span
            className={cn(
              "size-[7px] rounded-full shadow-[0_0_0_4px_var(--accent)] transition-colors duration-500",
              phase === "error"
                ? "bg-destructive"
                : active
                  ? "bg-online"
                  : "bg-neon",
              phase === "connected" &&
                "animate-[online-pulse_1.8s_ease-out_infinite]",
            )}
          />
          <Swap trigger={eyebrow} reduced={reduced}>
            {eyebrow}
          </Swap>
        </div>

        <h2 className="mt-3.5 mb-[9px] max-w-[580px] text-[clamp(24px,3vw,35px)] leading-[1.15] font-bold tracking-[-0.04em] text-foreground">
          {currentNode?.name || "选择一个安全节点"}
        </h2>
        <p className="m-0 text-[13px] text-muted-foreground">
          <Swap trigger={String(active)} reduced={reduced}>
            {active
              ? `${currentNode?.addr ?? "本地代理"} · ${routeLabel}已启用`
              : "启动后将通过加密通道保护你的网络流量"}
          </Swap>
        </p>

        <motion.div
          key={active ? "online" : "offline"}
          variants={CHIP_GROUP}
          initial="hidden"
          animate="show"
          className="mt-7 flex flex-wrap gap-2"
        >
          <MetaChip
            label="IP"
            value={currentNode?.addr || "—"}
            variants={chipItem}
          />
          <MetaChip label="MODE" value={mode} variants={chipItem} />
          <MetaChip
            label="DNS"
            value={currentNode ? "Fake-IP" : "—"}
            variants={chipItem}
          />
        </motion.div>
        <div className="mt-4 space-y-2">
          <div className="flex items-center gap-3">
            <span className="w-14 shrink-0 text-[11px] font-semibold text-muted-foreground">
              接管方式
            </span>
            <div
              className="inline-flex rounded-[10px] border bg-surface-1 p-1"
              aria-label="流量接管方式"
            >
              {CAPTURE_OPTIONS.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  aria-pressed={captureMode === option.value}
                  title={option.hint}
                  disabled={locked}
                  onClick={() => onModesChange(option.value, routeMode)}
                  className={cn(
                    "rounded-[7px] px-3 py-1.5 text-[11px] font-semibold transition disabled:cursor-wait disabled:opacity-55",
                    captureMode === option.value
                      ? "bg-primary text-primary-foreground shadow-sm"
                      : "text-muted-foreground hover:bg-surface-2 hover:text-foreground",
                  )}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </div>
          <div className="flex items-center gap-3">
            <span className="w-14 shrink-0 text-[11px] font-semibold text-muted-foreground">
              分流策略
            </span>
            <div
              className="inline-flex rounded-[10px] border bg-surface-1 p-1"
              aria-label="流量分流策略"
            >
              {ROUTE_OPTIONS.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  aria-pressed={routeMode === option.value}
                  title={option.hint}
                  disabled={locked}
                  onClick={() => onModesChange(captureMode, option.value)}
                  className={cn(
                    "rounded-[7px] px-3 py-1.5 text-[11px] font-semibold transition disabled:cursor-wait disabled:opacity-55",
                    routeMode === option.value
                      ? "bg-primary text-primary-foreground shadow-sm"
                      : "text-muted-foreground hover:bg-surface-2 hover:text-foreground",
                  )}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* The padding lifts the dial so the caption clears both rings; the
          caption is out of flow and cannot balance the group on its own. */}
      <div className="relative z-1 grid min-h-[320px] place-items-center pb-[22px]">
        <motion.div
          className="relative grid place-items-center"
          style={{ width: DIAL, height: DIAL }}
          animate={
            phase === "error" && !reduced ? { x: [0, -5, 4, -3, 2, 0] } : { x: 0 }
          }
          transition={{ duration: 0.42, ease: "easeInOut" }}
        >
          <motion.span
            aria-hidden
            className={cn(PINNED, "rounded-full")}
            style={{
              width: DIAL * 2.05,
              height: DIAL * 2.05,
              x: "-50%",
              y: "-50%",
              background: `radial-gradient(circle, ${
                active ? "var(--glow-online)" : "var(--glow-neon)"
              }, transparent 62%)`,
            }}
            animate={
              !reduced && (phase === "connected" || phase === "idle")
                ? { scale: [0.92, 1.28, 0.92], opacity: [0.4, 1, 0.4] }
                : { scale: 1, opacity: transitioning ? 1 : 0.55 }
            }
            transition={
              !reduced && (phase === "connected" || phase === "idle")
                ? BREATH
                : { duration: 0.5, ease: SOFT_OUT }
            }
          />

          {!reduced && (phase === "connected" || phase === "idle") && (
            <>
              <motion.span
                aria-hidden
                className={cn(
                  PINNED,
                  "rounded-full blur-[14px]",
                  active ? "bg-online/55" : "bg-neon/45",
                )}
                style={{
                  width: DIAL * 1.22,
                  height: DIAL * 1.22,
                  x: "-50%",
                  y: "-50%",
                }}
                animate={{ scale: [0.88, 1.2, 0.88], opacity: [0.35, 0.95, 0.35] }}
                transition={BREATH}
              />
              <motion.span
                aria-hidden
                className={cn(
                  PINNED,
                  "rounded-full border-[3px]",
                  active ? "border-online" : "border-neon",
                )}
                style={{
                  width: DIAL + 22,
                  height: DIAL + 22,
                  x: "-50%",
                  y: "-50%",
                }}
                animate={{ scale: [1, 1.16, 1], opacity: [0.2, 0.95, 0.2] }}
                transition={BREATH}
              />
            </>
          )}

          <OrbitRing
            working={transitioning}
            still={reduced}
            tone={active ? "border-online/15" : "border-neon/15"}
          />

          <svg
            aria-hidden
            width={RING}
            height={RING}
            viewBox={`0 0 ${RING} ${RING}`}
            className={cn(PINNED, "-translate-x-1/2 -translate-y-1/2")}
          >
            <g transform={`rotate(-90 ${RING / 2} ${RING / 2})`}>
              <circle
                cx={RING / 2}
                cy={RING / 2}
                r={RADIUS}
                fill="none"
                strokeWidth={STROKE}
                className={cn(
                  "transition-[stroke] duration-500",
                  active ? "stroke-online/15" : "stroke-neon/15",
                )}
              />
              <motion.circle
                cx={RING / 2}
                cy={RING / 2}
                r={RADIUS}
                fill="none"
                strokeWidth={STROKE}
                strokeLinecap="round"
                strokeDasharray={CIRCUMFERENCE}
                initial={false}
                animate={{ strokeDashoffset: CIRCUMFERENCE * (1 - progress) }}
                transition={{
                  duration: reduced ? 0 : 0.55,
                  ease: SOFT_OUT,
                }}
                className={cn(
                  "transition-[stroke] duration-500",
                  phase === "error"
                    ? "stroke-destructive"
                    : active
                      ? "stroke-online"
                      : "stroke-neon",
                )}
              />
            </g>
          </svg>

          {transitioning &&
            !reduced &&
            [0, 1, 2].map((index) => (
              <motion.span
                key={`${phase}-${index}`}
                aria-hidden
                className={cn(
                  PINNED,
                  "rounded-full border",
                  active ? "border-online/45" : "border-neon/45",
                )}
                style={{ width: DIAL, height: DIAL, x: "-50%", y: "-50%" }}
                initial={{ scale: outward ? 1 : MAX_REACH, opacity: 0 }}
                animate={
                  outward
                    ? { scale: [1, MAX_REACH], opacity: [0.4, 0] }
                    : { scale: [MAX_REACH, 1], opacity: [0, 0.36, 0] }
                }
                transition={{
                  duration: 2.2,
                  repeat: Infinity,
                  delay: index * 0.73,
                  ease: outward ? "easeOut" : "easeIn",
                }}
              />
            ))}

          {ripples.map((id) => (
            <motion.span
              key={id}
              aria-hidden
              className={cn(
                PINNED,
                "rounded-full border-2",
                active ? "border-online/50" : "border-neon/50",
              )}
              style={{ width: DIAL, height: DIAL, x: "-50%", y: "-50%" }}
              initial={{ scale: 1, opacity: 0.45 }}
              animate={{ scale: MAX_REACH, opacity: 0 }}
              transition={{ duration: 0.66, ease: [0.2, 0.8, 0.2, 1] }}
              onAnimationComplete={() =>
                setRipples((current) => current.filter((value) => value !== id))
              }
            />
          ))}

          {celebration > 0 && !reduced && (
            <motion.span
              key={`bloom-${celebration}`}
              aria-hidden
              className={cn(PINNED, "rounded-full")}
              style={{
                width: DIAL,
                height: DIAL,
                x: "-50%",
                y: "-50%",
                background:
                  "radial-gradient(circle, var(--glow-online), transparent 70%)",
              }}
              initial={{ scale: 0.4, opacity: 0.95 }}
              animate={{ scale: 3.6, opacity: 0 }}
              transition={{ duration: 0.62, ease: EXPO_OUT }}
            />
          )}

          {celebration > 0 && !reduced && (
            <motion.span
              key={`wave-${celebration}`}
              aria-hidden
              className={cn(PINNED, "rounded-full border-[3px] border-online")}
              style={{ width: DIAL, height: DIAL, x: "-50%", y: "-50%" }}
              initial={{ scale: 1, opacity: 0.7 }}
              animate={{ scale: MAX_REACH, opacity: 0 }}
              transition={{ duration: 0.72, ease: EXPO_OUT }}
            />
          )}

          <motion.button
            type="button"
            disabled={locked}
            aria-busy={transitioning}
            aria-label={active ? "断开 VPN" : "连接 VPN"}
            onClick={onToggle}
            onPointerDown={() => !locked && spawnRipple()}
            onKeyDown={(event) => {
              if (!locked && (event.key === "Enter" || event.key === " ")) {
                spawnRipple();
              }
            }}
            whileHover={locked || reduced ? undefined : { scale: 1.035 }}
            whileTap={locked || reduced ? undefined : { scale: 0.94 }}
            animate={
              !reduced && (phase === "connected" || phase === "idle")
                ? {
                    boxShadow: active
                      ? [
                          "0 0 0 8px rgba(18,163,106,0.08), 0 0 22px 4px rgba(18,163,106,0.22)",
                          "0 0 0 16px rgba(18,163,106,0.22), 0 0 48px 12px rgba(18,163,106,0.55)",
                          "0 0 0 8px rgba(18,163,106,0.08), 0 0 22px 4px rgba(18,163,106,0.22)",
                        ]
                      : [
                          "0 0 0 8px rgba(106,75,240,0.08), 0 0 22px 4px rgba(106,75,240,0.2)",
                          "0 0 0 16px rgba(106,75,240,0.22), 0 0 48px 12px rgba(106,75,240,0.5)",
                          "0 0 0 8px rgba(106,75,240,0.08), 0 0 22px 4px rgba(106,75,240,0.2)",
                        ],
                  }
                : undefined
            }
            transition={{
              scale: {
                type: "spring",
                stiffness: 520,
                damping: 18,
                mass: 0.7,
              },
              boxShadow:
                !reduced && (phase === "connected" || phase === "idle")
                  ? BREATH
                  : { duration: 0.45, ease: SOFT_OUT },
            }}
            style={{ width: DIAL, height: DIAL }}
            className={cn(
              "relative z-2 grid place-items-center overflow-hidden rounded-full border outline-none transition-[border-color,color] duration-500 focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-wait",
              active
                ? "border-online/70 text-online-foreground dark:shadow-[inset_0_0_24px_rgba(115,246,184,0.12)]"
                : "border-neon/70 text-neon dark:text-neon-soft dark:shadow-[inset_0_0_24px_rgba(164,149,255,0.12)]",
            )}
          >
            <span
              aria-hidden
              className="absolute inset-0 rounded-full bg-[radial-gradient(circle,rgba(106,75,240,0.14),rgba(106,75,240,0.05)),#ffffff] dark:bg-[radial-gradient(circle,rgba(119,93,255,0.25),rgba(44,35,93,0.8)),#1b1738]"
            />
            <motion.span
              aria-hidden
              initial={false}
              animate={{ opacity: active ? 1 : 0 }}
              transition={{ duration: 0.42, ease: SOFT_OUT }}
              className="absolute inset-0 rounded-full bg-[radial-gradient(circle,rgba(18,163,106,0.14),rgba(18,163,106,0.05)),#ffffff] dark:bg-[radial-gradient(circle,rgba(55,183,128,0.22),rgba(27,70,53,0.75)),#102b21]"
            />
            <motion.span
              key={celebration}
              className="relative z-1 grid place-items-center"
              initial={{ scale: 1 }}
              animate={{
                scale: celebration > 0 && !reduced ? [1, 1.2, 1] : 1,
              }}
              transition={{ duration: 0.52, ease: EXPO_OUT }}
            >
              <Power
                className={cn(
                  "size-10",
                  !reduced && outward && "power-charging",
                  !reduced && phase === "disconnecting" && "power-draining",
                )}
                strokeWidth={1.6}
              />
            </motion.span>
          </motion.button>

          <div
            aria-live="polite"
            className="absolute top-[calc(100%+62px)] left-1/2 flex -translate-x-1/2 items-center gap-1.5 whitespace-nowrap"
          >
            <motion.strong
              key={caption}
              initial={{ opacity: 0, y: reduced ? 0 : 4 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.24, ease: SOFT_OUT }}
              className={cn(
                "text-[11px] font-semibold tracking-[0.05em]",
                phase === "error"
                  ? "text-destructive-foreground"
                  : "text-muted-foreground",
              )}
            >
              {caption}
            </motion.strong>
            {stageCount && (
              <span className="font-mono text-[10px] tabular-nums text-muted-foreground/55">
                {stageCount[0]}/{stageCount[1]}
              </span>
            )}
          </div>
        </motion.div>
      </div>
    </section>
  );
}

/** Fades replacement text in without an exit pass, so rapid stage updates
 *  cannot queue up behind each other. */
function Swap({
  trigger,
  reduced,
  children,
}: {
  trigger: string;
  reduced: boolean;
  children: ReactNode;
}) {
  return (
    <motion.span
      key={trigger}
      initial={{ opacity: 0, y: reduced ? 0 : 4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.28, ease: SOFT_OUT }}
      className="inline-block"
    >
      {children}
    </motion.span>
  );
}
