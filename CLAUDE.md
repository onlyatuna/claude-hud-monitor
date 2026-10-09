# Autonomous Development Workflow — qtrs

Goal: the Rust `qtrs` HUD (`rust/qtrs`) matches the Python/PySide6 semantics. The authoritative
progress record is `rust/qtrs/PYTHON_QT_SEMANTIC_CONTRACT.md` (the Contract).

## 1. Standing authorization

The user has authorized the following. Do not ask before each one:

- After finishing an RC: run the required verification and create its own commit.
- After it passes acceptance: `git push origin main:develop`.
- After pushing: check the push CI. When needed, trigger the qtrs diagnostics workflow manually.
- Analyse CI failures, rerun the right tests, compare against the baseline, and classify each problem.
- Update the Contract, its evidence and its Residuals. Then continue with the next scheduled RC.
- Delegate read-only root-cause audits to Haiku M2 and check its conclusions yourself.

Never stop to ask "push?", "continue to the next item?", or about any other authorized routine
step. At the end of each phase, report the result briefly and go straight to the next step.
Reporting progress does not end the work.

## 2. Fixed procedure for every RC

1. Read the Contract, the qtrs source involved, the Python implementation and the Qt source.
2. Confirm the root cause. Never write an inference as a verified fact.
3. Design a regression test that catches the real root cause. Where it applies, keep the
   before-fix FAIL evidence.
4. Change only this RC's scope. Do not mix in other gaps.
5. Run tests, clippy, formatting and platform checks that match the risk of the change.
6. Update the Contract, the verification record and the Residuals.
7. Make one complete, self-contained commit per RC. Never squash different RCs together.
8. Check the working tree and the list of commits to push. After acceptance, push to develop.
9. Check the push CI. Trigger the manual diagnostics workflow explicitly.
10. Confirm the results, record the evidence, and continue with the next scheduled RC.

Never output only a plan or suggested commands for steps the available tools can carry out.

## 3. Push and Git safety

- Target branch `develop`, source branch `main`.
- Before pushing: `git status --short` and `git log --oneline origin/develop..main`.
- Push only accepted commits with a clear scope.
- Never force-push `develop` or `main`, rewrite remote history, or reset the user's work.
  - The only exception is the scratch CI trial branch `ci/qtrs-tests`, which exists for
    before/after trials: `git push -q -f origin main:ci/qtrs-tests`.
- Never push unrelated existing commits along with yours.
- Never amend or rewrite a commit that has already been pushed, whether to change its message or
  the Contract.
- If the branches have diverged unexpectedly, a commit's origin is unclear, or the working tree
  holds someone else's changes: protect the current state and work out a safe way forward first.
  Never push blindly.
- Never `git add` `rust/qtrs/tools/second_layer_harness/results.txt` or `results_qtrs.txt`.

## 4. Handling CI failures

Do not stop at a CI failure. Do not assume the current RC caused it.

- Get the full log. Find the actual failing test, assertion and exception.
  - `gh api repos/onlyatuna/claude-hud-monitor/actions/jobs/<id>/logs`
- Rerun flaky tests where appropriate. If needed, compare against the baseline commit.
- Classify each failure as one of: new regression, known flaky, existing baseline failure,
  environment, or not yet determined.
- A new regression from the current RC: keep investigating and fixing. Never mark a failing RC
  done.
- A failure with solid evidence that it is an unrelated, existing flaky failure: keep the
  evidence and continue with unaffected work. Do not mix unrelated fixes in just to get green.
- Always record the real CI result. A local pass never replaces cross-platform CI evidence.
- Never `gh run watch`. Poll only the job you need, in a background loop with `sleep 30`.

Never just report a CI failure and wait for instructions. Do the diagnosis and isolation you can
do first.

## 5. Using Haiku M2

- When the root cause is unclear or the impact is broad, you may first give Haiku M2 a read-only
  independent audit.
- During the audit, M2 must not change production code.
- Opus must check M2's files, line numbers, call chains and inferences itself.
- The final conclusion rests on source code, real runs, test results and CI evidence.
- Once the root cause is confirmed, go on to implementation and verification without waiting for
  the user to ask again.

## 6. Ordering and stop conditions

Choose the next item from the RC order the user has confirmed and from the Contract, by severity,
dependencies and risk. Record extra gaps you find as separate Residuals or to-dos. Do not fold
them into the current RC.

Pause and report to the user only in these cases:

- Credentials, permissions or external resources that only the user can provide are missing.
- The work would need an unauthorized destructive operation or would overwrite user data.
- Two reasonable options differ in major, irreversible product semantics, and neither the
  Contract nor the existing design decides between them.
- A tool or the environment genuinely prevents further progress.

Do not ask for confirmation of ordinary implementation choices, reversible engineering decisions
or routine pushes.

## 7. Definition of done

A phase is done only when its acceptance criteria, evidence record, commit state and required CI
checks have all been handled. Keep working through the scheduled items until a real blocker from
§6 appears or the user asks you to stop.

The final report states the completed RCs, their commits, push status, CI results and the gaps
still open. Never present a plan as completed work.

## Project reference

- Full local verification (from the repo root):
  `cd rust/qtrs && cargo test -j 1 --workspace --no-fail-fast -- --test-threads=1`, then from
  `rust/`: `cargo test -j 1 -- --test-threads=1`, `cargo clippy -j 1 -- -D warnings`,
  `cargo fmt --check`.
- Formatting: run `rustfmt --check --edition 2021` on touched files only and compare against
  `git show HEAD:<path> | rustfmt --check --edition 2021`. Never mass-format `rust/qtrs`.
- Native macOS evidence comes from `examples/appkit_main_thread.rs` in the qtrs macOS CI job
  (`qtrs Workspace Test (macos-latest)`).
- CI trial: push to `ci/qtrs-tests`, then run
  `gh workflow run ci.yml -R onlyatuna/claude-hud-monitor --ref ci/qtrs-tests`.
- Diagnostics after a develop push:
  `gh workflow run ci.yml -R onlyatuna/claude-hud-monitor --ref develop`.
- Known flaky tests, never attributed to an RC:
  - tooltip timing tests
  - `test_layered_interactive_resize`
  - the agy live test
  - `config::tests::test_resize_debounce_*` (Windows and macOS)
- Reports go to the user in Traditional Chinese.
