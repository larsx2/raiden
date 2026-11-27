import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { Hud, type Notice } from "./Hud";

const [notice, setNotice] = createSignal<Notice | null>(null);
void listen<Notice>("hud", (event) => setNotice(event.payload));
void invoke<Notice | null>("hud_message").then((held) => {
  if (held && notice() === null) {
    setNotice({ ...held, shown: false });
    requestAnimationFrame(() => setNotice((now) => (now?.shown === false ? held : now)));
  }
});
const root = document.querySelector<HTMLElement>("#app");
if (root) {
  render(() => <Hud notice={notice()} />, root);
}
