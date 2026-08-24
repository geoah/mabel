/**
 * The two identity components and the facts they read. Every screen that names
 * an identity imports from here: proposal 005 leaves no third rendering.
 */
export {
  type CardTestIds,
  IdentityCard,
  type IdentityCardEntry,
  IdentityCardList,
  type IdentityCardState,
  type IdentityFacts,
  type IdentityRecord,
  factsFromIdentity,
  factsFromResolved,
  listTestIds,
  pageTestIds,
} from "./IdentityCard";
export { IdentityInline } from "./IdentityInline";
export {
  type Machine,
  machineSentence,
  machinesOf,
  NOT_CONFIRMED,
  ON_OWN_RECORD,
} from "./machines";
export {
  bareIdentity,
  duplicateNames,
  IdentityListScope,
  nameWithNickname,
  type NameSource,
  type ResolvedName,
  resolveName,
  resolvedFrom,
  useSharedName,
  VerificationMark,
  type VerificationState,
  verificationState,
} from "./names";
export {
  IdentityPillBadge,
  IdentityPillScope,
  NO_PILL_FACTS,
  type Pill,
  type PillFacts,
  type PillKind,
  pillFor,
  trustedSubjects,
  usePill,
} from "./pill";
