import { Slot } from "@radix-ui/react-slot";
import { cva } from "class-variance-authority";
import type * as React from "react";

import { cn } from "@/lib/utils";

/**
 * The shadcn navigation menu, vendored down to the parts a link bar uses: the
 * root, the list, the items and the links. The class strings, the part names and
 * the `data-slot` attributes are the upstream ones. The trigger, the content and
 * the viewport are left out: this nav has no submenu, and those three parts are
 * the whole reason the upstream component reaches for Radix.
 *
 * The keyboard behaviour the Radix root gives a link bar is kept here: the
 * arrow keys walk the links, Home and End jump to the ends, and Tab leaves the
 * bar in one press because only the links are focusable.
 */
export function NavigationMenu({
  className,
  orientation = "horizontal",
  ...props
}: React.ComponentProps<"nav"> & { orientation?: "horizontal" | "vertical" }) {
  return (
    <nav
      data-slot="navigation-menu"
      data-orientation={orientation}
      className={cn("group/navigation-menu relative flex max-w-max flex-1 items-center", className)}
      {...props}
    />
  );
}

/** Which key moves where, per orientation. */
const NEXT_KEYS: Record<string, string[]> = {
  horizontal: ["ArrowRight", "ArrowDown"],
  vertical: ["ArrowDown"],
};
const PREVIOUS_KEYS: Record<string, string[]> = {
  horizontal: ["ArrowLeft", "ArrowUp"],
  vertical: ["ArrowUp"],
};

export function NavigationMenuList({
  className,
  onKeyDown,
  ...props
}: React.ComponentProps<"ul">) {
  function walk(event: React.KeyboardEvent<HTMLUListElement>) {
    onKeyDown?.(event);
    if (event.defaultPrevented) {
      return;
    }
    const orientation =
      event.currentTarget.closest("[data-slot=navigation-menu]")?.getAttribute("data-orientation") ??
      "horizontal";
    const links = [
      ...event.currentTarget.querySelectorAll<HTMLElement>("[data-slot=navigation-menu-link]"),
    ];
    const at = links.indexOf(document.activeElement as HTMLElement);
    if (links.length === 0) {
      return;
    }
    let next = -1;
    if (NEXT_KEYS[orientation].includes(event.key)) {
      next = at + 1 >= links.length ? 0 : at + 1;
    } else if (PREVIOUS_KEYS[orientation].includes(event.key)) {
      next = at <= 0 ? links.length - 1 : at - 1;
    } else if (event.key === "Home") {
      next = 0;
    } else if (event.key === "End") {
      next = links.length - 1;
    }
    if (next === -1) {
      return;
    }
    event.preventDefault();
    links[next].focus();
  }

  return (
    <ul
      data-slot="navigation-menu-list"
      onKeyDown={walk}
      className={cn("group flex flex-1 list-none items-center gap-1", className)}
      {...props}
    />
  );
}

export function NavigationMenuItem({ className, ...props }: React.ComponentProps<"li">) {
  return (
    <li data-slot="navigation-menu-item" className={cn("relative", className)} {...props} />
  );
}

export const navigationMenuTriggerStyle = cva(
  "group inline-flex h-9 w-max items-center justify-center rounded-md bg-background px-4 py-2 text-sm font-medium transition-colors outline-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 data-[active=true]:bg-accent/50 data-[active=true]:text-accent-foreground data-[active=true]:hover:bg-accent",
);

/**
 * One link of the bar. `asChild` hands the styling and the `data-slot` to a
 * router link, which is what the upstream component does with a framework's own
 * link element.
 */
export function NavigationMenuLink({
  className,
  active,
  asChild = false,
  ...props
}: React.ComponentProps<"a"> & { active?: boolean; asChild?: boolean }) {
  const Component = asChild ? Slot : "a";
  return (
    <Component
      data-slot="navigation-menu-link"
      data-active={active ? "true" : undefined}
      className={cn(
        "flex items-center gap-1 rounded-sm text-sm transition-colors outline-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 data-[active=true]:text-accent-foreground",
        className,
      )}
      {...props}
    />
  );
}
