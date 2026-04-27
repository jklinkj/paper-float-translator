import { PopupView } from "./PopupView";
import { SettingsView } from "./SettingsView";

export function App(): JSX.Element {
  const view = new URLSearchParams(window.location.search).get("view");
  return view === "popup" ? <PopupView /> : <SettingsView />;
}
