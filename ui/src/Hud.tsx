import { Match, Show, Switch, type JSX } from "solid-js";

export type Look = "working" | "done" | "problem";

export type Notice = { look: Look; text: string; undo: boolean; shown: boolean };

export function Hud(props: { notice: Notice | null }): JSX.Element {
  return (
    <div
      id="hud"
      role="status"
      classList={{ show: props.notice?.shown === true }}
      data-look={props.notice?.look}
    >
      <Switch>
        <Match when={props.notice?.look === "working"}>
          <span class="hud-spin" />
        </Match>
        <Match when={props.notice?.look === "problem"}>
          <svg viewBox="0 0 24 24" class="hud-problem">
            <path d="M12 7v6M12 17h.01" />
          </svg>
        </Match>
        <Match when={props.notice?.look === "done"}>
          <svg viewBox="0 0 24 24">
            <path d="M5 12.5l4.5 4.5L19 7.5" />
          </svg>
        </Match>
      </Switch>
      {props.notice?.text}
      <Show when={props.notice?.undo}>
        <span class="hud-x">
          Undo
          <span class="keys">
            <kbd>⌘</kbd>
            <kbd>Z</kbd>
          </span>
        </span>
      </Show>
    </div>
  );
}
