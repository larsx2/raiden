import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { render } from "solid-js/web";
import { App, type Call, type Hear, type Listen, type Opens, type Previews } from "./App";
import type { AnswerEvent } from "./ask";
import { fetchSearch } from "./search";

const call: Call = (command, args) => invoke(command, args);
const answers: Listen = (handler) =>
  listen<AnswerEvent>("answer", (event) => handler(event.payload));
const hear: Hear = (handler) => listen<string>("hotkey-pressed", (event) => handler(event.payload));
const opens: Opens = (handler) =>
  listen<boolean>("open-clipboard", (event) => handler(event.payload));
const previews: Previews = (handler) =>
  listen<string>("preview", (event) => handler(event.payload));
const root = document.querySelector<HTMLElement>("#app");
if (root) {
  render(
    () => (
      <App
        call={call}
        search={fetchSearch}
        listen={answers}
        hear={hear}
        opens={opens}
        previews={previews}
      />
    ),
    root,
  );
}
