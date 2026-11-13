import { useEffect, useState } from "react";
import { Save } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { errorText } from "@/lib/format";
import { getUserRules, saveUserRules, type UserRules } from "@/lib/api";

export function RuleEditor() {
  const [rules, setRules] = useState<UserRules>({ proxy: "", direct: "" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    void getUserRules().then(setRules).catch(() => undefined);
  }, []);

  async function save() {
    setBusy(true);
    setError("");
    try {
      await saveUserRules(rules);
      toast.success("分流规则已保存并重新加载");
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section id="rules">
      <div className="mb-4 flex items-center justify-end">
        <Button
          onClick={() => void save()}
          disabled={busy}
          className="gap-2 bg-primary hover:brightness-110"
        >
          <Save className="size-3.5" />
          {busy ? "保存中…" : "保存规则"}
        </Button>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <label className="space-y-2">
          <span className="text-xs text-accent-foreground">强制代理（每行一个正则）</span>
          <textarea
            value={rules.proxy}
            onChange={(event) =>
              setRules((current) => ({ ...current, proxy: event.target.value }))
            }
            spellCheck={false}
            className="h-40 w-full resize-y rounded-xl border bg-surface-1 p-3 font-mono text-[10px] text-card-foreground outline-none focus:border-primary/40"
            placeholder={"^example\\.com$\n.*\\.google\\.com$"}
          />
        </label>
        <label className="space-y-2">
          <span className="text-xs text-online-foreground">强制直连（每行一个正则）</span>
          <textarea
            value={rules.direct}
            onChange={(event) =>
              setRules((current) => ({ ...current, direct: event.target.value }))
            }
            spellCheck={false}
            className="h-40 w-full resize-y rounded-xl border bg-surface-1 p-3 font-mono text-[10px] text-card-foreground outline-none focus:border-online/40"
            placeholder={"^localhost$\n.*\\.lan$"}
          />
        </label>
      </div>
      {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
    </section>
  );
}
