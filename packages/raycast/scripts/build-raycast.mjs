import { execFileSync } from "node:child_process";

execFileSync("npx", ["lingui", "compile", "--typescript"], { stdio: "inherit" });
execFileSync("npx", ["tsc", "--noEmit"], { stdio: "inherit" });
execFileSync("npx", ["ray", "build", "-e", "dist"], { stdio: "inherit" });
