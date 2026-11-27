# Contributing

Thanks for helping with Raiden. This file covers how to build it, how changes are made, and what a
good change looks like. Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).

## Set up

Raiden builds on macOS only. Install [mise](https://mise.jdx.dev) as the README describes, then:

```sh
mise trust
mise install
mise run setup
```

`mise run setup` installs the dependencies and turns on the repository's git hooks.

## Before you start

- **Every change goes through a pull request**, from everyone, maintainers included. Nobody pushes
  to `main`.
- **Agree on a feature before building it.** Open an issue first and wait until a maintainer says
  it's wanted. A pull request for a feature nobody agreed to may be closed. Fixes can go straight to
  a pull request.
- **Keep at most 3 pull requests open.** See one through before starting the next: a stack of open
  ones goes stale as `main` moves.
- Issues labeled `good first issue` are a good place to start.

## Make a change

- **Keep it small.** One change per pull request: a fix, one step of a feature, a rename. A large
  rename or move goes in its own pull request.
- **Stay current.** Rebase on `main` instead of merging it in:
  `git fetch origin main && git rebase origin/main`.
- **Check it.** `mise run lint` and `mise run test` must pass. The pre-push hook runs both, so a push
  that fails them never leaves your Mac. `mise run test` includes the screens' behavior tests in
  WebKit.
- **Try it.** `mise run dev` runs a development copy beside the installed Raiden, on ⌥⇧Space.

## Write code

- **No comments.** Clear names, small functions and the pull request's description carry the why.
  Write one only when the code would be dangerous to change without it.
- **Tests come with the change.** A fix comes with a test that fails without it.
- **Nothing secret.** Never put API keys, tokens or personal data in code, tests or fixtures.

## AI

AI tools are welcome here, and Raiden is built with plenty of help from them. A person still
answers for every change:

- **Understand what you send.** You must be able to explain every line of your pull request, and how
  it fits the rest of Raiden, without asking an AI.
- **Write to people yourself.** Write the pull request description and your replies to reviewers in
  your own words. If you quote an AI, put it in a quote block and say so.
- **Say which tool you used**, such as Claude Code or Codex, in the pull request.
- **No pull requests from agents running on their own.** A person reviews the work before it is
  opened, and answers for it afterward.

## Commit messages

[Conventional commits](https://www.conventionalcommits.org), with the part of Raiden as the scope:

| Scope | Covers |
| --- | --- |
| `app` | The native side in `src/` |
| `ui` | The screens in `ui/` |
| `core` | Search, settings, answers and the rest in `core/` |

The type is `feat`, `fix`, `refactor`, `test`, `docs`, `style`, `ci` or `chore`, and the title
says what changed in under 72 characters. The why goes in the pull request's description:

```
feat(ui): Tab and Shift Tab switch Pull Requests tabs
fix(core): skip clipboard items marked concealed
docs: explain the development copy
```

Leave out the scope for changes to the whole repository.

## Pull requests

- Pull requests are squashed into one commit when merged, and the pull request's title becomes that
  commit's message, so write the title the same way.
- Fill in the template: what changed and why, how you tested it, and the AI tool you used.
- Include a screenshot or a short recording for anything you can see.
- CI runs lint and tests on macOS, and must pass.
- The project's owner, larsx2, approves every pull request from someone else before it merges, as
  [CODEOWNERS](.github/CODEOWNERS) and the rules on `main` require. Every review comment gets
  resolved, and a push after approval needs a new approval. The owner may merge their own pull
  requests once the build passes.

## Releases

Pushing a version tag starts the `release` workflow. One job runs lint and tests, and another
builds Raiden.app, both without any secrets. A second job, which runs no build code, signs the app with the
Developer ID certificate, has Apple notarize the app and its disk image, and publishes
`Raiden-arm64.dmg` and its SHA-256 checksum as a GitHub release. The notes list the `feat` and `fix`
commits since the previous release.

To release:

1. Set `version` in `Cargo.toml` and run `mise x -- cargo check`, so `Cargo.lock` follows.
2. Open a pull request titled `chore: release 0.2.0` and merge it into `main`.
3. Tag the merged commit and push the tag:
   `git fetch origin main && git tag -a v0.2.0 -m "Raiden 0.2.0" origin/main && git push origin v0.2.0`.

A tag must be on `main` and match the version in `Cargo.toml`, or the workflow stops. A tag with a
hyphen, such as `v0.2.0-beta.1`, makes a prerelease. Running the workflow by hand on `main` from
the Actions tab builds, signs and notarizes without publishing, and keeps the download for 7 days.

### Signing set-up

The workflow reads these secrets from the `release` environment, in the repository's Settings under
Environments. Its deployment rules let only the `main` branch and tags matching `v*` use it, and a
tag ruleset on `v*`, under Settings, Rules, lets only admins create, move or delete those tags. A
workflow on any other branch or tag can't read the secrets:

| Secret | Holds |
| --- | --- |
| `DEVELOPER_ID_CERTIFICATE` | The Developer ID Application certificate and its private key, exported from Keychain Access as a `.p12` file, in base64: `base64 -i raiden.p12 \| pbcopy` |
| `DEVELOPER_ID_CERTIFICATE_PASSWORD` | The password chosen when exporting the `.p12` file |
| `NOTARY_KEY` | The contents of an App Store Connect API key (`AuthKey_….p8`), made in App Store Connect under Users and Access, Integrations, with the Developer role |
| `NOTARY_KEY_ID` | That key's ID |
| `NOTARY_ISSUER` | The Issuer ID shown above the key list |

`mise run release` makes the same download on a Mac. It signs with the Developer ID Application
certificate in your keychain, and notarizes with the login saved by
`xcrun notarytool store-credentials raiden`. The download lands in `target/dist/`. Set
`RAIDEN_SIGNING_IDENTITY` to choose among several certificates, and `RAIDEN_NOTARY_PROFILE` to use a
login saved under another name.

## Security

Report security problems privately, as [SECURITY.md](SECURITY.md) explains, never in an issue.
