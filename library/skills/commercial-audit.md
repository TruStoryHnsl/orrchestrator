---
description: Run a blind commercial deployment audit on a project
user-invokable: true
---

Perform a comprehensive commercial deployment audit of the project at $ARGUMENTS (or the current working directory if no args).

## Audit Process

1. **Assume no prior knowledge** — read the codebase fresh as if you've never seen it
2. **Rate each category** on a 1-5 scale (1=missing, 5=production-ready)
3. **Document every finding** with file:line references
4. **Generate a deployment readiness score** (0-100%)

## Categories to Audit

### Functionality (weight: 30%)
- Do the core features work end-to-end?
- Are there stub modules masquerading as complete?
- Can a new user accomplish the primary use cases?

### Security (weight: 25%)
- Are secrets handled properly?
- Is encryption implemented correctly?
- Are there injection vectors or auth bypasses?
- Is data encrypted at rest and in transit?

### Reliability (weight: 20%)
- Error handling coverage
- Graceful degradation
- Recovery from failures
- Test coverage quality (not just count)

### Code Quality (weight: 15%)
- Consistent patterns
- Dead code / unused dependencies
- Documentation quality
- Type safety

### Deployment Readiness (weight: 10%)
- Build pipeline
- Configuration management
- Logging / observability
- Platform compatibility

## Output Format

Write findings to `{project}/commercial_deployment_database.md` with:

1. **Executive Summary** — 3 sentences on deployment readiness
2. **Score Card** — table of categories with scores
3. **Critical Blockers** — issues that must be fixed before any deployment
4. **High Priority** — issues that should be fixed before public release
5. **Medium Priority** — issues that affect quality but not function
6. **Low Priority** — nice-to-haves
7. **Strengths** — what's done well
8. **Architecture Assessment** — is the design sound for the stated goals?
9. **Recommendations** — ordered list of next steps
