<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount } from "svelte";
  import {
    githubApiUrl,
    installerDownloadUrl,
    MOCK_LATEST_RELEASE,
    updateStateForRelease,
    type GitHubRelease,
    type UpdateState,
  } from "./update";

  type Slot = "original" | "modified";
  type FileState = "idle" | "hovering" | "checking" | "valid" | "error";
  type WorkState = "idle" | "working" | "complete" | "error";
  type ExportMode = "coalition" | "flight";

  type MissionSlot = {
    path: string;
    state: FileState;
    message: string;
  };

  type MissionDiff = {
    safe_to_merge: boolean;
    summary: string;
    details: string[];
    warnings: string[];
  };

  type AppInfo = {
    version: string;
    github_repo_url: string;
    github_releases_url: string;
  };

  type FlightInfo = {
    id: string;
    name: string;
    aircraft_type?: string;
    aircraftType?: string;
    coalition: "blue" | "red";
    category: string;
  };

  const emptySlot = (message: string): MissionSlot => ({
    path: "",
    state: "idle",
    message,
  });

  const originalEmptyMessage = "Choose or drop the original .miz file.";
  const modifiedEmptyMessage = "Choose or drop the modified planning .miz file.";
  // Flip this off if character-by-character review output feels too busy.
  const ENABLE_REVIEW_TYPEWRITER = true;
  const REVIEW_TYPEWRITER_INTERVAL_MS = 18;
  const REVIEW_TYPEWRITER_MAX_CHARS = 900;
  const LARGE_REVIEW_INTRO_HOLD_MS = 1200;
  const LARGE_REVIEW_DOT_INTERVAL_MS = 420;
  const STARTUP_LOADER_MIN_MS = 850;
  // Set true in dev/tests to preview the large-mission review path with a normal mission.
  const FORCE_LARGE_REVIEW_RENDER = false;
  const reviewProgressText = "Opening mission archives...\nReading mission data...\nComparing mission changes...";
  const largeReviewIntroText = "Large .miz file detected.\nRendering full review.";

  let original = emptySlot(originalEmptyMessage);
  let modified = emptySlot(modifiedEmptyMessage);
  let activeDropTarget: Slot = "original";
  let exportState: WorkState = "idle";
  let mergeState: WorkState = "idle";
  let exportMessage = "";
  let mergeMessage = "";
  let exportedFile = "";
  let mergedFile = "";
  let showOverrideConfirm = false;
  let showUpdateConfirm = false;
  let comparison: MissionDiff | null = null;
  let reviewTyping = false;
  let reviewTypedText = "";
  let reviewTypedLines: string[] = [];
  let reviewAnimationId = 0;
  let reviewTypingTimer: number | undefined;
  let exportMode: ExportMode = "coalition";
  let exportCoalition: "blue" | "red" = "blue";
  let detectedFlights: FlightInfo[] = [];
  let selectedFlightId = "";
  let overrideCoalition: "blue" | "red" = "blue";
  let logPath = "";
  let logMessage = "";
  let logMessageTimer: number | undefined;
  let appInfo: AppInfo | null = null;
  let updateState: UpdateState = "idle";
  let latestRelease: GitHubRelease | null = null;
  let startupLoading = true;
  const appWindow = getCurrentWindow();

  const validateMission = async (slot: Slot, path: string) => {
    const next = { path, state: "checking" as FileState, message: "Checking mission archive..." };

    if (slot === "original") {
      original = next;
      exportState = "idle";
      exportMessage = "";
      exportedFile = "";
    } else {
      modified = next;
    }

    clearReviewAnimation();
    mergeState = "idle";
    mergeMessage = "";
    mergedFile = "";

    try {
      const message = await invoke<string>("validate_miz", { path });
      if (slot === "original") {
        original = { path, state: "valid", message };
        await loadDetectedFlights(path);
      } else {
        modified = { path, state: "valid", message };
      }
      await compareIfReady();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (slot === "original") {
        original = { path, state: "error", message };
      } else {
        modified = { path, state: "error", message };
      }
    }
  };

  const loadDetectedFlights = async (path: string) => {
    try {
      detectedFlights = await invoke<FlightInfo[]>("detect_flights", { path });
      syncSelectedFlight();
    } catch {
      detectedFlights = [];
      selectedFlightId = "";
    }
  };

  const compareIfReady = async () => {
    if (original.state !== "valid" || modified.state !== "valid") {
      return;
    }

    clearReviewAnimation();
    const compareAnimationId = reviewAnimationId;
    showReviewProgress(compareAnimationId);

    try {
      const nextComparison = await invoke<MissionDiff>("compare_modified_miz", {
        originalPath: original.path,
        modifiedPath: modified.path,
      });
      if (compareAnimationId !== reviewAnimationId) {
        return;
      }
      startReviewTypewriter(nextComparison);
    } catch (error) {
      if (compareAnimationId !== reviewAnimationId) {
        return;
      }
      startReviewTypewriter({
        safe_to_merge: false,
        summary: "Unable to compare missions.",
        details: [],
        warnings: [error instanceof Error ? error.message : String(error)],
      });
    }
  };

  const browseForMission = async (slot: Slot) => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "DCS Mission", extensions: ["miz"] }],
    });

    if (typeof selected === "string") {
      await validateMission(slot, selected);
    }
  };

  const exportPlanningMission = async () => {
    if (!original.path || original.state !== "valid") {
      exportState = "error";
      exportMessage = "Choose a valid original mission before exporting.";
      return;
    }

    const selectedFlight = selectedFlightForExport();
    if (exportMode === "flight" && !selectedFlight) {
      exportState = "error";
      exportMessage = "Choose a detected flight before exporting.";
      return;
    }

    const outputPath = await save({
      title: exportMode === "flight" && selectedFlight
        ? `Export ${flightLabel(selectedFlight)} planning mission`
        : `Export ${exportCoalition.toUpperCase()} planning mission`,
      defaultPath: defaultExportPath(selectedFlight),
      filters: [{ name: "DCS Mission", extensions: ["miz"] }],
    });

    if (!outputPath) {
      return;
    }

    if (samePath(outputPath, original.path)) {
      exportState = "error";
      exportMessage = "Choose a different output file so the original mission is preserved.";
      exportedFile = "";
      return;
    }

    exportState = "working";
    exportMessage = selectedFlight
      ? `Creating ${flightLabel(selectedFlight)} planning mission...`
      : `Creating ${exportCoalition.toUpperCase()} planning mission...`;
    exportedFile = "";

    try {
      const exportedPath = await invoke<string>("export_planning_miz", {
        sourcePath: original.path,
        outputPath,
        coalition: exportCoalition,
        flightId: selectedFlight?.id ?? null,
      });
      exportState = "complete";
      exportedFile = fileNameFromPath(exportedPath);
      exportMessage = "Planning mission exported.";
    } catch (error) {
      exportState = "error";
      exportMessage = error instanceof Error ? error.message : String(error);
      exportedFile = "";
    }
  };

  const mergeMission = async (forceMerge = false, coalitionOverride: "blue" | "red" | null = null) => {
    if (!comparison) {
      mergeState = "error";
      mergeMessage = "Review the comparison before merging.";
      return;
    }

    if (!comparison.safe_to_merge && !forceMerge) {
      mergeState = "error";
      mergeMessage = "Use override if you want to merge despite review warnings.";
      return;
    }

    mergeState = "idle";
    mergeMessage = "";
    mergedFile = "";

    const outputPath = await save({
      title: forceMerge ? "Save override merged mission" : "Save merged mission",
      defaultPath: original.path.replace(/\.miz$/i, "-merged.miz"),
      filters: [{ name: "DCS Mission", extensions: ["miz"] }],
    });

    if (!outputPath) {
      return;
    }

    if (samePath(outputPath, original.path) || samePath(outputPath, modified.path)) {
      mergeState = "error";
      mergeMessage = "Output matches a loaded mission.";
      mergedFile = "";
      return;
    }

    mergeState = "working";
    mergeMessage = forceMerge
      ? "Override merging planning coalition changes..."
      : "Merging planning coalition changes...";
    mergedFile = "";

    try {
      const mergedPath = await invoke<string>("merge_planning_miz", {
        originalPath: original.path,
        modifiedPath: modified.path,
        outputPath,
        forceMerge,
        coalitionOverride,
      });
      mergeState = "complete";
      mergedFile = fileNameFromPath(mergedPath);
      mergeMessage = forceMerge ? "Override merged mission saved." : "Merged mission saved.";
    } catch (error) {
      mergeState = "error";
      mergeMessage = error instanceof Error ? error.message : String(error);
      mergedFile = "";
    }
  };

  const requestOverrideMerge = () => {
    if (!comparison || comparison.safe_to_merge) {
      return;
    }

    showOverrideConfirm = true;
    overrideCoalition = exportCoalition;
  };

  const confirmOverrideMerge = () => {
    showOverrideConfirm = false;
    void mergeMission(true, overrideCoalition);
  };

  const selectExportCoalition = (coalition: "blue" | "red") => {
    exportCoalition = coalition;
    syncSelectedFlight();
  };

  const selectExportMode = (mode: ExportMode) => {
    exportMode = mode;
    syncSelectedFlight();
  };

  const resetMission = (slot: Slot) => {
    if (slot === "original") {
      original = emptySlot(originalEmptyMessage);
      exportState = "idle";
      exportMessage = "";
      exportedFile = "";
      detectedFlights = [];
      selectedFlightId = "";
      exportMode = "coalition";
    } else {
      modified = emptySlot(modifiedEmptyMessage);
    }

    clearReviewAnimation();
    mergeState = "idle";
    mergeMessage = "";
    mergedFile = "";
  };

  const handleBrowseKeydown = (event: KeyboardEvent, slot: Slot) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }

    event.preventDefault();
    void browseForMission(slot);
  };

  const fileNameFromPath = (path: string) => path.split(/[\\/]/).pop() ?? "mission.miz";

  const samePath = (left: string, right: string) => left.toLocaleLowerCase() === right.toLocaleLowerCase();

  const flightsForExportCoalition = () => detectedFlights.filter((flight) => flight.coalition === exportCoalition);

  const selectedFlightForExport = () => flightsForExportCoalition().find((flight) => flight.id === selectedFlightId);

  const canExport = () => original.state === "valid" && exportState !== "working" && (exportMode === "coalition" || !!selectedFlightId);

  const clearReviewAnimation = () => {
    reviewAnimationId += 1;
    window.clearTimeout(reviewTypingTimer);
    reviewTyping = false;
    reviewTypedText = "";
    reviewTypedLines = [];
    comparison = null;
  };

  const reviewTypewriterText = (diff: MissionDiff) => {
    const warningLines = diff.warnings.map((warning) => `Warning: ${warning}`);
    const detailLines = diff.details.length ? diff.details : ["No comparison details were generated."];
    return [...warningLines, ...detailLines].join("\n");
  };

  const shouldUseLargeReviewIntro = (reviewText: string) =>
    FORCE_LARGE_REVIEW_RENDER || reviewText.length > REVIEW_TYPEWRITER_MAX_CHARS;

  const setReviewTypedText = (text: string) => {
    reviewTypedText = text;
    reviewTypedLines = reviewTypedText
      .split("\n")
      .map((line) => line.trimEnd())
      .filter(Boolean);
  };

  const startReviewTextTypewriter = (text: string, animationId: number, onComplete?: () => void) => {
    window.clearTimeout(reviewTypingTimer);
    reviewTyping = true;
    setReviewTypedText("");

    const tick = () => {
      if (animationId !== reviewAnimationId) {
        return;
      }

      const nextLength = Math.min(reviewTypedText.length + 1, text.length);
      setReviewTypedText(text.slice(0, nextLength));

      if (nextLength < text.length) {
        reviewTypingTimer = window.setTimeout(tick, REVIEW_TYPEWRITER_INTERVAL_MS);
        return;
      }

      onComplete?.();
    };

    tick();
  };

  const finishLargeReviewIntro = (animationId: number, diff: MissionDiff) => {
    let elapsedMs = 0;
    let extraDots = 0;

    const tick = () => {
      if (animationId !== reviewAnimationId) {
        return;
      }

      if (elapsedMs >= LARGE_REVIEW_INTRO_HOLD_MS) {
        reviewTyping = false;
        setReviewTypedText("");
        comparison = diff;
        return;
      }

      extraDots += 1;
      elapsedMs += LARGE_REVIEW_DOT_INTERVAL_MS;
      setReviewTypedText(`${largeReviewIntroText} ${Array(extraDots).fill(".").join(" ")}`);
      reviewTypingTimer = window.setTimeout(tick, LARGE_REVIEW_DOT_INTERVAL_MS);
    };

    reviewTypingTimer = window.setTimeout(tick, LARGE_REVIEW_DOT_INTERVAL_MS);
  };

  const showReviewProgress = (animationId: number) => {
    window.clearTimeout(reviewTypingTimer);
    reviewTyping = true;
    setReviewTypedText(reviewProgressText);

    if (animationId !== reviewAnimationId) {
      return;
    }
  };

  const startReviewTypewriter = (diff: MissionDiff) => {
    const reviewText = reviewTypewriterText(diff);

    if (!ENABLE_REVIEW_TYPEWRITER) {
      window.clearTimeout(reviewTypingTimer);
      reviewTyping = false;
      setReviewTypedText("");
      comparison = diff;
      return;
    }

    const animationId = reviewAnimationId + 1;
    reviewAnimationId = animationId;
    comparison = null;

    if (shouldUseLargeReviewIntro(reviewText)) {
      startReviewTextTypewriter(largeReviewIntroText, animationId, () => {
        finishLargeReviewIntro(animationId, diff);
      });
      return;
    }

    startReviewTextTypewriter(reviewText, animationId, () => {
      reviewTyping = false;
      comparison = diff;
    });
  };

  const syncSelectedFlight = () => {
    const flights = flightsForExportCoalition();

    if (!flights.length) {
      selectedFlightId = "";
      return;
    }

    if (!flights.some((flight) => flight.id === selectedFlightId)) {
      selectedFlightId = flights[0].id;
    }
  };

  const flightAirframe = (flight: FlightInfo) => flight.aircraft_type ?? flight.aircraftType ?? "Unknown airframe";

  const flightLabel = (flight: FlightInfo) => `${flightAirframe(flight)} - ${flight.name}`;

  const safeFileSegment = (value: string) =>
    value
      .replace(/[<>:"/\\|?*]+/g, "-")
      .replace(/\s+/g, "-")
      .replace(/-+/g, "-")
      .replace(/^-|-$/g, "")
      .toLocaleLowerCase();

  const defaultExportPath = (selectedFlight: FlightInfo | undefined) => {
    if (selectedFlight) {
      return original.path.replace(
        /\.miz$/i,
        `-${safeFileSegment(flightAirframe(selectedFlight))}-${safeFileSegment(selectedFlight.name)}-planning.miz`,
      );
    }

    return original.path.replace(/\.miz$/i, `-${exportCoalition}-planning.miz`);
  };

  const checkForUpdates = async (info: AppInfo) => {
    if (MOCK_LATEST_RELEASE) {
      latestRelease = MOCK_LATEST_RELEASE;
      updateState = updateStateForRelease(info.version, MOCK_LATEST_RELEASE);
      return;
    }

    const apiUrl = githubApiUrl(info.github_releases_url);

    if (!apiUrl) {
      updateState = "error";
      return;
    }

    updateState = "checking";

    try {
      const response = await fetch(apiUrl, {
        headers: {
          Accept: "application/vnd.github+json",
        },
      });

      if (!response.ok) {
        throw new Error(`GitHub returned ${response.status}`);
      }

      const release = (await response.json()) as GitHubRelease;
      latestRelease = release;
      updateState = updateStateForRelease(info.version, release);
    } catch {
      updateState = "error";
    }
  };

  const reviewState = () => {
    if (reviewTyping) {
      return "typing";
    }

    if (!comparison) {
      return "idle";
    }

    return comparison.safe_to_merge ? "safe" : "blocked";
  };

  const reviewStatusLabel = () => {
    if (reviewTyping) {
      return "Reviewing";
    }

    if (!comparison) {
      return "Waiting";
    }

    return comparison.safe_to_merge ? "Ready to merge" : "Blocked";
  };

  const mergeStatusLabel = () => {
    if (mergeState === "complete") {
      return mergedFile ? `${mergeMessage} ${mergedFile}` : mergeMessage;
    }

    if (mergeState === "error") {
      return mergeMessage;
    }

    return mergeMessage;
  };

  const reviewTitle = () => {
    if (reviewTyping) {
      return "Reviewing mission changes";
    }

    return comparison ? comparison.summary : "Load both missions to compare changes";
  };

  const updateStatusLabel = () => {
    if (updateState === "available") {
      return "Update available";
    }

    if (updateState === "checking") {
      return "Checking update";
    }

    if (updateState === "error") {
      return "Update unknown";
    }

    return "Up-to-date";
  };

  const minimizeWindow = () => {
    void appWindow.minimize();
  };

  const closeWindow = () => {
    void appWindow.close();
  };

  const openProjectUrl = async () => {
    if (appInfo?.github_repo_url) {
      await openUrl(appInfo.github_repo_url);
    }
  };

  const openLatestRelease = async () => {
    if (!latestRelease) {
      return;
    }

    await openUrl(installerDownloadUrl(latestRelease));
  };

  const requestUpdateDownload = () => {
    if (updateState === "available" && latestRelease) {
      showUpdateConfirm = true;
    }
  };

  const confirmUpdateDownload = () => {
    showUpdateConfirm = false;
    void openLatestRelease();
  };

  const setTemporaryLogMessage = (message: string) => {
    logMessage = message;
    window.clearTimeout(logMessageTimer);
    logMessageTimer = window.setTimeout(() => {
      logMessage = "";
    }, 2400);
  };

  const copyLogPath = async () => {
    if (!logPath) {
      setTemporaryLogMessage("Log unavailable");
      return;
    }

    try {
      await navigator.clipboard.writeText(logPath);
      setTemporaryLogMessage("Log path copied");
    } catch {
      setTemporaryLogMessage("Copy failed");
    }
  };

  const startWindowDrag = (event: MouseEvent) => {
    if (event.button !== 0) {
      return;
    }

    void appWindow.startDragging();
  };

  const handleGlobalKeydown = (event: KeyboardEvent) => {
    if (event.key !== "Escape") {
      return;
    }

    showOverrideConfirm = false;
    showUpdateConfirm = false;
  };

  onMount(() => {
    let unlisten: (() => void) | undefined;
    const startupStartedAt = Date.now();
    const finishStartup = () => {
      const remaining = Math.max(0, STARTUP_LOADER_MIN_MS - (Date.now() - startupStartedAt));
      window.setTimeout(() => {
        startupLoading = false;
      }, remaining);
    };

    window.addEventListener("keydown", handleGlobalKeydown);

    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          if (activeDropTarget === "original") {
            original = { ...original, state: "hovering", message: "Release to load the original mission." };
          } else {
            modified = { ...modified, state: "hovering", message: "Release to load the modified mission." };
          }
          return;
        }

        if (event.payload.type === "drop") {
          const [path] = event.payload.paths;

          if (!path) {
            return;
          }

          void validateMission(activeDropTarget, path);
          return;
        }

        if (original.state === "hovering") {
          original = { ...original, state: original.path ? "valid" : "idle", message: original.path ? "Valid DCS .miz file" : originalEmptyMessage };
        }
        if (modified.state === "hovering") {
          modified = { ...modified, state: modified.path ? "valid" : "idle", message: modified.path ? "Valid DCS .miz file" : modifiedEmptyMessage };
        }
      })
      .then((handler) => {
        unlisten = handler;
      });

    const logPathRequest = invoke<string>("get_log_file_path")
      .then((path) => {
        logPath = path;
      })
      .catch((error) => {
        logMessage = error instanceof Error ? error.message : String(error);
      });

    const appInfoRequest = invoke<AppInfo>("get_app_info")
      .then((info) => {
        appInfo = info;
        void checkForUpdates(info);
      })
      .catch(() => {
        updateState = "error";
      });

    void Promise.allSettled([logPathRequest, appInfoRequest]).then(finishStartup);

    return () => {
      window.removeEventListener("keydown", handleGlobalKeydown);
      window.clearTimeout(logMessageTimer);
      window.clearTimeout(reviewTypingTimer);
      unlisten?.();
    };
  });
</script>

<main class="shell">
  {#if startupLoading}
    <section class="startup-loader" aria-label="Starting DCS Mission Composer">
      <img class="startup-mark" src="/icons/app-mark.svg" alt="" />
      <div class="startup-route" aria-hidden="true">
        <span></span>
        <span></span>
        <span></span>
      </div>
      <p>DCS Mission Composer</p>
    </section>
  {/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header class="app-header" data-tauri-drag-region on:mousedown={startWindowDrag}>
    <div class="brand" data-tauri-drag-region>
      <img class="brand-mark" src="/icons/app-mark.svg" alt="" data-tauri-drag-region />
      <div class="brand-copy" data-tauri-drag-region>
        <p class="eyebrow" data-tauri-drag-region>DCS Mission Composer</p>
      </div>
    </div>
    <div class="header-actions">
      <button
        class="update-button"
        class:available={updateState === "available"}
        type="button"
        title={latestRelease ? `Latest release ${latestRelease.tag_name}` : updateStatusLabel()}
        disabled={updateState !== "available" || !latestRelease}
        on:mousedown|stopPropagation
        on:click={requestUpdateDownload}
      >
        {updateStatusLabel()}
      </button>
      <button
        class="github-button"
        type="button"
        title={appInfo?.github_repo_url ?? "Open GitHub"}
        disabled={!appInfo?.github_repo_url}
        on:mousedown|stopPropagation
        on:click={openProjectUrl}
      >
        GitHub
      </button>
      <button
        class="log-button"
        type="button"
        title={logPath || "Log path unavailable"}
        disabled={!logPath}
        on:mousedown|stopPropagation
        on:click={copyLogPath}
      >
        {logMessage || "Copy log path"}
      </button>
      <div class="window-controls">
        <button
          class="window-control minimize"
          type="button"
          aria-label="Minimize window"
          title="Minimize"
          on:mousedown|stopPropagation
          on:click={minimizeWindow}
        >
          <span aria-hidden="true"></span>
        </button>
        <button
          class="window-control close"
          type="button"
          aria-label="Close window"
          title="Close"
          on:mousedown|stopPropagation
          on:click={closeWindow}
        >
          <span aria-hidden="true"></span>
        </button>
      </div>
    </div>
  </header>

  <section class="mission-grid" aria-label="Mission files">
    <section
      class="drop-zone"
      class:active={original.state === "hovering"}
      class:checking={original.state === "checking"}
      class:error={original.state === "error"}
      class:valid={original.state === "valid"}
      role="button"
      tabindex="0"
      on:dragenter={() => (activeDropTarget = "original")}
      on:mouseenter={() => (activeDropTarget = "original")}
      on:click={() => void browseForMission("original")}
      on:keydown={(event) => handleBrowseKeydown(event, "original")}
    >
      <div class="drop-icon" aria-hidden="true">MAIN</div>
      <div class="file-copy">
        <p class="panel-label">Original</p>
        <h2>{original.state === "valid" ? "Original mission loaded" : "Original mission"}</h2>
        <p>{original.message}</p>
        {#if original.path}
          <div class="file-loaded-row">
            <p class="file-chip" title={fileNameFromPath(original.path)}>{fileNameFromPath(original.path)} loaded</p>
            <button class="clear-file" type="button" on:click|stopPropagation={() => resetMission("original")}>Clear</button>
          </div>
        {/if}
      </div>
      {#if !original.path}
        <div class="file-actions">
          <p class="file-prompt">{original.state === "error" ? "Choose another .miz file" : "Click to browse"}</p>
        </div>
      {/if}
    </section>

    <section
      class="drop-zone"
      class:active={modified.state === "hovering"}
      class:checking={modified.state === "checking"}
      class:error={modified.state === "error"}
      class:valid={modified.state === "valid"}
      role="button"
      tabindex="0"
      on:dragenter={() => (activeDropTarget = "modified")}
      on:mouseenter={() => (activeDropTarget = "modified")}
      on:click={() => void browseForMission("modified")}
      on:keydown={(event) => handleBrowseKeydown(event, "modified")}
    >
      <div class="drop-icon modified" aria-hidden="true">EDIT</div>
      <div class="file-copy">
        <p class="panel-label">Modified</p>
        <h2>{modified.state === "valid" ? "Modified mission loaded" : "Modified planning mission"}</h2>
        <p>{modified.message}</p>
        {#if modified.path}
          <div class="file-loaded-row">
            <p class="file-chip" title={fileNameFromPath(modified.path)}>{fileNameFromPath(modified.path)} loaded</p>
            <button class="clear-file" type="button" on:click|stopPropagation={() => resetMission("modified")}>Clear</button>
          </div>
        {/if}
      </div>
      {#if !modified.path}
        <div class="file-actions">
          <p class="file-prompt">{modified.state === "error" ? "Choose another .miz file" : "Click to browse"}</p>
        </div>
      {/if}
    </section>
  </section>

  <section
    class="diff-panel"
    class:ready={reviewState() === "safe"}
    class:blocked={reviewState() === "blocked"}
    aria-labelledby="diff-title"
  >
    <div class="workflow-actions">
      <div>
        <p class="panel-label">Export</p>
        <h2>Create planning mission</h2>
      </div>
      <div class="export-controls">
        <div class="coalition-field">
          <span>Coalition</span>
          <div class="coalition-mode" role="radiogroup" aria-label="Export coalition">
            <button
              class="blue-option"
              class:active={exportCoalition === "blue"}
              type="button"
              role="radio"
              aria-checked={exportCoalition === "blue"}
              on:click={() => selectExportCoalition("blue")}
            >
              BLUE
            </button>
            <button
              class="red-option"
              class:active={exportCoalition === "red"}
              type="button"
              role="radio"
              aria-checked={exportCoalition === "red"}
              on:click={() => selectExportCoalition("red")}
            >
              RED
            </button>
          </div>
        </div>
        <div class="scope-field">
          <span>Scope</span>
          <div class="export-mode" aria-label="Export scope">
            <button
              class:active={exportMode === "coalition"}
              type="button"
              on:click={() => selectExportMode("coalition")}
            >
              Coalition
            </button>
            <button
              class:active={exportMode === "flight"}
              type="button"
              disabled={!detectedFlights.length}
              title={detectedFlights.length ? "Export one detected flight" : "No flights detected"}
              on:click={() => selectExportMode("flight")}
            >
              Flight
            </button>
          </div>
        </div>
        {#if exportMode === "flight"}
          <label class="flight-field">
            <span>Flight</span>
            <select
              class="flight-select"
              bind:value={selectedFlightId}
              disabled={!flightsForExportCoalition().length}
            >
              {#if flightsForExportCoalition().length}
                {#each flightsForExportCoalition() as flight}
                  <option value={flight.id}>{flightLabel(flight)}</option>
                {/each}
              {:else}
                <option value="">No flights detected</option>
              {/if}
            </select>
          </label>
        {/if}
        <button
          class="secondary-button"
          class:ready={canExport()}
          disabled={!canExport()}
          on:click={exportPlanningMission}
        >
          {exportState === "working" ? "Exporting..." : "Export"}
        </button>
      </div>
      {#if exportMessage}
        <p class:success={exportState === "complete"} class:error-text={exportState === "error"} class="status-message">{exportMessage}</p>
      {/if}
      {#if exportedFile}
        <p class="saved-file">{exportedFile}</p>
      {/if}
    </div>

    <div class="diff-header">
      <div>
        <div class="review-meta">
          <p class="panel-label">Review</p>
          {#if mergeMessage && mergeState !== "working"}
            <p
              class="review-status feedback"
              class:success={mergeState === "complete"}
              class:error={mergeState === "error"}
              title={mergedFile || mergeMessage}
            >
              {mergeStatusLabel()}
            </p>
          {:else}
            <p class="review-status" class:typing={reviewState() === "typing"} class:ready={reviewState() === "safe"} class:blocked={reviewState() === "blocked"}>
              {reviewStatusLabel()}
            </p>
          {/if}
        </div>
        <h2 id="diff-title">{reviewTitle()}</h2>
      </div>
      <div class="merge-actions">
        <button class="merge-button" class:ready={comparison?.safe_to_merge} disabled={!comparison?.safe_to_merge || mergeState === "working"} on:click={() => mergeMission()}>
          {mergeState === "working" ? "Merging..." : "Merge"}
        </button>
        {#if comparison && !comparison.safe_to_merge}
          <button class="override-button" disabled={mergeState === "working"} on:click={requestOverrideMerge}>
            Override
          </button>
        {/if}
      </div>
    </div>

    {#if comparison?.warnings.length}
      <div class="warnings">
        {#each comparison.warnings as warning}
          <p>{warning}</p>
        {/each}
      </div>
    {/if}

    {#if reviewTyping}
      <div class="change-list typing">
        {#each reviewTypedLines as detail}
          <p>{detail}</p>
        {/each}
      </div>
    {:else if comparison}
      <div class="change-list">
        {#each comparison.details as detail}
          <p>{detail}</p>
        {/each}
      </div>
    {:else}
      <div class="empty-review">No comparison yet.</div>
    {/if}

  </section>

  {#if showOverrideConfirm}
    <section class="modal-backdrop" aria-labelledby="override-title" role="presentation">
      <div class="override-modal" role="dialog" aria-modal="true" aria-labelledby="override-title">
        <div class="modal-alert-icon" aria-hidden="true">!</div>
        <div>
          <p class="panel-label danger">Override merge</p>
          <h2 id="override-title">Merge despite review warnings?</h2>
          <p class="modal-copy">
            This will generate a merged mission even though DCS Mission Composer marked the review as blocked.
          </p>
          <div class="override-field">
            <p>Planning coalition to merge</p>
            <div class="override-mode" role="radiogroup" aria-label="Planning coalition to merge">
              <button
                class="blue-option"
                class:active={overrideCoalition === "blue"}
                type="button"
                role="radio"
                aria-checked={overrideCoalition === "blue"}
                on:click={() => (overrideCoalition = "blue")}
              >
                BLUE
              </button>
              <button
                class="red-option"
                class:active={overrideCoalition === "red"}
                type="button"
                role="radio"
                aria-checked={overrideCoalition === "red"}
                on:click={() => (overrideCoalition = "red")}
              >
                RED
              </button>
            </div>
          </div>
        </div>
        <div class="modal-actions">
          <button class="modal-cancel" type="button" on:click={() => (showOverrideConfirm = false)}>Cancel</button>
          <button class="modal-danger" type="button" on:click={confirmOverrideMerge}>Override merge</button>
        </div>
      </div>
    </section>
  {/if}

  {#if showUpdateConfirm && latestRelease}
    <section class="modal-backdrop update-backdrop" aria-labelledby="update-title" role="presentation">
      <div class="override-modal update-modal" role="dialog" aria-modal="true" aria-labelledby="update-title">
        <div class="modal-alert-icon update" aria-hidden="true">i</div>
        <div>
          <p class="panel-label">Update available</p>
          <h2 id="update-title">Download {latestRelease.tag_name}?</h2>
          <p class="modal-copy">
            This will download the latest release.
          </p>
        </div>
        <div class="modal-actions">
          <button class="modal-cancel" type="button" on:click={() => (showUpdateConfirm = false)}>Cancel</button>
          <button class="modal-update" type="button" on:click={confirmUpdateDownload}>Download update</button>
        </div>
      </div>
    </section>
  {/if}

  <span class="app-version" data-tauri-drag-region>{appInfo ? `v${appInfo.version}` : "v..."}</span>
</main>
