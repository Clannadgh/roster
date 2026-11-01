import { useState } from "react";
import {
  CalendarCheck,
  Check,
  ChevronDown,
  Coins,
  Loader2,
  Settings,
  Zap,
  type LucideIcon,
} from "lucide-react";
import { NavLink, useLocation, useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { AppMark } from "@/components/app-mark";
import { PlatformIcon } from "@/components/shell/platform-icon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import * as api from "@/lib/api";
import {
  PLATFORMS,
  platformLabel,
  regionLabel,
  scopeOf,
  splitScope,
  type Destination,
  type Platform,
  type Scope,
} from "@/lib/scope";
import type { GlobalCheckinSummary } from "@/lib/types";
import { cn } from "@/lib/utils";

const NAV_ITEMS: { id: Destination; label: string; icon: LucideIcon }[] = [
  { id: "checkin", label: "签到", icon: CalendarCheck },
  { id: "credits", label: "积分统计", icon: Coins },
  { id: "settings", label: "设置", icon: Settings },
];

export function Sidebar({ scope }: { scope: Scope }) {
  const [checkingAll, setCheckingAll] = useState(false);
  const [result, setResult] = useState<GlobalCheckinSummary | null>(null);
  const [resultOpen, setResultOpen] = useState(false);
  const { platform, region } = splitScope(scope);
  const location = useLocation();
  const navigate = useNavigate();
  const currentDestination = (location.pathname.split("/")[3] || "checkin") as Destination;

  function switchPlatform(next: Platform) {
    const nextScope = scopeOf(next, region);
    navigate(`/${nextScope}/${currentDestination}`);
  }

  async function checkinAllScopes() {
    setCheckingAll(true);
    try {
      const summary = await api.checkinAllScopes();
      window.dispatchEvent(new Event("roster:checkin-all"));
      if (summary.scopes.length === 0) {
        toast.info("没有可签到的账号");
        return;
      }
      setResult(summary);
      setResultOpen(true);
    } catch (cause) {
      toast.error("全平台签到失败", { description: api.asError(cause) });
    } finally {
      setCheckingAll(false);
    }
  }

  return (
    <aside className="flex min-h-0 w-[220px] shrink-0 flex-col border-r border-sidebar-border bg-sidebar px-3 pb-4 pt-4">
      <div className="flex items-center gap-2.5 px-1 pb-5">
        <AppMark size={36} />
        <div className="min-w-0">
          <div
            className="truncate text-[15px] leading-5 tracking-[-0.02em] text-sidebar-foreground/90"
            style={{ fontWeight: 640 }}
          >
            Roster
          </div>
          <div className="truncate text-[11px] leading-4 text-sidebar-foreground/50">
            多账号工作台
          </div>
        </div>
      </div>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="flex w-full items-center gap-2 rounded-lg border border-border bg-background px-2.5 py-2 text-sm transition-colors hover:bg-accent"
          >
            <PlatformIcon platform={platform} size={20} />
            <span className="min-w-0 flex-1 truncate text-left">{platformLabel(platform)}</span>
            <ChevronDown className="size-3.5 text-muted-foreground" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-[196px]">
          {PLATFORMS.map((item) => (
            <DropdownMenuItem
              key={item.id}
              className="gap-2"
              onSelect={() => switchPlatform(item.id)}
            >
              <PlatformIcon platform={item.id} size={20} />
              <span className="flex-1">{item.label}</span>
              {item.id === platform ? <Check className="size-4" /> : null}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <nav className="mt-4 flex min-h-0 flex-1 flex-col gap-0.5">
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          return (
            <NavLink
              key={item.id}
              to={`/${scope}/${item.id}`}
              className={({ isActive }) =>
                cn(
                  "flex items-center gap-2.5 rounded-lg px-3 py-2.5 text-sm outline-none transition-colors",
                  isActive
                    ? "bg-foreground/[0.06] font-medium text-foreground"
                    : "text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground",
                )
              }
            >
              <Icon className="size-4" />
              {item.label}
            </NavLink>
          );
        })}
      </nav>

      <div className="pt-3">
        <Button
          className="h-10 w-full shadow-none"
          disabled={checkingAll}
          onClick={() => void checkinAllScopes()}
          title="签到 WorkBuddy / Trae / Qoder 的国内版和国际版全部账号"
        >
          {checkingAll ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <Zap className="size-4" />
          )}
          {checkingAll ? "正在签到…" : "全部签到"}
        </Button>
      </div>

      <Dialog open={resultOpen} onOpenChange={setResultOpen}>
        <DialogContent className="sm:max-w-[440px]">
          <DialogHeader>
            <DialogTitle>签到结果</DialogTitle>
            <DialogDescription>按平台与区域汇总本次全平台签到</DialogDescription>
          </DialogHeader>
          {result ? (
            <div className="space-y-1.5">
              {result.scopes.map((row) => {
                const parsed = splitScope(row.scope as Scope);
                return (
                  <div
                    key={row.scope}
                    className="flex items-center gap-3 rounded-lg border border-border/60 px-3 py-2"
                  >
                    <PlatformIcon platform={parsed.platform} size={22} />
                    <div className="min-w-0 flex-1">
                      <div className="text-sm font-medium leading-5">
                        {platformLabel(parsed.platform)}
                      </div>
                      <div className="text-[11px] leading-4 text-muted-foreground">
                        {regionLabel(parsed.region)}
                      </div>
                    </div>
                    <div className="flex flex-wrap items-center justify-end gap-1">
                      {row.success > 0 ? (
                        <Badge variant="success">成功 {row.success}</Badge>
                      ) : null}
                      {row.already > 0 ? (
                        <Badge variant="secondary">已签 {row.already}</Badge>
                      ) : null}
                      {row.failed > 0 ? (
                        <Badge variant="destructive">失败 {row.failed}</Badge>
                      ) : null}
                      {row.skipped > 0 ? (
                        <Badge variant="outline">跳过 {row.skipped}</Badge>
                      ) : null}
                    </div>
                  </div>
                );
              })}
              <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3 text-xs tabular-nums text-muted-foreground">
                <span>
                  共 {result.success + result.already + result.failed + result.skipped} 个账号
                </span>
                <span>
                  成功 {result.success} · 已签 {result.already} · 失败 {result.failed}
                  {result.skipped > 0 ? ` · 跳过 ${result.skipped}` : ""}
                </span>
              </div>
            </div>
          ) : null}
        </DialogContent>
      </Dialog>
    </aside>
  );
}
