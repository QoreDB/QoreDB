import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import { SettingsSidebar } from './SettingsSidebar';
import { SettingsSearch } from './SettingsSearch';
import { SettingsBreadcrumb } from './SettingsBreadcrumb';
import {
  GeneralSection,
  EditorSection,
  SecuritySection,
  DataSection,
  KeyboardShortcutsSection,
} from './sections';
import {
  SETTINGS_SECTIONS,
  filterSectionsBySearch,
  type SettingsSectionId,
} from './settingsConfig';

import { getSafetyPolicy, setSafetyPolicy, SafetyPolicy } from '@/lib/tauri';
import { X } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface SettingsPageProps {
  onClose?: () => void;
}

export function SettingsPage({ onClose }: SettingsPageProps) {
  const { t } = useTranslation();
  const [activeSection, setActiveSection] = useState<SettingsSectionId>('general');
  const [searchQuery, setSearchQuery] = useState('');

  const [policy, setPolicy] = useState<SafetyPolicy | null>(null);

  useEffect(() => {
    let active = true;
    getSafetyPolicy()
      .then(result => {
        if (!active) return;
        if (result.success && result.policy) {
          setPolicy(result.policy);
        }
      })
      .catch(() => {});

    return () => {
      active = false;
    };
  }, []);

  async function updatePolicy(next: SafetyPolicy) {
    setPolicy(next);
    try {
      const result = await setSafetyPolicy(next);
      if (result.success && result.policy) {
        setPolicy(result.policy);
      }
    } catch {
      // Error handled in SecuritySection
    }
  }

  // Filter sections based on search
  const visibleSections = searchQuery
    ? filterSectionsBySearch(SETTINGS_SECTIONS, searchQuery)
    : SETTINGS_SECTIONS;

  useEffect(() => {
    if (searchQuery && visibleSections.length > 0) {
      const currentVisible = visibleSections.find(s => s.id === activeSection);
      if (!currentVisible) {
        setActiveSection(visibleSections[0].id);
      }
    }
  }, [searchQuery, visibleSections, activeSection]);

  const renderSection = () => {
    if (searchQuery && visibleSections.length > 0) {
      return (
        <div className="space-y-6">
          {visibleSections.map(section => (
            <div key={section.id}>
              <h2 className="text-xs font-medium uppercase tracking-wider text-muted-foreground mb-2 pb-2 border-b border-border/50">
                {t(section.labelKey)}
              </h2>
              {renderSectionContent(section.id)}
            </div>
          ))}
        </div>
      );
    }

    return renderSectionContent(activeSection);
  };

  const renderSectionContent = (sectionId: SettingsSectionId) => {
    switch (sectionId) {
      case 'general':
        return <GeneralSection searchQuery={searchQuery} />;
      case 'editor':
        return <EditorSection searchQuery={searchQuery} />;
      case 'security':
        return <SecuritySection searchQuery={searchQuery} />;
      case 'data':
        return (
          <DataSection policy={policy} onApplyPolicy={updatePolicy} searchQuery={searchQuery} />
        );
      case 'shortcuts':
        return <KeyboardShortcutsSection searchQuery={searchQuery} />;
      default:
        return null;
    }
  };

  return (
    <div className="flex h-full bg-background">
      {/* Sidebar */}
      <aside className="w-52 shrink-0 border-r border-border p-4 pt-6">
        <div className="flex items-center mb-4 px-3">
          <h1 className="text-sm font-semibold">{t('settings.title')}</h1>
        </div>
        <SettingsSidebar
          activeSection={activeSection}
          onSectionChange={section => {
            setActiveSection(section);
            setSearchQuery(''); // Clear search when switching sections
          }}
        />
      </aside>

      {/* Main content */}
      <main className="flex-1 flex flex-col min-w-0 overflow-hidden">
        {/* Header with breadcrumb, search, and close */}
        <header className="flex items-center gap-4 px-6 py-3 border-b border-border">
          <div className="flex-1">
            <SettingsBreadcrumb
              currentSection={activeSection}
              onNavigateHome={() => setActiveSection('general')}
            />
          </div>
          <SettingsSearch value={searchQuery} onChange={setSearchQuery} />
          {onClose && (
            <Button
              variant="ghost"
              size="sm"
              className="gap-1.5 text-muted-foreground hover:text-foreground"
              onClick={onClose}
            >
              <X size={14} />
              <span className="text-xs">{t('common.close')}</span>
            </Button>
          )}
        </header>

        {/* Content area */}
        <div className="flex-1 overflow-auto px-6 py-4">
          <div className="max-w-xl">
            {searchQuery && visibleSections.length === 0 ? (
              <div className="text-center py-12 text-muted-foreground">
                <p className="text-sm">{t('settings.search.noResults')}</p>
              </div>
            ) : (
              <div className="divide-y divide-border/50">{renderSection()}</div>
            )}
          </div>
        </div>
      </main>
    </div>
  );
}
