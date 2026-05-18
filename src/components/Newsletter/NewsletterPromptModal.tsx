// SPDX-License-Identifier: Apache-2.0

import { openUrl } from '@tauri-apps/plugin-opener';
import { ExternalLink, Mail } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { markNewsletterPromptSeen, NEWSLETTER_URL } from '@/lib/newsletter';

const ACCENT = '#6B5CFF';

interface NewsletterPromptModalProps {
  open: boolean;
  onClose: () => void;
}

export function NewsletterPromptModal({ open, onClose }: NewsletterPromptModalProps) {
  const { t } = useTranslation();

  const handleSubscribe = () => {
    AnalyticsService.capture('newsletter_prompt_subscribe_clicked');
    markNewsletterPromptSeen();
    openUrl(NEWSLETTER_URL);
    onClose();
  };

  const handleDismiss = () => {
    AnalyticsService.capture('newsletter_prompt_dismissed');
    markNewsletterPromptSeen();
    onClose();
  };

  return (
    <Dialog open={open} onOpenChange={value => !value && handleDismiss()}>
      <DialogContent className="max-w-md gap-0 p-0">
        <div className="flex flex-col items-center gap-3 px-6 pt-8 pb-4 text-center">
          <div
            className="flex h-12 w-12 items-center justify-center rounded-full"
            style={{ background: 'rgba(107, 92, 255, 0.12)', color: ACCENT }}
          >
            <Mail size={22} aria-hidden />
          </div>
          <DialogTitle className="text-lg font-semibold">
            {t('newsletter.title', 'Stay in the loop')}
          </DialogTitle>
          <DialogDescription className="text-sm">
            {t(
              'newsletter.description',
              'One short email per month with new features and database tips. Unsubscribe in one click, never any spam.'
            )}
          </DialogDescription>
        </div>

        <div className="flex flex-col gap-2 border-t bg-muted/20 px-6 py-4">
          <Button
            size="sm"
            onClick={handleSubscribe}
            className="gap-1.5 text-xs"
            style={{ background: ACCENT, color: 'white' }}
          >
            {t('newsletter.subscribe', 'Subscribe')}
            <ExternalLink size={12} aria-hidden />
          </Button>
          <Button variant="ghost" size="sm" onClick={handleDismiss} className="text-xs">
            {t('newsletter.dismiss', 'No thanks')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
