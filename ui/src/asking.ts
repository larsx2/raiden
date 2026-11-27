import { batch, createSignal } from "solid-js";
import {
  DEFAULT_ACCOUNT,
  isAccountId,
  madeBy,
  sent,
  tagsFor,
  type Account,
  type AccountId,
  type AnswerEvent,
  type Conversation,
  type Front,
  type Tag,
} from "./ask";

export type Phase = "waiting" | "streaming" | "done" | "stopped" | "error" | "signin" | "signing";
export type ThreadTurn = { question: string; text: string; stopped: boolean };

type Call = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
type Flight = { conversation: string; question: string; text: string; model: string };

const ACCOUNT_NAMES: Record<AccountId, { name: string; through: string }> = {
  claude: { name: "Claude", through: "Claude Code" },
  chatgpt: { name: "ChatGPT", through: "Codex" },
};

const added = (before: Record<string, number>, now: Record<string, number>) =>
  Object.fromEntries(
    [...new Set([...Object.keys(before), ...Object.keys(now)])].map((kind) => [
      kind,
      (before[kind] ?? 0) + (now[kind] ?? 0),
    ]),
  );

const problemOf = (error: unknown) =>
  error instanceof Error ? error.message : typeof error === "string" ? error : "Asking failed.";

export function createAsking(options: {
  call: Call;
  frame: (callback: () => void) => void;
  saved: (conversation: Conversation) => void;
}) {
  const { call } = options;
  const [question, setQuestion] = createSignal("");
  const [text, setText] = createSignal("");
  const [model, setModel] = createSignal("");
  const [hiddenBefore, setHiddenBefore] = createSignal<Record<string, number>>({});
  const [hiddenNow, setHiddenNow] = createSignal<Record<string, number>>({});
  const hidden = () => added(hiddenBefore(), hiddenNow());
  const setHidden = (counts: Record<string, number>) =>
    batch(() => {
      setHiddenBefore(counts);
      setHiddenNow({});
    });
  const [phase, setPhase] = createSignal<Phase | null>(null);
  const [problem, setProblem] = createSignal("");
  const [searching, setSearching] = createSignal<string | null>(null);
  const [notice, setNotice] = createSignal("");
  const [tags, setTags] = createSignal<Tag[]>([]);
  const [front, setFront] = createSignal<Front | null>(null);
  const [accounts, setAccounts] = createSignal<Account[]>([]);
  const [turns, setTurns] = createSignal<ThreadTurn[]>([]);
  const [chosen, setChosen] = createSignal<AccountId>(DEFAULT_ACCOUNT);
  const [using, setUsing] = createSignal<AccountId>(DEFAULT_ACCOUNT);
  const flights = new Map<string, Flight>();
  const stopped = new Set<string>();
  const screen = Date.now().toString(36);
  let numbered = 0;
  let conversations = 0;
  let conversation = `${screen}-c0`;
  let spoken = false;
  let current: string | undefined;
  let round = 0;
  let flushing = false;
  let seeing: Promise<boolean> = Promise.resolve(true);

  const accountId = (): AccountId => using();
  const accountOf = (id: AccountId): Account =>
    accounts().find((found) => found.id === id) ?? { id, ...ACCOUNT_NAMES[id], signedIn: true };
  const account = (): Account => accountOf(accountId());
  const other = (): Account => accountOf(using() === "claude" ? "chatgpt" : "claude");
  const signedOut = () => !account().signedIn;

  const readChoice = () =>
    call<unknown>("chosen_ai")
      .then((found) => (isAccountId(found) ? found : chosen()))
      .catch(() => chosen());

  const refreshChoice = () => readChoice().then(setChosen);

  const refreshAccounts = () =>
    call<Account[] | null>("ai_accounts")
      .then((found) => {
        if (Array.isArray(found)) {
          setAccounts(found);
        }
      })
      .catch(() => undefined);

  const flush = () => {
    if (flushing) {
      return;
    }
    flushing = true;
    options.frame(() => {
      flushing = false;
      const flight = current === undefined ? undefined : flights.get(current);
      if (flight) {
        setText(flight.text);
      }
    });
  };

  const finish = (id: string) => {
    flights.delete(id);
  };

  const abandon = () => {
    const id = current;
    current = undefined;
    setSearching(null);
    if (id !== undefined && flights.has(id)) {
      finish(id);
      stopped.add(id);
      void call("ai_stop", { id });
    }
  };

  const endConversation = () => {
    if (spoken) {
      void call("ai_end", { conversation });
    }
    spoken = false;
  };

  const newConversation = (id?: string, engine: AccountId = chosen()) => {
    endConversation();
    conversations += 1;
    conversation = id ?? `${screen}-c${conversations}`;
    batch(() => {
      setUsing(engine);
      setTurns([]);
      setHidden({});
      setModel("");
    });
  };

  const tagsFixed = () => turns().length > 0 || phase() === "done" || phase() === "stopped";

  const keepCurrent = () => {
    const now = phase();
    const asked = question();
    const flight = current === undefined ? undefined : flights.get(current);
    const unsent = now === "streaming" && !flight;
    if (!asked || unsent || (now !== "streaming" && now !== "done" && now !== "stopped")) {
      return;
    }
    const said = flight?.text ?? text();
    setTurns((before) => [...before, { question: asked, text: said, stopped: now !== "done" }]);
    setHidden(hidden());
    if (now !== "done" && said.trim()) {
      save(conversation, asked, flight?.model || model(), said);
    }
  };

  const save = (id: string, asked: string, by: string, said: string) =>
    void call<Conversation>("save_turn", {
      conversation: id,
      question: asked,
      model: by,
      answer: said,
    })
      .then((kept) => kept && options.saved(kept))
      .catch(() => undefined);

  const fail = (message: string) => {
    const mine = round;
    void refreshAccounts().then(() => {
      if (mine !== round) {
        return;
      }
      batch(() => {
        if (signedOut()) {
          setPhase("signin");
        } else {
          setProblem(message);
          setPhase("error");
        }
      });
    });
  };

  const receive = (event: AnswerEvent) => {
    const flight = flights.get(event.id);
    if (!flight) {
      return;
    }
    const mine = event.id === current;
    if (event.kind === "searching") {
      if (mine) {
        setSearching(event.text);
      }
      return;
    }
    if (mine && (event.kind !== "delta" || event.text.trim())) {
      setSearching(null);
    }
    if (event.model) {
      flight.model = event.model;
    }
    if (event.kind === "delta") {
      flight.text += event.text;
    } else if (event.kind === "done" && event.text) {
      flight.text = event.text;
    }
    if (mine) {
      batch(() => {
        if (event.model) {
          setModel(event.model);
        }
        if (Object.keys(event.hidden ?? {}).length > 0) {
          setHiddenNow(event.hidden);
        }
      });
    }
    if (event.kind === "delta") {
      if (mine) {
        flush();
      }
      return;
    }
    finish(event.id);
    if (event.kind === "done") {
      if (mine) {
        batch(() => {
          setText(flight.text);
          setPhase("done");
        });
      }
      if (flight.text.trim()) {
        save(flight.conversation, flight.question, flight.model, flight.text);
      }
    } else if (mine) {
      setText(flight.text);
      fail(event.text || "The answer stopped with an error.");
    }
  };

  const ask = async (question: string) => {
    const mine = ++round;
    abandon();
    batch(() => {
      setQuestion(question);
      setText("");
      setSearching(null);
      setProblem("");
      setNotice("");
      setPhase(signedOut() ? "signin" : "streaming");
    });
    if (signedOut()) {
      return;
    }
    numbered += 1;
    const id = `${screen}-${numbered}`;
    current = id;
    spoken = true;
    flights.set(id, { conversation, question, text: "", model: "" });
    try {
      await call<string>("ask", {
        id: accountId(),
        conversation,
        question,
        context: sent(tags()),
        answer: id,
      });
      if (stopped.has(id)) {
        void call("ai_stop", { id });
      }
    } catch (error) {
      finish(id);
      if (mine === round) {
        fail(problemOf(error));
      }
    }
  };

  const lookAround = async (mine: number) => {
    const [seen, engine] = await Promise.all([
      call<Front | null>("front_context").catch(() => null),
      readChoice(),
    ]);
    if (mine !== round) {
      return false;
    }
    batch(() => {
      setFront(seen);
      setTags(tagsFor(seen));
      setChosen(engine);
      setUsing(engine);
    });
    return true;
  };

  const followUp = async (asked: string) => {
    keepCurrent();
    await ask(asked);
  };

  const start = async (asked: string) => {
    const mine = ++round;
    abandon();
    newConversation();
    batch(() => {
      setTags([]);
      setFront(null);
      setQuestion(asked);
      setText("");
      setHidden({});
      setPhase("streaming");
    });
    if (await lookAround(mine)) {
      await ask(asked);
    }
  };

  const begin = async () => {
    const mine = ++round;
    abandon();
    newConversation();
    batch(() => {
      setTags([]);
      setFront(null);
      setQuestion("");
      setText("");
      setModel("");
      setHidden({});
      setProblem("");
      setNotice("");
      setPhase("waiting");
    });
    seeing = lookAround(mine);
    await seeing;
  };

  const beginWith = async (given: Tag[]) => {
    const mine = ++round;
    abandon();
    newConversation();
    batch(() => {
      setTags(given);
      setFront(null);
      setQuestion("");
      setText("");
      setModel("");
      setHidden({});
      setProblem("");
      setNotice("");
      setPhase("waiting");
    });
    seeing = readChoice().then((engine) => {
      if (mine !== round) {
        return false;
      }
      batch(() => {
        setChosen(engine);
        setUsing(engine);
      });
      return true;
    });
    await seeing;
  };

  const askFirst = async (asked: string) => {
    const mine = round;
    batch(() => {
      setQuestion(asked);
      setPhase("streaming");
    });
    if ((await seeing) && mine === round) {
      await ask(asked);
    }
  };

  const stop = () => {
    if (phase() !== "streaming") {
      return;
    }
    round++;
    const flight = current === undefined ? undefined : flights.get(current);
    abandon();
    batch(() => {
      if (flight) {
        setText(flight.text);
      }
      setPhase("stopped");
    });
  };

  const signIn = async () => {
    const asked = question();
    const mine = round;
    setPhase("signing");
    let problem = "";
    try {
      await call("ai_sign_in", { id: accountId() });
    } catch (error) {
      problem = problemOf(error);
    }
    await refreshAccounts();
    if (mine !== round) {
      return;
    }
    setNotice(problem);
    if (signedOut()) {
      setPhase("signin");
    } else if (asked) {
      await ask(asked);
    } else {
      setPhase("waiting");
    }
  };

  const choose = async (engine: AccountId) => {
    const asked = round;
    try {
      await call("choose_ai", { id: engine });
    } catch (error) {
      if (asked === round) {
        setNotice(problemOf(error));
      }
      return;
    }
    setChosen(engine);
    if (asked !== round) {
      return;
    }
    const kept = tags().filter((tag) => tag.kind === "pulls");
    await (kept.length > 0 ? beginWith(kept) : begin());
    const mine = round;
    await refreshAccounts();
    if (mine === round && phase() === "waiting" && signedOut()) {
      setPhase("signin");
    }
  };

  const removeTag = (index?: number) => {
    const shown = tags();
    if (shown.length === 0 || tagsFixed()) {
      return false;
    }
    const at = index ?? shown.length - 1;
    setTags(shown.filter((_, other) => other !== at));
    if (phase() === "streaming") {
      abandon();
      newConversation();
      void ask(question());
    }
    return true;
  };

  const show = async (saved: Conversation) => {
    round++;
    abandon();
    newConversation(saved.id, madeBy(saved.model) ?? chosen());
    const last = saved.turns.at(-1);
    batch(() => {
      setTurns(
        saved.turns
          .slice(0, -1)
          .map((turn) => ({ question: turn.question, text: turn.answer, stopped: false })),
      );
      setTags([]);
      setQuestion(last?.question ?? "");
      setText(last?.answer ?? "");
      setModel(saved.model);
      setProblem("");
      setNotice("");
      setPhase("done");
    });
    const seen = await call<Front | null>("front_context").catch(() => null);
    setFront(seen);
  };

  const leave = () => {
    round++;
    abandon();
    newConversation();
    batch(() => {
      setPhase(null);
      setTags([]);
      setQuestion("");
      setText("");
      setNotice("");
    });
  };

  return {
    question,
    text,
    model,
    hidden,
    phase,
    problem,
    searching,
    notice,
    setNotice,
    tags,
    tagsFixed,
    turns,
    conversationId: () => conversation,
    front,
    account,
    accountOf,
    chosenAccount: () => accountOf(chosen()),
    other,
    choose,
    refreshAccounts,
    refreshChoice,
    receive,
    start,
    begin,
    beginWith,
    askFirst,
    ask,
    followUp,
    stop,
    signIn,
    removeTag,
    show,
    leave,
  };
}

export type Asking = ReturnType<typeof createAsking>;
