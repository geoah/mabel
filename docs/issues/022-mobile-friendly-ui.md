# 022: mobile-friendly UI

- Status: open
- Depends on: 013, 014

## Goal

Both UI routes read well on mobile, tablet, and desktop. Long identifiers
(52-character ids, endpoint ids, event ids) must never break the layout or
hide the information a reader needs.

## Scope

- Responsive layout for every wallet and witness screen: no horizontal
  page scroll at 360px, 768px, and 1280px widths; tables collapse to cards
  or scroll within their own container on narrow screens.
- Identifier display component: truncated middle (head and tail visible)
  with full value on tap/hover and a copy control; used everywhere an id,
  key, or endpoint renders. Monospace, wraps cleanly where full display is
  required (verify reports).
- Touch targets and form controls sized for mobile; panels stack in one
  column on narrow screens.
- Screenshot verification: a script (Playwright) captures every route in
  the demo build at 360x780, 768x1024, and 1280x800, light mode, saved
  under ui/screenshots/ (gitignored) for review; the ticket is done only
  after a human-readable pass confirms useful information is displayed
  correctly at all three widths.

## Acceptance criteria

- [ ] No horizontal page scroll on any route at 360px width.
- [ ] Ids are readable (head+tail) everywhere, with copy and full-value
      affordances; verify reports show full ids without layout breakage.
- [ ] Component tests still pass; new tests cover the identifier component.
- [ ] tests: npm test, npm run build, npm run lint green; screenshot script
      runs against the demo build and screenshots reviewed at all three
      widths.
