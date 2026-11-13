import { useEffect, useState, type FormEvent } from "react";
import { LockKeyhole, Mail } from "lucide-react";

import { BrandLogo } from "@/components/brand-logo";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { errorText } from "@/lib/format";
import { getSavedCredentials, login } from "@/lib/api";

interface LoginScreenProps {
  initialEmail?: string;
  onSuccess: () => void;
  onSkip?: () => void;
}

export function LoginScreen({ initialEmail = "", onSuccess, onSkip }: LoginScreenProps) {
  const [email, setEmail] = useState(initialEmail);
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  // Reading the DPAPI blob is a round trip, so anything the user managed to
  // type in the meantime wins over the stored value.
  useEffect(() => {
    let cancelled = false;
    void getSavedCredentials()
      .then((saved) => {
        if (cancelled) return;
        setEmail((current) => current || saved.email);
        setPassword((current) => current || saved.password);
        setRemember(saved.remember);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await login(email, password, remember);
      setPassword("");
      onSuccess();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="grid min-h-0 flex-1 place-items-center overflow-auto bg-[radial-gradient(circle_at_50%_0%,var(--page-glow),transparent_35%)] p-6">
      <Card className="w-full max-w-[420px] bg-popover shadow-[0_32px_100px_rgba(24,20,52,0.12)] dark:shadow-[0_32px_100px_rgba(0,0,0,0.45)]">
        <CardHeader className="items-center pt-3 text-center">
          <BrandLogo className="mb-3 size-14 rounded-2xl shadow-[0_10px_30px_rgba(103,78,255,0.3)]" />
          <CardTitle className="text-xl text-foreground">登录 IKUN PX</CardTitle>
          <CardDescription>
            使用 MarsLink 账号登录，凭据仅以 Windows DPAPI 加密保存在本机
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form className="space-y-4" onSubmit={submit}>
            <label className="block space-y-1.5">
              <span className="text-xs text-muted-foreground">邮箱</span>
              <span className="relative block">
                <Mail className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  type="email"
                  autoComplete="username"
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                  className="h-11 bg-surface-1 pl-10 dark:bg-surface-1"
                  required
                />
              </span>
            </label>
            <label className="block space-y-1.5">
              <span className="text-xs text-muted-foreground">密码</span>
              <span className="relative block">
                <LockKeyhole className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  type="password"
                  autoComplete="current-password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  className="h-11 bg-surface-1 pl-10 dark:bg-surface-1"
                  minLength={6}
                  required
                />
              </span>
            </label>
            <label className="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground">
              <input
                type="checkbox"
                checked={remember}
                onChange={(event) => setRemember(event.target.checked)}
                className="accent-primary"
              />
              在此 Windows 账户中加密保存密码
            </label>
            {error && (
              <p className="rounded-lg border border-destructive/25 bg-destructive/10 p-3 text-xs text-destructive-foreground">
                {error}
              </p>
            )}
            <Button
              type="submit"
              disabled={busy}
              className="h-11 w-full bg-[linear-gradient(135deg,#7963f3,#5a42cd)] font-semibold text-white"
            >
              {busy ? "正在登录…" : "登录"}
            </Button>
            {onSkip && (
              <Button
                type="button"
                variant="ghost"
                disabled={busy}
                onClick={onSkip}
                className="h-9 w-full text-xs font-normal text-muted-foreground hover:text-foreground"
              >
                暂不登录，先进入应用
              </Button>
            )}
          </form>
        </CardContent>
      </Card>
    </main>
  );
}
