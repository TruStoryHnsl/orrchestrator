<!-- Title format: <type>(<scope>)?: <description>  e.g. feat(tui): scrollable roadmap in project detail -->
<!-- Types: feat, fix, docs, refactor, perf, test, chore, ci, build. Breaking: feat! / fix! or BREAKING CHANGE footer. -->

## Summary

<!-- 1-3 sentences. What changes, and why. Drop articles if clear. -->

## Test plan

<!-- Concrete, observed-not-believed. State what you ran and what you saw, not what you think the change does. -->

- [ ] `cargo build` clean (warnings OK)
- [ ] `cargo test` passes (note new tests added, if any)
- [ ] `cargo clippy --all-targets` reviewed
- [ ] Ran the affected panel / surface manually and observed the behavior
- [ ] (if WebUI changes) verified local HTTP at `127.0.0.1:8484`
- [ ] (if workforce/operation changes) dry-ran the dispatch on a test project
- [ ] (if agent profile changes) the agent file parses (`cargo test -p orrch-agents`)

## Branch isolation

<!-- This repo enforces session branch isolation: every change ships on its own feat/fix/refactor/chore branch
     and merges to main via PR. Do NOT merge other session branches into this one. -->

- [ ] Branch name follows `feat/...` `fix/...` `refactor/...` `chore/...` (or `<type>/<slug>-<hex-suffix>` if running parallel sessions)
- [ ] Did not merge `main` into this branch (unless explicitly required)
- [ ] Commits follow Conventional Commits

## Breaking changes

<!-- "None" is a valid answer. If yes: describe the migration path. -->

## Related issues

<!-- Closes #123, refs #456 -->
