## DEVELOP FEATURE

Trigger: unprocessed instructions exist in the project instruction_inbox.md
Blocker: instructions from the Intelligence Resources Manager to pause development

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Project Manager | skill:synthesize_instructions | check for new instructions and incorporate any that exist into the project plan
2 | Resource Optimizer | skill:assess_tasks | read the fresh plan, assess each task's complexity, check Library for available models/harnesses, annotate tasks with model tier + harness + rationale
3 | Project Manager | skill:delegate_work | read the plan with optimization annotations and distribute tasks to appropriate agents with explicit model/harness recommendations
4 | Developer | * | execute coding tasks
4 | Researcher | * | conduct research on relevant technologies and solutions
4 | Software Engineer | * | design architecture for requested features
4 | UI Designer | * | design UX elements following established design patterns
4 | Feature Tester | skill:test-design | design tests that verify successful implementation
5 | Penetration Tester | skill:pen-test | attempt to exploit the program to exfiltrate data, infiltrate viruses, and execute arbitrary code
5 | Beta Tester | skill:go-nuts | attempt to break the implementation through aggressive usage
6 | Project Manager | skill:dev-loop | continue facilitating development until all relevant testers report acceptable results
7 | Project Manager | skill:compare-instructions-to-deliverable | verify that the work completed is high quality and functional
7 | Repository Manager | skill:commit-review | review the PM's chosen feature grouping and advise on optimal git commit packaging and branch strategy
8 | Project Manager | skill:log-dev | write a report of the session to the development log signed with an appropriate version tag
9 | Repository Manager | skill:versioning | determine the appropriate semantic version tag for this commit
10 | Repository Manager | mcp:github | stage the new version and commit to the appropriate branch in the repository
11 | Repository Manager | skill:merge-to-main | MANDATORY — merge the session branch back to main so all future sessions start from the integrated codebase; abort and escalate if conflicts occur (cross-session conflicts require human judgment); delete the session branch after successful merge

Interrupts: Intelligence Resources Manager issues pause directive; merge-to-main conflict requires user resolution
