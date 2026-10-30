import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { Loader2, Plus, X } from "lucide-react";
import { toast } from "sonner";
import { Page, PageHeader, Section } from "@/components/page";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import * as api from "@/lib/api";
import { platformLabel, regionLabel, splitScope } from "@/lib/scope";
import type { ScheduleConfig } from "@/lib/types";
import { useScope } from "@/lib/use-scope";

const DEFAULT_SCHEDULE: ScheduleConfig = {
  enabled: false,
  times: ["09:30"],
  jitterMinutes: 0,
  lastSuccessDate: null,
  lastRunAt: null,
  lastResult: null,
};

function Row({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border/50 px-4 py-3 last:border-b-0">
      <div className="min-w-0">
        <div className="text-sm font-medium leading-5">{title}</div>
        {description ? (
          <div className="mt-0.5 text-xs leading-5 text-muted-foreground">{description}</div>
        ) : null}
      </div>
      <div className="shrink-0">{action}</div>
    </div>
  );
}

function parseTime(value: string): boolean {
  return /^([01]\d|2[0-3]):[0-5]\d$/.test(value);
}

export default function SettingsPage() {
  const scope = useScope();
  const { platform, region } = splitScope(scope);
  const unsupported = platform === "workbuddy" && region === "global";
  const travelSupported = platform === "workbuddy" && region === "cn";
  const [config, setConfig] = useState<ScheduleConfig>(DEFAULT_SCHEDULE);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [running, setRunning] = useState(false);
  const [newTime, setNewTime] = useState("09:30");
  const [autostart, setAutostartState] = useState<boolean | null>(null);
  const [autostartBusy, setAutostartBusy] = useState(false);
  const [travelAuto, setTravelAuto] = useState<boolean | null>(null);
  const [travelAutoBusy, setTravelAutoBusy] = useState(false);

  useEffect(() => {
    if (!travelSupported) {
      setTravelAuto(null);
      return;
    }
    let cancelled = false;
    api
      .getTravelAuto(scope)
      .then((value) => {
        if (!cancelled) setTravelAuto(value);
      })
      .catch(() => {
        if (!cancelled) setTravelAuto(null);
      });
    return () => {
      cancelled = true;
    };
  }, [scope, travelSupported]);

  useEffect(() => {
    let cancelled = false;
    api
      .getAutostart()
      .then((value) => {
        if (!cancelled) setAutostartState(value);
      })
      .catch(() => {
        if (!cancelled) setAutostartState(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function refreshAutostart() {
    try {
      setAutostartState(await api.getAutostart());
    } catch {
      setAutostartState(null);
    }
  }

  async function toggleAutostart(checked: boolean) {
    setAutostartBusy(true);
    try {
      const value = await api.setAutostart(checked);
      setAutostartState(value);
      toast.success(value ? "已开启开机自启" : "已关闭开机自启");
    } catch (cause) {
      toast.error("设置开机自启失败", { description: api.asError(cause) });
      void refreshAutostart();
    } finally {
      setAutostartBusy(false);
    }
  }

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api
      .getSchedule(scope)
      .then((value) => {
        if (!cancelled) setConfig(value);
      })
      .catch((cause) => {
        if (!cancelled) {
          toast.error("读取排程失败", { description: api.asError(cause) });
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [scope]);

  function addTime() {
    if (!parseTime(newTime)) {
      toast.error("时间格式不对", { description: "请使用 HH:MM，例如 09:30" });
      return;
    }
    if (config.times.includes(newTime)) return;
    setConfig((prev) => ({
      ...prev,
      times: [...prev.times, newTime].sort(),
    }));
  }

  function removeTime(time: string) {
    setConfig((prev) => ({ ...prev, times: prev.times.filter((value) => value !== time) }));
  }

  async function save() {
    setSaving(true);
    try {
      const saved = await api.saveSchedule(scope, config);
      setConfig(saved);
      toast.success("排程已保存");
    } catch (cause) {
      toast.error("保存排程失败", { description: api.asError(cause) });
    } finally {
      setSaving(false);
    }
  }

  async function runNow() {
    setRunning(true);
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
      const fresh = await api.getSchedule(scope);
      setConfig(fresh);
    } catch (cause) {
      toast.error("签到失败", { description: api.asError(cause) });
    } finally {
      setRunning(false);
    }
  }

  async function toggleTravelAuto(checked: boolean) {
    setTravelAutoBusy(true);
    try {
      const value = await api.setTravelAuto(scope, checked);
      setTravelAuto(value);
      toast.success(value ? "自动旅行已开启" : "自动旅行已关闭");
    } catch (cause) {
      toast.error("自动旅行设置失败", { description: api.asError(cause) });
    } finally {
      setTravelAutoBusy(false);
    }
  }

  return (
    <Page narrow>
      <PageHeader
        title="设置"
        description={`当前：${platformLabel(platform)} · ${regionLabel(region)}`}
      />

      <div className="space-y-12">
        <Section title="自动签到">
          {unsupported ? (
            <div className="rounded-xl border border-amber-500/25 bg-amber-500/[0.06] px-4 py-3 text-xs leading-5 text-amber-700 dark:text-amber-300">
              WorkBuddy 国际版没有签到接口，无法配置自动签到。
            </div>
          ) : (
            <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
              <Row
                title="启用自动签到"
                description="需保持本应用运行；错过时点会在启动后补签"
                action={
                  <Switch
                    checked={config.enabled}
                    disabled={loading}
                    onCheckedChange={(checked) =>
                      setConfig((prev) => ({ ...prev, enabled: checked }))
                    }
                  />
                }
              />
              <div className="border-b border-border/50 px-4 py-3">
                <div className="text-sm font-medium leading-5">签到时点</div>
                <div className="mt-2 flex flex-wrap items-center gap-1.5">
                  {config.times.length === 0 ? (
                    <span className="text-xs text-muted-foreground">还没有时点</span>
                  ) : (
                    config.times.map((time) => (
                      <Badge key={time} variant="secondary" className="gap-1 pr-1 tabular-nums">
                        {time}
                        <button
                          type="button"
                          className="rounded-full p-0.5 hover:bg-foreground/10"
                          onClick={() => removeTime(time)}
                          aria-label={`删除 ${time}`}
                        >
                          <X className="size-3" />
                        </button>
                      </Badge>
                    ))
                  )}
                </div>
                <div className="mt-3 flex items-center gap-2">
                  <Input
                    type="time"
                    value={newTime}
                    onChange={(event) => setNewTime(event.target.value)}
                    className="h-8 w-[120px] text-xs"
                  />
                  <Button size="sm" variant="outline" className="h-8 gap-1 text-xs" onClick={addTime}>
                    <Plus className="size-3.5" />
                    添加时点
                  </Button>
                </div>
              </div>
              <Row
                title="随机抖动"
                description="让每天的实际触发时间错开，而不是整点齐发"
                action={
                  <Select
                    value={String(config.jitterMinutes)}
                    onValueChange={(value) =>
                      setConfig((prev) => ({ ...prev, jitterMinutes: Number(value) }))
                    }
                  >
                    <SelectTrigger className="h-8 w-[120px] text-xs">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="0">不抖动</SelectItem>
                      <SelectItem value="5">± 5 分钟</SelectItem>
                      <SelectItem value="15">± 15 分钟</SelectItem>
                      <SelectItem value="30">± 30 分钟</SelectItem>
                    </SelectContent>
                  </Select>
                }
              />
              <Row
                title="最近运行"
                description={
                  config.lastRunAt
                    ? `${new Date(config.lastRunAt).toLocaleString("zh-CN")} · ${config.lastResult ?? ""}`
                    : "尚未运行"
                }
                action={<Badge variant="outline">{config.lastSuccessDate ?? "未成功"}</Badge>}
              />
              <div className="flex items-center justify-end gap-2 px-4 py-3">
                <Button
                  size="sm"
                  variant="outline"
                  className="gap-1.5"
                  disabled={running}
                  onClick={() => void runNow()}
                >
                  {running ? <Loader2 className="size-3.5 animate-spin" /> : null}
                  立即签到
                </Button>
                <Button size="sm" className="gap-1.5" disabled={saving} onClick={() => void save()}>
                  {saving ? <Loader2 className="size-3.5 animate-spin" /> : null}
                  保存排程
                </Button>
              </div>
            </Card>
          )}
        </Section>

        {travelSupported ? (
          <Section title="WorkBuddy 旅行">
            <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
              <Row
                title="启用自动旅行"
                description="按官方状态自动派发或领取：每 15 分钟检查领奖，每 30 分钟检查出发"
                action={
                  <Switch
                    checked={travelAuto === true}
                    disabled={travelAuto === null || travelAutoBusy}
                    onCheckedChange={(checked) => void toggleTravelAuto(checked)}
                  />
                }
              />
            </Card>
          </Section>
        ) : null}

        <Section title="启动">
          <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
            <Row
              title="开机自启"
              description="开机后以隐藏窗口启动，仅在托盘运行，排程才能无人值守"
              action={
                <Switch
                  checked={autostart === true}
                  disabled={autostart === null || autostartBusy}
                  onCheckedChange={(checked) => void toggleAutostart(checked)}
                />
              }
            />
          </Card>
        </Section>

        <Section title="外观">
          <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
            <Row
              title="主题"
              description="跟随系统（浅色 / 深色）"
              action={<Badge variant="outline">系统</Badge>}
            />
          </Card>
        </Section>

        <Section title="关于">
          <Card className="min-w-0 gap-0 overflow-hidden rounded-xl py-0 shadow-none">
            <Row
              title="Roster"
              action={<span className="text-xs text-muted-foreground tabular-nums">v0.1.0</span>}
            />
          </Card>
        </Section>
      </div>
    </Page>
  );
}
