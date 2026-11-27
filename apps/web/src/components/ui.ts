/** Shared Tailwind classes for the web UI (sendcms design tokens). */

export const fieldWrapper = "space-y-1.5";

export const fieldLabel = "text-sm font-medium text-foreground";

export const textInput =
  "w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground shadow-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-2 focus:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50";

export const primaryButton =
  "inline-flex w-full items-center justify-center rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground shadow-sm transition-colors hover:bg-primary/90 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-50";

export const outlineButton =
  "inline-flex w-full items-center justify-center rounded-md border border-input bg-background px-4 py-2 text-sm font-medium text-foreground shadow-sm transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-50";

export const alertError =
  "rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive";

export const alertSuccess =
  "rounded-md border border-success/50 bg-success/10 px-3 py-2 text-sm text-success-foreground";

export const card =
  "rounded-lg border border-border bg-card text-card-foreground shadow-sm";

export const link =
  "text-sm font-medium text-primary underline-offset-4 hover:underline";
