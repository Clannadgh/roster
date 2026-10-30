import appIcon from "@/assets/app-icon.png";
import { cn } from "@/lib/utils";

export function AppMark({ size = 36, className }: { size?: number; className?: string }) {
  return (
    <img
      src={appIcon}
      width={size}
      height={size}
      alt=""
      aria-hidden="true"
      className={cn("shrink-0 rounded-[22%] object-cover shadow-sm", className)}
    />
  );
}
