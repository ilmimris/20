import { mount } from "svelte";
import BreakOverlay from "./components/BreakOverlay.svelte";
import "./app.css";

const app = mount(BreakOverlay, {
  target: document.getElementById("overlay-app"),
});

export default app;
