import { Command as CommandPrimitive } from "cmdk";
import { RiSearchLine } from "@remixicon/react";
import type * as React from "react";

import { cn } from "../lib/utils";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./dialog";

export function Command({ className, ...props }: React.ComponentProps<typeof CommandPrimitive>) {
  return (
    <CommandPrimitive
      data-slot="command"
      className={cn("flex size-full flex-col overflow-hidden rounded-lg bg-surface text-text", className)}
      {...props}
    />
  );
}

export function CommandDialog({
  title = "Command palette",
  description = "Search for a command to run",
  filter,
  children,
  ...props
}: React.ComponentProps<typeof Dialog> & {
  title?: string;
  description?: string;
  filter?: React.ComponentProps<typeof CommandPrimitive>["filter"];
}) {
  return (
    <Dialog {...props}>
      <DialogContent className="pointer-events-none! border-0 bg-transparent p-0" motion={false}>
        <div className="sr-only">
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </div>
        <Command filter={filter} className="pointer-events-none gap-2 overflow-visible bg-transparent [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:tracking-caps [&_[cmdk-group-heading]]:text-text-muted [&_[cmdk-group-heading]]:uppercase">
          {children}
        </Command>
      </DialogContent>
    </Dialog>
  );
}

export function CommandInput({
  className,
  rowClassName,
  placeholder,
  trailing,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Input> & {
  rowClassName?: string;
  trailing?: React.ReactNode;
}) {
  return (
    <div
      data-slot="command-input"
      className={cn(
        "flex items-center gap-2 p-2",
        "has-[input:focus-visible]:outline-2 has-[input:focus-visible]:outline-solid",
        "has-[input:focus-visible]:-outline-offset-1 has-[input:focus-visible]:outline-ring",
        rowClassName,
      )}
    >
      <RiSearchLine className="size-4 shrink-0 text-text-muted" />
      <span className="relative flex min-w-0 flex-1 items-center">
        {/* WebKit drops text-overflow on a focused input, and this one holds focus the whole time
            the palette is open, so the placeholder has to be a real element to ellipsize at all. */}
        <CommandPrimitive.Input
          aria-label={placeholder}
          {...props}
          placeholder=" "
          className={cn(
            "peer relative z-10 h-8 w-full bg-transparent text-base text-text outline-none",
            "disabled:opacity-45",
            className,
          )}
        />
        <span
          aria-hidden="true"
          className={cn(
            "pointer-events-none absolute inset-0 z-0 hidden items-center",
            "text-base text-text-muted peer-placeholder-shown:flex",
          )}
        >
          <span className="min-w-0 flex-1 truncate">{placeholder}</span>
        </span>
      </span>
      {trailing}
    </div>
  );
}

export function CommandList({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.List>) {
  return (
    <CommandPrimitive.List
      data-slot="command-list"
      className={cn("max-h-72 scroll-py-1 overflow-x-hidden overflow-y-auto p-1", className)}
      {...props}
    />
  );
}

export function CommandEmpty({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Empty>) {
  return (
    <CommandPrimitive.Empty
      className={cn("py-6 text-center text-base text-text-muted", className)}
      {...props}
    />
  );
}

export function CommandGroup({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Group>) {
  return (
    <CommandPrimitive.Group
      className={cn(
        "overflow-hidden text-text",
        "[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5",
        "[&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium",
        "[&_[cmdk-group-heading]]:tracking-caps [&_[cmdk-group-heading]]:uppercase",
        "[&_[cmdk-group-heading]]:text-text-muted",
        className,
      )}
      {...props}
    />
  );
}

export function CommandItem({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Item>) {
  return (
    <CommandPrimitive.Item
      className={cn(
        "relative flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm",
        "text-text outline-none select-none transition-colors",
        "data-[selected=true]:bg-accent-soft data-[selected=true]:text-accent-text",
        "data-[selected=true]:outline-2 data-[selected=true]:outline-solid",
        "data-[selected=true]:-outline-offset-2 data-[selected=true]:outline-accent",
        "data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-45",
        "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
        className,
      )}
      {...props}
    />
  );
}

export function CommandSeparator({
  className,
  ...props
}: React.ComponentProps<typeof CommandPrimitive.Separator>) {
  return <CommandPrimitive.Separator className={cn("-mx-1 my-1 h-px bg-border", className)} {...props} />;
}

export function CommandShortcut({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span
      className={cn("ml-auto font-mono text-xs tracking-caps text-text-muted", className)}
      {...props}
    />
  );
}
