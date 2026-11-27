<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/raiden-logo-dark.svg">
    <img src="assets/raiden-logo-light.svg" alt="Raiden" width="220">
  </picture>
</p>

<p align="center">
  A fast, keyboard-first launcher for macOS, with AI answers from the Claude and ChatGPT
  subscriptions you already have.
</p>

---

Press a hotkey, type a few letters, press Enter. Raiden opens apps and files, keeps what you copied,
lists your pull requests, opens folders in your terminal, and answers questions about whatever is
in front of you, all without leaving the keyboard.

> **Status: experimental.** Raiden runs only on macOS and is built from source. Expect rough edges.

## Contents

- [What it does](#what-it-does)
- [Requirements](#requirements)
- [Install](#install)
- [First run](#first-run)
- [Keys](#keys)
- [AI answers](#ai-answers)
- [Settings](#settings)
- [Privacy and security](#privacy-and-security)
- [Update and uninstall](#update-and-uninstall)
- [Develop](#develop)
- [Contributing](#contributing)
- [License](#license)

## What it does

- **Search.** Apps, settings, Raiden's own commands and, from 3 letters on, files in your home
  folder through Spotlight's index. Results learn from what you pick.
- **Ask AI.** Press <kbd>Tab</kbd> to turn what you typed into a question. Raiden answers as a chat,
  with the selected text and front window attached as context, colored code blocks, and follow-ups.
  Paste the answer straight back into the app you came from.
- **Clipboard history.** <kbd>⌃⌥V</kbd> shows the last 7 days of text, links, files and images you
  copied. Password managers' items and anything that looks like a key or token are never kept.
- **Fix grammar.** Corrects the selected text in place, in any app where you can edit it, using
  Claude Code.
- **Pull Requests.** Your open pull requests across your repositories and organizations, with
  checks, conflicts and size, read through your own `gh`.
- **Open in Terminal.** <kbd>⌘⇧T</kbd> opens a folder in Ghostty, iTerm2, Kitty, Alacritty, Warp,
  Rex, Tern or Terminal.
- **Vim keys**, if you want them: <kbd>j</kbd>/<kbd>k</kbd>, <kbd>gg</kbd>/<kbd>G</kbd>,
  <kbd>⌃D</kbd>/<kbd>⌃U</kbd>, <kbd>gt</kbd>/<kbd>gT</kbd>.
- **Two sizes.** <kbd>⌘⇧E</kbd> switches between compact and expanded, and Raiden remembers it.

## Requirements

- macOS. Raiden is built and tested on macOS 15 (Sequoia) on Apple silicon.
- Xcode Command Line Tools: `xcode-select --install`
- [Homebrew](https://brew.sh), to install mise.
- [mise](https://mise.jdx.dev), which installs the exact Rust, Node and pnpm versions Raiden builds
  with. Install it and hook it into your shell once:

  ```sh
  brew install mise
  echo 'eval "$(mise activate zsh)"' >> ~/.zshrc
  exec zsh
  ```

Optional, for the features that use them:

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) or
  [Codex](https://github.com/openai/codex), signed in, for Ask AI. Fix grammar uses Claude Code.
- [GitHub CLI](https://cli.github.com), signed in with `gh auth login`, for Pull Requests.

## Install

```sh
git clone https://github.com/larsx2/raiden.git
cd raiden
mise trust
mise install
mise run install
```

`mise run install` builds `Raiden.app`, copies it to `~/Applications`, links the `raiden` command into
`~/.local/bin`, and starts it. The first build takes a few minutes; later ones are much faster. If
`~/.local/bin` isn't on your `PATH` yet, add it so the `raiden` command works:
`echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc`.

The app is signed with a certificate made on your Mac, called "Raiden Local" and kept in its own
keychain (`~/Library/Keychains/raiden-signing.keychain-db`). It never leaves your Mac, needs no
Apple developer account, and lets macOS remember Raiden's permissions from one build to the next.

To build the app without installing it, run `mise run bundle`. It ends up in
`target/release/bundle/macos/Raiden.app`.

## First run

1. Press <kbd>⌥Space</kbd>. Raiden opens over whatever you were doing.
2. When macOS asks, allow Raiden in **System Settings → Privacy & Security → Accessibility**. Raiden
   needs it to paste answers into the front app, read the selected text and fix grammar.
3. Raiden adds itself to **Login Items**, so it starts when you log in and restarts if it crashes.
   Turn it off there, or from Raiden's Settings, whenever you like.

To open Raiden with <kbd>⌘Space</kbd>, first turn off Spotlight's shortcut in **System Settings →
Keyboard → Keyboard Shortcuts → Spotlight**, then set `hotkey = "⌘Space"` (see
[Settings](#settings)).

## Keys

| Key | Does |
| --- | --- |
| <kbd>⌥Space</kbd> | Open or close Raiden (change it in Settings) |
| <kbd>↵</kbd> | Open the selected result |
| <kbd>⌘↵</kbd> | Show the selected file in Finder |
| <kbd>Tab</kbd> | Ask AI about what you typed |
| <kbd>⌘K</kbd> | Every action for the selected item |
| <kbd>⌘,</kbd> | Settings |
| <kbd>⌃⌥V</kbd> | Clipboard history |
| <kbd>⌘⇧T</kbd> | Open the selected folder, or a file's folder, in your terminal |
| <kbd>⌘⇧E</kbd> | Expand or shrink the window |
| <kbd>↑</kbd> <kbd>↓</kbd>, <kbd>⌃N</kbd> <kbd>⌃P</kbd>, <kbd>⌃J</kbd> <kbd>⌃K</kbd> | Move through the list |
| <kbd>Esc</kbd> | Clear what you typed, go back, then close |

Inside Pull Requests, <kbd>Tab</kbd> and <kbd>⇧Tab</kbd> switch between All, Failing, Ready and
Drafts. Lists stop at their first and last row rather than wrapping around.

## AI answers

Raiden uses the Claude and ChatGPT subscriptions you already pay for. No API keys, no extra bill.

- **Claude** answers through Claude Code, and **ChatGPT** through Codex, each as already signed in
  on your Mac. Choose one in Settings, or with `ai = "claude"` or `ai = "chatgpt"`.
- Raiden talks to them through their open-source ACP adapters
  ([claude-agent-acp](https://www.npmjs.com/package/@agentclientprotocol/claude-agent-acp) and
  [codex-acp](https://www.npmjs.com/package/@agentclientprotocol/codex-acp)), installed on first
  use from the pinned lockfiles in `adapters/`.
- `effort = "low"` (the default), `"medium"` or `"high"` trades speed for depth.

## Settings

Settings live in `~/.config/raiden/raiden.toml`. Raiden creates the file with every setting
commented out at its default, and applies a saved change within a second. The Settings command
(<kbd>⌘,</kbd>) changes the common ones from inside Raiden and keeps your comments.

```toml
hotkey = "⌥Space"
clipboard_hotkey = "⌃⌥V"
ai = "claude"
effort = "low"
terminal = "ghostty"
vim = false
pinned_repos = ["owner/repo"]
```

| Setting | What it does |
| --- | --- |
| `hotkey` | The keys that open Raiden, such as `"⌃⌥Space"` or `"Control+Option+Space"` |
| `clipboard_hotkey` | The keys that open Clipboard history |
| `ai` | `"claude"` or `"chatgpt"` |
| `effort` | `"low"`, `"medium"` or `"high"` |
| `terminal` | `"ghostty"`, `"iterm"`, `"kitty"`, `"alacritty"`, `"warp"`, `"rex"`, `"tern"` or `"terminal"`. Left out, Raiden uses the first one installed |
| `vim` | `true` turns on Vim keys |
| `pinned_repos` | Repositories offered as quick filters in Pull Requests |
| `pull_requests` | Whether Pull Requests may read your pull requests through `gh`; Continue sets it |

A setting Raiden can't use is noted in `~/Library/Logs/raiden/settings.log`, and Raiden keeps what
it had.

## Privacy and security

- **Your AI sign-ins stay with their tools.** Raiden never reads, copies or stores Claude Code's or
  Codex's credentials, and starts them without `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`.
- **Secrets are hidden before anything is sent.** API keys, tokens and passwords found in your
  question or its context are replaced before it reaches the AI, and before it is saved.
- **Your GitHub token stays with `gh`.** Pull Requests only reads, refuses any request that could
  change something, and lists nothing until you press Continue.
- **No images from the internet.** Raiden's screens load nothing remote. Your GitHub avatar is
  fetched once a week with `curl` and kept on your Mac.
- **Your data stays local.** Clipboard history, answers and caches live in
  `~/Library/Application Support/raiden`, readable only by you.

## Update and uninstall

To update, pull and install again:

```sh
git pull
mise install
mise run install
```

To uninstall, run `mise run uninstall`. It quits Raiden, removes the launch agent that starts it at
login, and removes the app and the `raiden` command. To also remove your settings,
history and the signing keychain:

```sh
rm -rf ~/.config/raiden ~/Library/Application\ Support/raiden ~/Library/Logs/raiden ~/.raiden-dev
security delete-keychain ~/Library/Keychains/raiden-signing.keychain-db
```

## Develop

```sh
mise run dev     # run Raiden from source with live reload of the screens
mise run test    # unit tests, and the screens' behavior in WebKit
mise run lint    # lint, format check and type check
mise run fmt     # format everything
mise run release # a signed, notarized download (see CONTRIBUTING.md)
```

`mise run dev` runs a separate development copy that opens with <kbd>⌥⇧Space</kbd> (Clipboard history
on <kbd>⌃⌥⇧V</kbd>) and keeps its settings and data in `~/.raiden-dev`, so it can run beside the
installed Raiden.

Raiden is a [Tauri 2](https://tauri.app) app: the native side is Rust in `src/`, the screens are
[Solid](https://www.solidjs.com) in `ui/`, and `core/` holds everything that isn't drawing (search,
settings, answers, pull requests), with no macOS code, so it tests anywhere. Search runs as
WebAssembly built from `core/`.

`raiden probe speed` reports how fast the installed Raiden opens, responds to keys and scrolls, and
how much memory it uses at rest, against its limits.

## Contributing

Bugs, ideas and questions are welcome in [Issues](https://github.com/larsx2/raiden/issues), and
pull requests too: [CONTRIBUTING.md](CONTRIBUTING.md) explains how changes are made. Everyone
taking part follows the [code of conduct](CODE_OF_CONDUCT.md), and security problems are reported
privately, as [SECURITY.md](SECURITY.md) explains.

## License

[MIT](LICENSE). The Kitty, Alacritty and Warp icons in `ui/src/terminal-icons/` belong to their
projects and are shown only to name those terminals.
