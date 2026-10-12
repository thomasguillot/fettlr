import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
  Checkbox,
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
  Input,
  Label,
  ScrollArea,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Separator,
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Toaster,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
  toast,
} from "@fettlr/design-system";
import { RiDownloadLine, RiMoonLine, RiBox3Line, RiRefreshLine, RiSunLine, RiDeleteBinLine } from "@remixicon/react";
import { useState, type ReactNode } from "react";

import { StaleBadge } from "../components/badges";

const neutralTokens = [
  "bg",
  "bg-subtle",
  "surface",
  "surface-hover",
  "border",
  "border-strong",
  "text",
  "text-muted",
  "inverse",
];

const brandTokens = [
  "accent",
  "accent-hover",
  "accent-text",
  "accent-soft",
];

const statusTokens = [
  "blue",
  "blue-text",
  "blue-display",
  "blue-soft",
  "danger",
  "danger-text",
  "danger-soft",
  "success",
  "success-text",
  "success-soft",
  "warning",
  "warning-text",
  "warning-soft",
];

const typeScale = [
  ["3xl", "text-3xl"],
  ["2xl", "text-2xl"],
  ["xl", "text-xl"],
  ["lg", "text-lg"],
  ["base", "text-base"],
  ["sm", "text-sm"],
  ["xs", "text-xs"],
] as const;

const radii = ["xs", "sm", "md", "lg", "xl"];
const shadows = ["md", "lg"];

const skills = [
  { name: "brainstorming", scope: "User", version: "1.4.0", state: "Enabled" },
  { name: "writing-plans", scope: "Project", version: "2.0.1", state: "Update" },
  { name: "systematic-debugging", scope: "User", version: "0.9.7", state: "Disabled" },
  { name: "test-driven-development", scope: "Plugin", version: "3.2.0", state: "Enabled" },
];

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-xs font-medium tracking-caps text-text-muted uppercase">{title}</h2>
      {children}
    </section>
  );
}

function Swatches({ names }: { names: string[] }) {
  return (
    <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-5">
      {names.map((name) => (
        <div key={name} className="flex items-center gap-2.5">
          <span
            className="size-9 shrink-0 rounded-md border border-border"
            style={{ background: `var(--color-${name})` }}
          />
          <span className="font-mono text-xs text-text-muted">{name}</span>
        </div>
      ))}
    </div>
  );
}

export function Gallery() {
  const [dark, setDark] = useState(() => document.documentElement.classList.contains("dark"));
  const [notifications, setNotifications] = useState(true);
  const [checkboxUnchecked, setCheckboxUnchecked] = useState(false);
  const [checkboxChecked, setCheckboxChecked] = useState(true);

  const toggleTheme = (next: boolean) => {
    setDark(next);
    document.documentElement.classList.toggle("dark", next);
  };

  return (
    <TooltipProvider>
      <div className="min-h-dvh bg-bg text-text">
        <header className="sticky top-0 z-40 border-b border-border bg-bg/85 backdrop-blur-sm">
          <div className="mx-auto flex h-14 max-w-5xl items-center gap-3 px-6">
            <span className="size-6 rounded-md bg-accent" />
            <span className="font-semibold tracking-tight">Fettlr design system</span>
            <Badge tone="accent" variant="soft">
              v0.1
            </Badge>
            <div className="ml-auto flex items-center gap-2">
              <RiSunLine className="size-4 text-text-muted" />
              <Switch checked={dark} onCheckedChange={toggleTheme} aria-label="Dark mode" />
              <RiMoonLine className="size-4 text-text-muted" />
            </div>
          </div>
        </header>

        <main className="mx-auto flex max-w-5xl flex-col gap-12 px-6 py-10">
          <div className="flex flex-col gap-2">
            <h1 className="text-3xl font-semibold tracking-tighter">
              Every component, both modes
            </h1>
            <p className="max-w-prose text-text-muted">
              The whole vocabulary the app is built from. Flip the switch to check that nothing
              relies on a single mode.
            </p>
          </div>

          <Section title="Neutrals">
            <Swatches names={neutralTokens} />
          </Section>

          <Section title="Brand">
            <Swatches names={brandTokens} />
          </Section>

          <Section title="Status">
            <Swatches names={statusTokens} />
          </Section>

          <Section title="Type scale">
            <div className="flex flex-col gap-2">
              {typeScale.map(([name, className]) => (
                <div key={name} className="flex items-baseline gap-4">
                  <span className="w-12 shrink-0 font-mono text-xs text-text-muted">{name}</span>
                  <span className={`${className} truncate tracking-tight`}>
                    Manage skills, plugins, and MCP servers
                  </span>
                </div>
              ))}
            </div>
          </Section>

          <Section title="Radii and elevation">
            <div className="flex flex-wrap gap-6">
              {radii.map((name) => (
                <div key={name} className="flex flex-col items-center gap-2">
                  <span
                    className="size-14 border border-border bg-surface"
                    style={{ borderRadius: `var(--radius-${name})` }}
                  />
                  <span className="font-mono text-xs text-text-muted">{name}</span>
                </div>
              ))}
              {shadows.map((name) => (
                <div key={name} className="flex flex-col items-center gap-2">
                  <span
                    className="size-14 rounded-lg bg-surface"
                    style={{ boxShadow: `var(--shadow-${name})` }}
                  />
                  <span className="font-mono text-xs text-text-muted">shadow {name}</span>
                </div>
              ))}
            </div>
          </Section>

          <Section title="Buttons">
            <div className="flex flex-wrap items-center gap-2">
              <Button>
                <RiDownloadLine />
                Install
              </Button>
              <Button variant="secondary">Details</Button>
              <Button variant="ghost">Cancel</Button>
              <Button variant="danger">
                <RiDeleteBinLine />
                Remove
              </Button>
              <Button variant="link">Read the docs</Button>
              <Button size="icon" variant="secondary" aria-label="Refresh">
                <RiRefreshLine />
              </Button>
              <Button disabled>Disabled</Button>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm">Small</Button>
              <Button size="md">Medium</Button>
              <Button size="lg">Large</Button>
            </div>
          </Section>

          <Section title="Badges">
            <div className="flex flex-wrap items-center gap-2">
              <Badge>Neutral</Badge>
              <Badge>Enabled</Badge>
              <Badge tone="blue" variant="solid">
                3
              </Badge>
              <Badge tone="blue" variant="solid">
                Needs updating
              </Badge>
              <Badge tone="success">Up to date</Badge>
              <Badge tone="warning">Unverified</Badge>
              <Badge tone="danger">Failed</Badge>
              <Badge variant="outline">1.4.0</Badge>
              <Badge tone="neutral" variant="solid">
                Plugin
              </Badge>
            </div>
          </Section>

          <Section title="Forms">
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="gallery-search">Marketplace</Label>
                <Input id="gallery-search" placeholder="github.com/owner/repo" />
              </div>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="gallery-scope">Scope</Label>
                <Select defaultValue="user">
                  <SelectTrigger id="gallery-scope">
                    <SelectValue placeholder="Pick a scope" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="user">User</SelectItem>
                    <SelectItem value="project">Project</SelectItem>
                    <SelectItem value="local">Project (local)</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="gallery-invalid">Invalid field</Label>
                <Input id="gallery-invalid" aria-invalid defaultValue="not-a-path" />
              </div>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="gallery-disabled">Disabled</Label>
                <Input id="gallery-disabled" disabled defaultValue="Read only" />
              </div>
              <div className="flex items-center gap-3">
                <Switch
                  id="gallery-notifications"
                  checked={notifications}
                  onCheckedChange={setNotifications}
                />
                <Label htmlFor="gallery-notifications">Notify me about updates</Label>
              </div>
              <div className="flex flex-wrap items-center gap-6 sm:col-span-2">
                <div className="flex items-center gap-2">
                  <Checkbox
                    id="gallery-checkbox-unchecked"
                    checked={checkboxUnchecked}
                    onCheckedChange={(checked) => setCheckboxUnchecked(checked === true)}
                  />
                  <Label htmlFor="gallery-checkbox-unchecked">Unchecked</Label>
                </div>
                <div className="flex items-center gap-2">
                  <Checkbox
                    id="gallery-checkbox-checked"
                    checked={checkboxChecked}
                    onCheckedChange={(checked) => setCheckboxChecked(checked === true)}
                  />
                  <Label htmlFor="gallery-checkbox-checked">Checked</Label>
                </div>
                <div className="flex items-center gap-2">
                  <Checkbox id="gallery-checkbox-disabled" checked disabled />
                  <Label htmlFor="gallery-checkbox-disabled">Disabled</Label>
                </div>
              </div>
            </div>
          </Section>

          <Section title="Tabs and table">
            <Tabs defaultValue="installed" className="mb-6">
              <TabsList variant="segmented">
                <TabsTrigger value="installed">Installed</TabsTrigger>
                <TabsTrigger value="browse">Browse</TabsTrigger>
                <TabsTrigger value="archived" disabled>
                  Archived
                </TabsTrigger>
              </TabsList>
            </Tabs>

            <Tabs defaultValue="skills">
              <TabsList>
                <TabsTrigger value="skills">
                  <RiBox3Line />
                  Skills
                </TabsTrigger>
                <TabsTrigger value="plugins">Plugins</TabsTrigger>
                <TabsTrigger value="servers">MCP servers</TabsTrigger>
                <TabsTrigger value="disabled" disabled>
                  Archived
                </TabsTrigger>
              </TabsList>
              <TabsContent value="skills">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Name</TableHead>
                      <TableHead>Scope</TableHead>
                      <TableHead>Version</TableHead>
                      <TableHead>State</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {skills.map((skill, index) => (
                      <TableRow key={skill.name} data-state={index === 1 ? "selected" : undefined}>
                        <TableCell className="font-mono">{skill.name}</TableCell>
                        <TableCell className="text-text-muted">{skill.scope}</TableCell>
                        <TableCell className="text-text-muted">{skill.version}</TableCell>
                        <TableCell>
                          {skill.state === "Enabled" ? <Badge tone="success">Enabled</Badge> : null}
                          {skill.state === "Update" ? (
                            <StaleBadge>2 commits behind</StaleBadge>
                          ) : null}
                          {skill.state === "Disabled" ? <Badge>Disabled</Badge> : null}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </TabsContent>
              <TabsContent value="plugins">
                <p className="text-text-muted">Nothing installed yet.</p>
              </TabsContent>
              <TabsContent value="servers">
                <p className="text-text-muted">Nothing installed yet.</p>
              </TabsContent>
            </Tabs>
          </Section>

          <Section title="Card, separator, scroll area">
            <div className="grid gap-4 sm:grid-cols-2">
              <Card>
                <CardHeader>
                  <CardTitle>superpowers</CardTitle>
                  <CardDescription>
                    Skills for planning, testing, and shipping without losing the thread.
                  </CardDescription>
                </CardHeader>
                <CardContent className="flex items-center gap-2 text-sm text-text-muted">
                  <Badge variant="outline">2.0.1</Badge>
                  <Separator orientation="vertical" className="h-4" />
                  <span>obra/superpowers</span>
                </CardContent>
                <CardFooter>
                  <Button size="sm">Update</Button>
                  <Button size="sm" variant="ghost">
                    Disable
                  </Button>
                </CardFooter>
              </Card>
              <Card>
                <CardHeader>
                  <CardTitle>Recent activity</CardTitle>
                </CardHeader>
                <CardContent>
                  <ScrollArea className="h-32 rounded-md border border-border">
                    <ul className="divide-y divide-border text-sm">
                      {Array.from({ length: 10 }, (_, index) => (
                        <li key={index} className="flex items-center justify-between px-3 py-2">
                          <span>Checked marketplace {index + 1}</span>
                          <span className="text-xs text-text-muted tabular-nums">
                            {index + 1}m ago
                          </span>
                        </li>
                      ))}
                    </ul>
                  </ScrollArea>
                </CardContent>
              </Card>
            </div>
          </Section>

          <Section title="Overlays">
            <div className="flex flex-wrap items-center gap-2">
              <Dialog>
                <DialogTrigger asChild>
                  <Button variant="secondary">Open dialog</Button>
                </DialogTrigger>
                <DialogContent>
                  <DialogHeader>
                    <DialogTitle>Remove this skill?</DialogTitle>
                    <DialogDescription>
                      It moves to the trash, so you can put it back from Finder if you change your
                      mind.
                    </DialogDescription>
                  </DialogHeader>
                  <DialogFooter>
                    <DialogClose asChild>
                      <Button variant="ghost">Cancel</Button>
                    </DialogClose>
                    <Button variant="danger">Remove</Button>
                  </DialogFooter>
                </DialogContent>
              </Dialog>

              <Sheet>
                <SheetTrigger asChild>
                  <Button variant="secondary">Open sheet</Button>
                </SheetTrigger>
                <SheetContent>
                  <SheetHeader>
                    <SheetTitle>brainstorming</SheetTitle>
                    <SheetDescription>User scope, installed from obra/superpowers.</SheetDescription>
                  </SheetHeader>
                  <p className="px-5 text-base text-text-muted">
                    Explores intent, requirements, and design before any implementation work starts.
                  </p>
                  <SheetFooter>
                    <Button variant="ghost">Disable</Button>
                    <Button>Update</Button>
                  </SheetFooter>
                </SheetContent>
              </Sheet>

              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="secondary">Open menu</Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent>
                  <DropdownMenuLabel>Actions</DropdownMenuLabel>
                  <DropdownMenuItem>
                    <RiRefreshLine />
                    Check for updates
                    <DropdownMenuShortcut>⌘R</DropdownMenuShortcut>
                  </DropdownMenuItem>
                  <DropdownMenuCheckboxItem checked>Show disabled</DropdownMenuCheckboxItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem tone="danger">
                    <RiDeleteBinLine />
                    Remove
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="secondary">Hover for a tooltip</Button>
                </TooltipTrigger>
                <TooltipContent>Last checked 4 minutes ago</TooltipContent>
              </Tooltip>

              <Button variant="secondary" onClick={() =>
                  toast.success("Marketplace updated", {
                    description: "12 skills and 3 plugins are now current.",
                  })
                }>
                Toast
              </Button>
              <Button variant="secondary" onClick={() =>
                  toast.error("Could not reach the registry", {
                    description: "Check your connection and try again.",
                  })
                }>
                Error toast
              </Button>
            </div>

            <Command className="max-w-md rounded-lg border border-border">
              <CommandInput placeholder="Search skills, plugins, servers" />
              <CommandList className="border-t border-border">
                <CommandEmpty>Nothing matches that.</CommandEmpty>
                <CommandGroup heading="Skills">
                  <CommandItem>
                    <RiBox3Line />
                    brainstorming
                    <CommandShortcut>User</CommandShortcut>
                  </CommandItem>
                  <CommandItem>
                    <RiBox3Line />
                    writing-plans
                    <CommandShortcut>Project</CommandShortcut>
                  </CommandItem>
                </CommandGroup>
                <CommandGroup heading="Actions">
                  <CommandItem>
                    <RiRefreshLine />
                    Check every marketplace
                  </CommandItem>
                </CommandGroup>
              </CommandList>
            </Command>
          </Section>
        </main>
        <Toaster />
      </div>
    </TooltipProvider>
  );
}
