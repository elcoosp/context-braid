import "./i18n";
import { t } from "@lingui/core/macro";
import { showHUD } from "@raycast/api";
import { queryDaemon } from "./socketClient";

export default async function Command() {
  await queryDaemon({ type: "jump_next_alert" });
  await showHUD(t`Jumped to next agent alert`);
}
