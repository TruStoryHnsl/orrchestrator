---
name: GitHub
description: MCP server for GitHub operations (issues, PRs, etc.)
transport: sse
url: http://localhost:8001/mcp/github
enabled: true
assigned_roles:
  - repository_manager
---

Provides access to GitHub repositories.

**Capabilities:**
- List issues
- Create issues
- Comment on issues
- List pull requests
- Create pull requests
- Merge pull requests
