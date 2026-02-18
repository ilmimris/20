<script>
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  let { onClose } = $props();

  let config = $state({
    work_interval_minutes: 20,
    break_duration_seconds: 20,
    strict_mode: false,
    overlay_theme: "dark",
    sound: "off",
    launch_at_login: true,
    pre_warning_seconds: 60,
    meeting_detection: true,
  });

  let isSaving = $state(false);
  let saveError = $state(null);
  let saveSuccess = $state(false);

  onMount(async () => {
    try {
      const loaded = await invoke("get_config");
      config = { ...config, ...loaded };
    } catch (e) {
      console.error("Failed to load config:", e);
    }
  });

  async function save() {
    isSaving = true;
    saveError = null;
    saveSuccess = false;
    try {
      await invoke("save_config", { config });
      saveSuccess = true;
      setTimeout(() => {
        saveSuccess = false;
      }, 2000);
    } catch (e) {
      saveError = e.toString();
    } finally {
      isSaving = false;
    }
  }
</script>

<div class="flex flex-col h-full max-h-[500px] overflow-hidden">
  <!-- Header -->
  <div class="flex items-center justify-between px-4 py-3 border-b border-gray-100 dark:border-gray-800 bg-gray-50 dark:bg-gray-900/80">
    <h1 class="text-sm font-semibold text-gray-900 dark:text-white">Settings</h1>
    <button
      onclick={onClose}
      class="text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 text-lg leading-none"
      aria-label="Close settings"
    >
      ×
    </button>
  </div>

  <!-- Scrollable content -->
  <div class="flex-1 overflow-y-auto px-4 py-3 space-y-5 dark:bg-gray-900">

    <!-- Timer section -->
    <section>
      <h2 class="text-xs font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider mb-2">Timer</h2>
      <div class="space-y-3">
        <label class="flex items-center justify-between">
          <span class="text-sm text-gray-700 dark:text-gray-300">Work interval (minutes)</span>
          <input
            type="number"
            min="1"
            max="60"
            bind:value={config.work_interval_minutes}
            class="w-16 text-sm text-right border border-gray-300 dark:border-gray-600 rounded px-2 py-1 dark:bg-gray-800 dark:text-white"
          />
        </label>
        <label class="flex items-center justify-between">
          <span class="text-sm text-gray-700 dark:text-gray-300">Break duration (seconds)</span>
          <input
            type="number"
            min="5"
            max="60"
            bind:value={config.break_duration_seconds}
            class="w-16 text-sm text-right border border-gray-300 dark:border-gray-600 rounded px-2 py-1 dark:bg-gray-800 dark:text-white"
          />
        </label>
      </div>
    </section>

    <!-- Behavior section -->
    <section>
      <h2 class="text-xs font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider mb-2">Behavior</h2>
      <div class="space-y-3">
        <label class="flex items-center justify-between">
          <div>
            <span class="text-sm text-gray-700 dark:text-gray-300 block">Strict mode</span>
            <span class="text-xs text-gray-400 dark:text-gray-500">Disable skip/pause. Press Esc × 3 to exit.</span>
          </div>
          <input
            type="checkbox"
            bind:checked={config.strict_mode}
            class="ml-3 h-4 w-4 rounded border-gray-300 text-indigo-600 dark:border-gray-600"
          />
        </label>
        <label class="flex items-center justify-between">
          <span class="text-sm text-gray-700 dark:text-gray-300">Launch at login</span>
          <input
            type="checkbox"
            bind:checked={config.launch_at_login}
            class="ml-3 h-4 w-4 rounded border-gray-300 text-indigo-600 dark:border-gray-600"
          />
        </label>
        <label class="flex items-center justify-between">
          <span class="text-sm text-gray-700 dark:text-gray-300">Meeting detection (auto-pause)</span>
          <input
            type="checkbox"
            bind:checked={config.meeting_detection}
            class="ml-3 h-4 w-4 rounded border-gray-300 text-indigo-600 dark:border-gray-600"
          />
        </label>
      </div>
    </section>

    <!-- Appearance section -->
    <section>
      <h2 class="text-xs font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider mb-2">Appearance</h2>
      <div class="space-y-3">
        <label class="flex items-center justify-between">
          <span class="text-sm text-gray-700 dark:text-gray-300">Overlay theme</span>
          <select
            bind:value={config.overlay_theme}
            class="text-sm border border-gray-300 dark:border-gray-600 rounded px-2 py-1 dark:bg-gray-800 dark:text-white"
          >
            <option value="dark">Dark</option>
            <option value="light">Light</option>
            <option value="nature">Nature</option>
          </select>
        </label>
        <label class="flex items-center justify-between">
          <span class="text-sm text-gray-700 dark:text-gray-300">Sound</span>
          <select
            bind:value={config.sound}
            class="text-sm border border-gray-300 dark:border-gray-600 rounded px-2 py-1 dark:bg-gray-800 dark:text-white"
          >
            <option value="off">Off</option>
            <option value="chime">Chime</option>
            <option value="whitenoise">White noise</option>
          </select>
        </label>
      </div>
    </section>

    <!-- Notifications section -->
    <section>
      <h2 class="text-xs font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider mb-2">Notifications</h2>
      <label class="flex items-center justify-between">
        <span class="text-sm text-gray-700 dark:text-gray-300">Pre-break warning</span>
        <select
          bind:value={config.pre_warning_seconds}
          class="text-sm border border-gray-300 dark:border-gray-600 rounded px-2 py-1 dark:bg-gray-800 dark:text-white"
        >
          <option value={0}>Off</option>
          <option value={30}>30 seconds</option>
          <option value={60}>1 minute</option>
          <option value={120}>2 minutes</option>
        </select>
      </label>
    </section>
  </div>

  <!-- Save button -->
  <div class="px-4 py-3 border-t border-gray-100 dark:border-gray-800 dark:bg-gray-900">
    {#if saveError}
      <p class="text-xs text-red-500 mb-2">{saveError}</p>
    {/if}
    {#if saveSuccess}
      <p class="text-xs text-green-500 mb-2">Settings saved!</p>
    {/if}
    <button
      onclick={save}
      disabled={isSaving}
      class="w-full bg-indigo-600 hover:bg-indigo-700 disabled:opacity-50 text-white text-sm font-medium py-2 px-4 rounded-lg transition-colors"
    >
      {isSaving ? "Saving…" : "Save Settings"}
    </button>
  </div>
</div>
