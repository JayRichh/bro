# SDD ledger — plan: docs/superpowers/plans/2026-10-14-kete-ado-workshop-delivery.md
Spec: docs/superpowers/specs/2026-10-14-ado-integration-design.md (Audit resolutions override). Visual: docs/design/kete-desktop-brief.md
Authority: Jay authorised unattended run, auto-approve, merges and force-push (2026-10-14 night).
Branch: feat/ado-workshop off windows, after the rebrand merge.

## Pre-flight scan
| Pair / task | Shared | Finding | Ruling |
|---|---|---|---|
| T1↔T6/T7/T10/T13 | AdoRemote, naming fns | consistent names | ok |
| T2↔T3 | Credential.header, TokenSource::token | consistent | ok |
| T3↔T6..T10 | AdoClient get/post_json/patch_json/put_json | consistent | ok |
| T4↔T7/T9/T10/T13 | AdoStore with_repo, remember_reviewer, ui | T13 needs ado_save_ui command, no owning backend task | Ruling: T13 adds ado_save_ui + ado_context commands itself — the UI task owns its glue — cost: none |
| T5↔T9 | strip_ai_trailers, rewrite_unpushed | name says "ai" but strips all co-authors (per Jay) | Ruling: keep the name, doc comment says it strips all — cost: naming only |
| T7↔T8 | PrRef filled from prs | T7 precedes T8 | Ruling: T7 leaves pr None, T8 fills it in ado_branches — cost: none |
| T8 main.rs poll_prs | gh vs ADO | GitHub path kept | ok |
| T11↔T16/T17/T18 | diff_range/diff_blob/hunks | consistent | ok |
| T12↔T14 | parent, subagents | consistent | ok |
| T13↔T15..T18 | nav, Chip, api.ts (T15 creates api.ts) | T13's Rail needs ado_context in api.ts before T15 | Ruling: T13 creates lib/ado/api.ts with ado_context/ado_save_ui; T15 extends it — cost: none |
| T20↔T21 | release assets | consistent | ok |
| Each task self-consistency | tests vs code | T5 test "rewrite_only_touches_unpushed" needs bare-remote setup; fine | ok |
| T21 commit rewrite | force push | authorised by Jay | Ruling: rewrite only messages a00b760..HEAD (our commits) — cost: SHAs change, forks none |

Rebrand merged 6f737a4, pushed windows+main; work branch feat/ado-workshop

Ruling: run in waves of parallel implementers, each in its own git worktree on a disjoint file set, merged into feat/ado-workshop by the controller between waves — 21 sequential tasks cannot finish overnight and Jay asked for completion by morning — cost if wrong: merge conflicts in main.rs command registration, resolved at merge.
Ruling: bundle T1-T4 into one implementer (one ado/ module, strictly sequential interfaces) — cost: a bigger review surface.
Waves: W1 = [T1-T4 bundle], [T12], [T19]. W2 = [T5], [T6+T10], [T7], [T8+T9], [T11]. W3 = [T13+T14]. W4 = [T15+T16], [T17], [T18]. W5 = [T20]. T21 by controller + one agent.
Ruling: reviewers read the skill's reviewer template from .superpowers/sdd/2026-10-14-kete-ado-workshop-delivery/reviewer-instructions.md with placeholders supplied in the dispatch — keeps dispatches small — cost: none
Task 19: implemented 15abd54 (impl a254fb73), review dispatched. Ruling: make kete-dark/kete-light the default theme and the target of 'auto' — the brief says Workshop is the identity — cost: users who liked One Dark switch it back in settings. Carried as a fix item into T19's review loop.
Task 19: review Approved (spec ✅). ⚠️ invariant tests: implementer reports nebula-tui 1344 passing; controller re-runs root gates at merge. ⚠️ is_our_scheme: checked at merge via setup dry-run.
Task 19: minor (deferred): one_dark_scheme() only used by tests; theme.rs doc line long; brightBlack contrast ~3:1 on soot.
Task 19: fix round 1/5 dispatched (controller ruling: kete-dark default + auto -> kete presets).
Task 19: fix round 1/5 (3 addressed, 0 Critical/Important open; commits 15abd54..6fb2308)
Task 19: minor (deferred): docs/configuration.md theme row still says fresh config starts on one-dark on Windows; vscode fallback still ONE_DARK.
Task 19: complete (commits 6f737a4..6fb2308, review clean)
Task 1-4: implemented 837fad3..5938d6f (DONE_WITH_CONCERNS: no token cache, with_repo drops write errors); review dispatched. Note: worktree agents cannot write to the main checkout's .superpowers; controller copies reports.
Task 1-4: review Needs fixes — Important: store write errors silently dropped. ⚠️ off-UI-thread: later command tasks must await token() in async commands (carried into T6-T10 dispatches). ⚠️ useHttpPath multi-org: verify in live smoke.
Ruling: pull the token cache (keyed org_url, expires on JWT exp, cleared + `git credential reject` on 401) and a non-persisting read_repo into this fix round — later tasks make many calls per view and need a read path — cost: a slightly larger fix round.
Task 1-4: minor (deferred): az dead branch; builder unwrap_or_default; Retry-After HTTP-date; no sync_all before rename; legacy remote shapes; case-insensitive branch collision + HEAD; test for second 429; client test through real TokenSource.
Task 12: implemented 222ab57 (DONE_WITH_CONCERNS); review dispatched.
Ruling: bump MIN_COMPATIBLE_PROTOCOL to 53 as well — rmp positional snapshots make v52 clients fail mid-stream otherwise; a clean handshake refusal is better, and TUI/desktop/daemon ship as one release — cost: an older kete TUI must upgrade to talk to a new daemon (kete upgrade already handles this).
Ruling: parent comes from NEBULA_AGENT_ID (no KETE_SESSION exists) — plan named a non-existent var — cost: none.
Task 1-4: fix round 1/5 (4 addressed, 0 open; commits 5938d6f..a5129f0)
Task 1-4: minor (deferred): AdoStore::open treats locked/permission read errors as empty (first write overwrites); az 401 retry may reuse same token.
Task 1-4: complete (commits 6f737a4..a5129f0, review clean)
W2 dispatched (3 at a time for CPU): T5 (a5ba449b), T6+T10 (a4a1ca03), T8+T9 (a978abcf). T7, T11 queued. Ruling: T8+T9 builds a stub attribution.rs replaced by T5's at merge, and its own PrCtx unified with AdoContext at merge — parallelism — cost: a merge reconciliation step.
Task 12: review Needs fixes — Important: PreToolUse matcher excludes Agent/Task (descriptions empty); restart drops running subagents. Fix round 1 dispatched with MIN=53 ruling + minors 1-4.
Ruling: carry the subagent book across in-place restarts, keeping rows running; uncarried rows become EndedUnknown — better than the spec's blanket "ended (unknown)" because the agents really are still running — cost: more handoff state.
Task 12: minor (deferred): exit.rs close_subagents before was_registered race (tiny window).
Task 12: fix round 1/5 (4 addressed, 0 open; commits 222ab57..90f9db5)
Task 12: minor (deferred): restore_subagents doesn't re-apply ENDED_KEPT; PROTOCOL_VERSION doc wording loose.
Task 12: complete (commits 6f737a4..90f9db5, review clean)
Task 8-9: implemented 28b7fb4..f9bfdb8 (base 6e38c36; worktree started on windows and was reset by the agent). Review dispatched.
Note: Agent isolation:worktree branches from the repo's default/checked-out ref inconsistently — future dispatches tell agents to `git reset --hard feat/ado-workshop` first.
Ruling: connectionData uses api-version 7.1-preview.1 — the endpoint is preview-only — cost: none if confirmed in live smoke.
Ruling: fold PrState's TokenSource/AdoStore into one shared AdoContext at merge (one token cache, one sign-in prompt) — cost: merge work.
Task 5: implemented 3a15308 (base 6f737a4). Codex instruction line not added — no instructions channel in harness.rs.
Ruling: add the Codex "plain commit messages, no co-author trailers" line via the UserPromptSubmit injection after T12 merged (it has) — as a T5 fix-round item — cost: one extra line per Codex prompt.
Task 8-9: review Needs fixes — Important: local failures shown as "Azure DevOps refused". Fix round 1 dispatched with rulings: vote refuses drafts; background poll uses silent tokens only; minors (double thread fetch, blind link PATCH, empty title, no-user error variant).
Task 8-9: minor (deferred): list fails whole on one hydration error + sequential GETs; process-global REPO_IDS/MY_IDS never expire; PrCtx::from_worktree/vote_label dead code.
Task 8-9: ⚠️ live smoke must confirm vssps identity id is accepted as reviewers[].id.
Task 5: review dispatched.
Note: Linux Docker checks must use a private target volume per worktree (shared /src mount gives false errors).
Task 5: review Needs fixes — Important: upstream exclusion dead (refs/heads/b@{upstream}). Fix round 1 dispatched (rebase onto feat, Codex line via UserPromptSubmit, minors: non-UTF8 refuse, base verify, branch normalise, copilot installs independent).
Ruling: Copilot attribution settings only in Copilot-spawned worktrees — only Copilot reads that file — cost: none.
Task 5: minor (deferred): shared unpushed local branches diverge after rewrite (doc note); tracked copilot settings file edge; attribution object overwrite.
Task 6-10: implemented 00060aa..53fd0ee (base 6e38c36); review dispatched. Rulings: offline task cache + set-team command + task_prompt command added in fix round (spec requires offline cache & team picker) — cost: more work.
Task 7: implemented d31e1ba (base 7fa3f81); review dispatched. Task 8-9: fix round 1 done 68d5e5f; re-review dispatched.
Task 8-9: fix round 1/5 (all addressed, 0 open; commits f9bfdb8..68d5e5f)
Task 8-9: complete (commits 6e38c36..68d5e5f, review clean)
Ruling: after W2 merges, one integration dispatch unifies PrState into AdoState/AdoContext (single TokenSource + AdoStore), dedupes patch_json_patch, swaps the attribution stub for T5's real module + wires find_unpushed note, and links PrRef into branches — the parallel split created these seams — cost: one extra task + review.
Task 7: review Approved. Ruling: ado_checkout_worktree returning the existing worktree is the right UX (open it) — the spec's refusal applies to creating a new branch in an occupied worktree — cost: none.
Task 7: minor (deferred): BranchRow literal duplicated; double model snapshot; branch -d refuses squash-merged (UI decides force); ado_fetch no timeout; case-sensitive person match; refusal guards untested.
Task 7: complete (commits 7fa3f81..d31e1ba, review clean)
Task 7: merged e0ea2b0 (registration conflict resolved by controller: union of both command blocks).
Task 6-10: review Approved. Fix round 1 dispatched (rulings: offline cache, set-team, task_prompt cmd, base = ADO defaultBranch, Iteration=mine with consistent Completed handling; minors 2,3,4,5,6; rebase onto feat incl. Local/patch_json_patch dedupe).
Task 6-10: minor (deferred): NewAgent.worktree serde(default) loosens create_agent; no TDD red stage observed.
Task 5: fix round 1/5 (all addressed, 0 open; commits 87400cd..0e2a849)
Task 5: complete (commits 7fa3f81..0e2a849, review clean)
Task 5: merged (real attribution.rs replaces the T8-9 stub). Integration breakage carried to the integration task: (a) find_unpushed_ai_commits unused → clippy dead_code; (b) prs::create passes <remote>/<target> base which may be unfetched → test a_failed_push_never_posts fails ("base nowhere/main not found"). Ruling: create() fetches the target ref first; if still missing, rewrite with --not --remotes only — cost: one fetch per PR create.
Task 11: implemented 97d5c3f..a2e075f (base 7fa3f81); review dispatched.
Task 6-10: fix round 1/5 (all addressed, 0 open; commits 1c54ec2..2bb7a42)
Task 6-10: minor (deferred): unset-upstream side effect untested; empty-repo silent base fallback; cache key ignores default-team change; 30s rpc timeout on CreateWorktree.
Task 6-10: complete (commits e0ea2b0..2bb7a42, review clean)
Integration task dispatched (brief: integration-brief.md). Task 6-10 merged 6005905.
Task 11: review Needs fixes — Important: ensure_refs never refreshes present refs (stale PR/target diffs). Fix round 1 dispatched (+ minors 2-8, rebase onto feat).
Task 11: fix round 1/5 (all addressed, 0 open; commits 05e1a88..17eb515)
Task 11: minor (deferred): offline flag set when target branch only local; git_fetch pipe drain; ensure_refs dead code + #[allow(dead_code)] on merge_base/diff_files_range/ensure_refs (plan-named, unused).
Task 11: complete (commits 6005905..17eb515, review clean)
Integration: implemented 7edc1da..d648105 (base 6005905); review dispatched. Ruling: when there is no base AND no remote-tracking refs, the clean is skipped entirely (never rewrite a whole local history) — cost: trailers could ship on a brand-new repo's first push; carried into the integration fix round.
Integration: review Approved. Fix round 1 dispatched. Ruling: on the publish path, clean commits even when they sit on a local upstream — the no-co-author rule is a hard rule from Jay and outranks keeping a local upstream branch's SHAs — cost: that local upstream diverges from the published branch (rare: hand-made stacked branches).
Integration: minor (deferred): REPO_IDS/MY_IDS statics outside AdoState (double connectionData); take_last_error unused (store write failures invisible) — UI task should surface it.
Integration: fix round 1/5 (all addressed, 0 open; commits d648105..98d1aad)
Integration: complete (commits 6005905..98d1aad, review clean)
Task 13-14: implemented bafc77c..6dac921 (DONE_WITH_CONCERNS; touched Board/Session/Diff/Terminal/QuickPrompt/Empty, deleted Sidebar/StatusDot). Note: agent ran stale kete.exe un-isolated once; Jay's bro data migrated to kete\data (expected first-run behaviour; profiles/jayrich verified present).
Task 13-14: review dispatched (opus).
Task 13-14: review Needs fixes — Important: overview scroll fires on every navigation. Fix round 1 dispatched with all 9 minors (spinner, ember exclusivity, rail count words, reduced-motion retract, askAdo retry, ADO entries for every ADO repo, QuickPrompt pick + chevrons, save_ui view allowlist, Board retract).
Task 13-14: minor (deferred): committed desktop/screenshots binaries; deleted public/kete-64/256.png; vite __dirname change.
Ruling: tell Jay in the morning that his bro data moved to %LOCALAPPDATA%\kete\data (the designed migration ran early via a stale binary; profiles/jayrich intact) — cost: none.
Task 20: implemented a8dc53f (base 58713ab); review dispatched. Ruling: NSIS install dir must not be %LOCALAPPDATA%\Kete (kete's data root) — use %LOCALAPPDATA%\Programs\Kete — cost: none.
Task 13-14: fix round 1/5 (all addressed, 0 open; commits e2e8f3b..30a24f0)
Task 13-14: minor (deferred): rail pick from a non-overview view doesn't scroll after mount (revealed initialised to current seq).
Task 13-14: complete (commits 58713ab..30a24f0, review clean)
Task 13-14: merged (ff to 30a24f0).
Task 20: review Approved. Fix round 1 dispatched (install dir ruling, iex/param verification, download error detail, end-of-run failure line, kete desktop finds NSIS dir, no bin copy).
W4 dispatched: T15+16 (a8ad4403), T17 (a0264e22). T18 queued after one finishes (CPU). Ruling: each view puts wrappers in its own *-api.ts and only swaps its App.svelte placeholder — parallel merge safety — cost: three small API files instead of one.
Task 20: fix round 1/5 (all addressed, 0 open; commits a605601..9ca5f1f)
Task 20: minor (deferred): custom NSIS template must be re-copied on tauri-cli upgrades; uninstall leaves HKCU install-path key; desktop-windows CI job unrun until first tag.
Task 20: complete (commits 30a24f0..9ca5f1f, review clean)
Task 18 dispatched (DiffPane reusable component, diff-api.ts).
Task 15-16: implemented 560aa19..076f8ea (base 30a24f0); review dispatched.
Ruling: no auth weakening for demos (the permission classifier was right to refuse a non-test fixed credential); screenshots come from a browser IPC stand-in with labelled synthetic data — cost: screenshots don't exercise the Rust backend.
Ruling: "Diff vs main" will route to the Diff view's DiffPane once Task 18 lands (follow-up wiring in the final integration) — cost: interim inline list.
Task 15-16: review Needs fixes — Important: window Enter hijack; Branches reload failures hidden. Fix round 1 dispatched with all minors (incl. demo files moved to desktop/demo/).
Task 15-16: minor (deferred): ado_* wrappers spread over three api modules (ruled earlier for parallel safety).
Task 17: implemented d3bb13b..99945d3 (base 30a24f0); review dispatched with explicit security check of KETE_ADO_BASE dummy-token path.
Task 15-16: fix round 1/5 (all addressed, 0 open; commits 076f8ea..071cc9c)
Task 15-16: complete (commits 30a24f0..071cc9c, review clean)
Task 17: review Needs fixes — Important (security): KETE_ADO_BASE prefix check bypassable via userinfo URL → dummy token + PR content to real ADO. Ruling: parsed-URL loopback check AND override compiled only under cfg(debug_assertions) — release binaries never read it — cost: demo screenshots need a debug build.
Task 17: fix round 1 dispatched (+ Mine reload, retry task id, reset vote, breadcrumb, composer entries from Branches/Tasks).
Task 18: implemented feb958b..aefcb87 (base 9ca5f1f); review dispatched.
Task 17: fix round 1/5 (all addressed incl. security; 0 open; commits 26d3a59..ac9c11b)
Task 17: minor (deferred): prsState.created never consumed (forces Mine tab on remount, any repo); duplicate load() after create; review diff uses origin/<source> not lastMergeSourceCommit.
Task 17: complete (commits 063687b..ac9c11b, review clean)
Task 18: review Approved.
Task 18: minor (deferred): landOn fires on wrong file; window F7 before input guard; F7 skip race; Discard confirm no Esc/focus; timing test flake risk; line count +1; Missing/Binary states unscreenshotted.
Task 18: complete (commits 9ca5f1f..aefcb87, review clean)
Ruling: one UI consolidation task next — PrDiff becomes a DiffPane wrapper, Branches "Diff vs main" uses DiffPane/openDiffRange, Blob/RangeDiff/DiffSide types imported from diff-api.ts, plus the cheap deferred UI minors — the parallel split duplicated the diff UI — cost: one more task + review.
Task 18 merged a724040 (Breadcrumb import conflict resolved by controller: both imports). Consolidation dispatched.
Controller gate on a724040: fmt ✓, clippy ✓, Linux Docker clippy EXIT=0 ✓, cargo test: 1 flaky failure (branch_switch a_refused_switch... "git program not found") passing on 2 reruns — a test mutating PATH in parallel.
Controller: minor (deferred to final review): PATH-mutating tests race with git-spawning tests in nebula-tui (serialise with a shared env lock).
Consolidation: implemented 255cd02..d7331b2 (10 commits, 28 screenshots); review dispatched. Controller viewed composer-light + tasks-dark: good.
Consolidation: review Approved.
Consolidation: minor (deferred): DiffPane toolbar truncates totals at default split (range mode unreadable); dead diff wrappers in prs-api/branches-api; stray blank line; "no changes" on too-large; stale overview reveal unbounded; thin tests for created/reveal; quick-prompt select clips value.
Consolidation: complete (commits a724040..d7331b2, review clean)
Consolidation merged d7331b2. All tasks complete. Final whole-branch review dispatched (opus) over 6f737a4..d7331b2 with ledger triage.
Final review: With fixes — Important #1-6 block main (silent 401 rejects GCM; CreateWorktree 30s timeout; ado.json wiped on transient read error; review diff unpinned; sign-in UX missing; PATH test race). Plus #7 (concurrent PR hydration), #8 (reconcile outcome), and minors 1,2,5,6,7,8 + ado_fetch timeout + force delete. ONE fix wave dispatched.
Final review triage: all other deferred minors and rulings accepted to ship (see final review report).
Live-smoke checklist for Jay: vssps id as reviewers[].id; connectionData preview api; reviewerId vs group reviewers; Codex additionalContext; GCM credential.interactive=never; useHttpPath multi-org.
Final fix wave: implemented d7331b2..f87d428 (10 commits). Scoped re-review dispatched.
Final fix wave: re-review all addressed (9/9), no new Critical/Important; commits d7331b2..f87d428.
Ruling: "Signing in…" excludes Create PR and Start task (push/checkout would trigger it falsely; they keep their own progress labels) — cost: a sign-in prompt during those two shows their own label instead.
Final fix wave: minor (deferred): global signin.slow/error flags; sign_in resumes poll on failure; concurrent 401 double-reject window; Branches checkout reconcile untested (240s wait); duplicate merge-ref fetch in ^1..^2 fallback.
Merged feat/ado-workshop into windows (ff) and pushed windows + main at f87d428.
Ruling: release tag is v0.45.0 (not v0.1.0) — kete upgrade/update-check compare the binary's version against release tags; restarting at 0.1.0 would make installed 0.44 builds ignore it — cost: version number continues nebula's line.
Task 21 (content): 4a252e7 version 0.45.0, b5f5f5b README scenarios + CHANGELOG; merged ff into windows (not pushed yet).
CI on main f87d428: unix failures — desktop icons not RGBA (tauri generate_context panics on linux/mac); e2e_tui tui_git_diff_modal + tui_project_rename (unix-only) don't expect icon glyphs. Fix dispatched.
Task 21: GitHub social preview set (usesCustomOpenGraphImage=true, verified via GraphQL).
CI fix: 7406bef icons RGBA, 2f285f3 e2e_tui harness pins theme default + icons off (cause: kete-dark truecolor default vs indexed selection check). Merged + pushed.
History rewritten (128 msgs, trees identical) and force-pushed main+windows at 5074e23. Tag v0.45.0 pushed.
