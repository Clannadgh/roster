import { useLocation, useNavigate } from "react-router-dom";
import { REGIONS, scopeOf, splitScope } from "@/lib/scope";
import { useScope } from "@/lib/use-scope";
import { cn } from "@/lib/utils";

export function RegionSwitch() {
  const scope = useScope();
  const { platform, region } = splitScope(scope);
  const navigate = useNavigate();
  const location = useLocation();
  const destination = location.pathname.split("/")[3] || "checkin";

  return (
    <div className="flex gap-1 rounded-lg bg-muted p-1" role="tablist" aria-label="区域">
      {REGIONS.map((item) => (
        <button
          key={item.id}
          type="button"
          role="tab"
          aria-selected={item.id === region}
          onClick={() => navigate(`/${scopeOf(platform, item.id)}/${destination}`)}
          className={cn(
            "rounded-md px-2.5 py-1.5 text-xs transition-colors",
            item.id === region
              ? "bg-background font-medium text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
