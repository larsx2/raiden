import {
  Index,
  Match,
  Show,
  Switch,
  createContext,
  createEffect,
  createMemo,
  createRenderEffect,
  createSignal,
  useContext,
  type JSX,
} from "solid-js";
import { codeOf, type Token } from "./highlight";
import { blocksAfter, type Block, type List, type Span } from "./markdown";
import { nextTask } from "./speed";

type Open = (url: string) => void;

const blockOf = <K extends Block["kind"]>(block: Block, kind: K) =>
  block.kind === kind ? (block as Extract<Block, { kind: K }>) : undefined;

const spanOf = <K extends Span["kind"]>(span: Span, kind: K) =>
  span.kind === kind ? (span as Extract<Span, { kind: K }>) : undefined;

function Caret(props: { when: boolean }): JSX.Element {
  return (
    <Show when={props.when}>
      <span class="caret" />
    </Show>
  );
}

function SpanView(props: { span: Span; open: Open }): JSX.Element {
  return (
    <Switch>
      <Match when={spanOf(props.span, "text")}>{(span) => <>{span().text}</>}</Match>
      <Match when={spanOf(props.span, "code")}>{(span) => <code>{span().text}</code>}</Match>
      <Match when={spanOf(props.span, "strong")}>
        {(span) => (
          <strong>
            <Spans spans={span().spans} open={props.open} />
          </strong>
        )}
      </Match>
      <Match when={spanOf(props.span, "em")}>
        {(span) => (
          <em>
            <Spans spans={span().spans} open={props.open} />
          </em>
        )}
      </Match>
      <Match when={spanOf(props.span, "link")}>
        {(span) => (
          <a
            role="link"
            title={span().url}
            onMouseDown={(event) => event.preventDefault()}
            onContextMenu={(event) => event.preventDefault()}
            onClick={() => props.open(span().href)}
          >
            <Spans spans={span().spans} open={props.open} />
          </a>
        )}
      </Match>
    </Switch>
  );
}

function Spans(props: { spans: Span[]; open: Open }): JSX.Element {
  return <Index each={props.spans}>{(span) => <SpanView span={span()} open={props.open} />}</Index>;
}

function ListView(props: { list: List; caret: boolean; open: Open }): JSX.Element {
  const items = () => (
    <Index each={props.list.items}>
      {(item, index) => {
        const last = () => props.caret && index === props.list.items.length - 1;
        return (
          <li>
            <Spans spans={item().spans} open={props.open} />
            <Caret when={last() && item().blocks.length === 0} />
            <Blocks blocks={item().blocks} caret={last()} open={props.open} />
          </li>
        );
      }}
    </Index>
  );
  return (
    <Show when={props.list.ordered} fallback={<ul>{items()}</ul>}>
      <ol start={props.list.start}>{items()}</ol>
    </Show>
  );
}

const Streaming = createContext<() => boolean>(() => false);

type CodeBlock = Extract<Block, { kind: "code" }>;

function Tokens(props: { tokens: Token[] }): JSX.Element {
  const holder = document.createElement("span");
  createRenderEffect(() => {
    holder.replaceChildren(
      ...props.tokens.map((token) => {
        if (!token.tone) {
          return document.createTextNode(token.text);
        }
        const span = document.createElement("span");
        span.className = `tk-${token.tone}`;
        span.textContent = token.text;
        return span;
      }),
    );
  });
  return holder;
}

function CodeView(props: { block: CodeBlock; caret: boolean }): JSX.Element {
  const streaming = useContext(Streaming);
  const [settled, setSettled] = createSignal(!streaming());
  const code = createMemo(() =>
    codeOf(
      props.block.text,
      props.block.lang,
      props.block.open && props.caret ? "arriving" : settled() ? "settled" : "closed",
    ),
  );
  createEffect(() => {
    if (!streaming() && !settled()) {
      nextTask(() => setSettled(true));
    }
  });
  return (
    <pre>
      <Show when={code()} fallback={props.block.text}>
        {(shown) => (
          <code>
            <Index each={shown().done}>{(tokens) => <Tokens tokens={tokens()} />}</Index>
            {shown().tail}
          </code>
        )}
      </Show>
      <Caret when={props.caret} />
    </pre>
  );
}

function BlockView(props: { block: Block; caret: boolean; open: Open }): JSX.Element {
  return (
    <Switch>
      <Match when={blockOf(props.block, "code")}>
        {(block) => <CodeView block={block()} caret={props.caret} />}
      </Match>
      <Match when={blockOf(props.block, "heading")}>
        {(block) => (
          <h3>
            <Spans spans={block().spans} open={props.open} />
            <Caret when={props.caret} />
          </h3>
        )}
      </Match>
      <Match when={blockOf(props.block, "paragraph")}>
        {(block) => (
          <p>
            <Spans spans={block().spans} open={props.open} />
            <Caret when={props.caret} />
          </p>
        )}
      </Match>
      <Match when={blockOf(props.block, "quote")}>
        {(block) => (
          <blockquote>
            <Blocks blocks={block().blocks} caret={props.caret} open={props.open} />
          </blockquote>
        )}
      </Match>
      <Match when={blockOf(props.block, "list")}>
        {(block) => <ListView list={block()} caret={props.caret} open={props.open} />}
      </Match>
    </Switch>
  );
}

function Blocks(props: { blocks: Block[]; caret: boolean; open: Open }): JSX.Element {
  return (
    <Index each={props.blocks}>
      {(block, index) => (
        <BlockView
          block={block()}
          caret={props.caret && index === props.blocks.length - 1}
          open={props.open}
        />
      )}
    </Index>
  );
}

export function Reply(props: { text: string; streaming: boolean; open: Open }): JSX.Element {
  const blocks = createMemo((previous: Block[]) => blocksAfter(previous, props.text), []);
  return (
    <Streaming.Provider value={() => props.streaming}>
      <Blocks blocks={blocks()} caret={props.streaming} open={props.open} />
      <Show when={props.streaming && blocks().length === 0}>
        <p>
          <span class="caret" />
        </p>
      </Show>
    </Streaming.Provider>
  );
}
