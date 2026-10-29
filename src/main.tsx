import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import DockApp from "./DockApp";
import "./index.css";
import { isDock } from "./lib/notify";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isDock() ? <DockApp /> : <App />}</React.StrictMode>,
);
