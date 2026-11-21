import { useState } from 'react';
import { t } from '@lingui/core/macro';
import { Action, ActionPanel, Form, showToast, Toast, useNavigation } from '@raycast/api';
import { runBraidInstall } from './bootstrap';
import './i18n';

export function PairBrowserForm({ onDone }: { onDone: () => void }) {
  const { pop } = useNavigation();
  const [extId, setExtId] = useState('');

  const handleSubmit = async () => {
    const clean = extId.trim().toLowerCase();
    if (!/^[a-p]{32}$/.test(clean)) {
      await showToast({ style: Toast.Style.Failure, title: t`Invalid ID (32 lowercase letters expected)` });
      return;
    }
    const toast = await showToast({ style: Toast.Style.Animated, title: t`Registering extension...` });
    try {
      await runBraidInstall(clean);
      toast.style = Toast.Style.Success;
      toast.title = t`Extension registered! Click Reload (↻) in your browser.`;
      onDone();
      pop();
    } catch (err) {
      toast.style = Toast.Style.Failure;
      toast.title = t`Error`;
      toast.message = err instanceof Error ? err.message : String(err);
    }
  };

  return (
    <Form
      navigationTitle={t`Register Local Browser Extension ID`}
      actions={
        <ActionPanel>
          <Action.SubmitForm title={t`Save and Reconnect`} onSubmit={handleSubmit} />
        </ActionPanel>
      }
    >
      <Form.TextField
        id="extId"
        title={t`32-character Extension ID`}
        placeholder="npejknmlpfojkmpniiklpndcjhpjehaf"
        value={extId}
        onChange={setExtId}
      />
      <Form.Description
        title={t`How to find it`}
        text={t`1. Open chrome://extensions or brave://extensions\n2. Enable Developer Mode\n3. Copy the ID displayed under ContextBraid`}
      />
    </Form>
  );
}
