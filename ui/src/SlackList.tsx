import { For, Index, Match, Show, Switch, createEffect, type JSX } from "solid-js";
import { NoMatch } from "./parts";
import { STEPS, detailOf, type SlackKind, type Slack } from "./slack";

function KindGlyph(props: { kind: SlackKind }): JSX.Element {
  return (
    <span class={`sk ${props.kind}`}>
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <Switch>
          <Match when={props.kind === "person"}>
            <circle cx="12" cy="8" r="3.4" />
            <path d="M5.5 19a6.5 6.5 0 0 1 13 0" />
          </Match>
          <Match when={props.kind === "channel"}>
            <path d="M9 4 7 20M17 4l-2 16M4 9h16M3 15h16" />
          </Match>
          <Match when={props.kind === "group"}>
            <circle cx="9" cy="9" r="2.8" />
            <circle cx="16.5" cy="10.5" r="2.2" />
            <path d="M3.5 18a5.5 5.5 0 0 1 11 0M14 18a4.5 4.5 0 0 1 7 0" />
          </Match>
          <Match when={props.kind === "message"}>
            <path d="M5 5h14v10H9l-4 4z" />
          </Match>
        </Switch>
      </svg>
    </span>
  );
}

export function SlackMark(): JSX.Element {
  return (
    <span class="ico slack" aria-hidden="true">
      <svg viewBox="0 0 24 24">
        <path d="M6 14.5A1.75 1.75 0 1 1 4.25 12.75H6zM7 14.5a1.75 1.75 0 0 1 3.5 0v4.4a1.75 1.75 0 1 1-3.5 0zM9.5 6a1.75 1.75 0 1 1 1.75-1.75V6zM9.5 7a1.75 1.75 0 0 1 0 3.5h-4.4a1.75 1.75 0 1 1 0-3.5zM18 9.5a1.75 1.75 0 1 1 1.75 1.75H18zM17 9.5a1.75 1.75 0 0 1-3.5 0v-4.4a1.75 1.75 0 1 1 3.5 0zM14.5 18a1.75 1.75 0 1 1-1.75 1.75V18zM14.5 17a1.75 1.75 0 0 1 0-3.5h4.4a1.75 1.75 0 1 1 0 3.5z" />
      </svg>
    </span>
  );
}

function Gate(props: { slack: Slack }): JSX.Element {
  const slack = () => props.slack;
  const message = () => slack().problem()?.text ?? slack().context();
  return (
    <div class="empty welcome slack-gate">
      <SlackMark />
      <div class="et">Search Slack</div>
      <ol class="steps">
        <Index each={STEPS}>{(step) => <li>{step()}</li>}</Index>
      </ol>
      <div class="gate-buttons">
        <button
          type="button"
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => slack().openBuilder()}
        >
          Open Slack app builder
        </button>
        <button
          type="button"
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => slack().copyManifest()}
        >
          Copy manifest
        </button>
      </div>
      <Show when={message()}>{(text) => <div class="ep">{text()}</div>}</Show>
    </div>
  );
}

export function SlackList(props: { slack: Slack }): JSX.Element {
  const slack = () => props.slack;
  let pointer: string | undefined;

  const hover = (event: MouseEvent, at: number) => {
    const where = `${event.screenX},${event.screenY}`;
    if (pointer !== undefined && pointer !== where) {
      slack().select(at);
    }
    pointer = where;
  };

  return (
    <Show when={slack().gate() === null} fallback={<Gate slack={slack()} />}>
      <div class="list slack" role="listbox" aria-label="Slack results">
        <Show
          when={slack().shown().length > 0}
          fallback={
            <Show when={slack().loaded()}>
              <Show
                when={slack().problem()}
                fallback={
                  <Show
                    when={slack().query().trim()}
                    fallback={<div class="none">Type to search people, channels and messages</div>}
                  >
                    {(query) => <NoMatch things="Slack results" typed={query()} />}
                  </Show>
                }
              >
                {(trouble) => <div class="none">{trouble().text}</div>}
              </Show>
            </Show>
          }
        >
          <For each={slack().shown()}>
            {(hit, at) => {
              const chosen = () => slack().selected() === at();
              return (
                <div
                  ref={(element) =>
                    createEffect(() => {
                      if (chosen()) {
                        element.scrollIntoView({ block: "nearest" });
                      }
                    })
                  }
                  class="set slackrow"
                  classList={{ selected: chosen() }}
                  role="option"
                  aria-selected={chosen()}
                  data-kind={hit.kind}
                  onMouseMove={(event) => hover(event, at())}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    slack().select(at());
                    slack().act();
                  }}
                >
                  <KindGlyph kind={hit.kind} />
                  <span class="what">
                    <span class="name">{hit.title}</span>
                    <span class="hint">{detailOf(hit)}</span>
                  </span>
                  <Show when={hit.unread > 0}>
                    <span class="badge unread">{hit.unread}</span>
                  </Show>
                </div>
              );
            }}
          </For>
        </Show>
      </div>
    </Show>
  );
}
