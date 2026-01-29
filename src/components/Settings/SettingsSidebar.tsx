import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';
import { SETTINGS_SECTIONS, type SettingsSectionId } from './settingsConfig';

interface SettingsSidebarProps {
  activeSection: SettingsSectionId;
  onSectionChange: (section: SettingsSectionId) => void;
  modifiedSections?: SettingsSectionId[];
}

export function SettingsSidebar({
  activeSection,
  onSectionChange,
  modifiedSections = [],
}: SettingsSidebarProps) {
  const { t } = useTranslation();

  return (
    <nav className="flex flex-col gap-0.5">
      {SETTINGS_SECTIONS.map(section => {
        const Icon = section.icon;
        const isActive = activeSection === section.id;
        const isModified = modifiedSections.includes(section.id);

        return (
          <button
            key={section.id}
            onClick={() => onSectionChange(section.id)}
            className={cn(
              'flex items-center gap-2.5 px-3 py-2 rounded-md text-sm transition-colors',
              'hover:bg-accent/50',
              isActive && 'bg-primary text-primary-foreground hover:bg-primary/90',
              !isActive && 'text-muted-foreground hover:text-foreground'
            )}
          >
            <Icon size={16} className="shrink-0" />
            <span className="flex-1 text-left">{t(section.labelKey)}</span>
            {isModified && !isActive && (
              <span className="w-1.5 h-1.5 rounded-full bg-primary shrink-0" />
            )}
          </button>
        );
      })}
    </nav>
  );
}
