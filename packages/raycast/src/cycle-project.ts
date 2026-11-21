import { appendFileSync } from "node:fs";
import { queryDaemon } from "./socketClient";

export default async function Command() {
  const ts = new Date().toISOString();
  try {
    const res = await queryDaemon({ type: "cycle_project", repo_name: null });
    appendFileSync(
      "/tmp/braid-raycast-cycle.log",
      `${ts} OK res=${JSON.stringify(res)}\n`,
    );
  } catch (err) {
    appendFileSync(
      "/tmp/braid-raycast-cycle.log",
      `${ts} ERR ${String(err)}\n`,
    );
    throw err;
  }
}
