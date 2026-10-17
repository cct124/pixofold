import { useEffect, useRef } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import type { Language } from '../../stores/preferences';
import type { WorkspaceController, WorkspaceView } from './controller';
import { workspaceText } from './messages';

export function OverwriteConfirmationDialog({
  controller,
  view,
  language,
}: {
  controller: WorkspaceController;
  view: WorkspaceView;
  language: Language;
}) {
  const t = workspaceText(language);
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!view.pending) cancel.current?.focus();
  }, [view.pending]);
  return (
    <Dialog
      title={t('overwriteWarningTitle')}
      closeLabel={t('closeDialog')}
      onClose={() => controller.cancelOverwrite()}
      alert
    >
      <div className="dialog-body">
        <p>{t('overwriteWarningBody')}</p>
        <p>{t('overwriteWarningAlternative')}</p>
        <p>{t('overwriteWarningOnce')}</p>
        {view.overwriteSaveFailed && <p role="alert">{t('overwriteSaveFailed')}</p>}
        {view.needsRecovery && <p role="alert">{t('operation')}</p>}
      </div>
      <div className="dialog-actions">
        <button
          ref={cancel}
          className="button"
          disabled={view.pending}
          onClick={() => controller.cancelOverwrite()}
        >
          {t('cancelOverwrite')}
        </button>
        <button
          className="button primary"
          disabled={view.pending || view.needsRecovery}
          onClick={() => void controller.confirmOverwrite()}
        >
          {view.pending ? t('pending') : t('confirmOverwrite')}
        </button>
      </div>
    </Dialog>
  );
}
