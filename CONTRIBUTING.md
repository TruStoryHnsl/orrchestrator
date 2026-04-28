# Contributing to orrchestrator

Short version: file an issue or open a PR. Conventional Commits. Branch per session. No merging `main` into your branch unless explicitly required. Token efficiency is the design constraint — don't add context cost without saying why.

## Filing an issue

- **Bugs**: use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.yml). Include exact reproduction steps and the version/commit you're on (`orrchestrator --version` or `git rev-parse --short HEAD`).
- **Feature requests**: use the [feature request template](.github/ISSUE_TEMPLATE/feature_request.yml). Be honest about token / context cost.
- **Open-ended questions**: use [Discussions](https://github.com/TruStoryHnsl/orrchestrator/discussions), not an issue.
- **Security disclosures**: email the maintainer privately (`colton.j.orr@gmail.com`). Do not file a public issue.

Search existing issues before opening a new one.

## Opening a PR

### 1. Branch isolation (mandatory)

Every change ships on its own branch. Never commit directly to `main`. Branch naming:

```
feat/<slug>          new feature
fix/<slug>           bug fix
refactor/<slug>      restructuring without behavior change
perf/<slug>          performance work
docs/<slug>          docs-only
test/<slug>          tests-only
chore/<slug>         maintenance
ci/<slug>            CI changes
build/<slug>         build-system changes
```

If multiple sessions or contributors might pick similar slugs, append a short hex suffix: `feat/scrollable-roadmap-a3f9`.

**Do not** merge `main` into your branch unless the maintainer asks. **Do not** merge another contributor's branch into yours. Keep branches independent until reconciliation.

### 2. Conventional Commits

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

| Type | SemVer bump | Example |
|------|-------------|---------|
| `feat:` | minor | `feat(tui): scrollable roadmap in project detail` |
| `fix:` | patch | `fix(webui): bind 0.0.0.0 when ORRCH_WEBUI_BIND set` |
| `feat!:` / `fix!:` / `BREAKING CHANGE:` footer | major | `feat!(workforce): pipe-delimited step format v2` |
| `docs:` | none | `docs: clarify TLS env vars` |
| `refactor:` | none | `refactor(core): extract session lifecycle` |
| `perf:` | patch | `perf(workforce): cache parsed step tables` |
| `test:` | none | `test(agents): cover ResourceOptimizer annotations` |
| `chore:` | none | `chore(deps): bump ratatui` |

### 3. Local checks before pushing

```bash
cargo build              # warnings OK
cargo test               # all crates
cargo clippy --all-targets
```

Run the panel or surface you changed and **observe the behavior**. State problems and successes as observed, not as believed — "the WebUI session list now updates after expand, verified by clicking the row" beats "I think this fixes it."

### 4. PR description

Use the [PR template](.github/PULL_REQUEST_TEMPLATE.md). Specifically:

- Concrete test plan (what you ran, what you saw).
- Note breaking changes explicitly. "None" is a valid answer.
- Link related issues.

### 5. Review

PRs need a maintainer review before merge. The merge style is squash-merge (`gh pr merge --squash`) so the commit history on `main` stays linear and readable. Don't force-push during review — push fixup commits; the squash will collapse them.

## Architecture notes for contributors

- **Rust workspace, 9 crates** — see the [Architecture section of the README](README.md#architecture) for the layout.
- **One session per workflow, not per agent.** This is the token-efficiency rule that drives most of the surrounding design. Don't introduce per-agent sessions without discussion.
- **Hypervisor is a dispatcher, not an agent.** Workflow execution lives in declarative markdown step tables (`operations/*.md`). Don't move dispatch logic into LLM calls.
- **Deterministic tools between steps.** `library/tools/` shell scripts handle compression, briefing, clustering. Adding LLM judgment where a tool would do is rejected.
- **File-cluster batching.** Tasks group by shared files, not agent role. Duplicate roles (3 Developers) is fine if they reduce file overlap.
- **PLAN.md is per-project; the *master* state is `instructions_inbox.md`** during active intake, and per-project `PLAN.md` for the long-lived roadmap.
- **No screenshots in commits.** Dev-session screenshots are broken-state captures and are explicitly out of scope for the README. Curated marketing imagery comes later, intentionally.

## Style

- Rust 2024 edition.
- Format with `cargo fmt`.
- `cargo clippy --all-targets` should be clean for new code; pre-existing lints are OK to leave alone unless the PR explicitly addresses them.
- Match surrounding code conventions for naming, error handling (`thiserror` + `anyhow`), and async (`tokio`).
- Avoid emojis in source, docs, and commit messages unless explicitly requested.

## Scope

orrchestrator is currently a single-user / home-lab tool. Patches that assume multi-tenant, hosted, or SaaS deployment will need a clear rationale. Patches that hard-depend on a specific commercial LLM provider (no fallback) will be rejected — the three-tier model layer (enterprise / mid-tier / local) is part of the architecture.

## License

By contributing you agree your contribution is licensed under the [MIT License](LICENSE).
