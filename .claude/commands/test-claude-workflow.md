---
description: Test the Claude Review GitHub workflow (setup checks, then a real run on GitHub)
argument-hint: "[check | deps | pr]"
---

Test `.github/workflows/claude-review.yml`, the workflow in which Claude reviews every pull request
and writes the weekly "Dependency report" issue. The README section "Automatic review by Claude"
describes the setup.

Mode: `$ARGUMENTS` (empty means `check`).
- `check`: local checks and setup checks only. Nothing is started on GitHub.
- `deps`: `check`, then run the dependency report job on GitHub and look at the issue it writes.
- `pr`: `check`, then open a throwaway pull request and look at the review Claude posts on it.

## 1. Local checks (every mode)

1. The file parses as YAML and has the jobs `review` and `dependencies`:
   `python3 -c "import yaml; d=yaml.safe_load(open('.github/workflows/claude-review.yml')); print(list(d['jobs']))"`.
   If `actionlint` is installed, run it on the file too.
2. Both jobs pass `claude_code_oauth_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}`, and nothing
   uses `anthropic_api_key`.
3. The workflow's local steps work: in `mpc-rv/`, run `cargo update --dry-run --verbose` (lists
   outdated crates; changes nothing) and, if `cargo audit` is installed, `cargo audit`. Show a
   short summary of each.

## 2. Setup checks (every mode)

1. `gh auth status`. If the GitHub CLI isn't logged in, stop and tell the user to run
   `gh auth login`; the remaining checks need it.
2. `gh secret list` shows `CLAUDE_CODE_OAUTH_TOKEN`. If it's missing, point to the README steps
   (`claude setup-token`, then Settings > Secrets and variables > Actions). Never ask for the token
   or print it.
3. `gh workflow list` shows "Claude Review". If it doesn't, the workflow isn't on GitHub's default
   branch (`main`) yet: say so. GitHub only lists a workflow and allows "Run workflow" once it's on
   the default branch, though pull request reviews already work from a release branch.
4. The Claude GitHub App can't be checked from here. Say that a run failing with an app or
   permission error means it isn't installed (https://github.com/apps/claude).

In `check` mode, stop here and report.

## 3a. `deps`: run the dependency report

1. Ask the user before starting it, since it uses their Claude plan.
2. `gh workflow run claude-review.yml --ref <current branch>`, then find the run with
   `gh run list --workflow claude-review.yml --limit 1` and follow it with `gh run watch <id>`.
3. If it failed, show the failing step's log (`gh run view <id> --log-failed`) and explain the
   likely cause (missing secret, expired token, app not installed).
4. If it passed, show the "Dependency report" issue:
   `gh issue list --search "Dependency report in:title"`, then `gh issue view <number>`.

## 3b. `pr`: test a review on a throwaway pull request

1. Ask the user before pushing anything, and say a test PR will be opened and then closed.
2. Make a branch `test/claude-review-<date>` from the current `release/*` branch (the newest one on
   origin). On it, add a small, clearly marked test change with one obvious bug Claude should
   catch: for example, a new function in `mpc-rv/src/config.rs` that indexes a `Vec` with an
   unchecked value from a file. Commit it as "TEST: Claude review (do not merge)".
3. Push it and open a pull request against that release branch, titled
   "TEST: Claude review (do not merge)".
4. Follow the "Claude Review" run for that PR (`gh run list --workflow claude-review.yml
   --branch <branch> --limit 1`, then `gh run watch <id>`).
5. Show what Claude posted: the summary comment (`gh pr view <number> --comments`) and the inline
   comments (`gh api repos/{owner}/{repo}/pulls/<number>/comments`). Say whether it found the
   planted bug.
6. Ask the user, then clean up: `gh pr close <number> --delete-branch`, switch back to the branch
   you started on, and delete the local test branch.

## Report

End with a short checklist of what passed, what failed and what the user needs to do, such as
adding the secret, installing the app, or merging the workflow into `main` so "Run workflow" works.
