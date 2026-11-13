import { invoke } from "@tauri-apps/api/core";

export type CaptureMode = "local" | "system" | "tun";
export type RouteMode = "smart" | "global" | "direct";

export interface MarsConfig {
  email: string;
  last_select_node: string;
  capture_mode: CaptureMode;
  route_mode: RouteMode;
  api: string;
  web: string;
  device_id: string;
  password_saved: boolean;
  logged_in: boolean;
  git_proxy: boolean;
  ssh_proxy: boolean;
  auto_connect: boolean;
  auto_start: boolean;
  hotkeys_enabled: boolean;
  show_node_icons: boolean;
  sidebar_collapsed: boolean;
}

/** Prefill for the login form. `password` is only non-empty when the user
 *  asked Windows to remember it. */
export interface SavedCredentials {
  email: string;
  password: string;
  remember: boolean;
}

export interface UserInfo {
  uuid: string;
  email: string;
  /** Remaining traffic in bytes, sent by the server as a decimal string. */
  transfer: string;
  charge_type: number;
  charge_expire_date: number;
  server_time: number;
  payment_qr_code: string;
}

export interface Node {
  uuid: string;
  name: string;
  addr: string;
  port: number;
  random: boolean;
  happy_wife: string;
  happy_life: string;
  region: string;
  free: unknown;
  udp: unknown;
  lat: unknown;
  lon: unknown;
  bandwidth_current: unknown;
  bandwidth_total: unknown;
}

export interface EngineStatus {
  http_proxy_open: boolean;
  socks5_open: boolean;
  socks5_ok: boolean;
  process_running: boolean;
  tun_running: boolean;
  node_uuid: string;
  node_name: string;
  http_address: string;
  socks5_address: string;
  upload_bytes: number;
  download_bytes: number;
}

export interface NodeMeasurement {
  uuid: string;
  latency_ms: number | null;
  successful: number;
  attempts: number;
}

export interface UserRules {
  proxy: string;
  direct: string;
}

export interface InitConfig {
  sys_proxy_file: string;
  sys_local_file: string;
  country_file: string;
  public_msg_title: string;
  public_msg_content: string;
}

/** A past connection and the route it actually took. Historical: later rule
 *  changes do not rewrite it. */
export interface DomainEvent {
  timestamp: number;
  host: string;
  route: "direct" | "proxy";
}

/** Domains currently pinned by the per-row buttons, i.e. what future
 *  connections will do. */
export interface PinnedDomains {
  proxy: string[];
  direct: string[];
}

/** One measured target: the connected node's address, or a page fetched
 *  through the local proxy. `speed_kbps` is only filled in by the page probe. */
export interface DiagnosticProbe {
  target: string;
  ok: boolean;
  latency_ms: number | null;
  speed_kbps: number | null;
  detail: string;
}

export interface DiagnosticReport {
  checks: Array<{
    name: string;
    ok: boolean;
    detail: string;
  }>;
  node: DiagnosticProbe | null;
  web: DiagnosticProbe | null;
}

export type VpnDirection = "connect" | "disconnect";

/**
 * Emitted once before each connect/disconnect stage starts, so the stage that
 * is still current when a command rejects is the one that failed. `total`
 * depends on which integrations are enabled and is fixed for a given run.
 */
export interface VpnProgress {
  direction: VpnDirection;
  phase: string;
  label: string;
  step: number;
  total: number;
}

export const VPN_PROGRESS_EVENT = "vpn-progress";

export const getConfig = () => invoke<MarsConfig>("get_config");
export const setUiPreferences = (
  showNodeIcons: boolean,
  sidebarCollapsed: boolean,
) =>
  invoke<MarsConfig>("set_ui_preferences", {
    showNodeIcons,
    sidebarCollapsed,
  });
export const getUserInfo = () => invoke<UserInfo>("get_user_info");
export const getNodes = () => invoke<Node[]>("get_nodes");
export const measureNodes = () => invoke<NodeMeasurement[]>("measure_nodes");
export const getStatus = () => invoke<EngineStatus>("get_status");
export const switchNode = (uuid: string) => invoke<void>("switch_node", { uuid });
export const vpnStart = () => invoke<void>("vpn_start");
export const vpnStop = () => invoke<void>("vpn_stop");
export const login = (
  email: string,
  password: string,
  rememberPassword: boolean,
) =>
  invoke<void>("login", { email, password, rememberPassword });
export const getSavedCredentials = () =>
  invoke<SavedCredentials>("get_saved_credentials");
/** `forget` also drops the remembered email and password, not just the session. */
export const logout = (forget = false) => invoke<void>("logout", { forget });
export const getInitConfig = () => invoke<InitConfig>("get_init_config");
export const getUserRules = () => invoke<UserRules>("get_user_rules");
export const saveUserRules = (rules: UserRules) =>
  invoke<void>("save_user_rules", { rules });
export const setDomainRoute = (host: string, route: "direct" | "proxy") =>
  invoke<UserRules>("set_domain_route", { host, route });
export const clearDomainRoute = (host: string) =>
  invoke<UserRules>("clear_domain_route", { host });
export const getPinnedDomains = () =>
  invoke<PinnedDomains>("get_pinned_domains");
export const getDomainEvents = () =>
  invoke<DomainEvent[]>("get_domain_events");
export const clearDomainEvents = () => invoke<void>("clear_domain_events");
export const copyProxyEnvironment = () =>
  invoke<void>("copy_proxy_environment");
export const copyProxyAddresses = () => invoke<void>("copy_proxy_addresses");
export const openProxyConsole = () => invoke<void>("open_proxy_console");
export const repairNetwork = () => invoke<string>("repair_network");
export const networkDiagnostics = () =>
  invoke<DiagnosticReport>("network_diagnostics");
export const setIntegrations = (
  gitProxy: boolean,
  sshProxy: boolean,
  autoConnect: boolean,
  autoStart: boolean,
  hotkeysEnabled: boolean,
) =>
  invoke<void>("set_integrations", {
    gitProxy,
    sshProxy,
    autoConnect,
    autoStart,
    hotkeysEnabled,
  });
export const setNetworkModes = (
  captureMode: CaptureMode,
  routeMode: RouteMode,
) =>
  invoke<MarsConfig>("set_network_modes", {
    captureMode,
    routeMode,
  });