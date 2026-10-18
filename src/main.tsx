import React from "react";
import ReactDOM from "react-dom/client";
import { HashRouter } from "react-router-dom";
import App from "./App";
import { StartupUpdater } from "./components/StartupUpdater";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <HashRouter>
      <App />
      <StartupUpdater />
    </HashRouter>
  </React.StrictMode>,
);
