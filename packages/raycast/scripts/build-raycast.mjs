import { execFileSync } from "node:child_process";

execFileSync("node", ["../../scripts/sync-locales.mjs"], { stdio: "inherit" });
execFileSync("node", ["../../scripts/sync-version.mjs", "--check"], { stdio: "inherit" });
execFileSync("npx", ["lingui", "compile", "--typescript"], { stdio: "inherit" });
execFileSync("npx", ["tsc", "--noEmit"], { stdio: "inherit" });
execFileSync("npx", ["ray", "build", "-e", "dist"], { stdio: "inherit" });
