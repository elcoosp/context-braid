import { useState } from 'react';
import { Action, ActionPanel, Form, Icon, showToast, Toast, useNavigation } from '@raycast/api';
import { ensureBraidInstalledAndRunning } from './bootstrap';

export function PairBrowserForm({ onDone }: { onDone: () => void }) {
  const { pop } = useNavigation();
  const [extId, setExtId] = useState('');

  const handleSubmit = async () => {
    const toast = await showToast({ style: Toast.Style.Animated, title: 'Configuration en cours...' });
    try {
      await ensureBraidInstalledAndRunning(extId);
      toast.style = Toast.Style.Success;
      toast.title = 'Navigateur connecté';
      onDone();
      pop();
    } catch (err) {
      toast.style = Toast.Style.Failure;
      toast.title = 'Échec de la configuration';
      toast.message = err instanceof Error ? err.message : String(err);
    }
  };

  return (
    <Form
      actions={
        <ActionPanel>
          <Action.SubmitForm title="Connecter l'extension" icon={Icon.Check} onSubmit={handleSubmit} />
        </ActionPanel>
      }
    >
      <Form.TextField
        id="extId"
        title="Extension ID (optionnel)"
        placeholder="ex: dglmcdpnglffnfdnhaeaifholhncglgc"
        value={extId}
        onChange={setExtId}
        info="Laissez vide pour l'ID officiel Chrome Web Store / Firefox Add-ons."
      />
    </Form>
  );
}
