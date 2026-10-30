export const PLATFORMS = [
  { id: "workbuddy", label: "WorkBuddy" },
  { id: "trae", label: "Trae" },
  { id: "qoder", label: "Qoder" },
] as const;

export type Platform = (typeof PLATFORMS)[number]["id"];

export const REGIONS = [
  { id: "cn", label: "国内版" },
  { id: "global", label: "国际版" },
] as const;

export type Region = (typeof REGIONS)[number]["id"];

export type Scope = `${Platform}-${Region}`;

export type Destination = "checkin" | "credits" | "settings";

export const DEFAULT_SCOPE: Scope = "workbuddy-cn";

const SCOPE_PATTERN = /^(workbuddy|trae|qoder)-(cn|global)$/;

export function isScope(value: string): value is Scope {
  return SCOPE_PATTERN.test(value);
}

export function scopeOf(platform: Platform, region: Region): Scope {
  return `${platform}-${region}`;
}

export function splitScope(scope: Scope): { platform: Platform; region: Region } {
  const index = scope.lastIndexOf("-");
  return {
    platform: scope.slice(0, index) as Platform,
    region: scope.slice(index + 1) as Region,
  };
}

export function platformLabel(platform: Platform): string {
  return PLATFORMS.find((item) => item.id === platform)?.label ?? platform;
}

export function regionLabel(region: Region): string {
  return REGIONS.find((item) => item.id === region)?.label ?? region;
}
