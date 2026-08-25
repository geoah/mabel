import { describe, expect, it } from "vitest";

import { shownPayload } from "@/components/EventLines";
import { ALICE, BOB } from "@/mocks/fixtures";

const ENDPOINT = "zbj22dym2k3btlvjftxmj7kwujgwjgovqthhsjl6ixh5qe43mctq";
const EVENT = "th2a3dvusqj6x5dwkpsg7sltxdu74ajyyvn4n3ypsll4yl2iq5la";

/**
 * The contents of an entry, as the screen shows them. An identity id in there
 * reads the way it reads everywhere else (decision 019), and nothing else does:
 * an endpoint, a key and an entry id are not identities, and two payloads spell
 * a field the same way while meaning different things.
 */
describe("the contents of one entry", () => {
  it("names the identity a trust attestation is about", () => {
    expect(shownPayload("trust_attestation", { subject: BOB })).toBe(
      `{"subject":"mabel://${BOB}"}`,
    );
  });

  it("names every identity a witness set chose", () => {
    expect(shownPayload("witness_set", { witnesses: [ALICE, BOB] })).toBe(
      `{"witnesses":["mabel://${ALICE}","mabel://${BOB}"]}`,
    );
  });

  it("leaves the endpoints of an advertisement bare", () => {
    expect(shownPayload("endpoint_advertisement", { endpoints: [ENDPOINT] })).toBe(
      `{"endpoints":["${ENDPOINT}"]}`,
    );
  });

  // The two traps. `target` is an identity under one kind and an entry id under
  // the other, and `witnesses` holds identities under the kind that is written
  // now and endpoints under the retired one. A rule keyed on the field name
  // would get both of these wrong.
  it("tells the two meanings of target apart", () => {
    expect(shownPayload("membership_removal", { target: BOB })).toBe(
      `{"target":"mabel://${BOB}"}`,
    );
    expect(shownPayload("trust_revocation", { target: EVENT })).toBe(`{"target":"${EVENT}"}`);
  });

  it("tells the two meanings of witnesses apart", () => {
    expect(shownPayload("witness_config", { witnesses: [ENDPOINT] })).toBe(
      `{"witnesses":["${ENDPOINT}"]}`,
    );
  });

  it("names the founder of an identity root and leaves the key beside it bare", () => {
    const payload = {
      declared_kind: "organization",
      root: { identity_root: { founder: ALICE, founder_key: ENDPOINT } },
    };
    expect(shownPayload("inception", payload)).toContain(`"founder":"mabel://${ALICE}"`);
    expect(shownPayload("inception", payload)).toContain(`"founder_key":"${ENDPOINT}"`);
  });

  it("passes a kind it does not know through untouched", () => {
    expect(shownPayload("something_new", { subject: BOB })).toBe(`{"subject":"${BOB}"}`);
  });
});
