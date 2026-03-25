# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.4 — Pre-Mainnet

**Shipped:** 2026-03-25
**Phases:** 16 | **Plans:** 58 | **Commits:** 324

### What Was Built
- Canonical deployment infrastructure: deployment.json config system, generate-constants.ts, verify.ts, 7-phase deploy-all.sh pipeline
- Mainnet credentials and preflight safety: isolated wallets, env separation, binary hash verification, balance checks
- Arweave token metadata: permanent logos + JSON for all 3 tokens on Arweave via Irys
- Bonding curve testing: devnet deadline feature flag, clean-room deploy, both curves filled + graduated
- Protocol E2E: all 8 swap pairs, tax verification (71/24/5), VRF epochs, Carnage, staking lifecycle, 9hr soak
- Squads governance: 2-of-3 multisig, 11 authorities transferred, 1hr timelock, upgrade round-trip proven
- Production infrastructure: Cloudflare DNS (fraudsworth.fun), Railway mainnet services, Helius mainnet RPC + webhook
- Nextra documentation: every page production-accurate with illustrated SVG diagrams
- Mainnet deployment: 6 programs + 3 vanity tokens, bonding curves graduated, crank running 10+ days
- Off-chain hardening: 7 Bulwark findings closed (RPC proxy, webhook decode, supply chain, rate limiting)
- Open-source release: MetalLegBob/drfraudsworth with 5-pass secret sanitization
- OtterSec verified builds: all 6 active programs verified on Solscan

### What Worked
- **Stage-based deployment pipeline**: Stages 0-7 decomposition made mainnet deployment repeatable and auditable. Each stage is independently runnable with verification.
- **Devnet dress rehearsal before mainnet**: Phases 94-98 were an exact mainnet rehearsal. Every pitfall discovered on devnet (25.54 SOL cost, zsh compat, Solana CLI v3 changes, two-pass deploy unnecessary) was documented and avoided on mainnet.
- **Pitfall documentation during execution**: WARN blocks in checklist captured 15 pitfalls with 18 references. These prevented several costly mistakes during mainnet deploy.
- **Authority transfer scripts with verification**: transfer-authority.ts + verify-authority.ts made the most dangerous operation (irreversible authority transfer) mechanically safe. Two critical bugs caught and fixed during devnet testing.
- **Fresh repo for open-source**: Starting from rsync copy with sanitization (not forking git history) eliminated all historical secret exposure. 5-pass verification with multiple tools caught everything.
- **User review checkpoints**: 3-pass manual user verification before public push caught 2 additional redactions that automated tools missed.

### What Was Inefficient
- **REQUIREMENTS.md checkboxes never ticked (again)**: Fourth consecutive milestone with this problem. All 61 items still `[ ]` despite work being complete. The traceability table is useless without per-plan updates.
- **SUMMARY.md files not written for 10 plans**: Phases 101, 102, and 94-02 were executed without summary files. Historical record incomplete. Execution outside GSD workflow loses the tracking benefits.
- **VERIFICATION.md missing for 5 phases**: Phases 94, 99, 100, 101, 102 have no verification. Late-added phases (100-104) skipped the verification step entirely.
- **Phase 94-02 superseded but not formally closed**: Plan existed, was never executed, and was implicitly replaced by Phase 95. Should have been explicitly marked as superseded in the roadmap.
- **Scope creep from 9 to 16 phases**: Original v1.4 was Phases 91-99. Phases 100-104 were added during execution. The milestone absorbed the entire mainnet launch + post-launch operations, far exceeding the original "ready to push the button" goal.

### Patterns Established
- **Stage-based deployment**: Stages 0-7 pattern for complex multi-step deployments with checkpoint/verification at each stage
- **Pitfall-annotated checklists**: WARN blocks with pitfall IDs (PITFALL-01 through PITFALL-15) embedded at relevant steps
- **Sanitization pipeline**: rsync exclusion → file-specific edits → multi-pass automated scanning → manual user review
- **OtterSec verified builds**: solana-verify CLI submits verification PDAs linking program to public repo commit hash

### Key Lessons
1. **Scope the milestone, don't expand it** — v1.4 absorbed 7 extra phases (100-104), each significant. Should have completed v1.4 at Phase 99 and started v1.5 for mainnet + open-source.
2. **Write SUMMARY.md during execution, not after** — 10 missing summaries prove that deferring summaries means they never get written.
3. **REQUIREMENTS.md checkbox automation is overdue** — fourth milestone with this lesson. Consider making it a pre-commit hook or plan execution step.
4. **Devnet rehearsal saves real money** — 10 pitfalls discovered during devnet saved an estimated 5-10 SOL of failed mainnet transactions and prevented at least one authority burn accident.
5. **Manual user verification is essential for open-source** — automated tools missed partial key bytes and garbled placeholders that human review caught.

### Cost Observations
- Model mix: ~80% opus, ~20% sonnet (quality profile)
- Sessions: ~30+ across 14 days
- Notable: Highest plan count of any milestone (58). Mainnet deployment and open-source release created long sequential dependency chains that limited parallelization.
- Mainnet cost: ~20.83 SOL for 6 programs + init + ALT

---

## Milestone: v1.3 — Protocol Hardening & Polish

**Shipped:** 2026-03-12
**Phases:** 16 | **Plans:** 45 | **Commits:** 211

### What Was Built
- Security hardening across all 7 programs: authority gating, rent-exempt guards, checked casts, compile-time mainnet guards, vault solvency buffer
- Carnage refactor: 1800 lines deduplicated into shared module, CU profile unchanged
- VRF/crank hardening: TOCTOU recovery, circuit breaker, spending cap, configurable epochs, health endpoint
- Frontend: RPC proxy, webhook+SSE pipeline, dynamic priority fees, mobile responsive, mobile wallet adapter, environment-aware config
- Test coverage: dual-curve LiteSVM integration tests, edge case audit, proptest regression fix, GitHub Actions CI
- Documentation: 4 core spec rewrites, Nextra site rewrite, bonding curve math proofs, state machine docs
- Audit remediation: all SOS/BOK/VulnHunter findings closed, 57/57 requirements verified

### What Worked
- **Audit-driven development**: SOS, BOK (formal verification), and VulnHunter (variant analysis) provided structured finding lists. Each finding became a requirement, each requirement mapped to a phase. Zero ambiguity about "done."
- **Inserted phases for late findings**: Phases 85.1, 85.2, 90, 90.1 inserted dynamically as new findings emerged. Decimal phase numbering allowed clean insertion without renumbering.
- **Gap closure pattern**: Milestone audit → gap identification → targeted phases → re-audit cycle worked well. v1.3 audit found 6 gaps, Phase 90+90.1 closed all of them.
- **Parallel plan execution**: Multiple plans per phase executed in waves. 45 plans across 16 phases in 5 days.
- **Shared module refactor (Phase 82)**: Binary size comparison (518,592 bytes identical) validated zero behavioral regression without running full integration test suite.

### What Was Inefficient
- **REQUIREMENTS.md checkboxes never ticked**: Same lesson as v1.2 — all 57 items still `[ ]` despite being satisfied. Audit had to verify each manually. Must automate or enforce per-plan checkbox updates.
- **Nyquist validation still missing**: 15/16 phases have no VALIDATION.md. Phase 80 has partial. Retroactive validation is consistently skipped — consider removing the requirement or making it part of plan execution.
- **Two audit cycles**: Initial audit (2026-03-09) found 6 gaps. Required Phase 90+90.1 creation and execution before re-audit (2026-03-12). Earlier integration of audit checks during phase execution would eliminate the second pass.
- **SUMMARY frontmatter inconsistency**: v1.3 phases predate some conventions (requirements_completed field). Cross-referencing required manual reading of SUMMARY bodies.

### Patterns Established
- **3-audit verification pattern**: SOS (adversarial) + BOK (formal verification with Kani proofs) + VulnHunter (variant analysis) — comprehensive coverage from three independent methodologies
- **Compile-time mainnet guards**: `compile_error!` on placeholder pubkeys prevents accidental mainnet deployment with wrong addresses
- **Webhook+SSE over polling**: Helius webhooks → server processing → SSE to browser. Eliminates periodic RPC calls for real-time data.
- **Code-first spec rewrites**: Read program source → write spec. Specs describe actual code, not design intent.

### Key Lessons
1. **Tick REQUIREMENTS.md checkboxes per plan, not per milestone** — third milestone in a row where this was deferred. Needs enforcement or automation.
2. **Formal verification (Kani) finds what tests miss** — BOK's vault solvency buffer finding (BOK-1) was a genuine edge case not caught by 13.5M proptest iterations
3. **Circuit breaker and spending cap are cheap insurance** — 5 consecutive errors → pause, 0.5 SOL/hr cap. Prevents runaway crank from draining wallet.
4. **Environment-aware config from day one** — retrofitting DEVNET_* constants to CLUSTER_CONFIG was 4 plans of refactoring work (Phase 84). Starting with environment-aware config saves a full phase.
5. **Decimal phases work well for late-discovered work** — 85.1, 85.2, 90.1 all inserted cleanly without disrupting existing numbering or dependencies.

### Cost Observations
- Model mix: ~80% opus, ~20% sonnet (quality profile)
- Sessions: ~20 across 5 days
- Notable: Highest plan count per day (~9 plans/day) of any milestone. Hardening work is highly parallelizable — independent findings map to independent plans.

---

## Milestone: v1.2 — Bonding Curves & Launch Page

**Shipped:** 2026-03-07
**Phases:** 8 | **Plans:** 25 | **Commits:** 134

### What Was Built
- 7th on-chain program: dual linear bonding curves with buy/sell, 15% sell tax escrow, coupled graduation, proportional refunds (4,432 LOC Rust)
- Launch page frontend: steampunk /launch with pressure gauges, buy/sell panel, countdown timer, refund UI (~2,847 LOC TypeScript/TSX)
- Protocol integration: Transfer Hook whitelist, graduation orchestration, deploy pipeline extension, ALT update
- 13.5M proptest iterations verifying vault solvency, round-trip loss, refund order-independence

### What Worked
- **Spec-first approach**: Phase 70 (specification update) before any code. Every subsequent phase had a clear, unambiguous reference. Zero spec-vs-code conflicts.
- **Property testing integrated per-phase**: Buy math (Phase 71), sell math (Phase 72), refund math (Phase 73) each had dedicated proptest suites. Caught 1-lamport rounding edge cases early.
- **Multi-TX graduation pattern**: Avoided monolithic 32-account TX limit by decomposing into checkpoint/resume orchestration script. Reused existing AMM/vault instructions.
- **BigInt math port**: Client-side curve-math.ts is a variable-for-variable port of on-chain math.rs. No math divergence possible.
- **Gap closure phases (76, 77)**: Audit-driven gap closure was fast and targeted. Procedural phases completed in hours, not days.

### What Was Inefficient
- **SUMMARY frontmatter convention gap**: 14/28 requirements lack `requirements_completed` in SUMMARY frontmatter because plans predate the convention. Needed manual cross-referencing during audit.
- **Phase 77 (Nyquist validation) felt like busywork**: Retroactive VALIDATION.md creation for already-verified phases. Consider integrating Nyquist into plan execution to avoid retroactive compliance.
- **Two audit runs**: First audit found gaps, required Phase 76 + 77 to close them, then re-audit. Could have caught the missing VERIFICATION.md and checkbox issues during Phase 74/75 execution.

### Patterns Established
- **Tax escrow as separate PDA**: Clean separation of curve reserves vs. tax for conditional routing. Pattern reusable for future tax-bearing instruments.
- **Permissionless triggers**: Anyone can call graduation/failure trigger -- no admin dependency. Pattern for all protocol state transitions.
- **State machine-driven UI**: StateMachineWrapper renders different components based on curve status (active/graduated/failed). Clean conditional rendering pattern.
- **3-source cross-reference audit**: VERIFICATION.md + SUMMARY frontmatter + REQUIREMENTS.md checkboxes. Three independent verification channels.

### Key Lessons
1. **Integrate Nyquist into plan execution, not as a retroactive phase** -- saves a full phase of audit-driven gap closure
2. **Update REQUIREMENTS.md checkboxes as each phase completes** -- don't batch checkbox updates; leads to stale traceability
3. **Ceil-rounded math always favors the protocol** -- 1-lamport rounding drift is inherent in integer math; design rounding to favor solvency, then test the bounds
4. **Per-sell solvency checks, not integral equality** -- ceil(a)+ceil(b) >= ceil(a+b) causes unavoidable 1-lamport drift in sum-of-parts vs whole; per-operation invariant is the correct check
5. **Feature-gated mints pattern** -- devnet/localnet/mainnet compile-time mint selection works cleanly for 7 programs; established pattern holds

### Cost Observations
- Model mix: ~80% opus, ~20% sonnet (quality profile)
- Sessions: ~15 across 5 days
- Notable: Proptest iterations (13.5M) ran fast due to pure math functions with no I/O

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Commits | Phases | Key Change |
|-----------|---------|--------|------------|
| v0.1 | 345 | 8 | Documentation-first, 14-category framework |
| v0.2 | 115 | 6 | One program per milestone pattern |
| v0.7 | 134 | 9 | Integration testing, devnet deployment |
| v0.9 | - | 7 | Security audit-driven hardening |
| v1.0 | 134 | 8 | Design system, zero-dep visual layer |
| v1.1 | 156 | 9 | Component kit, reusable primitives |
| v1.2 | 134 | 8 | Spec-first, proptest-integrated, audit-verified |
| v1.3 | 211 | 16 | 3-audit verification, formal verification, audit-driven phases |
| v1.4 | 324 | 16 | Stage-based deployment, mainnet launch, open-source release |

### Cumulative Quality

| Milestone | Proptest Iterations | New LOC | Programs |
|-----------|-------------------|---------|----------|
| v0.2 | 30,000 | 10,648 Rust | 1 (AMM) |
| v0.6 | 40,000 | 2,576 Rust | 1 (Staking) |
| v1.2 | 13,500,000 | 4,432 Rust | 1 (Bonding Curve) |
| v1.3 | — (hardening) | +9,805 Rust | 7 (hardened) |
| v1.4 | — (deployment) | +1,034 Rust | 6 active (BC closed) |
| **Total** | **~13.57M** | **~40,128 Rust** | **6 active programs** |

### Top Lessons (Verified Across Milestones)

1. **Spec-first prevents rework** -- v0.1 docs audit → zero spec conflicts in v0.2-v0.7; v1.2 Phase 70 spec update → zero ambiguity in Phases 71-75
2. **Property testing catches what unit tests miss** -- v0.6 staking (40K iterations), v1.2 curves (13.5M iterations) both found edge cases impossible to hit manually
3. **Feature-gated compilation** -- devnet/mainnet feature flags established in v0.5, refined through v0.7/v0.9/v1.1/v1.2. Pattern now covers all 7 programs.
4. **Zero npm dependencies for visual/interactive layers** -- v1.0 CSS animations, v1.1 audio via HTMLAudioElement, v1.2 SVG pressure gauges. Avoids Turbopack compatibility issues.
5. **Multi-TX patterns for complex operations** -- v0.7 Carnage (2-IX atomic bundle), v1.2 graduation (5-step orchestration). Solana TX limits require decomposition, not workarounds.
6. **Audit-driven development accelerates hardening** -- v0.9 Fortress audit → targeted fixes, v1.2 self-audit → gap closure, v1.3 three independent audits → comprehensive remediation. Structured findings eliminate guesswork.
7. **Tick requirements checkboxes per plan** -- lesson identified in v1.2, repeated in v1.3. Still not enforced. Consider automation.
