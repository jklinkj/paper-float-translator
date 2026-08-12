import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./styles.css";

const isPopupView = new URLSearchParams(window.location.search).get("view") === "popup";
document.documentElement.dataset.view = isPopupView ? "popup" : "settings";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
