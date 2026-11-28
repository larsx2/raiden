# Raiden

A keyboard-first launcher for macOS. Read `CONTRIBUTING.md`: its rules apply to agents too. This
file adds how agents work here, whoever runs them.

## Starting

- Never edit in the main checkout. Start a worktree with a short lowercase name using hyphens:
  `git fetch origin main && git worktree add .worktrees/<name> -b <name> origin/main && cd .worktrees/<name> && mise trust && mise run setup`
- Run every tool through mise: `mise x -- <command>`, or a task with `mise run <task>`.
- On a fork, add larsx2/raiden as `upstream` and use `upstream` wherever this file says `origin`,
  except when pushing your branch.

## Before a pull request

- Start a sub-agent that has not seen your work, and give it only the task and
  `git diff origin/main...HEAD`. Fix the bugs it finds and anything `CONTRIBUTING.md` requires, or
  say why not. Review again only if a fix or a later change rewrites how the code works, and then
  once: fix what that review finds without another round. List minor findings in the pull
  request's draft instead of fixing them one round at a time.
- Commit every file, new ones included. No co-author lines.
- Rebase on `main`: `git fetch origin main && git rebase origin/main`.

## Opening the pull request

- Open one only when the person you work for has seen the change and asks for it.
- Once the person has put the draft in their own words, push your branch with
  `git push -u origin <name>` and open the pull request with `gh pr create`.
  Its title is a conventional commit title, such as `fix(ui): ...`, because it becomes the commit
  on `main`. Without write access to larsx2/raiden, push to your fork and open the pull request
  against larsx2/raiden's `main` with `gh pr create --repo larsx2/raiden`.
- Fill in the template's AI section with which AI tool you are. Draft the other sections for the
  person, who puts them in their own words before the pull request is opened. Leave the checklist
  unticked: it is for the person to tick.
- Never push to `main`. Never merge a pull request, and never approve one. A person does that.
- End with the pull request's link, the review's findings and anything left undone.

## After it merges

- From the main checkout: `git worktree remove .worktrees/<name> && git branch -D <name>`, and
  `git fetch origin main && git merge --ff-only origin/main` to keep it current.
