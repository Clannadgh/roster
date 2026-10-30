import { useCallback, useEffect, useMemo, useState } from "react";
import { CalendarCheck, Loader2, Plus, Search, TriangleAlert, UserRound } from "lucide-react";
import { toast } from "sonner";
import { AccountCard } from "@/components/accounts/account-card";
import { AddAccountDialog } from "@/components/accounts/add-account-dialog";
import { EmptyState, Page, PageHeader, Section } from "@/components/page";
import { RegionSwitch } from "@/components/shell/region-switch";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import * as api from "@/lib/api";
import { platformLabel, regionLabel, splitScope } from "@/lib/scope";
import type { AccountMeta, CheckinStatus, CreditAccount, TravelStatus } from "@/lib/types";
import { useAccounts } from "@/lib/use-accounts";
import { useScope } from "@/lib/use-scope";

export default function CheckinPage() {
  const scope = useScope();
  const { platform, region } = splitScope(scope);
  const checkinSupported = !(platform === "workbuddy" && region === "global");
  const canCheckin = platform !== "workbuddy" || region === "cn";
  const canSwitch = true;
  const canSaveProfile = platform === "trae";
  const travelCapable = platform === "workbuddy" && region === "cn";
  const { accounts, loading, error, refresh } = useAccounts(scope);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [statuses, setStatuses] = useState<Record<string, CheckinStatus>>({});
  const [credits, setCredits] = useState<Record<string, CreditAccount>>({});
  const [travelStatuses, setTravelStatuses] = useState<Record<string, TravelStatus>>({});
  const [travelBusyId, setTravelBusyId] = useState<string | null>(null);
  const [checkingId, setCheckingId] = useState<string | null>(null);
  const [checkingAll, setCheckingAll] = useState(false);

  const filtered = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    if (!keyword) return accounts;
    return accounts.filter((account) =>
      [account.email, account.nickname, account.uid, account.enterpriseName]
        .filter((value): value is string => Boolean(value))
        .some((value) => value.toLowerCase().includes(keyword)),
    );
  }, [accounts, query]);

  const loadStatuses = useCallback(async () => {
    if (!canCheckin || accounts.length === 0) {
      setStatuses({});
      return;
    }
    const entries = await Promise.all(
      accounts.map(async (account) => {
        try {
          return [account.id, await api.checkinStatus(scope, account.id)] as const;
        } catch {
          return [account.id, null] as const;
        }
      }),
    );
    const next: Record<string, CheckinStatus> = {};
    for (const [id, status] of entries) {
      if (status) next[id] = status;
    }
    setStatuses(next);
  }, [accounts, canCheckin, scope]);

  useEffect(() => {
    void loadStatuses();
  }, [loadStatuses]);

  const loadCredits = useCallback(async () => {
    if (accounts.length === 0) {
      setCredits({});
      return;
    }
    try {
      const summary = await api.getCreditsSummary(scope);
      const next: Record<string, CreditAccount> = {};
      for (const item of summary.accounts) {
        next[item.accountId] = item;
      }
      setCredits(next);
    } catch {
      setCredits({});
    }
  }, [accounts.length, scope]);

  useEffect(() => {
    void loadCredits();
  }, [loadCredits]);

  const loadTravelStatuses = useCallback(async () => {
    if (!travelCapable || accounts.length === 0) {
      setTravelStatuses({});
      return;
    }
    const entries = await Promise.all(
      accounts.map(async (account) => {
        try {
          return [account.id, await api.getTravelStatus(scope, account.id)] as const;
        } catch {
          return [account.id, null] as const;
        }
      }),
    );
    const next: Record<string, TravelStatus> = {};
    for (const [id, status] of entries) {
      if (status) next[id] = status;
    }
    setTravelStatuses(next);
  }, [accounts, travelCapable, scope]);

  useEffect(() => {
    void loadTravelStatuses();
  }, [loadTravelStatuses]);

  useEffect(() => {
    if (!travelCapable) return;
    const timer = window.setInterval(() => {
      void loadTravelStatuses();
    }, 60_000);
    return () => window.clearInterval(timer);
  }, [travelCapable, loadTravelStatuses]);

  const handleAdded = useCallback(
    (_account: AccountMeta) => {
      void refresh();
    },
    [refresh],
  );

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await api.deleteAccount(scope, id);
        toast.success("账号已删除");
        void refresh();
      } catch (cause) {
        toast.error("删除失败", { description: api.asError(cause) });
      }
    },
    [scope, refresh],
  );

  const handleCheckin = useCallback(
    async (account: AccountMeta) => {
      setCheckingId(account.id);
      try {
        const outcome = await api.checkinAccount(scope, account.id);
        if (outcome.result === "success") {
          toast.success("签到成功", {
            description: outcome.amount ? `+${outcome.amount} 积分` : undefined,
          });
        } else if (outcome.result === "already") {
          toast.info("今天已经签过了");
        } else if (outcome.result === "unsupported") {
          toast.info(outcome.error ?? "该区域不支持签到");
        } else {
          toast.error("签到失败", { description: outcome.error ?? undefined });
        }
        if (outcome.result === "success" || outcome.result === "already") {
          try {
            const status = await api.checkinStatus(scope, account.id);
            setStatuses((prev) => ({ ...prev, [account.id]: status }));
          } catch {
            // 状态刷新失败不影响签到结果。
          }
          void loadCredits();
        }
      } catch (cause) {
        toast.error("签到失败", { description: api.asError(cause) });
      } finally {
        setCheckingId(null);
      }
    },
    [scope, loadCredits],
  );

  const handleCheckinAll = useCallback(async () => {
    setCheckingAll(true);
    try {
      const outcomes = await api.checkinAll(scope);
      const success = outcomes.filter((outcome) => outcome.result === "success").length;
      const already = outcomes.filter((outcome) => outcome.result === "already").length;
      const failed = outcomes.filter((outcome) => outcome.result === "error").length;
      const summary = `成功 ${success}，已签 ${already}${failed ? `，失败 ${failed}` : ""}`;
      if (failed > 0) {
        toast.error("签到完成，有失败", { description: summary });
      } else {
        toast.success("签到完成", { description: summary });
      }
      await loadStatuses();
      void loadCredits();
    } catch (cause) {
      toast.error("一键签到失败", { description: api.asError(cause) });
    } finally {
      setCheckingAll(false);
    }
  }, [scope, loadStatuses, loadCredits]);

  const handleDialogChange = useCallback((open: boolean) => setDialogOpen(open), []);
  const openDialog = useCallback(() => setDialogOpen(true), []);

  const handleTravel = useCallback(
    async (account: AccountMeta) => {
      const claim = travelStatuses[account.id]?.state === "arrived";
      setTravelBusyId(account.id);
      try {
        const next = claim
          ? await api.travelClaim(scope, account.id)
          : await api.travelDepart(scope, account.id);
        setTravelStatuses((prev) => ({ ...prev, [account.id]: next }));
        if (claim) {
          toast.success("旅行奖励已领取", {
            description:
              next.rewardCredit !== null ? `+${next.rewardCredit} 积分` : undefined,
          });
          void loadCredits();
        } else if (next.state === "traveling") {
          toast.success("Buddy 已出发", { description: next.locationName ?? undefined });
        } else if (next.state === "no_buddy") {
          toast.info("该账号还没有 Buddy", {
            description: "先在 WorkBuddy 成长中心领养 Buddy 后再旅行",
          });
        } else if (next.state === "unsupported") {
          toast.info("该账号不支持成长中心", {
            description: "企业/教育账号没有派猫猫旅行",
          });
        } else if (next.dailyLimitReached) {
          toast.info("今日旅行已完成");
        }
      } catch (cause) {
        toast.error(claim ? "领取失败" : "出发失败", { description: api.asError(cause) });
      } finally {
        setTravelBusyId(null);
      }
    },
    [scope, travelStatuses, loadCredits],
  );

  const handleSwitch = useCallback(
    async (account: AccountMeta) => {
      try {
        await api.switchAccount(scope, account.id);
        toast.success("已切换账号", {
          description: account.nickname ?? account.email ?? undefined,
        });
        void refresh();
      } catch (cause) {
        toast.error("切换失败", { description: api.asError(cause) });
      }
    },
    [scope, refresh],
  );

  const handleSaveProfile = useCallback(
    async (account: AccountMeta) => {
      try {
        const count = await api.saveTraeProfile(scope, account.id);
        toast.success("登录态已保存", {
          description: `已复制 ${count} 项到该账号的快照`,
        });
      } catch (cause) {
        toast.error("保存登录态失败", { description: api.asError(cause) });
      }
    },
    [scope],
  );

  return (
    <Page>
      <PageHeader
        title="签到"
        description={
          checkinSupported
            ? `${platformLabel(platform)} · ${regionLabel(region)} · 每日签到、积分与自动排程`
            : `${platformLabel(platform)} · ${regionLabel(region)} · 上游没有签到接口，账号仍可管理`
        }
        actions={
          <div className="flex items-center gap-2">
            {checkinSupported ? null : <Badge variant="warning">该区域无签到接口</Badge>}
            <RegionSwitch />
          </div>
        }
      />

      <Section
        title="账号"
        action={
          accounts.length > 0 ? (
            <span className="text-[11px] tabular-nums text-muted-foreground">
              {accounts.length} 个
            </span>
          ) : null
        }
      >
        {checkinSupported ? null : (
          <div className="flex items-start gap-2 rounded-xl border border-amber-500/25 bg-amber-500/[0.06] px-4 py-3 text-xs leading-5 text-amber-700 dark:text-amber-300">
            <TriangleAlert className="mt-0.5 size-4 shrink-0" />
            <span>
              WorkBuddy 国际版没有签到接口（上游直接返回 unsupported）。账号可以正常添加与管理，签到按钮会保持禁用。
            </span>
          </div>
        )}

        <Card className="min-w-0 gap-0 overflow-hidden rounded-2xl py-0 shadow-none">
          <div className="flex flex-wrap items-center gap-2 border-b border-border/60 px-3 py-2.5">
            <div className="relative min-w-[180px] max-w-[280px] flex-1">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="搜索账号名、邮箱、备注…"
                className="h-8 rounded-md border-0 bg-muted/40 pl-8 text-xs shadow-none"
              />
            </div>
            <div className="ml-auto flex items-center gap-2">
              {canCheckin && accounts.length > 0 ? (
                <Button
                  size="sm"
                  variant="outline"
                  className="h-8 gap-1.5 text-xs"
                  disabled={checkingAll}
                  onClick={() => void handleCheckinAll()}
                >
                  {checkingAll ? (
                    <Loader2 className="size-3.5 animate-spin" />
                  ) : (
                    <CalendarCheck className="size-3.5" />
                  )}
                  一键签到
                </Button>
              ) : null}
              <Button size="sm" className="h-8 gap-1.5 text-xs" onClick={openDialog}>
                <Plus className="size-3.5" />
                添加账号
              </Button>
            </div>
          </div>

          <div className="p-4">
            {loading ? (
              <p className="py-10 text-center text-xs text-muted-foreground">正在读取账号…</p>
            ) : error ? (
              <p className="py-10 text-center text-xs text-destructive">{error}</p>
            ) : filtered.length === 0 ? (
              <EmptyState
                icon={UserRound}
                title={accounts.length === 0 ? "还没有账号" : "没有匹配的账号"}
                description={
                  accounts.length > 0
                    ? "换一个关键字试试。"
                    : platform === "trae"
                      ? "点击「添加账号」，粘贴 Trae 客户端登录态里的 Cloud-IDE-JWT 即可导入。"
                      : `点击「添加账号」，在浏览器中完成 ${platformLabel(platform)} 登录授权；账号会出现在这里，并显示签到状态与积分余额。`
                }
                action={
                  accounts.length === 0 ? (
                    <Button size="sm" onClick={openDialog}>
                      <Plus className="size-3.5" />
                      添加第一个账号
                    </Button>
                  ) : undefined
                }
              />
            ) : (
              <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
                {filtered.map((account) => (
                  <AccountCard
                    key={account.id}
                    account={account}
                    credits={credits[account.id]}
                    travelStatus={travelCapable ? travelStatuses[account.id] : undefined}
                    travelBusy={travelBusyId === account.id}
                    onTravel={handleTravel}
                    onDelete={handleDelete}
                    status={statuses[account.id]}
                    canCheckin={canCheckin}
                    checkingIn={checkingId === account.id}
                    onCheckin={handleCheckin}
                    canSwitch={canSwitch}
                    onSwitch={handleSwitch}
                    canSaveProfile={canSaveProfile}
                    onSaveProfile={handleSaveProfile}
                  />
                ))}
              </div>
            )}
          </div>
        </Card>
      </Section>

      <AddAccountDialog
        open={dialogOpen}
        onOpenChange={handleDialogChange}
        scope={scope}
        platform={platform}
        region={region}
        onAdded={handleAdded}
      />
    </Page>
  );
}
