import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

export function Page({
  narrow = false,
  children,
}: {
  narrow?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      className={cn(
        "mx-auto w-full min-w-0 px-6 py-8 sm:px-8 sm:py-9",
        narrow ? "max-w-3xl" : "max-w-[1180px]",
      )}
    >
      {children}
    </div>
  );
}

export function PageHeader({
  title,
  description,
  actions,
}: {
  title: string;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
      <div className="min-w-0">
        <h1 className="text-[28px] font-semibold leading-9 tracking-tight">{title}</h1>
        {description ? (
          <p className="mt-2 text-sm leading-6 text-muted-foreground">{description}</p>
        ) : null}
      </div>
      {actions ? (
        <div className="flex shrink-0 flex-wrap items-center gap-2 pt-1">{actions}</div>
      ) : null}
    </header>
  );
}

export function Section({
  title,
  action,
  children,
}: {
  title?: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="min-w-0 space-y-2.5">
      {title ? (
        <div className="flex items-center justify-between px-1">
          <h2 className="text-[13px] font-medium leading-5">{title}</h2>
          {action}
        </div>
      ) : null}
      {children}
    </section>
  );
}

export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
}: {
  icon: LucideIcon;
  title: string;
  description?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border px-6 py-14 text-center">
      <div className="flex size-10 items-center justify-center rounded-xl bg-muted text-muted-foreground">
        <Icon className="size-5" />
      </div>
      <div className="space-y-1">
        <p className="text-sm font-medium">{title}</p>
        {description ? (
          <p className="mx-auto max-w-[440px] text-xs leading-5 text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
      {action}
    </div>
  );
}
