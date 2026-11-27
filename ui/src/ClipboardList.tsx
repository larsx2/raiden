import { For, Index, Show, createEffect, type JSX } from "solid-js";
import { subtitleOf, type ClipKind, type Clipboard } from "./clipboard";
import { Glyph, NoMatch, type GlyphName } from "./parts";

const GLYPH_OF: Record<ClipKind, GlyphName> = {
  text: "text",
  link: "link",
  file: "file",
  image: "image",
};

export function ClipboardList(props: { clipboard: Clipboard }): JSX.Element {
  const clip = () => props.clipboard;
  let pointer: string | undefined;

  const hover = (event: MouseEvent, at: number) => {
    const where = `${event.screenX},${event.screenY}`;
    if (pointer !== undefined && pointer !== where) {
      clip().select(at);
    }
    pointer = where;
  };

  return (
    <div class="list prefs clips" role="listbox" aria-label="Clipboard history">
      <Show
        when={clip().shown().length > 0}
        fallback={
          <Show when={clip().loaded()}>
            <Show
              when={clip().query().trim()}
              fallback={<div class="none">Nothing copied in the last 7 days</div>}
            >
              {(query) => <NoMatch things="clipboard items" typed={query()} />}
            </Show>
          </Show>
        }
      >
        <Index each={clip().sections()}>
          {(section) => (
            <>
              <div class="sec" role="presentation">
                {section()[0]}
              </div>
              <For each={section()[1]}>
                {(item) => {
                  const at = () => clip().indexOf(item);
                  const chosen = () => clip().selected() === at();
                  return (
                    <div
                      ref={(element) =>
                        createEffect(() => {
                          if (chosen()) {
                            element.scrollIntoView({ block: "nearest" });
                          }
                        })
                      }
                      class="set clip"
                      classList={{ selected: chosen() }}
                      role="option"
                      aria-selected={chosen()}
                      data-kind={item.kind}
                      onMouseMove={(event) => hover(event, at())}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => {
                        clip().select(at());
                        clip().act();
                      }}
                    >
                      <Show
                        when={item.thumb && clip().thumb(item.id)}
                        fallback={<Glyph name={GLYPH_OF[item.kind]} plain />}
                      >
                        {(url) => <img class="thumb" alt="" src={url()} />}
                      </Show>
                      <span class="what">
                        <span class="name">{item.title}</span>
                        <span class="hint">{subtitleOf(item, clip().now())}</span>
                      </span>
                    </div>
                  );
                }}
              </For>
            </>
          )}
        </Index>
      </Show>
    </div>
  );
}
