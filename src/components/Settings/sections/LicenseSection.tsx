import { useTranslation } from 'react-i18next';
import { SettingsCard } from '../SettingsCard';
import { LicenseActivation } from '@/components/License/LicenseActivation';

interface LicenseSectionProps {
  searchQuery?: string;
}

export function LicenseSection({ searchQuery }: LicenseSectionProps) {
  const { t } = useTranslation();

  return (
    <SettingsCard
      title={t('settings.license.title')}
      description={t('settings.license.description')}
      searchQuery={searchQuery}
    >
      <LicenseActivation />
    </SettingsCard>
  );
}
