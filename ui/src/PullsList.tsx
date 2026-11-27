import { For, Match, Show, Switch, createEffect, type JSX } from "solid-js";
import { GitHubMark } from "./parts";
import {
  RETRY_TEXT,
  TABS,
  checkLines,
  compactAge,
  emptyText,
  problemAfter,
  repoOf,
  updatedAgo,
  type Checks,
  type Pull,
  type Pulls,
} from "./pulls";

const CI_LOOK: Record<Checks, "pass" | "fail" | "run" | "none"> = {
  passed: "pass",
  failed: "fail",
  running: "run",
  none: "none",
};

function PullGlyph(props: { draft: boolean }): JSX.Element {
  return (
    <span class={props.draft ? "pri draft" : "pri open"}>
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="6" cy="6" r="2.2" />
        <circle cx="6" cy="18" r="2.2" />
        <circle cx="18" cy="18" r="2.2" />
        <path
          d={
            props.draft
              ? "M6 8.2v7.6M18 7v.01M18 11v.01"
              : "M6 8.2v7.6M18 15.8V9.5a3 3 0 0 0-3-3h-4.5M12.5 4.3 10.3 6.5l2.2 2.2"
          }
        />
      </svg>
    </span>
  );
}

function CheckGlyph(props: { look: "pass" | "fail" | "run" | "none" }): JSX.Element {
  return (
    <span class={`ci ${props.look}`}>
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <Switch>
          <Match when={props.look === "pass"}>
            <path d="M5 12.5l4.5 4.5L19 7.5" />
          </Match>
          <Match when={props.look === "fail"}>
            <path d="M7 7l10 10M17 7L7 17" />
          </Match>
          <Match when={props.look === "run"}>
            <circle cx="12" cy="12" r="7" />
          </Match>
          <Match when={props.look === "none"}>
            <path d="M8 12h8" />
          </Match>
        </Switch>
      </svg>
    </span>
  );
}

function Detail(props: { pull: Pull; now: number }): JSX.Element {
  const pull = () => props.pull;
  const files = () => `${pull().files} ${pull().files === 1 ? "file" : "files"}`;
  return (
    <div class="det">
      <div class={`blocker ${pull().look}`}>{pull().status}</div>
      <div class="dt">{pull().title}</div>
      <div class="meta">
        {`${repoOf(pull())} #${pull().number}, `}
        <code>{pull().branch}</code> into <code>{pull().base}</code>
        {`, ${updatedAgo(pull().updatedAt, props.now)}`}
      </div>
      <div class="meta">
        <span class="add">{`+${pull().additions}`}</span>{" "}
        <span class="del">{`−${pull().deletions}`}</span> in {files()}
      </div>
      <Show when={checkLines(pull()).length > 0}>
        <div class="h">Checks</div>
        <For each={checkLines(pull())}>
          {(line) => (
            <div class="chk">
              <CheckGlyph look={line.look} />
              <span>{line.text}</span>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}

function Account(props: { login: string; image: string | null }): JSX.Element {
  return (
    <div class="account">
      <Show
        when={props.image}
        fallback={<span class="face letter">{[...props.login][0]?.toUpperCase()}</span>}
      >
        {(image) => <img class="face" src={image()} alt="" />}
      </Show>
      <span class="who">
        <span class="login">{props.login}</span>
        <span class="host">
          Signed in to <span class="badge">gh</span> on github.com
        </span>
      </span>
    </div>
  );
}

function Gate(props: { pulls: Pulls }): JSX.Element {
  const view = () => props.pulls.view();
  const login = () => view()?.login;
  return (
    <div class="empty" classList={{ welcome: props.pulls.gate() === "allow" }}>
      <GitHubMark />
      <Show
        when={props.pulls.gate() === "allow"}
        fallback={
          <>
            <div class="et">Sign in to GitHub with gh.</div>
            <div class="ep">{view()?.problem?.text ?? ""}</div>
          </>
        }
      >
        <div class="et">Pull Requests</div>
        <Show
          when={login()}
          fallback={
            <Show when={view()?.problem?.text}>
              {(text) => <div class="ep unknown">{text()}</div>}
            </Show>
          }
        >
          {(name) => <Account login={name()} image={props.pulls.avatar()} />}
        </Show>
        <div class="ep">Uses gh, never your token.</div>
      </Show>
    </div>
  );
}

function Nothing(props: { pulls: Pulls }): JSX.Element {
  const pulls = () => props.pulls;
  const typed = () => pulls().query().trim();
  return (
    <div class="full">
      <Switch fallback={<div class="none">{emptyText(pulls().tab(), pulls().onlyLabel())}</div>}>
        <Match when={pulls().lost()}>
          {(repo) => (
            <div class="none">
              {`You no longer have access to ${repo()}.`}
              <span>⌘⌫ unpins.</span>
            </div>
          )}
        </Match>
        <Match when={pulls().failedFirst()}>
          {(problem) => (
            <div class="none">
              {problem().text}
              <Show when={problemAfter(problem())}>{(after) => <span>{after()}</span>}</Show>
            </div>
          )}
        </Match>
        <Match when={typed()}>
          {(query) => (
            <div class="none">
              {`Nothing matches “${query()}”.`}
              <span>
                {pulls().onlyLabel() && pulls().outside() > 0
                  ? `${pulls().outside()} match outside ${pulls().onlyLabel()}; Esc clears the filter.`
                  : "Press Esc to clear the search."}
              </span>
            </div>
          )}
        </Match>
      </Switch>
    </div>
  );
}

export function PullsList(props: {
  pulls: Pulls;
  menuOpen: boolean;
  onPick: (at: number) => void;
}): JSX.Element {
  const pulls = () => props.pulls;
  let pointer: string | undefined;

  const hover = (event: MouseEvent, at: number) => {
    const where = `${event.screenX},${event.screenY}`;
    if (pointer !== undefined && pointer !== where && !props.menuOpen) {
      pulls().select(at);
    }
    pointer = where;
  };

  return (
    <Show
      when={pulls().loaded()}
      fallback={
        <Show when={pulls().unread()}>
          {(text) => (
            <div class="full">
              <div class="none">
                {text()}
                <span>{RETRY_TEXT}</span>
              </div>
            </div>
          )}
        </Show>
      }
    >
      <Show when={pulls().gate() === null} fallback={<Gate pulls={pulls()} />}>
        <div class="scopes" role="tablist" aria-label="Pull request states">
          <For each={TABS}>
            {(tab) => (
              <span
                class="sc"
                classList={{ on: pulls().tab() === tab.id }}
                role="tab"
                aria-selected={pulls().tab() === tab.id}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => pulls().showTab(tab.id)}
              >
                {tab.title}
                <Show when={!pulls().reading() && !pulls().failedFirst()}>
                  <span class="n">{pulls().counts()[tab.id]}</span>
                </Show>
              </span>
            )}
          </For>
          <span class="sk">
            <kbd>⌘</kbd>
            <kbd>1</kbd>…<kbd>⌘</kbd>
            <kbd>4</kbd>
          </span>
        </div>
        <Switch fallback={<Nothing pulls={pulls()} />}>
          <Match when={pulls().reading()}>
            <div class="full">
              <div class="none">Reading your pull requests through gh…</div>
            </div>
          </Match>
          <Match when={pulls().selectedPull()}>
            {(chosen) => (
              <div class="split">
                <div class="lst" role="listbox" aria-label="Pull requests">
                  <For each={pulls().shown()}>
                    {(pull, at) => {
                      const picked = () => pulls().selected() === at();
                      return (
                        <div
                          ref={(element) =>
                            createEffect(() => {
                              if (picked()) {
                                element.scrollIntoView({ block: "nearest" });
                              }
                            })
                          }
                          class="pr"
                          classList={{ selected: picked() }}
                          role="option"
                          aria-selected={picked()}
                          onMouseMove={(event) => hover(event, at())}
                          onMouseDown={(event) => event.preventDefault()}
                          onClick={() => props.onPick(at())}
                        >
                          <PullGlyph draft={pull.draft} />
                          <span class="what">
                            <span class="name">{pull.title}</span>
                            <span class="hint">
                              <span>{pull.shown}</span>
                              <span>{`#${pull.number}`}</span>
                            </span>
                          </span>
                          <span class="value">
                            <Show when={pull.conflicts}>
                              <span class="tagc">conflicts</span>
                            </Show>
                            <CheckGlyph look={CI_LOOK[pull.checks]} />
                            <span class="age">{compactAge(pull.updatedAt, pulls().now())}</span>
                          </span>
                        </div>
                      );
                    }}
                  </For>
                </div>
                <Detail pull={chosen()} now={pulls().now()} />
              </div>
            )}
          </Match>
        </Switch>
      </Show>
    </Show>
  );
}
