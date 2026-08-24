import { NavLink, Navigate, Route, Routes, useParams } from "react-router";

import { getNode } from "@/api/client";
import { useResource } from "@/hooks/useResource";
import { cn } from "@/lib/utils";
import { IdentityPage } from "@/routes/identity/IdentityPage";
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
const WITNESS_LINKS = [{ to: "/witness", label: "Records", testId: "nav-witness" }];

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
      {/*
        The header names the app and nothing else. Decision 017: a counter the
        header cannot explain does not belong in the header.
      */}
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
        <Route path="*" element={<p data-testid="route-not-found">no such page</p>} />
      </Routes>
    </div>
  );
}
