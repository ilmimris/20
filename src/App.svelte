<script>
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import TrayPopover from "./components/TrayPopover.svelte";
  import Settings from "./pages/Settings.svelte";

  let currentView = $state("popover"); // "popover" | "settings"
  let timerState = $state({
    secondsRemaining: 1200,
    isPaused: false,
    pauseReason: null, // null | "manual" | "meeting"
    isStrictMode: false,
  });

  onMount(async () => {
    // Fetch initial state from backend (backend returns snake_case keys).
    try {
      const state = await invoke("get_timer_state");
      timerState = {
        ...timerState,
        secondsRemaining: state.seconds_remaining ?? timerState.secondsRemaining,
        isPaused: state.is_paused ?? timerState.isPaused,
        pauseReason: state.pause_reason ?? timerState.pauseReason,
        isStrictMode: state.is_strict_mode ?? timerState.isStrictMode,
      };
    } catch (e) {
      console.error("Failed to get timer state:", e);
    }

    // Listen for timer ticks
    const unlistenTick = await listen("timer:tick", (event) => {
      timerState.secondsRemaining = event.payload.seconds_remaining;
      timerState.isPaused = event.payload.is_paused;
      timerState.pauseReason = event.payload.pause_reason;
    });

    return () => {
      unlistenTick();
    };
  });

  function openSettings() {
    currentView = "settings";
  }

  function closeSettings() {
    currentView = "popover";
  }
</script>

<div class="w-[280px] min-h-[180px] bg-white dark:bg-gray-900 rounded-lg shadow-xl overflow-hidden">
  {#if currentView === "popover"}
    <TrayPopover
      {timerState}
      onOpenSettings={openSettings}
    />
  {:else}
    <Settings onClose={closeSettings} />
  {/if}
</div>
