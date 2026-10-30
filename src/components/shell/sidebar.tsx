import {
  CalendarCheck,
  Check,
  ChevronDown,
  Coins,
  Settings,
  type LucideIcon,
} from "lucide-react";
import { NavLink, useLocation, useNavigate } from "react-router-dom";
import { AppMark } from "@/components/app-mark";
import { PlatformIcon } from "@/components/shell/platform-icon";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  PLATFORMS,
  platformLabel,
  scopeOf,
  splitScope,
  type Destination,
  type Platform,
  type Scope,
} from "@/lib/scope";
import { cn } from "@/lib/utils";

const NAV_ITEMS: { id: Destination; label: string; icon: LucideIcon }[] = [
  { id: "checkin", label: "签到", icon: CalendarCheck },
  { id: "credits", label: "积分统计", icon: Coins },
  { id: "settings", label: "设置", icon: Settings },
];

export function Sidebar({ scope }: { scope: Scope }) {
  const { platform, region } = splitScope(scope);
  const location = useLocation();
  const navigate = useNavigate();
  const currentDestination = (location.pathname.split("/")[3] || "checkin") as Destination;

  function switchPlatform(next: Platform) {
    const nextScope = scopeOf(next, region);
    navigate(`/${nextScope}/${currentDestination}`);
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
    </aside>
  );
}
