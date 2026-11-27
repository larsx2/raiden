import { For, Index, Match, Show, Switch, createEffect, type JSX } from "solid-js";
import type { AccountId } from "./ask";
import { Brand, Glyph, Keys, MenuIcon, NoMatch, type GlyphName } from "./parts";
import { terminalNamed } from "./terminals";
import {
  effortName,
  groupsOf,
  hotkeyKeys,
  OPEN_FILE_KEYS,
  type RowId,
  type Settings,
} from "./settings";

const GLYPH_OF: Record<RowId, GlyphName> = {
  hotkey: "keyboard",
  clipboard: "clipboard",
  login: "power",
  terminal: "terminal",
  vim: "prompt",
  ai: "sparkle",
  effort: "gauge",
  grammar: "pen",
  currency: "gauge",
  file: "file",
};

const ENGINES: Array<{ id: AccountId; name: string }> = [
  { id: "claude", name: "Claude" },
  { id: "chatgpt", name: "ChatGPT" },
];

type Icon = (path: string) => string | undefined;

export function SettingsList(props: { settings: Settings; icon: Icon }): JSX.Element {
  const settings = () => props.settings;
  let pointer: string | undefined;

  const hover = (event: MouseEvent, at: number) => {
    const where = `${event.screenX},${event.screenY}`;
    if (pointer !== undefined && pointer !== where && !settings().recording()) {
      settings().select(at);
    }
    pointer = where;
  };

  return (
    <div class="list prefs" role="listbox" aria-label="Settings">
      <Show
        when={settings().rows().length > 0}
        fallback={<NoMatch things="settings" typed={settings().query().trim()} />}
      >
        <For each={groupsOf(settings().rows())}>
          {([group, rows]) => (
            <>
              <div class="sec" role="presentation">
                {group}
              </div>
              <Index each={rows}>
                {(row) => {
                  const at = () => settings().rows().indexOf(row());
                  const chosen = () => settings().selected() === at();
                  const hint = () => settings().hintOf(row());
                  return (
                    <div
                      ref={(element) =>
                        createEffect(() => {
                          if (chosen()) {
                            element.scrollIntoView({ block: "nearest" });
                          }
                        })
                      }
                      class="set"
                      classList={{ selected: chosen() }}
                      role="option"
                      aria-selected={chosen()}
                      data-row={row().id}
                      onMouseMove={(event) => hover(event, at())}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => {
                        if (!settings().recording()) {
                          settings().select(at());
                          settings().act(row());
                        }
                      }}
                    >
                      <Glyph name={GLYPH_OF[row().id]} plain />
                      <span class="what">
                        <span class="name">{row().name}</span>
                        <Show
                          when={row().id === "file"}
                          fallback={
                            <span class="hint" classList={{ warn: hint().warn }}>
                              {hint().text}
                            </span>
                          }
                        >
                          <Show
                            when={hint().warn}
                            fallback={<span class="path">{settings().view()?.file}</span>}
                          >
                            <span class="hint warn">{hint().text}</span>
                          </Show>
                        </Show>
                      </span>
                      <span class="value">
                        <Value id={row().id} settings={settings()} icon={props.icon} />
                      </span>
                    </div>
                  );
                }}
              </Index>
            </>
          )}
        </For>
      </Show>
    </div>
  );
}

function Value(props: { id: RowId; settings: Settings; icon: Icon }): JSX.Element {
  const view = () => props.settings.view();
  return (
    <Switch>
      <Match when={props.settings.recording() === props.id}>
        <span class="recording">Press the new keys…</span>
      </Match>
      <Match when={props.id === "hotkey" && view()}>
        {(shown) => (
          <span class="hotkey">
            <Keys keys={hotkeyKeys(shown().hotkey).join(" ")} />
          </span>
        )}
      </Match>
      <Match when={props.id === "clipboard" && view()}>
        {(shown) => (
          <span class="hotkey">
            <Keys keys={hotkeyKeys(shown().clipboardHotkey).join(" ")} />
          </span>
        )}
      </Match>
      <Match when={props.id === "login" && view()}>
        {(shown) => (
          <span
            class="switch"
            classList={{ on: shown().atLogin.on, fixed: !shown().atLogin.canChange }}
            role="switch"
            aria-checked={shown().atLogin.on}
            aria-label="Open at login"
          />
        )}
      </Match>
      <Match when={props.id === "terminal" && view()}>
        {(shown) => (
          <Show when={terminalNamed(shown().terminals, shown().terminal)}>
            {(terminal) => (
              <span class="term">
                <MenuIcon src={terminal().app ? props.icon(terminal().app ?? "") : undefined} />
                {terminal().name}
              </span>
            )}
          </Show>
        )}
      </Match>
      <Match when={props.id === "vim" && view()}>
        {(shown) => (
          <span
            class="switch"
            classList={{ on: shown().vim }}
            role="switch"
            aria-checked={shown().vim}
            aria-label="Vim keys"
          />
        )}
      </Match>
      <Match when={props.id === "ai" && view()}>
        <span class="seg" role="radiogroup" aria-label="Answers come from">
          <For each={ENGINES}>
            {(engine) => (
              <button
                type="button"
                role="radio"
                classList={{ on: props.settings.chosen() === engine.id }}
                aria-checked={props.settings.chosen() === engine.id}
                onMouseDown={(event) => event.preventDefault()}
                onClick={(event) => {
                  event.stopPropagation();
                  props.settings.pick(engine.id);
                }}
              >
                <Brand id={engine.id} />
                {engine.name}
              </button>
            )}
          </For>
        </span>
      </Match>
      <Match when={props.id === "effort" && view()}>
        {(shown) => <>{effortName(shown().effort)}</>}
      </Match>
      <Match when={props.id === "grammar"}>Claude</Match>
      <Match when={props.id === "currency" && view()}>
        {(shown) => <>{shown().fixerKey ? `Fixer ${shown().fixerKey}` : "ECB"}</>}
      </Match>
      <Match when={props.id === "file"}>
        Open <Keys keys={OPEN_FILE_KEYS} />
      </Match>
    </Switch>
  );
}
