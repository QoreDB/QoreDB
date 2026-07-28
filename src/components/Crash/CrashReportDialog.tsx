// SPDX-License-Identifier: Apache-2.0

import { openPath } from '@tauri-apps/plugin-opener';
import { Copy, ExternalLink, FolderOpen, ShieldCheck } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { type CrashReport, getLogsDirectory } from '@/lib/tauri';
import { openExternal } from '@/lib/transport';
import { buildCrashIssueUrl } from './crashIssue';

interface CrashReportDialogProps {
  open: boolean;
  report: CrashReport;
  otherCount: number;
  onDismiss: () => void;
}

export function CrashReportDialog({ open, report, otherCount, onDismiss }: CrashReportDialogProps) {
  const { t } = useTranslation();

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(report.content);
      toast.success(t('crashReport.copied'));
    } catch {
      toast.error(t('crashReport.copyError'));
    }
  };

  const handleOpenFolder = async () => {
    try {
      await openPath(await getLogsDirectory());
    } catch {
      toast.error(t('crashReport.openFolderError'));
    }
  };

  const handleReport = () => {
    openExternal(buildCrashIssueUrl(report, t('crashReport.issueWhatHappened')));
  };

  return (
    <Dialog open={open} onOpenChange={value => !value && onDismiss()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t('crashReport.title')}</DialogTitle>
          <DialogDescription>
            {t('crashReport.subtitle', {
              kind: report.kind,
              date: report.recordedAt,
            })}
            {otherCount > 0 && ` ${t('crashReport.others', { count: otherCount })}`}
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-start gap-2 rounded-md border border-dashed bg-muted/20 p-3">
          <ShieldCheck size={14} className="mt-0.5 shrink-0 text-muted-foreground" aria-hidden />
          <span className="text-xs leading-relaxed text-muted-foreground">
            {t('crashReport.privacyNote')}
          </span>
        </div>

        <pre className="max-h-64 overflow-auto rounded-md bg-muted p-3 font-mono text-xs whitespace-pre-wrap">
          {report.content}
        </pre>

        <div className="flex flex-wrap items-center justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={handleOpenFolder} className="gap-1.5">
            <FolderOpen size={14} aria-hidden />
            {t('crashReport.openFolder')}
          </Button>
          <Button variant="ghost" size="sm" onClick={handleCopy} className="gap-1.5">
            <Copy size={14} aria-hidden />
            {t('crashReport.copy')}
          </Button>
          <Button size="sm" onClick={handleReport} className="gap-1.5">
            {t('crashReport.report')}
            <ExternalLink size={12} aria-hidden />
          </Button>
          <Button variant="outline" size="sm" onClick={onDismiss}>
            {t('crashReport.dismiss')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
