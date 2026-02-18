<script>
  import { invoke } from "@tauri-apps/api/core";

  let { timerState, onOpenSettings } = $props();

  function formatTime(seconds) {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  async function skipNextBreak() {
    try {
      await invoke("skip_break");
    } catch (e) {
      console.error("Failed to skip break:", e);
    }
  }

  async function pauseFor(minutes) {
    try {
      await invoke("pause_timer", { minutes });
    } catch (e) {
      console.error("Failed to pause timer:", e);
    }
  }

  async function resumeTimer() {
    try {
      await invoke("resume_timer");
    } catch (e) {
      console.error("Failed to resume timer:", e);
    }
  }

  async function quit() {
    if (!confirm("Quit EyeBreak?")) return;
    try {
      await invoke("quit_app");
    } catch (e) {
      console.error("Failed to quit:", e);
    }
  }

  let isMeetingPaused = $derived(timerState.pauseReason === "meeting");
  let isManualPaused = $derived(timerState.pauseReason === "manual");
  let isStrictMode = $derived(timerState.isStrictMode);
</script>

<div class="flex flex-col divide-y divide-gray-100 dark:divide-gray-800">
  <!-- Header -->
  <div class="px-4 py-3 flex items-center gap-3">
    <div class="text-2xl">👁</div>
    <div>
      <p class="text-sm font-semibold text-gray-900 dark:text-white">EyeBreak</p>
      {#if isMeetingPaused}
        <p class="text-xs text-amber-500 font-medium">Meeting detected — paused</p>
      {:else if isManualPaused}
        <p class="text-xs text-blue-500 font-medium">Paused manually</p>
      {:else}
        <p class="text-xs text-gray-500 dark:text-gray-400">
          Next break in <span class="font-mono font-bold text-gray-700 dark:text-gray-300">{formatTime(timerState.secondsRemaining)}</span>
        </p>
      {/if}
    </div>
  </div>

  <!-- Meeting badge (if active) -->
  {#if isMeetingPaused}
    <div class="px-4 py-2 bg-amber-50 dark:bg-amber-900/20">
      <p class="text-xs text-amber-700 dark:text-amber-400 flex items-center gap-1">
        <span>📹</span>
        <span>Meeting in progress — timer paused</span>
      </p>
    </div>
  {/if}

  <!-- Actions -->
  {#if !isStrictMode}
    <div class="px-4 py-2 space-y-1">
      {#if isManualPaused}
        <button
          onclick={resumeTimer}
          class="w-full text-left text-sm text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-200 py-1 px-2 rounded hover:bg-blue-50 dark:hover:bg-blue-900/30 transition-colors"
        >
          ▶ Resume timer
        </button>
      {:else if !isMeetingPaused}
        <button
          onclick={skipNextBreak}
          class="w-full text-left text-sm text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-100 py-1 px-2 rounded hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
        >
          ⏭ Skip next break
        </button>
        <button
          onclick={() => pauseFor(30)}
          class="w-full text-left text-sm text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-100 py-1 px-2 rounded hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
        >
          ⏸ Pause for 30 min
        </button>
        <button
          onclick={() => pauseFor(60)}
          class="w-full text-left text-sm text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-100 py-1 px-2 rounded hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
        >
          ⏸ Pause for 1 hr
        </button>
      {/if}
    </div>
  {:else}
    <div class="px-4 py-2">
      <p class="text-xs text-red-500 dark:text-red-400 font-medium flex items-center gap-1">
        <span>🔒</span>
        <span>Strict mode — skipping disabled</span>
      </p>
    </div>
  {/if}

  <!-- Footer -->
  <div class="px-4 py-2 flex justify-between">
    <button
      onclick={onOpenSettings}
      class="text-sm text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 transition-colors"
    >
      ⚙ Settings
    </button>
    <button
      onclick={quit}
      class="text-sm text-gray-500 dark:text-gray-400 hover:text-red-600 dark:hover:text-red-400 transition-colors"
    >
      Quit
    </button>
  </div>
</div>
