import { spawn } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";

const tauriScript = join("node_modules", "@tauri-apps", "cli", "tauri.js");
const cargoTargetDir = join(tmpdir(), "dcs-mission-composer-target");

const child = spawn(process.execPath, [tauriScript, ...process.argv.slice(2)], {
  env: {
    ...process.env,
    CARGO_TARGET_DIR: cargoTargetDir,
  },
  stdio: "inherit",
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }

  process.exit(code ?? 1);
});
