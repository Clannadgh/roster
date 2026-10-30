import { useState } from "react";
import { Loader2, Save, Trash2 } from "lucide-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type { AccountMeta, CheckinStatus, CreditAccount } from "@/lib/types";
import { cn } from "@/lib/utils";

function displayName(account: AccountMeta): string {
  return account.email || account.nickname || account.uid || "未知账号";
}

function subtitle(account: AccountMeta): string {
  const parts = [
    account.nickname && account.email ? account.nickname : null,
    account.enterpriseName,
    account.domain,
  ].filter((value): value is string => Boolean(value));
  return parts.length > 0 ? parts.join(" · ") : "—";
}

function expiryText(account: AccountMeta): string {
  if (!account.expiresAt) return "凭据无过期时间";
  return `凭据有效期至 ${new Date(account.expiresAt).toLocaleDateString("zh-CN")}`;
}

function formatAmount(value: number): string {
  return value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
}

function shortDate(millis: number): string {
  const date = new Date(millis);
  return date.toLocaleDateString(
    "zh-CN",
    date.getFullYear() === new Date().getFullYear()
      ? { month: "2-digit", day: "2-digit" }
      : { year: "numeric", month: "2-digit", day: "2-digit" },
  );
}

export function AccountCard({
  account,
  credits,
  onDelete,
  status,
  canCheckin = false,
  checkingIn = false,
  onCheckin,
  canSwitch = false,
  onSwitch,
  canSaveProfile = false,
  onSaveProfile,
}: {
  account: AccountMeta;
  credits?: CreditAccount;
  onDelete: (id: string) => void;
  status?: CheckinStatus | null;
  canCheckin?: boolean;
  checkingIn?: boolean;
  onCheckin?: (account: AccountMeta) => void;
  canSwitch?: boolean;
  onSwitch?: (account: AccountMeta) => void;
  canSaveProfile?: boolean;
  onSaveProfile?: (account: AccountMeta) => void;
}) {
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [switchOpen, setSwitchOpen] = useState(false);
  const initial = (account.email || account.nickname || account.uid || "?")
    .slice(0, 1)
    .toUpperCase();
  const remaining = credits?.ok ? credits.remaining : null;

  return (
    <Card
      className={cn(
        "group min-w-0 gap-0 overflow-hidden rounded-2xl py-0 shadow-none transition-colors hover:bg-accent/40",
        account.isCurrent && "border-primary/45 ring-1 ring-primary/10",
      )}
    >
      <div className="flex items-start gap-3 p-4">
        <div className="flex size-10 shrink-0 items-center justify-center rounded-full bg-primary/12 text-sm font-semibold text-primary">
          {initial}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate text-sm font-medium">{displayName(account)}</span>
            {account.isCurrent ? (
              <Badge className="shrink-0 gap-1 px-1.5 text-[10px]">
                <span className="size-1.5 rounded-full bg-primary-foreground/80" />
                当前登录
              </Badge>
            ) : null}
          </div>
          <div className="mt-0.5 truncate text-xs text-muted-foreground">{subtitle(account)}</div>
        </div>
        <div className="flex shrink-0 items-center gap-0.5 opacity-60 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
          {canSaveProfile ? (
            <Button
              variant="ghost"
              size="icon"
              className="size-7 text-muted-foreground"
              title="保存当前登录态"
              onClick={() => onSaveProfile?.(account)}
            >
              <Save className="size-3.5" />
            </Button>
          ) : null}
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-muted-foreground hover:text-destructive"
            title="删除账号"
            onClick={() => setConfirmOpen(true)}
          >
            <Trash2 className="size-3.5" />
          </Button>
        </div>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 px-4 py-2.5">
        <div className="min-w-0 space-y-0.5" title={expiryText(account)}>
          {remaining !== null && remaining !== undefined ? (
            <div className="truncate text-[12px] leading-4 tabular-nums text-muted-foreground">
              积分 <span className="font-medium text-foreground">{formatAmount(remaining)}</span>
              {credits?.total !== null && credits?.total !== undefined ? (
                <span> / {formatAmount(credits.total)}</span>
              ) : null}
            </div>
          ) : (
            <div
              className="truncate text-[11px] leading-4 text-muted-foreground"
              title={credits?.error ?? undefined}
            >
              {credits?.error ? "积分读取失败" : expiryText(account)}
            </div>
          )}
          {remaining !== null &&
          remaining !== undefined &&
          credits?.expiresAt &&
          credits.expiring !== null ? (
            <div className="truncate text-[11px] leading-4 tabular-nums text-amber-600 dark:text-amber-400">
              {formatAmount(credits.expiring)} 积分 {shortDate(credits.expiresAt)} 到期
            </div>
          ) : null}
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          {status?.ok ? (
            <Badge variant={status.todayCheckedIn ? "success" : "outline"}>
              {status.todayCheckedIn ? "今日已签到" : "今日未签到"}
            </Badge>
          ) : null}
          {account.needsRelogin ? <Badge variant="warning">需重新登录</Badge> : null}
          {canCheckin && !status?.todayCheckedIn ? (
            <Button
              size="sm"
              variant="outline"
              className="h-7 gap-1 px-2 text-[11px]"
              disabled={checkingIn}
              onClick={() => onCheckin?.(account)}
            >
              {checkingIn ? <Loader2 className="size-3 animate-spin" /> : null}
              签到
            </Button>
          ) : null}
          {canSwitch && !account.isCurrent ? (
            <Button
              size="sm"
              variant="outline"
              className="h-7 px-2 text-[11px]"
              onClick={() => setSwitchOpen(true)}
            >
              切换
            </Button>
          ) : null}
        </div>
      </div>

      <AlertDialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除账号？</AlertDialogTitle>
            <AlertDialogDescription>
              将从本机账号库移除 {displayName(account)}，其登录凭据一并删除。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction onClick={() => onDelete(account.id)}>删除</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={switchOpen} onOpenChange={setSwitchOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>切换到这个账号？</AlertDialogTitle>
            <AlertDialogDescription>
              将把客户端登录态切换为 {displayName(account)}。切换前会自动备份当前登录态；请先退出对应客户端再确认。Trae 需要先保存该账号的登录态快照。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction onClick={() => onSwitch?.(account)}>切换</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  );
}
