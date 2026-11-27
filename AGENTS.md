# Raiden

A keyboard-first launcher for macOS. Read `CONTRIBUTING.md`: its rules apply to agents too. This
file adds how agents work here.

## Starting

- Never edit in the main checkout. Start a worktree with a short lowercase name using hyphens:
  `git fetch origin main && git worktree add .worktrees/<name> -b <name> origin/main && cd .worktrees/<name> && mise trust && mise run setup`
- Run every tool through mise: `mise x -- <command>`, or a task with `mise run <task>`.

## Landing

- Before landing, start a sub-agent that has not seen your work, and give it only the task and
  `git diff origin/main...HEAD`. Fix what it finds, or say why not. If the code changes after the
  review, have the new diff reviewed again.
- Commit every file, new ones included. No co-author lines.
- Land on `main` directly: `git fetch origin main && git rebase origin/main && git push origin HEAD:main`.
  The pre-push hook runs lint and tests first. If `main` moved, rebase and push again.
- Then check `main`'s build: `gh run list --branch main --limit 1`.

## Finishing

- Reinstall from the worktree, which now matches `main`, so the owner sees the change:
  `mise run install`.
- Then, from the main checkout: `git worktree remove .worktrees/<name> && git branch -d <name>`,
  and `git merge --ff-only origin/main` to keep it current.
- End with the commits that landed, the review's findings, how `main`'s build went, and anything
  left undone.
