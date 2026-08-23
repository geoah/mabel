import { ForksPanel } from "./ForksPanel";
import { LedgerListPanel } from "./LedgerListPanel";
import { WitnessNodeInfoPanel } from "./WitnessNodeInfoPanel";

/**
 * The witness debug surface of proposal 001 sections 5 and 6: what this one
 * witness holds and the forks it recorded. Every request is a read.
 */
export function WitnessHome() {
  return (
    <div className="space-y-4" data-testid="witness-home">
      <WitnessNodeInfoPanel />
      <LedgerListPanel />
      <ForksPanel />
    </div>
  );
}
