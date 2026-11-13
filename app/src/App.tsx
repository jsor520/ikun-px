import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { LogIn, RefreshCw, Search, TriangleAlert, X } from "lucide-react";
import { toast } from "sonner";

import { AppSidebar } from "@/components/app-sidebar";
import { ConnectBar } from "@/components/connect-bar";
import { ConnectionHero } from "@/components/connection-hero";
import { LoginScreen } from "@/components/login-screen";
import { NodeCard } from "@/components/node-card";
import { OverviewActions } from "@/components/overview-actions";
import { OverviewNodePicker } from "@/components/overview-node-picker";
import { ProxyManager } from "@/components/proxy-manager";
import { RuleEditor } from "@/components/rule-editor";
import { SettingsPage } from "@/components/settings-page";
import { StatCards } from "@/components/stat-cards";
import { TitleBar } from "@/components/title-bar";
import { Alert, AlertAction, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import { useConnectionLifecycle } from "@/lib/connection";
import { countryOf, errorText } from "@/lib/format";
import { isSectionId, sectionById, type SectionId } from "@/lib/sections";
import {
  type CaptureMode,
  type EngineStatus,
  type MarsConfig,
  type Node,
  type NodeMeasurement,
  type RouteMode,
  type UserInfo,
  getConfig,
  getInitConfig,
  getNodes,
  getStatus,
  getUserInfo,
  logout,
  measureNodes,
  setNetworkModes,
  setUiPreferences,
  switchNode,
  vpnStart,
  vpnStop,
} from "@/lib/api";

async function waitForStatus(expectedOnline: boolean) {
  let latest = await getStatus();
  for (let attempt = 0; attempt < 15; attempt += 1) {
    if (latest.socks5_ok === expectedOnline) return latest;
    await new Promise((resolve) => window.setTimeout(resolve, 800));
    latest = await getStatus();
  }
  return latest;
}

function AppFrame({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-screen flex-col overflow-hidden">
      <TitleBar />
      {children}
    </div>
  );
}

function App() {
  const [config, setConfig] = useState<MarsConfig | null>(null);
  const [user, setUser] = useState<UserInfo | null>(null);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [measurements, setMeasurements] = useState<
    Record<string, NodeMeasurement>
  >({});
  const [status, setStatus] = useState<EngineStatus | null>(null);
  const [section, setSection] = useState<SectionId>("overview");
  const [targetUuid, setTargetUuid] = useState("");
  const [query, setQuery] = useState("");
  const [country, setCountry] = useState("全部");
  const [initialLoading, setInitialLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [action, setAction] = useState<
    "connect" | "toggle" | "mode" | null
  >(null);
  const [savingUiPreferences, setSavingUiPreferences] = useState(false);
  const [error, setError] = useState("");
  // Startup and every logout land on the login card; dismissing it drops the
  // user into the app as a guest until they ask for the card again.
  const [loginVisible, setLoginVisible] = useState(true);
  const initialized = useRef(false);
  const autoStarted = useRef(false);

  const online = status?.socks5_ok === true;
  const loggedOut = config !== null && !config.logged_in;
  const connection = useConnectionLifecycle(online);
  const runConnection = connection.run;

  /** Guest mode has no token, so the control plane would reject the call with
   *  an opaque error. Send the user back to the login card instead. */
  function blockedByLogin() {
    if (!loggedOut) return false;
    setLoginVisible(true);
    toast.info("请先登录后再连接");
    return true;
  }

  /** Reads the rolling probe stats the backend keeps collecting, so the ratio
   *  climbs on its own instead of freezing at whatever the last refresh saw. */
  const refreshMeasurements = useCallback(async () => {
    try {
      const values = await measureNodes();
      if (values.length) {
        setMeasurements(
          Object.fromEntries(values.map((value) => [value.uuid, value])),
        );
      }
    } catch {
      // A poll that loses the race with a logout has nothing to report.
    }
  }, []);

  const loadData = useCallback(async (initial = false) => {
    if (initial) setInitialLoading(true);
    else setRefreshing(true);
    setError("");

    const failures: string[] = [];
    let nextConfig: MarsConfig;
    try {
      nextConfig = await getConfig();
      setConfig(nextConfig);
      setTargetUuid((current) => current || nextConfig.last_select_node);
    } catch (cause) {
      setError(`读取配置：${errorText(cause)}`);
      setInitialLoading(false);
      setRefreshing(false);
      return;
    }

    if (!nextConfig.logged_in) {
      setUser(null);
      setNodes([]);
      setInitialLoading(false);
      setRefreshing(false);
      return;
    }

    const [userResult, nodesResult, statusResult] = await Promise.allSettled([
      getUserInfo(),
      getNodes(),
      getStatus(),
    ]);

    if (userResult.status === "fulfilled") setUser(userResult.value);
    else failures.push(`账户信息：${errorText(userResult.reason)}`);

    if (nodesResult.status === "fulfilled") {
      setNodes(nodesResult.value);
      void refreshMeasurements();
    }
    else failures.push(`节点列表：${errorText(nodesResult.reason)}`);

    if (statusResult.status === "fulfilled") setStatus(statusResult.value);
    else failures.push(`引擎状态：${errorText(statusResult.reason)}`);

    if (failures.length) setError(failures.join("；"));
    setInitialLoading(false);
    setRefreshing(false);
  }, [refreshMeasurements]);

  useEffect(() => {
    if (!initialized.current) {
      initialized.current = true;
      void loadData(true);
    }
    const timer = window.setInterval(() => {
      void getStatus().then(setStatus).catch(() => undefined);
      void refreshMeasurements();
    }, 3000);
    return () => window.clearInterval(timer);
  }, [loadData, refreshMeasurements]);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    void listen<string>("navigate-section", (event) => {
      if (isSectionId(event.payload)) setSection(event.payload);
    }).then((unlisten) => unlisteners.push(unlisten));
    void listen<string>("request-logout", (event) => {
      void logout(event.payload === "forget").then(() => {
        setLoginVisible(true);
        return loadData(true);
      });
    }).then((unlisten) => unlisteners.push(unlisten));
    void listen<string>("network-repaired", (event) => {
      toast.success(event.payload);
      void loadData();
    }).then((unlisten) => unlisteners.push(unlisten));
    void listen<string>("integration-error", (event) => {
      toast.error(event.payload);
    }).then((unlisten) => unlisteners.push(unlisten));
    void listen("config-changed", () => {
      void loadData();
    }).then((unlisten) => unlisteners.push(unlisten));
    void listen("show-announcement", () => {
      void getInitConfig()
        .then((value) =>
          toast.info(value.public_msg_title || "公告", {
            description: value.public_msg_content || "暂无公告",
            duration: 10000,
          }),
        )
        .catch((cause) => toast.error(errorText(cause)));
    }).then((unlisten) => unlisteners.push(unlisten));
    return () => unlisteners.forEach((unlisten) => unlisten());
  }, [loadData]);

  useEffect(() => {
    if (
      config?.logged_in &&
      config.auto_connect &&
      status &&
      !status.socks5_ok &&
      !autoStarted.current
    ) {
      autoStarted.current = true;
      void runConnection("connect", () => vpnStart())
        .then(() => toast.success("已按设置自动连接"))
        .catch((cause) => setError(errorText(cause)));
    }
  }, [config, runConnection, status]);

  const currentNode = nodes.find((node) => node.uuid === config?.last_select_node);
  const targetNode = nodes.find((node) => node.uuid === targetUuid);

  const countries = useMemo(
    () => ["全部", ...Array.from(new Set(nodes.map(countryOf)))],
    [nodes],
  );

  const regionCount = useMemo(
    () => new Set(nodes.map(countryOf)).size,
    [nodes],
  );

  const filteredNodes = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return nodes.filter((node) => {
      const matchesCountry = country === "全部" || countryOf(node) === country;
      const matchesQuery =
        !needle ||
        node.name.toLocaleLowerCase().includes(needle) ||
        node.addr.includes(needle);
      return matchesCountry && matchesQuery;
    });
  }, [country, nodes, query]);

  async function connectTarget() {
    if (!targetNode || action) return;
    if (online && targetNode.uuid === config?.last_select_node) return;
    if (blockedByLogin()) return;

    setAction("connect");
    setError("");
    try {
      await runConnection("connect", async () => {
        if (targetNode.uuid !== config?.last_select_node) {
          await switchNode(targetNode.uuid);
        }
        // switch_node only restarts a runtime that was already up, so an
        // offline switch still needs an explicit start.
        if (!online) {
          await vpnStart();
        }
        const nextStatus = await waitForStatus(true);
        setStatus(nextStatus);
        setConfig(await getConfig());
        if (!nextStatus.socks5_ok) {
          throw new Error("引擎已启动，但 SOCKS5 端口尚未就绪");
        }
      });
      toast.success(`已连接到 ${targetNode.name}`);
    } catch (cause) {
      setError(errorText(cause));
      void getConfig().then(setConfig).catch(() => undefined);
    } finally {
      setAction(null);
    }
  }

  async function toggleVpn() {
    if (action) return;
    if (!online && blockedByLogin()) return;
    const direction = online ? "disconnect" : "connect";
    setAction("toggle");
    setError("");
    try {
      await runConnection(direction, async () => {
        if (direction === "disconnect") {
          await vpnStop();
          setStatus(await waitForStatus(false));
          return;
        }
        await vpnStart();
        const nextStatus = await waitForStatus(true);
        setStatus(nextStatus);
        if (!nextStatus.socks5_ok) {
          throw new Error("引擎已启动，但代理端口未就绪");
        }
      });
      toast.success(direction === "disconnect" ? "VPN 已断开" : "VPN 已连接");
    } catch (cause) {
      setError(errorText(cause));
      void getStatus().then(setStatus).catch(() => undefined);
    } finally {
      setAction(null);
    }
  }

  async function handleLogout() {
    if (action) return;
    setAction("toggle");
    setError("");
    try {
      await logout();
      setUser(null);
      setNodes([]);
      setTargetUuid("");
      setLoginVisible(true);
      setConfig(await getConfig());
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setAction(null);
    }
  }

  async function handleModesChange(
    captureMode: CaptureMode,
    routeMode: RouteMode,
  ) {
    if (action) return;
    setAction("mode");
    setError("");
    try {
      setConfig(await setNetworkModes(captureMode, routeMode));
      toast.success("接管方式与分流策略已更新");
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setAction(null);
    }
  }

  async function handleUiPreferences(
    showNodeIcons: boolean,
    sidebarCollapsed: boolean,
  ) {
    if (savingUiPreferences) return;
    const previous = config;
    setConfig((current) =>
      current
        ? {
            ...current,
            show_node_icons: showNodeIcons,
            sidebar_collapsed: sidebarCollapsed,
          }
        : current,
    );
    setSavingUiPreferences(true);
    try {
      setConfig(await setUiPreferences(showNodeIcons, sidebarCollapsed));
    } catch (cause) {
      setConfig(previous);
      setError(errorText(cause));
      toast.error("界面设置保存失败");
    } finally {
      setSavingUiPreferences(false);
    }
  }

  const connectDisabled =
    !targetNode ||
    Boolean(action) ||
    (online && targetNode.uuid === config?.last_select_node);

  const connectLabel =
    action === "connect"
      ? "正在切换…"
      : online && targetNode?.uuid === config?.last_select_node
        ? "当前已连接"
        : "连接此节点";

  if (!initialLoading && loggedOut && loginVisible) {
    return (
      <AppFrame>
        <LoginScreen
          initialEmail={config?.email}
          onSuccess={() => {
            setLoginVisible(false);
            void loadData(true);
          }}
          onSkip={() => setLoginVisible(false)}
        />
      </AppFrame>
    );
  }

  const page = sectionById(section);

  return (
    <AppFrame>
      {/* The explicit row keeps the grid area at the container's height so the
          scroll areas inside can actually overflow. */}
      <div
        className={cn(
          "grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)] overflow-hidden transition-[grid-template-columns] duration-200",
          config?.sidebar_collapsed
            ? "grid-cols-[72px_minmax(0,1fr)]"
            : "grid-cols-[228px_minmax(0,1fr)] max-[980px]:grid-cols-[190px_minmax(0,1fr)]",
        )}
      >
        <AppSidebar
          config={config}
          status={status}
          nodeCount={nodes.length}
          online={online}
          active={section}
          onNavigate={setSection}
          onLogout={() => void handleLogout()}
          onLogin={() => setLoginVisible(true)}
          collapsed={config?.sidebar_collapsed ?? false}
        />

        <ScrollArea className="h-full">
          <main className="mx-auto w-full max-w-[1280px] px-[38px] pt-8 pb-7 max-[980px]:px-6">
            <header className="mb-6 flex items-end justify-between gap-5">
              <div>
                <p className="m-0 mb-1 text-[11px] font-extrabold tracking-[0.18em] text-eyebrow">
                  {page.eyebrow}
                </p>
                <h1 className="m-0 text-[28px] font-bold tracking-[-0.035em] text-foreground">
                  {page.title}
                </h1>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                {section === "nodes" && (
                  <div className="relative w-[260px]">
                    <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
                    <Input
                      value={query}
                      onChange={(event) => setQuery(event.target.value)}
                      placeholder="搜索国家或 IP"
                      aria-label="搜索节点"
                      className="h-10 rounded-xl bg-surface-1 pl-9 text-xs text-card-foreground placeholder:text-muted-foreground/80 focus-visible:border-primary/40 focus-visible:bg-accent md:text-xs dark:bg-surface-1"
                    />
                  </div>
                )}
                {section !== "overview" && (
                  <Button
                    variant="outline"
                    size="icon"
                    disabled={refreshing}
                    onClick={() => void loadData()}
                    title="刷新数据"
                    aria-label="刷新数据"
                    className="size-10 rounded-xl bg-surface-2 text-muted-foreground hover:border-primary/45 hover:bg-accent hover:text-accent-foreground disabled:cursor-wait disabled:opacity-55"
                  >
                    <RefreshCw
                      className={cn("size-4", refreshing && "animate-spin")}
                    />
                  </Button>
                )}
              </div>
            </header>

            {error && (
              <Alert
                variant="destructive"
                className="mb-4 border-destructive/30 bg-destructive/10 pr-10 text-[11px]"
              >
                <TriangleAlert />
                <AlertDescription className="text-[11px] text-destructive-foreground [overflow-wrap:anywhere]">
                  {error}
                </AlertDescription>
                <AlertAction>
                  <button
                    type="button"
                    onClick={() => setError("")}
                    aria-label="关闭错误"
                    className="rounded-md p-1 text-destructive-foreground/75 transition-colors hover:bg-surface-2 hover:text-destructive-foreground"
                  >
                    <X className="size-3.5" />
                  </button>
                </AlertAction>
              </Alert>
            )}

            {loggedOut && (
              <Alert className="mb-4 border-primary/25 bg-primary/5 pr-24 text-[11px]">
                <LogIn className="text-primary" />
                <AlertDescription className="text-[11px] text-card-foreground">
                  当前以访客身份浏览，登录后才能获取节点列表并建立连接。
                </AlertDescription>
                <AlertAction>
                  <Button
                    size="xs"
                    onClick={() => setLoginVisible(true)}
                    className="h-7 rounded-lg px-3 text-[11px]"
                  >
                    去登录
                  </Button>
                </AlertAction>
              </Alert>
            )}

            {section === "overview" && (
              <>
                <ConnectionHero
                  phase={connection.phase}
                  progress={connection.progress}
                  stageLabel={connection.stageLabel}
                  stageCount={connection.stageCount}
                  busy={Boolean(action)}
                  config={config}
                  currentNode={currentNode}
                  onToggle={() => void toggleVpn()}
                  onModesChange={(captureMode, routeMode) =>
                    void handleModesChange(captureMode, routeMode)
                  }
                />

                <OverviewNodePicker
                  nodes={nodes}
                  measurements={measurements}
                  targetUuid={targetUuid}
                  currentUuid={config?.last_select_node}
                  online={online}
                  busy={Boolean(action)}
                  onSelect={setTargetUuid}
                  onConnect={() => void connectTarget()}
                  onViewAll={() => setSection("nodes")}
                  showIcons={config?.show_node_icons ?? false}
                />

                <StatCards
                  user={user}
                  nodeCount={nodes.length}
                  regionCount={regionCount}
                />
              </>
            )}

            {section === "nodes" && (
              <section>
                <ScrollArea orientation="horizontal" className="mb-2 pb-2.5">
                  <div className="flex gap-[5px]" aria-label="地区筛选">
                    {countries.map((item) => (
                      <Button
                        key={item}
                        variant="ghost"
                        size="xs"
                        onClick={() => setCountry(item)}
                        className={cn(
                          "h-auto shrink-0 rounded-lg border border-transparent px-3 py-2 text-[11px] font-medium",
                          country === item
                            ? "border-primary/25 bg-accent text-accent-foreground"
                            : "text-muted-foreground hover:bg-surface-2 hover:text-foreground",
                        )}
                      >
                        {item}
                      </Button>
                    ))}
                  </div>
                </ScrollArea>

                {initialLoading ? (
                  <div className="grid grid-cols-2 gap-3">
                    {Array.from({ length: 6 }).map((_, index) => (
                      <div
                        key={index}
                        className="grid min-h-[86px] grid-cols-[48px_minmax(0,1fr)_auto] items-center gap-3 rounded-2xl border bg-surface-1 p-4"
                      >
                        <Skeleton className="size-11 rounded-xl" />
                        <Skeleton className="h-2.5 w-full rounded-md" />
                        <Skeleton className="h-2.5 w-[55px] rounded-md" />
                      </div>
                    ))}
                  </div>
                ) : filteredNodes.length ? (
                  <div className="grid grid-cols-2 gap-3">
                    {filteredNodes.map((node) => (
                      <NodeCard
                        key={node.uuid}
                        node={node}
                        selected={node.uuid === targetUuid}
                        active={node.uuid === config?.last_select_node && online}
                        onSelect={setTargetUuid}
                        measurement={measurements[node.uuid]}
                        showIcon={config?.show_node_icons ?? false}
                      />
                    ))}
                  </div>
                ) : (
                  <div className="grid min-h-[190px] place-items-center content-center rounded-[15px] border border-dashed text-muted-foreground">
                    <Search className="size-6" />
                    <strong className="mt-2 text-[13px] text-card-foreground">
                      没有匹配的节点
                    </strong>
                    <p className="m-0 mt-1 text-[11px]">
                      换一个关键词或地区试试
                    </p>
                  </div>
                )}

                <ConnectBar
                  targetNode={targetNode}
                  disabled={connectDisabled}
                  label={connectLabel}
                  onConnect={() => void connectTarget()}
                  showIcon={config?.show_node_icons ?? false}
                />
              </section>
            )}

            {section === "rules" && <RuleEditor />}

            {section === "proxy-manager" && (
              <ProxyManager
                config={config}
                status={status}
                onConfigChanged={() => void loadData()}
              />
            )}

            {section === "settings" && (
              <>
                <OverviewActions
                  config={config}
                  user={user}
                  refreshing={refreshing}
                  onRefresh={() => void loadData()}
                  onHelp={() => setSection("proxy-manager")}
                />
                <SettingsPage
                  showNodeIcons={config?.show_node_icons ?? false}
                  sidebarCollapsed={config?.sidebar_collapsed ?? false}
                  saving={savingUiPreferences}
                  onChange={(showNodeIcons, sidebarCollapsed) =>
                    void handleUiPreferences(showNodeIcons, sidebarCollapsed)
                  }
                />
              </>
            )}

            <footer className="flex justify-between px-0.5 pt-6 pb-0.5 text-[10px] text-muted-foreground/60">
              <span>IKUN PX · Independent compatible client</span>
              <span>Rust 本地内核 · 凭据由 DPAPI 保护</span>
            </footer>
          </main>
        </ScrollArea>
      </div>
    </AppFrame>
  );
}

export default App;
