## PLATFORM PORT

Trigger: user requests port of a project to a new target platform (OS, architecture, or runtime)
Blocker: target platform toolchain not available on the build host

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Researcher | skill:platform-audit | enumerate current platform assumptions (OS syscalls, file paths, terminal APIs, IPC, GPU access, package formats); produce a compatibility matrix for the target platform
2 | Software Engineer | skill:dep-audit | audit all dependencies for target-platform support; flag deps with no port, partial port, or requiring replacement; produce a porting blockers list
3 | Project Manager | skill:delegate_work | distribute porting tasks based on blockers list — assign build system, runtime shims, and platform-specific code paths to appropriate agents
4 | Developer | * | adapt build system (Cargo targets, CMake toolchain files, CI matrix) for target platform
4 | Developer | * | implement platform-specific shims or replace incompatible deps
4 | UI Designer | * | verify UI rendering on target platform; adapt if terminal/window APIs differ
5 | Feature Tester | skill:smoke-test | run smoke tests on target platform (or cross-compiled binary under emulation); verify core user flows pass
5 | Beta Tester | skill:go-nuts | attempt to break the port through aggressive usage on the target platform
6 | Project Manager | skill:dev-loop | iterate until Feature Tester and Beta Tester both report acceptable results on the target platform
7 | Developer | * | add target platform to CI matrix; configure cross-compilation or native runner as appropriate
8 | Project Manager | skill:compare-instructions-to-deliverable | verify all porting blockers are resolved and the project builds cleanly on the target
9 | Repository Manager | skill:commit-review | review platform-port changes for isolation — platform-specific code must be gated behind cfg or feature flags, never polluting the primary target
10 | Repository Manager | mcp:github | commit platform port, open PR targeting the release branch

Interrupts: target platform toolchain becomes unavailable mid-port — pause, document blocker, report to user
