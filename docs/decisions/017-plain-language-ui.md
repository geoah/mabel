# 017: plain language in the UI

- Date: 2026-08-25
- Status: accepted

Every sentence the UI shows is written for a person who has never read
this repository. Rules:

- No middle dot separators. Never string values together with `·`.
  Related values get labels or full sentences.
- No em dashes or en dashes anywhere in UI copy.
- No protocol vocabulary in user-facing text. "Attestation",
  "crawl", "fold", "descriptor", "bundle" and friends are replaced by
  what the thing means to the user, or explained in the same sentence
  the first time a screen needs the word.
- A note exists to help the user act, not to disclaim. A sentence that
  only hedges is deleted.
- No developer mode. Diagnostic data the UI does not explain does not
  ship; the CLI and the HTTP API are the diagnostic surfaces.
- Creating an identity offers the user their two secret keys to save,
  in plain words about what the keys are and what losing them means.
