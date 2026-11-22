import { i18n } from "./i18n";
import "./i18n";
import { showHUD } from "@raycast/api";
import { queryDaemon } from "./socketClient";

export default async function Command() {
  await queryDaemon({ type: "jump_next_alert" });
  await showHUD(
    i18n._({
      id: "Q0dD37",
      message: "Jumped to next agent alert",
    }),
  );
}
