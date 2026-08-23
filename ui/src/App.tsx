import { useState } from "react";
import { NavLink, Navigate, Route, Routes, useParams } from "react-router";

import { getNode } from "@/api/client";
import { useResource } from "@/hooks/useResource";
import { cn } from "@/lib/utils";
import { useDeveloperMode } from "@/lib/preferences";
import { IdentityPage } from "@/routes/identity/IdentityPage";
import { GraphSyncControl } from "@/routes/wallet/GraphSyncControl";
import { WalletHome } from "@/routes/wallet/WalletHome";
import { WitnessHome } from "@/routes/witness/WitnessHome";
import { WitnessLedgerDetail } from "@/routes/witness/WitnessLedgerDetail";
import { WitnessLedgersPage } from "@/routes/witnesses/WitnessLedgersPage";
import { WitnessesPage } from "@/routes/witnesses/WitnessesPage";

/** Two entries, and no third: the wallet is a list of identities and a list of witnesses. */
const WALLET_LINKS = [
  { to: "/wallet", label: "Wallet", testId: "nav-wallet" },
  { to: "/witnesses", label: "Witnesses", testId: "nav-witnesses" },
];

/** A witness node serves no wallet, so its nav names the one screen it has. */
const WITNESS_LINKS = [{ to: "/witness", label: "Ledgers", testId: "nav-witness" }];

/**
 * The header menu holding the developer-mode toggle (decision 014). The panel
 * is a plain block under the button: it needs one entry, not a menu library.
 */
function AppMenu() {
  const [open, setOpen] = useState(false);
  const [developer, setDeveloper] = useDeveloperMode();

  return (
    <div className="relative">
      <button
        type="button"
        data-testid="app-menu-button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="inline-flex min-h-8 items-center rounded-md border px-2 text-sm hover:bg-accent"
      >
        Menu
      </button>
      {open && (
        <div
          data-testid="app-menu"
          className="absolute right-0 top-full z-30 mt-1 w-72 rounded-md border bg-card p-3 text-left shadow-md"
        >
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              data-testid="developer-mode-toggle"
              checked={developer}
              onChange={(event) => setDeveloper(event.target.checked)}
            />
            Developer mode
          </label>
          <p className="mt-1 text-xs text-muted-foreground">
            Shows head event ids, witness endpoint ids, principal keys, sync freshness, crawl
            provenance and the raw response document. Nothing is removed while it is off.
          </p>
        </div>
      )}
    </div>
  );
}

/** The two routes the wallet kept from before proposal 004, pointed at the identity page. */
function RedirectToIdentity() {
  const { identityId = "" } = useParams();
  return <Navigate to={`/identities/${identityId}`} replace />;
}

export function App() {
  // A node has one role. The witness binary serves this same bundle, and its
  // debug route is the only screen there: it holds no identities to list.
  const node = useResource(getNode, []);
  const witness = node.data?.role === "witness";
  const links = witness ? WITNESS_LINKS : WALLET_LINKS;

  return (
    // pb-20 keeps the last card clear of the bar the nav becomes on a phone.
    <div className="mx-auto max-w-6xl px-3 pt-3 pb-20 sm:px-4 sm:pt-4 md:pb-4">
      <header className="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2 border-b pb-3">
        <span className="text-sm font-semibold" data-testid="app-title">
          mabel
        </span>
        {/*
          One nav element at every width: a row in the header on md+, a fixed
          bottom bar below it, where a thumb reaches it.
        */}
        <nav className="flex max-md:fixed max-md:inset-x-0 max-md:bottom-0 max-md:z-20 max-md:border-t max-md:bg-background md:gap-3">
          {links.map((link) => (
            <NavLink
              key={link.to}
              to={link.to}
              end={link.to === "/wallet"}
              data-testid={link.testId}
              className={({ isActive }) =>
                cn(
                  "flex min-h-11 items-center justify-center px-3 text-sm max-md:flex-1 md:min-h-0 md:px-0",
                  isActive ? "font-medium underline" : "text-muted-foreground",
                )
              }
            >
              {link.label}
            </NavLink>
          ))}
        </nav>
        <div className="ml-auto flex items-center gap-2">
          {/* The graph is the wallet's own crawl; a witness never runs one. */}
          {!witness && <GraphSyncControl />}
          <AppMenu />
        </div>
      </header>
      <Routes>
        <Route path="/" element={<Navigate to={witness ? "/witness" : "/wallet"} replace />} />
        <Route path="/wallet" element={<WalletHome />} />
        <Route path="/identities/:identityId" element={<IdentityPage />} />
        <Route path="/witnesses" element={<WitnessesPage />} />
        <Route path="/witnesses/:endpointId" element={<WitnessLedgersPage />} />
        <Route path="/witness" element={<WitnessHome />} />
        <Route path="/witness/ledgers/:ledgerId" element={<WitnessLedgerDetail />} />
        {/* Bookmarks from the four-tab wallet, so no saved link 404s. */}
        <Route path="/wallet/identities/:identityId" element={<RedirectToIdentity />} />
        <Route path="/wallet/lookup/:identityId" element={<RedirectToIdentity />} />
        <Route path="*" element={<p data-testid="route-not-found">no such route</p>} />
      </Routes>
    </div>
  );
}
