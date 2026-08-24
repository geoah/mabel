import { useMemo } from "react";

import { getNode, listIdentities, listWitnesses } from "@/api/client";
import type { NodeInfo, WitnessSummary } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  bareIdentity,
  factsFromResolved,
  type IdentityCardEntry,
  IdentityCardList,
  IdentityInline,
  IdentityPillScope,
  machinesOf,
  type PillFacts,
  trustedSubjects,
} from "@/components/identity";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { PageSections, Section } from "@/components/Section";
import { useResource } from "@/hooks/useResource";

/** The relay setting, in the two words it is worth. */
const RELAY: Record<string, string> = {
  n0: "public relays",
  disabled: "direct connections only",
};

/** Bytes as a person reads them: three significant figures and a unit. */
function bytes(value: number): string {
  const units = ["bytes", "kB", "MB", "GB", "TB"];
  let scaled = value;
  let unit = 0;
  while (scaled >= 1000 && unit < units.length - 1) {
    scaled /= 1000;
    unit += 1;
  }
  const rounded = unit === 0 ? scaled : Math.round(scaled * 10) / 10;
  return `${rounded} ${units[unit]}`;
}

/**
 * What this home holds, when it holds no key of its own. A node with no
 * identities is not broken and not a different program: it signs for nothing
 * and keeps records for other people (proposal 006 section 8).
 */
function heldWithoutKeys(node: NodeInfo): string {
  const records = `${node.ledger_count} ${node.ledger_count === 1 ? "record" : "records"}`;
  if (node.witness_for.length === 0) {
    return `This home holds no keys, so it signs for nothing and adds nothing to any record. It keeps ${records}.`;
  }
  const identities =
    node.witness_for.length === 1 ? "one identity" : `${node.witness_for.length} identities`;
  return `This home holds no keys, so it signs for nothing and adds nothing to any record. It keeps ${records} and accepts new entries for ${identities}.`;
}

/**
 * This node, as short rows: the Iroh ID other nodes dial it by, how it is
 * reachable, what it holds and which build is running. There is no role row:
 * what a node can do is read from what it holds.
 */
export function NodePage() {
  const node = useResource(getNode, []);
  const witnesses = useResource(listWitnesses, []);
  const identities = useResource(listIdentities, []);
  const data = node.data;
  const held = identities.data?.identities ?? [];
  const defaults = (witnesses.data?.witnesses ?? []).filter((witness) => witness.is_node_default);
  const pills = useMemo<PillFacts>(
    () => ({
      own: new Set(held.map((identity) => identity.identity_id)),
      trusted: trustedSubjects(held),
      degrees: new Map<string, number>(),
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [identities.data],
  );
  // The witness list is loaded for the cards below, so the identities this node
  // keeps records for are named from it rather than from a request of their own.
  const name = (identityId: string) => {
    const witness = (witnesses.data?.witnesses ?? []).find(
      (entry) => entry.identity_id === identityId,
    );
    return {
      ...bareIdentity(identityId),
      display_name: witness?.display_name ?? null,
    };
  };
  const entries: IdentityCardEntry[] = defaults.map((witness: WitnessSummary) => ({
    facts: factsFromResolved(
      { ...bareIdentity(witness.identity_id), display_name: witness.display_name },
      {
        to: `/identities/${witness.identity_id}`,
        stored: witness.stored,
        machines: machinesOf(null, witness),
      },
    ),
  }));

  return (
    <IdentityPillScope facts={pills}>
      <PageSections>
        <Section testId="node-page" title="This node">
          {node.loading && <p data-testid="node-loading">loading</p>}
          {node.error && <ErrorEnvelopeView error={node.error} testId="node-error" />}
          {data && (
            <>
              <KeyValueTable>
                <KeyValue label="Iroh ID" testId="node-endpoint-id">
                  <Identifier value={data.endpoint_id} full copyLabel="Copy Iroh ID" />
                </KeyValue>
                <KeyValue label="relay" testId="node-relay">
                  {RELAY[data.relay] ?? data.relay}
                </KeyValue>
                <KeyValue label="identities" testId="node-identity-count">
                  {data.identity_count}
                </KeyValue>
                {/* Who this node accepts records for, which is what makes it a
                    witness. It witnesses for nobody until an identity is listed
                    here, and the row says so in the one word it deserves. */}
                <KeyValue label="keeps records for" testId="node-witness-for">
                  {data.witness_for.length === 0 ? (
                    "none"
                  ) : (
                    <span className="flex flex-col gap-1">
                      {data.witness_for.map((entry) => (
                        <IdentityInline
                          key={entry.identity}
                          identity={name(entry.identity)}
                          testId={`node-witness-for-${entry.identity}`}
                          to={`/identities/${entry.identity}`}
                        />
                      ))}
                    </span>
                  )}
                </KeyValue>
                <KeyValue label="records" testId="node-ledger-count">
                  {data.ledger_count}
                </KeyValue>
                <KeyValue label="conflicts" testId="node-fork-count">
                  {data.fork_count}
                </KeyValue>
                <KeyValue label="space used" testId="node-storage">
                  {bytes(data.storage_used)} of {bytes(data.storage_capacity)}
                </KeyValue>
                <KeyValue label="version" testId="node-version">
                  {data.version}
                </KeyValue>
              </KeyValueTable>
              {data.identity_count === 0 && (
                <p data-testid="node-no-keys" className="text-sm">
                  {heldWithoutKeys(data)}
                </p>
              )}
            </>
          )}
        </Section>
        {data && (
          <Section
            testId="node-witnesses"
            title="Witnesses it uses by default"
            description="An identity that names no witness of its own gets these."
          >
            {witnesses.error && (
              <ErrorEnvelopeView error={witnesses.error} testId="node-witnesses-error" />
            )}
            {witnesses.data && (
              <IdentityCardList
                entries={entries}
                testId="node-witness-cards"
                empty="none"
                emptyTestId="node-witnesses-empty"
              />
            )}
          </Section>
        )}
      </PageSections>
    </IdentityPillScope>
  );
}
