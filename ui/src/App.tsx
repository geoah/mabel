import { NavLink, Navigate, Route, Routes, useParams } from "react-router";

import { apiBaseUrl, getNode } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  NavigationMenu,
  NavigationMenuItem,
  NavigationMenuLink,
  NavigationMenuList,
} from "@/components/ui/navigation-menu";
import { useResource } from "@/hooks/useResource";
import { cn } from "@/lib/utils";
import { IdentityPage } from "@/routes/identity/IdentityPage";
import { NodePage } from "@/routes/node/NodePage";
import { WalletHome } from "@/routes/wallet/WalletHome";
import { WitnessHome } from "@/routes/witness/WitnessHome";
import { WitnessLedgerDetail } from "@/routes/witness/WitnessLedgerDetail";
import { WitnessLedgersPage } from "@/routes/witnesses/WitnessLedgersPage";
import { WitnessesPage } from "@/routes/witnesses/WitnessesPage";

/**
 * Three entries, and no fourth: the identities this wallet holds, the witnesses
 * it knows, and the program doing the work.
 */
const WALLET_LINKS = [
  { to: "/wallet", label: "Wallet", testId: "nav-wallet" },
  { to: "/witnesses", label: "Witnesses", testId: "nav-witnesses" },
  { to: "/node", label: "Node", testId: "nav-node" },
];

/** A witness node serves no wallet, so its nav names the records it keeps. */
const WITNESS_LINKS = [
  { to: "/witness", label: "Records", testId: "nav-witness" },
  { to: "/node", label: "Node", testId: "nav-node" },
];

/** The two routes the wallet kept from before proposal 004, pointed at the identity page. */
function RedirectToIdentity() {
  const { identityId = "" } = useParams();
  return <Navigate to={`/identities/${identityId}`} replace />;
}

/**
 * The front door, once the node has said which role it serves. Redirecting
 * before the answer arrives sends a witness operator to the wallet, which is a
 * screen their node does not serve, so `/` waits.
 */
function RoleHome({ role, blocked }: { role: "wallet" | "witness" | null; blocked: boolean }) {
  if (role !== null) {
    return <Navigate to={role === "witness" ? "/witness" : "/wallet"} replace />;
  }
  // The shell above already says which node it could not reach, so a failed
  // question is not answered twice here.
  return blocked ? null : <p data-testid="app-role-loading">loading</p>;
}

export function App() {
  // A node has one role. The witness binary serves this same bundle, and its
  // debug route is the only screen there: it holds no identities to list.
  const node = useResource(getNode, []);
  const witness = node.data?.role === "witness";
  const links = witness ? WITNESS_LINKS : WALLET_LINKS;
  // Null until the node answers: a wallet is what this build shows by default,
  // never what it assumes while the question is still open.
  const role = node.data === null ? null : witness ? "witness" : "wallet";

  return (
    // One readable column at every width, and margins rather than a second
    // column on a desktop (proposal 005). pb-20 keeps the last card clear of the
    // bar the nav becomes on a phone.
    <div className="mx-auto w-full max-w-2xl px-3 pt-3 pb-20 sm:px-4 sm:pt-4 md:pb-4">
      {/*
        The header names the app and nothing else. Decision 017: a counter the
        header cannot explain does not belong in the header.
      */}
      <header className="mb-4 flex h-12 items-center justify-between gap-4 border-b">
        <span className="text-base font-semibold tracking-tight" data-testid="app-title">
          mabel
        </span>
        {/*
          One nav element at every width: the link row of a shadcn navigation
          menu in the header on md+, and the same links as a fixed bottom bar
          below it, where a thumb reaches them. Three entries share the width of
          a phone without scrolling sideways.
        */}
        <NavigationMenu
          className={cn(
            "max-md:fixed max-md:inset-x-0 max-md:bottom-0 max-md:z-20 max-md:max-w-none",
            "max-md:border-t max-md:bg-background max-md:p-1",
            "max-md:pb-[max(0.25rem,env(safe-area-inset-bottom))]",
          )}
        >
          <NavigationMenuList className="gap-1 max-md:w-full">
            {links.map((link) => (
              <NavigationMenuItem key={link.to} className="max-md:flex-1">
                <NavigationMenuLink asChild>
                  <NavLink
                    to={link.to}
                    end={link.to === "/wallet"}
                    data-testid={link.testId}
                    className={cn(
                      "flex h-12 items-center justify-center rounded-md px-3 text-sm",
                      "font-medium text-muted-foreground transition-colors",
                      "hover:bg-accent hover:text-accent-foreground md:h-9",
                      "aria-[current=page]:bg-accent aria-[current=page]:text-accent-foreground",
                    )}
                  >
                    {link.label}
                  </NavLink>
                </NavigationMenuLink>
              </NavigationMenuItem>
            ))}
          </NavigationMenuList>
        </NavigationMenu>
      </header>
      {/*
        The node document is what every screen here stands on: which role this
        node serves, and whether it is answering at all. A failure to read it is
        said once, in the shell, naming the address it asked.
      */}
      {node.error && (
        <div data-testid="shell-node-error" className="mb-4 space-y-2">
          <p data-testid="shell-node-error-sentence" className="text-sm">
            This page could not read the node at{" "}
            <span data-testid="shell-node-error-base-url" className="font-mono break-all">
              {apiBaseUrl()}
            </span>
            . Nothing below is up to date.
          </p>
          <ErrorEnvelopeView error={node.error} testId="shell-node-error-envelope" />
        </div>
      )}
      <Routes>
        <Route path="/" element={<RoleHome role={role} blocked={node.error !== null} />} />
        <Route path="/wallet" element={<WalletHome />} />
        <Route path="/identities/:identityId" element={<IdentityPage />} />
        <Route path="/witnesses" element={<WitnessesPage />} />
        <Route path="/node" element={<NodePage />} />
        <Route path="/witnesses/:endpointId" element={<WitnessLedgersPage />} />
        <Route path="/witness" element={<WitnessHome />} />
        <Route path="/witness/ledgers/:ledgerId" element={<WitnessLedgerDetail />} />
        {/* Bookmarks from the four-tab wallet, so no saved link 404s. */}
        <Route path="/wallet/identities/:identityId" element={<RedirectToIdentity />} />
        <Route path="/wallet/lookup/:identityId" element={<RedirectToIdentity />} />
        <Route path="*" element={<p data-testid="route-not-found">no such page</p>} />
      </Routes>
    </div>
  );
}
