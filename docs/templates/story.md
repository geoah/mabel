# NNN: short name

- Status: draft | draft, blocked on <tickets> | implemented
- Surfaces: CLI | wallet UI | witness UI | wallet HTTP API | witness HTTP API
  (which are exercised)
- Test: path to the e2e test implementing this story (once written)

Optional: one paragraph saying what the story proves, followed by the
environment notes a runner needs (which topology, which ports, what `dc` or
any other shorthand stands for). Both are optional and neither belongs inside
Actors.

## Actors

One line per actor and what they run (wallet node, witness node).

## Story

Numbered steps in plain language, each an action one actor takes and the
visible result. A stranger should be able to execute the story by hand.

## Verified outcomes

Bullets: the exact assertions the e2e test makes at the end.
