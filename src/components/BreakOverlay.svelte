<script>
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";

  let initialBreakDuration = $state(20);
  let secondsLeft = $state(20);
  let isPrimary = $state(false);
  let isStrictMode = $state(false);
  let escapeCount = $state(0);
  let escapeResetTimer = null;

  onMount(async () => {
    // Primary source: initialization script injected by Rust before page load.
    const injected = window.__EYEBREAK_OVERLAY_CONFIG__;
    if (injected) {
      initialBreakDuration = injected.breakDuration ?? 20;
      secondsLeft = injected.breakDuration ?? 20;
      isPrimary = injected.isPrimary ?? false;
      isStrictMode = injected.isStrictMode ?? false;
    } else {
      // Fallback: fetch from backend (development mode / direct open).
      try {
        const config = await invoke("get_overlay_config");
        initialBreakDuration = config.break_duration;
        secondsLeft = config.break_duration;
        isPrimary = config.is_primary;
        isStrictMode = config.is_strict_mode;
      } catch (e) {
        console.error("Failed to get overlay config:", e);
      }
    }

    // Listen for countdown ticks
    const unlistenBreakTick = await listen("break:tick", (event) => {
      secondsLeft = event.payload.seconds_remaining;
    });

    // Listen for break end
    const unlistenBreakEnd = await listen("break:end", () => {
      // Overlay will be closed by backend; no action needed
    });

    // Handle keyboard for strict mode escape
    if (isStrictMode) {
      document.addEventListener("keydown", handleKeyDown);
    }

    return () => {
      unlistenBreakTick();
      unlistenBreakEnd();
      if (isStrictMode) {
        document.removeEventListener("keydown", handleKeyDown);
      }
    };
  });

  function handleKeyDown(e) {
    if (e.key === "Escape") {
      escapeCount++;

      // Clear previous reset timer
      if (escapeResetTimer) clearTimeout(escapeResetTimer);

      // Reset escape count after 5 seconds
      escapeResetTimer = setTimeout(() => {
        escapeCount = 0;
      }, 5000);

      if (escapeCount >= 3) {
        escapeCount = 0;
        forceSkip();
      }
    }
  }

  async function forceSkip() {
    try {
      await invoke("force_skip_break");
    } catch (e) {
      console.error("Failed to force skip:", e);
    }
  }

  // Compute progress for the circular timer
  let progress = $derived(initialBreakDuration > 0 ? secondsLeft / initialBreakDuration : 0);
  let circumference = 2 * Math.PI * 80; // r=80
  let dashoffset = $derived(circumference * (1 - progress));
</script>

<div
  class="fixed inset-0 flex items-center justify-center bg-gray-950/80 backdrop-blur-sm animate-fade-in"
  aria-live="polite"
  aria-atomic="true"
  role="dialog"
  aria-label="Eye break — look away from your screen"
>
  <!-- Breathing background circle -->
  <div
    class="absolute inset-0 flex items-center justify-center pointer-events-none"
    aria-hidden="true"
  >
    <div
      class="w-[600px] h-[600px] rounded-full bg-indigo-500/10 animate-breathe"
    ></div>
  </div>

  <!-- Main content (primary display only shows timer) -->
  {#if isPrimary}
    <div class="relative flex flex-col items-center gap-8 z-10">
      <!-- Circular countdown -->
      <div class="relative w-56 h-56" role="timer" aria-label="{secondsLeft} seconds remaining">
        <svg
          class="w-full h-full -rotate-90"
          viewBox="0 0 180 180"
          aria-hidden="true"
        >
          <!-- Background ring -->
          <circle
            cx="90"
            cy="90"
            r="80"
            fill="none"
            stroke="rgba(255,255,255,0.1)"
            stroke-width="8"
          />
          <!-- Progress ring -->
          <circle
            cx="90"
            cy="90"
            r="80"
            fill="none"
            stroke="rgba(99,102,241,0.9)"
            stroke-width="8"
            stroke-linecap="round"
            stroke-dasharray={circumference}
            stroke-dashoffset={dashoffset}
            class="transition-all duration-1000 ease-linear"
          />
        </svg>
        <!-- Timer number -->
        <div
          class="absolute inset-0 flex items-center justify-center"
        >
          <span
            class="text-7xl font-thin text-white tabular-nums"
            style="font-size: clamp(3rem, 8vw, 4.5rem)"
          >
            {secondsLeft}
          </span>
        </div>
      </div>

      <!-- Instruction text -->
      <div class="text-center space-y-3">
        <p class="text-3xl font-light text-white tracking-wide" style="font-size: max(2rem, 36px)">
          Look 20 feet away
        </p>
        <p class="text-lg text-white/60 font-light">
          Rest your eyes for 20 seconds
        </p>
      </div>

      <!-- Strict mode escape hint -->
      {#if isStrictMode}
        <p class="text-sm text-white/30 mt-4">
          {#if escapeCount === 0}
            Press Esc × 3 to skip in an emergency
          {:else}
            Esc pressed {escapeCount}/3 — keep going to force skip
          {/if}
        </p>
      {/if}
    </div>
  {:else}
    <!-- Secondary display: just the dim overlay with a subtle label -->
    <div class="text-white/40 text-xl font-light tracking-widest uppercase">
      EyeBreak
    </div>
  {/if}
</div>
