import { i18n } from "./i18n";
import { useState } from "react";
import {
  Action,
  ActionPanel,
  Form,
  showToast,
  Toast,
  useNavigation,
} from "@raycast/api";
import { runBraidInstall } from "./bootstrap";
import "./i18n";

export function PairBrowserForm({ onDone }: { onDone: () => void }) {
  const { pop } = useNavigation();
  const [extId, setExtId] = useState("");

  const handleSubmit = async () => {
    const clean = extId.trim().toLowerCase();
    if (!/^[a-p]{32}$/.test(clean)) {
      await showToast({
        style: Toast.Style.Failure,
        title: i18n._({
          id: "Wo6Rfr",
          message: "Invalid ID (32 lowercase letters expected)",
        }),
      });
      return;
    }
    const toast = await showToast({
      style: Toast.Style.Animated,
      title: i18n._({
        id: "Q_agIs",
        message: "Registering extension...",
      }),
    });
    try {
      await runBraidInstall(clean);
      toast.style = Toast.Style.Success;
      toast.title = i18n._({
        id: "QCxG5o",
        message: "Extension registered! Click Reload (↻) in your browser.",
      });
      onDone();
      pop();
    } catch (err) {
      toast.style = Toast.Style.Failure;
      toast.title = i18n._({
        id: "SlfejT",
        message: "Error",
      });
      toast.message = err instanceof Error ? err.message : String(err);
    }
  };

  return (
    <Form
      navigationTitle={i18n._({
        id: "e5M56e",
        message: "Register Local Browser Extension ID",
      })}
      actions={
        <ActionPanel>
          <Action.SubmitForm
            title={i18n._({
              id: "Vcjiat",
              message: "Save and Reconnect",
            })}
            onSubmit={handleSubmit}
          />
        </ActionPanel>
      }
    >
      <Form.TextField
        id="extId"
        title={i18n._({
          id: "Uu52G6",
          message: "32-character Extension ID",
        })}
        placeholder="npejknmlpfojkmpniiklpndcjhpjehaf"
        value={extId}
        onChange={setExtId}
      />
      <Form.Description
        title={i18n._({
          id: "WN8Hf4",
          message: "How to find it",
        })}
        text={i18n._({
          id: "pHJwW9",
          message:
            "1. Open chrome://extensions or brave://extensions\n2. Enable Developer Mode\n3. Copy the ID displayed under ContextBraid",
        })}
      />
    </Form>
  );
}
