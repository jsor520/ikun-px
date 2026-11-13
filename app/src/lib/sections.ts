import {
  Activity,
  Globe,
  Network,
  Route,
  Settings,
  type LucideIcon,
} from "lucide-react";

export type SectionId =
  | "overview"
  | "nodes"
  | "rules"
  | "proxy-manager"
  | "settings";

interface Section {
  id: SectionId;
  icon: LucideIcon;
  label: string;
  eyebrow: string;
  title: string;
}

export const SECTIONS: Section[] = [
  {
    id: "overview",
    icon: Activity,
    label: "连接概览",
    eyebrow: "NETWORK CONTROL",
    title: "",
  },
  {
    id: "nodes",
    icon: Globe,
    label: "全球节点",
    eyebrow: "GLOBAL LOCATIONS",
    title: "选择连接节点",
  },
  {
    id: "rules",
    icon: Route,
    label: "分流规则",
    eyebrow: "ROUTING RULES",
    title: "自定义分流",
  },
  {
    id: "proxy-manager",
    icon: Network,
    label: "代理管理",
    eyebrow: "PROXY MANAGER",
    title: "代理管理与诊断",
  },
  {
    id: "settings",
    icon: Settings,
    label: "设置",
    eyebrow: "PREFERENCES",
    title: "界面与行为设置",
  },
];

/** Local-time greeting for the overview header. Boundaries are hour starts. */
export function greetingForHour(hour: number): string {
  if (hour < 5) return "夜深了";
  if (hour < 11) return "早上好";
  if (hour < 14) return "中午好";
  if (hour < 18) return "下午好";
  return "晚上好";
}

export function overviewTitle(now = new Date()): string {
  return `${greetingForHour(now.getHours())}，连接世界。`;
}

export function sectionById(id: SectionId) {
  const section = SECTIONS.find((item) => item.id === id) ?? SECTIONS[0];
  if (section.id !== "overview") return section;
  return { ...section, title: overviewTitle() };
}

export function isSectionId(value: string): value is SectionId {
  return SECTIONS.some((section) => section.id === value);
}
