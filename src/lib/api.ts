import { invoke } from "@tauri-apps/api/core";
import type {
  AccountMeta,
  CheckinOutcome,
  CheckinStatus,
  CreditsSummary,
  GlobalCheckinSummary,
  LoginPoll,
  LoginStart,
  ScheduleConfig,
  TravelStatus,
} from "./types";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function asError(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  if (typeof cause === "string") return cause;
  return "未知错误";
}

async function call<T>(command: string, args: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw new Error("浏览器预览不支持登录，请使用桌面应用");
  }
  return invoke<T>(command, args);
}

export function listAccounts(scope: string): Promise<AccountMeta[]> {
  return call("list_accounts", { scope });
}

export function loginStart(scope: string): Promise<LoginStart> {
  return call("login_start", { scope });
}

export function loginPoll(scope: string, loginId: string): Promise<LoginPoll> {
  return call("login_poll", { scope, loginId });
}

export function importTraeJwt(scope: string, jwt: string): Promise<AccountMeta> {
  return call("import_trae_jwt", { scope, jwt });
}

export function deleteAccount(scope: string, id: string): Promise<void> {
  return call("delete_account", { scope, id });
}

export function switchAccount(scope: string, accountId: string): Promise<void> {
  return call("switch_account", { scope, accountId });
}

export function saveTraeProfile(scope: string, accountId: string): Promise<number> {
  return call("save_trae_profile", { scope, accountId });
}

export function checkinStatus(scope: string, accountId: string): Promise<CheckinStatus> {
  return call("checkin_status", { scope, accountId });
}

export function checkinAccount(scope: string, accountId: string): Promise<CheckinOutcome> {
  return call("checkin_account", { scope, accountId });
}

export function checkinAll(scope: string): Promise<CheckinOutcome[]> {
  return call("checkin_all", { scope });
}

export function checkinAllScopes(): Promise<GlobalCheckinSummary> {
  return call("checkin_all_scopes", {});
}

export function getSchedule(scope: string): Promise<ScheduleConfig> {
  return call("get_schedule", { scope });
}

export function saveSchedule(scope: string, config: ScheduleConfig): Promise<ScheduleConfig> {
  return call("save_schedule", { scope, config });
}

export function getAutostart(): Promise<boolean> {
  return call("get_autostart", {});
}

export function setAutostart(enabled: boolean): Promise<boolean> {
  return call("set_autostart", { enabled });
}

export function getCreditsSummary(scope: string): Promise<CreditsSummary> {
  return call("get_credits_summary", { scope });
}

export function getTravelStatus(scope: string, accountId: string): Promise<TravelStatus> {
  return call("travel_status", { scope, accountId });
}

export function travelDepart(scope: string, accountId: string): Promise<TravelStatus> {
  return call("travel_depart", { scope, accountId });
}

export function travelClaim(scope: string, accountId: string): Promise<TravelStatus> {
  return call("travel_claim", { scope, accountId });
}

export function getTravelAuto(scope: string): Promise<boolean> {
  return call("get_travel_auto", { scope });
}

export function setTravelAuto(scope: string, enabled: boolean): Promise<boolean> {
  return call("set_travel_auto", { scope, enabled });
}
