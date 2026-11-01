export type AccountMeta = {
  id: string;
  uid: string | null;
  email: string | null;
  nickname: string | null;
  enterpriseName: string | null;
  domain: string;
  expiresAt: number | null;
  refreshExpiresAt: number | null;
  createdAt: number;
  updatedAt: number;
  needsRelogin: boolean;
  isCurrent: boolean;
};

export type LoginStart = {
  loginId: string;
  verificationUri: string;
  expiresIn: number;
};

export type LoginPoll = {
  done: boolean;
  result?: AccountMeta | null;
  error?: string | null;
};

export type CheckinStatus = {
  ok: boolean;
  todayCheckedIn: boolean;
  dailyCredit: number | null;
  todayCredit: number | null;
  streakDays: number | null;
  error: string | null;
};

export type CheckinOutcome = {
  accountId: string;
  result: "success" | "already" | "error" | "unsupported" | string;
  amount: number | null;
  error: string | null;
};

export type ScopeCheckinSummary = {
  scope: string;
  success: number;
  already: number;
  failed: number;
  skipped: number;
};

export type GlobalCheckinSummary = {
  success: number;
  already: number;
  failed: number;
  skipped: number;
  scopes: ScopeCheckinSummary[];
};

export type ScheduleConfig = {
  enabled: boolean;
  times: string[];
  jitterMinutes: number;
  lastSuccessDate: string | null;
  lastRunAt: number | null;
  lastResult: string | null;
};

export type CreditAccount = {
  accountId: string;
  uid: string;
  name: string;
  ok: boolean;
  remaining: number | null;
  total: number | null;
  today: number | null;
  expiresAt: number | null;
  expiring: number | null;
  plan: string | null;
  error: string | null;
};

export type DailyPoint = {
  date: string;
  amount: number;
};

export type CreditsSummary = {
  platform: string;
  accounts: CreditAccount[];
  totalRemaining: number;
  today: number;
  week: number;
  month: number;
  daily: DailyPoint[];
  okAccounts: number;
  failedAccounts: number;
};

export type TravelStatus = {
  state: "idle" | "traveling" | "arrived" | "no_buddy" | "unsupported";
  locationName: string | null;
  rewardCredit: number | null;
  arriveAt: number | null;
  dailyLimitReached: boolean;
};
