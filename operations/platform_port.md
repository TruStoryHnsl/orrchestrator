## PLATFORM PORT

Trigger: user requests port of a project to a new distribution platform (PyPI, crates.io, npm, Docker Hub, Flathub, AUR, Homebrew, apt/deb, GitHub Releases, etc.)
Blocker: target platform toolchain, credentials, or publishing account not available on the build host

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Researcher | skill:platform-audit | research target platform requirements: submission guidelines, manifest formats, signing/notarization rules, required credentials, review process, artifact layout, naming conventions; produce a platform requirements doc
2 | Software Engineer | skill:dep-audit | audit all project dependencies against target platform policies; flag deps with incompatible licenses, missing platform packages, native build requirements, or vendoring obligations; produce a dependency readiness report
3 | Project Manager | skill:delegate_work | distribute porting tasks based on the requirements doc and dependency report — assign build adaptation, packaging, CI, docs, and release work to the appropriate agents
4 | Developer | * | adapt build system for target-platform artifacts: configure Cargo targets / setuptools / npm scripts / Dockerfile / PKGBUILD / debian rules / Formula / build.yaml as required by the platform
4 | Developer | * | produce the platform-native package artifact (wheel + sdist, crate tarball, npm tarball, deb/rpm, docker image, flatpak bundle, AUR PKGBUILD, Homebrew Formula, signed GitHub release asset) with correct metadata, version, license, and entry points
4 | UI Designer | * | prepare platform-required assets: icons, screenshots, long description, categories, keywords, desktop/appstream metadata — whatever the target storefront mandates
5 | Repository Manager | skill:ci-integration | add or extend CI workflow (.github/workflows/*) to build and smoke-test the platform artifact on every push; wire platform credentials into CI secrets
5 | Feature Tester | skill:test-matrix | verify the packaged artifact builds and installs cleanly across the supported OS/arch/runtime matrix (linux/mac/win, x86_64/arm64, supported language-runtime versions); record the pass/fail matrix
6 | UX Specialist | skill:docs-update | update README / project docs with platform-specific install instructions, uninstall steps, and troubleshooting notes; add platform badges and link the artifact
7 | Beta Tester | skill:pre-release-validation | install the packaged artifact from a local file on a clean environment, exercise core user flows end-to-end, confirm install → run → upgrade → uninstall all work; report any regressions vs. source-install
8 | Project Manager | skill:dev-loop | iterate until Feature Tester matrix is green and Beta Tester pre-release validation passes on every supported target
9 | Project Manager | skill:compare-instructions-to-deliverable | confirm platform requirements doc is fully satisfied, legal/license metadata is correct, and release notes are drafted
10 | Repository Manager | skill:versioning | assign the appropriate semantic version tag for the release and update CHANGELOG.md
11 | Repository Manager | mcp:publish | publish to the target platform (cargo publish / twine upload / npm publish / docker push / flatpak-builder --export / makepkg / brew audit+release / dput / gh release create) and create the matching git tag + GitHub Release
12 | Beta Tester | skill:post-publish-verification | pull the published artifact from the live public platform on a clean machine, install via the documented instructions, confirm the install path works end-to-end; report install telemetry back to the PM

Interrupts: target platform toolchain, credentials, or signing identity becomes unavailable mid-port — pause, document blocker, report to user. Platform rejects submission during review — capture the reviewer feedback, reroute to step 4 for remediation.
