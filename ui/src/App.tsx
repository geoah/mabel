import { NavLink, Navigate, Route, Routes } from "react-router";

import { cn } from "@/lib/utils";
import { IdentityDetail } from "@/routes/wallet/IdentityDetail";
import { VerifyPage } from "@/routes/wallet/VerifyPage";
import { WalletHome } from "@/routes/wallet/WalletHome";
import { WitnessHome } from "@/routes/witness/WitnessHome";
import { WitnessLedgerDetail } from "@/routes/witness/WitnessLedgerDetail";

const LINKS = [
  { to: "/wallet", label: "Wallet", testId: "nav-wallet" },
  { to: "/wallet/verify", label: "Verify", testId: "nav-verify" },
  { to: "/witness", label: "Witness", testId: "nav-witness" },
];

export function App() {
  return (
    <div className="mx-auto max-w-6xl p-4">
      <header className="mb-4 flex items-baseline gap-4 border-b pb-3">
        <span className="text-sm font-semibold" data-testid="app-title">
          mabel
        </span>
        <nav className="flex gap-3">
          {LINKS.map((link) => (
            <NavLink
              key={link.to}
              to={link.to}
              end={link.to === "/wallet"}
              data-testid={link.testId}
              className={({ isActive }) =>
                cn("text-sm", isActive ? "font-medium underline" : "text-muted-foreground")
              }
            >
              {link.label}
            </NavLink>
          ))}
        </nav>
      </header>
      <Routes>
        <Route path="/" element={<Navigate to="/wallet" replace />} />
        <Route path="/wallet" element={<WalletHome />} />
        <Route path="/wallet/verify" element={<VerifyPage />} />
        <Route path="/wallet/identities/:identityId" element={<IdentityDetail />} />
        <Route path="/witness" element={<WitnessHome />} />
        <Route path="/witness/ledgers/:ledgerId" element={<WitnessLedgerDetail />} />
        <Route path="*" element={<p data-testid="route-not-found">no such route</p>} />
      </Routes>
    </div>
  );
}
