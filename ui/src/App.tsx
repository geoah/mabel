import { useState } from "react";
import { NavLink, Navigate, Route, Routes, useLocation } from "react-router";

import { cn } from "@/lib/utils";
import { useDeveloperMode } from "@/lib/preferences";
import { GraphSyncControl } from "@/routes/wallet/GraphSyncControl";
import { IdentityDetail } from "@/routes/wallet/IdentityDetail";
import { LookupPage } from "@/routes/wallet/LookupPage";
import { VerifyPage } from "@/routes/wallet/VerifyPage";
import { WalletHome } from "@/routes/wallet/WalletHome";
import { WitnessHome } from "@/routes/witness/WitnessHome";
import { WitnessLedgerDetail } from "@/routes/witness/WitnessLedgerDetail";

const LINKS = [
  { to: "/wallet", label: "Wallet", testId: "nav-wallet" },
  { to: "/wallet/lookup", label: "Lookup", testId: "nav-lookup" },
  { to: "/wallet/verify", label: "Verify", testId: "nav-verify" },
  { to: "/witness", label: "Witness", testId: "nav-witness" },
];

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

export function App() {
  const wallet = useLocation().pathname.startsWith("/wallet");

  return (
    // pb-20 keeps the last card clear of the bar the nav becomes on a phone.
    // The wider cap on xl is what lets the nine-column witness table fit.
    <div className="mx-auto max-w-6xl px-3 pt-3 pb-20 sm:px-4 sm:pt-4 md:pb-4 xl:max-w-7xl">
      <header className="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2 border-b pb-3">
        <span className="text-sm font-semibold" data-testid="app-title">
          mabel
        </span>
        {/*
          One nav element at every width: a row in the header on md+, a fixed
          bottom bar below it, where a thumb reaches it.
        */}
        <nav className="flex max-md:fixed max-md:inset-x-0 max-md:bottom-0 max-md:z-20 max-md:border-t max-md:bg-background md:gap-3">
          {LINKS.map((link) => (
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
          {wallet && <GraphSyncControl />}
          <AppMenu />
        </div>
      </header>
      <Routes>
        <Route path="/" element={<Navigate to="/wallet" replace />} />
        <Route path="/wallet" element={<WalletHome />} />
        <Route path="/wallet/verify" element={<VerifyPage />} />
        <Route path="/wallet/lookup" element={<LookupPage />} />
        <Route path="/wallet/lookup/:identityId" element={<LookupPage />} />
        <Route path="/wallet/identities/:identityId" element={<IdentityDetail />} />
        <Route path="/witness" element={<WitnessHome />} />
        <Route path="/witness/ledgers/:ledgerId" element={<WitnessLedgerDetail />} />
        <Route path="*" element={<p data-testid="route-not-found">no such route</p>} />
      </Routes>
    </div>
  );
}
