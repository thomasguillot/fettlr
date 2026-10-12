import { cva, type VariantProps } from "class-variance-authority";
import { Tabs as TabsPrimitive } from "radix-ui";
import { createContext, use, type ComponentProps } from "react";

import { cn, focusRing } from "../lib/utils";

const tabsListVariants = cva("inline-flex items-center", {
  variants: {
    variant: {
      line: "w-full gap-4 border-b border-border",
      segmented: "w-fit gap-0.5 rounded-lg border border-border-strong bg-bg-subtle p-0.5",
    },
  },
  defaultVariants: { variant: "line" },
});

const tabsTriggerVariants = cva(
  cn(
    "relative inline-flex items-center gap-1.5 whitespace-nowrap text-sm font-medium",
    "text-text-muted transition-colors hover:text-text",
    "disabled:pointer-events-none disabled:opacity-45",
    "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
    focusRing,
  ),
  {
    variants: {
      variant: {
        line: cn(
          "h-9 data-[state=active]:text-text",
          "after:absolute after:inset-x-0 after:-bottom-px after:h-0.5 after:rounded-full",
          "data-[state=active]:after:bg-accent-text",
        ),
        segmented:
          "h-7.5 rounded-sm px-3 data-[state=active]:bg-selected data-[state=active]:text-selected-contrast",
      },
    },
    defaultVariants: { variant: "line" },
  },
);

type TabsVariant = NonNullable<VariantProps<typeof tabsListVariants>["variant"]>;

// The trigger's shape follows the list it sits in, and Radix gives the two no shared node to hang
// that off, so the list publishes its variant rather than every call site repeating it.
const TabsVariantContext = createContext<TabsVariant>("line");

export function Tabs({ className, ...props }: ComponentProps<typeof TabsPrimitive.Root>) {
  return <TabsPrimitive.Root data-slot="tabs" className={cn("flex flex-col", className)} {...props} />;
}

export function TabsList({
  className,
  variant = "line",
  ...props
}: ComponentProps<typeof TabsPrimitive.List> & { variant?: TabsVariant }) {
  return (
    <TabsVariantContext value={variant}>
      <TabsPrimitive.List
        data-slot="tabs-list"
        data-variant={variant}
        className={cn(tabsListVariants({ variant }), className)}
        {...props}
      />
    </TabsVariantContext>
  );
}

export function TabsTrigger({ className, ...props }: ComponentProps<typeof TabsPrimitive.Trigger>) {
  const variant = use(TabsVariantContext);
  return (
    <TabsPrimitive.Trigger
      data-slot="tabs-trigger"
      className={cn(tabsTriggerVariants({ variant }), className)}
      {...props}
    />
  );
}

export function TabsContent({ className, ...props }: ComponentProps<typeof TabsPrimitive.Content>) {
  return <TabsPrimitive.Content className={cn("mt-4", focusRing, className)} {...props} />;
}
