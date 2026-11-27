# Contributing

Thanks for helping with Raiden. This file covers how to build it, how changes are made, and what a
good change looks like.

## Set up

Raiden builds on macOS only. Install [mise](https://mise.jdx.dev) as the README describes, then:

```sh
mise trust
mise install
mise run setup
```

`mise run setup` installs the dependencies and turns on the repository's git hooks.

## Make a change

- **Keep it small.** One change per commit: a fix, one step of a feature, a rename. A large rename
  or move goes in its own commit.
- **Stay current.** Rebase on `main` instead of merging it in:
  `git fetch origin main && git rebase origin/main`.
- **Check it.** `mise run lint` and `mise run test` must pass. The pre-push hook runs both, so a push
  that fails them never leaves your Mac. `mise run test` includes the screens' behavior tests in
  WebKit.
- **Try it.** `mise run dev` runs a development copy beside the installed Raiden, on ⌥⇧Space.

## Write code

- **No comments.** Clear names, small functions and the commit message carry the why. Write one only
  when the code would be dangerous to change without it.
- **Tests come with the change.** A fix comes with a test that fails without it.
- **Nothing secret.** Never put API keys, tokens or personal data in code, tests or fixtures.

## Commit messages

[Conventional commits](https://www.conventionalcommits.org), with the part of Raiden as the scope:

| Scope | Covers |
| --- | --- |
| `app` | The native side in `src/` |
| `ui` | The screens in `ui/` |
| `core` | Search, settings, answers and the rest in `core/` |

The type is `feat`, `fix`, `refactor`, `test`, `docs`, `style`, `ci` or `chore`, and the message
says what changed and why:

```
feat(ui): Tab and Shift Tab switch Pull Requests tabs
fix(core): skip clipboard items marked concealed
docs: explain the development copy
```

Leave out the scope for changes to the whole repository.

## Pull requests

- One change per pull request, with a description of what it does and why.
- Include a screenshot or a short recording for anything you can see.
- CI runs lint and tests on macOS, and must pass.

## Security

Please report security problems privately, through GitHub's
[security advisories](https://github.com/larsx2/raiden/security/advisories/new), not in an issue.
