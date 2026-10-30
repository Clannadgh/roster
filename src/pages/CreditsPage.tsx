import { useCallback, useEffect, useState } from "react";
import { Coins, Loader2, RefreshCw } from "lucide-react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { EmptyState, Page, PageHeader, Section } from "@/components/page";
import { RegionSwitch } from "@/components/shell/region-switch";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import * as api from "@/lib/api";
import { DISPLAY_FONT } from "@/lib/fonts";
import { platformLabel, regionLabel, splitScope } from "@/lib/scope";
import type { CreditsSummary } from "@/lib/types";
import { useScope } from "@/lib/use-scope";

function formatAmount(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  return value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
}

export default function CreditsPage() {
  const scope = useScope();
  const { platform, region } = splitScope(scope);
  const [summary, setSummary] = useState<CreditsSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setError(null);
      setSummary(await api.getCreditsSummary(scope));
    } catch (cause) {
      setError(api.asError(cause));
      setSummary(null);
    } finally {
      setLoading(false);
    }
  }, [scope]);

  useEffect(() => {
    setLoading(true);
    void load();
  }, [load]);

  async function refresh() {
    setRefreshing(true);
    try {
      await load();
    } finally {
      setRefreshing(false);
    }
  }

  const tiles = [
    { label: "剩余积分", value: summary?.totalRemaining },
    { label: "今日消耗", value: summary?.today },
    { label: "近 7 天消耗", value: summary?.week },
    { label: "本月消耗", value: summary?.month },
  ];

  return (
    <Page>
      <PageHeader
        title="积分统计"
        description={`${platformLabel(platform)} · ${regionLabel(region)} · 余额、消耗与到期`}
        actions={
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              className="h-8 gap-1.5 text-xs"
              disabled={refreshing || loading}
              onClick={() => void refresh()}
            >
              {refreshing ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <RefreshCw className="size-3.5" />
              )}
              刷新
            </Button>
            <RegionSwitch />
          </div>
        }
      />

      {loading ? (
        <p className="py-16 text-center text-xs text-muted-foreground">正在读取积分…</p>
      ) : error ? (
        <p className="py-16 text-center text-xs text-destructive">{error}</p>
      ) : !summary || summary.accounts.length === 0 ? (
        <Card className="min-w-0 gap-0 overflow-hidden rounded-2xl py-0 shadow-none">
          <div className="p-4">
            <EmptyState
              icon={Coins}
              title="还没有账号"
              description="先在「签到」页添加账号，这里会显示余额、到期与消耗趋势。"
            />
          </div>
        </Card>
      ) : (
        <div className="space-y-12">
          <Section title="概览">
            <Card className="min-w-0 gap-0 overflow-hidden rounded-2xl bg-card/70 py-0 shadow-none">
              <div className="grid grid-cols-1 divide-y divide-border/60 sm:grid-cols-4 sm:divide-y-0">
                {tiles.map((tile) => (
                  <div key={tile.label} className="px-5 py-4">
                    <div className="text-[13px] font-medium leading-5 text-muted-foreground">
                      {tile.label}
                    </div>
                    <div
                      className="mt-2 text-[26px] font-semibold leading-8 tracking-[-0.025em] tabular-nums"
                      style={{ fontFamily: DISPLAY_FONT }}
                    >
                      {formatAmount(tile.value)}
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          </Section>

          <Section title="近 30 天消耗">
            <Card className="min-w-0 gap-0 overflow-hidden rounded-2xl py-0 shadow-none">
              <div className="h-[240px] w-full px-3 py-4">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={summary.daily} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                    <CartesianGrid vertical={false} strokeDasharray="3 3" stroke="var(--border)" />
                    <XAxis
                      dataKey="date"
                      tickFormatter={(value: string) => value.slice(5)}
                      tickLine={false}
                      axisLine={false}
                      fontSize={11}
                      interval="preserveStartEnd"
                    />
                    <YAxis tickLine={false} axisLine={false} fontSize={11} width={56} />
                    <Tooltip
                      formatter={(value) => [`${formatAmount(Number(value))} 积分`, "消耗"]}
                      labelFormatter={(label) => String(label)}
                      contentStyle={{
                        borderRadius: 10,
                        border: "1px solid var(--border)",
                        background: "var(--popover)",
                        color: "var(--popover-foreground)",
                        fontSize: 12,
                      }}
                    />
                    <Bar dataKey="amount" fill="var(--primary)" radius={[4, 4, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </Card>
          </Section>

          <Section
            title="账号明细"
            action={
              <span className="text-[11px] text-muted-foreground">
                成功 {summary.okAccounts} · 失败 {summary.failedAccounts}
              </span>
            }
          >
            <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
              {summary.accounts.map((account) => (
                <div
                  key={account.accountId}
                  className="flex flex-wrap items-center justify-between gap-3 border-b border-border/50 px-4 py-3 last:border-b-0"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-sm font-medium">{account.name}</span>
                      {account.plan ? <Badge variant="outline">{account.plan}</Badge> : null}
                      {!account.ok ? <Badge variant="warning">读取失败</Badge> : null}
                    </div>
                    <div className="mt-0.5 truncate text-xs text-muted-foreground">
                      {account.error
                        ? account.error
                        : account.expiresAt
                          ? `最近到期：${new Date(account.expiresAt).toLocaleDateString("zh-CN")}`
                          : "—"}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-6 text-right">
                    <div>
                      <div className="text-[11px] text-muted-foreground">剩余 / 总量</div>
                      <div className="text-sm tabular-nums">
                        {formatAmount(account.remaining)}
                        <span className="text-muted-foreground"> / {formatAmount(account.total)}</span>
                      </div>
                    </div>
                    <div>
                      <div className="text-[11px] text-muted-foreground">今日消耗</div>
                      <div className="text-sm tabular-nums">{formatAmount(account.today)}</div>
                    </div>
                  </div>
                </div>
              ))}
            </Card>
          </Section>
        </div>
      )}
    </Page>
  );
}
