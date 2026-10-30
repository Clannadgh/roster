import qoderIcon from "@/assets/qoder.png";
import traeIcon from "@/assets/trae.png";
import workbuddyIcon from "@/assets/workbuddy-official-icon.png";
import type { Platform } from "@/lib/scope";
import { cn } from "@/lib/utils";

const ICONS: Record<Platform, string> = {
  workbuddy: workbuddyIcon,
  trae: traeIcon,
  qoder: qoderIcon,
};

export function PlatformIcon({
  platform,
  size = 20,
  className,
}: {
  platform: Platform;
  size?: number;
  className?: string;
}) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "relative inline-flex shrink-0 overflow-hidden rounded-[22%]",
        platform === "qoder" && "bg-white",
        className,
      )}
      style={{ width: size, height: size }}
    >
      <img
        src={ICONS[platform]}
        alt=""
        className={cn(
          "absolute left-1/2 top-1/2 max-w-none -translate-x-1/2 -translate-y-1/2 object-cover",
          platform === "workbuddy" ? "size-[118%]" : "size-full",
        )}
      />
    </span>
  );
}
