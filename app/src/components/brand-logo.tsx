import { useId } from "react";

import { cn } from "@/lib/utils";

/**
 * Vector twin of `src-tauri/app-icon.svg`. Both must stay in sync so the window
 * chrome, the sidebar and the OS icons show the same mark.
 */
export function BrandLogo({ className }: { className?: string }) {
  const id = useId();
  const tile = `${id}-tile`;
  const sheen = `${id}-sheen`;

  return (
    <svg
      viewBox="0 0 1024 1024"
      role="img"
      aria-label="IKUN PX"
      className={cn("shrink-0", className)}
    >
      <defs>
        <linearGradient
          id={tile}
          x1="96"
          y1="0"
          x2="928"
          y2="1024"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset="0" stopColor="#8B6DFF" />
          <stop offset="0.52" stopColor="#6A4BF0" />
          <stop offset="1" stopColor="#3E2FA3" />
        </linearGradient>
        <linearGradient
          id={sheen}
          x1="512"
          y1="0"
          x2="512"
          y2="1024"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset="0" stopColor="#FFFFFF" stopOpacity="0.26" />
          <stop offset="0.45" stopColor="#FFFFFF" stopOpacity="0" />
        </linearGradient>
      </defs>

      <rect width="1024" height="1024" rx="232" fill={`url(#${tile})`} />
      <rect width="1024" height="1024" rx="232" fill={`url(#${sheen})`} />
      <rect
        x="6"
        y="6"
        width="1012"
        height="1012"
        rx="226"
        fill="none"
        stroke="#AFA3FF"
        strokeOpacity="0.45"
        strokeWidth="12"
      />

      <g fill="#FFFFFF">
        <rect
          x="297"
          y="222"
          width="430"
          height="116"
          rx="58"
          opacity="0.7"
          transform="rotate(-42 512 280)"
        />
        <rect
          x="297"
          y="454"
          width="430"
          height="116"
          rx="58"
          transform="rotate(-42 512 512)"
        />
        <rect
          x="297"
          y="686"
          width="430"
          height="116"
          rx="58"
          opacity="0.45"
          transform="rotate(-42 512 744)"
        />
      </g>
    </svg>
  );
}
