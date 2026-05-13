---
name: Brands README
description: Brand profile storage. Each .md file in this directory is a style guide associated with a project (or a project family).
---

# Brand Profiles

This directory holds brand profile markdown files. Each file is the
**definitive style guide** for one project (or project family) — it's the
single source of truth for visual identity, voice & tone, color palette,
typography, logo usage, taglines, naming conventions, and any
brand-aligned rules that affect distribution surfaces (README badges,
release notes, marketing copy, app store listings, screenshots).

## File format

```markdown
---
name: <Brand Name>
description: <one-line summary>
projects:
  - <project_name>
  - <project_name>
accent_color: "#E94560"
secondary_color: "#16213E"
---

# <Brand Name>

## Voice & Tone

<copy-the-house-style here>

## Visual

- Primary: <hex>
- Secondary: <hex>
- Background: <hex>
- Text: <hex>

## Logo Rules

<…>

## Taglines

- <approved tagline 1>
- <approved tagline 2>
```

## How orrchestrator uses these

The Publish > Brands tab lists every file in this directory and previews
the selected one in markdown form. Future iterations will:

- Auto-attach the brand profile to its declared `projects` when generating
  marketing metadata, README assets, and release notes.
- Validate distribution artifacts (e.g. README, app store screenshots)
  against the brand's color/typography/voice rules.
- Surface brand violations in the Analyze panel.

For now, the Brands tab is read-only — author profiles in your editor of
choice and orrchestrator picks them up on the next reload.
