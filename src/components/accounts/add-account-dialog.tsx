import { useEffect, useRef, useState } from "react";
import { Check, Copy, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import * as api from "@/lib/api";
import { platformLabel, regionLabel, type Platform, type Region } from "@/lib/scope";
import type { AccountMeta } from "@/lib/types";

type Phase = "idle" | "starting" | "waiting" | "error";

async function openInBrowser(url: string): Promise<void> {
  if (!api.isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

export function AddAccountDialog({
  open,
  onOpenChange,
  scope,
  platform,
  region,
  onAdded,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  scope: string;
  platform: Platform;
  region: Region;
  onAdded: (account: AccountMeta) => void;
}) {
  const isTrae = platform === "trae";
  const [phase, setPhase] = useState<Phase>("idle");
  const [uri, setUri] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [jwtText, setJwtText] = useState("");
  const [jwtError, setJwtError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const loginIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (!open) {
      setPhase("idle");
      setUri(null);
      setMessage(null);
      setCopied(false);
      setJwtText("");
      setJwtError(null);
      loginIdRef.current = null;
      return;
    }
    if (isTrae) {
      setPhase("idle");
      return;
    }

    let cancelled = false;
    let timer: number | undefined;

    async function start() {
      try {
        setPhase("starting");
        setUri(null);
        setMessage(null);
        setCopied(false);
        const started = await api.loginStart(scope);
        if (cancelled) return;
        loginIdRef.current = started.loginId;
        setUri(started.verificationUri);
        setPhase("waiting");
        // 国内版直接唤起系统浏览器；国际版不自动打开，交给用户用无痕窗口打开。
        if (region === "cn") {
          try {
            await openInBrowser(started.verificationUri);
          } catch {
            // 自动打开失败不终止：弹窗里有链接和「在浏览器中打开」按钮。
          }
        }
        timer = window.setInterval(async () => {
          const loginId = loginIdRef.current;
          if (!loginId || cancelled) return;
          try {
            const result = await api.loginPoll(scope, loginId);
            if (cancelled || !result.done) return;
            if (timer) window.clearInterval(timer);
            if (result.error) {
              setPhase("error");
              setMessage(result.error);
              return;
            }
            if (result.result) {
              toast.success("账号已添加", {
                description: result.result.email ?? result.result.nickname ?? undefined,
              });
              onAdded(result.result);
              onOpenChange(false);
            }
          } catch {
            // 单次轮询失败不终止流程，下一轮继续。
          }
        }, 2500);
      } catch (cause) {
        if (cancelled) return;
        setPhase("error");
        setMessage(api.asError(cause));
      }
    }

    void start();
    return () => {
      cancelled = true;
      if (timer) window.clearInterval(timer);
    };
  }, [open, isTrae, scope, region, attempt, onAdded, onOpenChange]);

  async function copyLink() {
    if (!uri) return;
    try {
      await navigator.clipboard.writeText(uri);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error("复制链接失败", { description: "请手动选中链接复制。" });
    }
  }

  async function submitJwt() {
    setSubmitting(true);
    setJwtError(null);
    try {
      const account = await api.importTraeJwt(scope, jwtText);
      toast.success("账号已导入", {
        description: account.nickname ?? account.uid ?? undefined,
      });
      onAdded(account);
      onOpenChange(false);
    } catch (cause) {
      setJwtError(api.asError(cause));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>添加 {platformLabel(platform)} 账号</DialogTitle>
          <DialogDescription>
            {platformLabel(platform)} · {regionLabel(region)} ·{" "}
            {isTrae ? "粘贴 JWT 导入" : "浏览器登录授权"}
          </DialogDescription>
        </DialogHeader>

        {isTrae ? (
          <div className="min-w-0 space-y-3 py-1">
            <Textarea
              value={jwtText}
              onChange={(event) => setJwtText(event.target.value)}
              placeholder="粘贴 Cloud-IDE-JWT ...（可带前缀，也可只贴裸 token）"
              wrap="soft"
              className="max-h-[40vh] min-h-[120px] max-w-full min-w-0 font-mono text-xs break-all"
            />
            <Button
              className="w-full"
              disabled={!jwtText.trim() || submitting}
              onClick={() => void submitJwt()}
            >
              {submitting ? "正在导入…" : "导入账号"}
            </Button>
            {jwtError ? (
              <p className="text-xs leading-5 text-destructive">{jwtError}</p>
            ) : (
              <p className="text-xs leading-5 text-muted-foreground">
                JWT 取自 Trae 客户端的登录态（Cloud-IDE-JWT）。导入时只解析载荷里的用户 ID 与到期时间，不校验签名。
              </p>
            )}
          </div>
        ) : (
          <div className="space-y-3 py-1">
            {phase === "starting" ? (
              <div className="flex items-center justify-center gap-2 rounded-xl border border-dashed border-border px-4 py-12 text-sm text-muted-foreground">
                <Loader2 className="size-4 animate-spin" />
                正在发起登录…
              </div>
            ) : null}

            {phase === "waiting" && uri ? (
              <>
                {platform === "workbuddy" && region === "global" ? (
                  <Alert variant="warning">
                    <AlertTitle>建议用无痕窗口打开</AlertTitle>
                    <AlertDescription>
                      若浏览器已登录 workbuddy.ai，授权页可能直接跳到「登录成功」而不绑定账号。请复制链接到浏览器的无痕（隐私）窗口完成登录。
                    </AlertDescription>
                  </Alert>
                ) : null}
                <Alert>
                  <ExternalLink className="size-4" />
                  <AlertDescription className="break-all">
                    <button
                      type="button"
                      className="text-left text-primary underline-offset-2 hover:underline"
                      onClick={() => void openInBrowser(uri)}
                    >
                      {uri}
                    </button>
                  </AlertDescription>
                </Alert>
                <div className="flex gap-2">
                  <Button
                    size="sm"
                    variant="outline"
                    className="flex-1 gap-1.5"
                    onClick={() => void openInBrowser(uri)}
                  >
                    <ExternalLink className="size-3.5" />
                    在浏览器中打开
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    className="flex-1 gap-1.5"
                    onClick={() => void copyLink()}
                  >
                    {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
                    {copied ? "已复制" : "复制链接"}
                  </Button>
                </div>
                <p className="text-center text-xs leading-5 text-muted-foreground">
                  请在浏览器中完成登录授权，成功后账号会自动添加。
                </p>
              </>
            ) : null}

            {phase === "error" ? (
              <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-border px-4 py-8">
                <p className="text-center text-xs leading-5 text-destructive">
                  {message ?? "登录失败"}
                </p>
                <Button
                  size="sm"
                  variant="outline"
                  className="gap-1.5"
                  onClick={() => setAttempt((value) => value + 1)}
                >
                  <RefreshCw className="size-3.5" />
                  重新发起登录
                </Button>
              </div>
            ) : null}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
