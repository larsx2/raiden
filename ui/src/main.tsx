import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { render } from "solid-js/web";
import { App, type Call, type ContentsReady, type Hear, type Listen, type Opens } from "./App";
import type { AnswerEvent } from "./ask";
import { fetchSearch } from "./search";

const call: Call = (command, args) => invoke(command, args);
const answers: Listen = (handler) =>
  listen<AnswerEvent>("answer", (event) => handler(event.payload));
const hear: Hear = (handler) => listen<string>("hotkey-pressed", (event) => handler(event.payload));
const opens: Opens = (handler) =>
  listen<boolean>("open-clipboard", (event) => handler(event.payload));
const contentsReady: ContentsReady = (handler) =>
  listen<string>("contents", (event) => {
    if (event.payload === "ready") {
      handler();
    }
  });
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
        contentsReady={contentsReady}
      />
    ),
    root,
  );
}
