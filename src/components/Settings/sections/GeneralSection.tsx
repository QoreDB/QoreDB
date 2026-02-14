import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Moon, Sun, Monitor, ChevronDown } from 'lucide-react';

import { useTheme } from '@/hooks/useTheme';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { SettingsCard } from '../SettingsCard';

interface GeneralSectionProps {
  searchQuery?: string;
}

const STARTUP_PREFS_KEY = 'qoredb_startup_preferences';

interface StartupPreferences {
  restoreSession: boolean;
  checkUpdates: boolean;
}

const DEFAULT_STARTUP_PREFS: StartupPreferences = {
  restoreSession: true,
  checkUpdates: true,
};

function getStartupPreferences(): StartupPreferences {
  try {
    const stored = localStorage.getItem(STARTUP_PREFS_KEY);
    if (stored) {
      return { ...DEFAULT_STARTUP_PREFS, ...JSON.parse(stored) };
    }
  } catch {
    // ignore
  }
  return DEFAULT_STARTUP_PREFS;
}

function setStartupPreferences(prefs: StartupPreferences): void {
  localStorage.setItem(STARTUP_PREFS_KEY, JSON.stringify(prefs));
}

export function GeneralSection({ searchQuery }: GeneralSectionProps) {
  const { t, i18n } = useTranslation();
  const { theme, resolvedTheme, setTheme } = useTheme();
  const [startupPrefs, setStartupPrefs] = useState<StartupPreferences>(getStartupPreferences);

  useEffect(() => {
    setStartupPreferences(startupPrefs);
  }, [startupPrefs]);

  const isLanguageModified = !i18n.language.startsWith('en');
  const isThemeModified = theme !== 'auto';
  const isStartupModified =
    startupPrefs.restoreSession !== DEFAULT_STARTUP_PREFS.restoreSession ||
    startupPrefs.checkUpdates !== DEFAULT_STARTUP_PREFS.checkUpdates;

  return (
    <>
      <SettingsCard
        id="language"
        title={t('settings.language')}
        description={t('settings.languageDescription')}
        isModified={isLanguageModified}
        searchQuery={searchQuery}
      >
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" className="w-44 justify-between">
              {i18n.language.startsWith('fr') ? 'Français' : 'English'}
              <ChevronDown className="ml-2 h-3.5 w-3.5 opacity-50" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent className="w-44">
            <DropdownMenuItem onClick={() => i18n.changeLanguage('en')}>English</DropdownMenuItem>
            <DropdownMenuItem onClick={() => i18n.changeLanguage('fr')}>Français</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SettingsCard>

      <SettingsCard
        id="theme"
        title={t('settings.theme')}
        description={t('settings.themeDescription')}
        isModified={isThemeModified}
        searchQuery={searchQuery}
      >
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" className="w-50 justify-between">
              <div className="flex items-center gap-2">
                {theme === 'auto' ? (
                  <Monitor size={14} />
                ) : theme === 'dark' ? (
                  <Moon size={14} />
                ) : (
                  <Sun size={14} />
                )}
                <span className="text-sm">
                  {theme === 'auto'
                    ? `${t('settings.themeSystem')} (${resolvedTheme === 'dark' ? t('settings.themeDark') : t('settings.themeLight')})`
                    : theme === 'dark'
                      ? t('settings.themeDark')
                      : t('settings.themeLight')}
                </span>
              </div>
              <ChevronDown className="ml-2 h-3.5 w-3.5 opacity-50" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent className="w-50">
            <DropdownMenuItem onClick={() => setTheme('auto')}>
              <Monitor size={14} className="mr-2" />
              {t('settings.themeSystem')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setTheme('light')}>
              <Sun size={14} className="mr-2" />
              {t('settings.themeLight')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setTheme('dark')}>
              <Moon size={14} className="mr-2" />
              {t('settings.themeDark')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SettingsCard>

      <SettingsCard
        id="startup"
        title={t('settings.startup.title')}
        description={t('settings.startup.description')}
        isModified={isStartupModified}
        searchQuery={searchQuery}
      >
        <div className="space-y-3">
          <label className="flex items-start gap-2.5 text-sm cursor-pointer">
            <Checkbox
              checked={startupPrefs.restoreSession}
              onCheckedChange={checked =>
                setStartupPrefs(prev => ({ ...prev, restoreSession: !!checked }))
              }
              className="mt-0.5"
            />
            <span>
              <span className="font-medium text-foreground">
                {t('settings.startup.restoreSession')}
              </span>
              <span className="block text-xs text-muted-foreground mt-0.5">
                {t('settings.startup.restoreSessionDescription')}
              </span>
            </span>
          </label>

          <label className="flex items-start gap-2.5 text-sm cursor-pointer">
            <Checkbox
              checked={startupPrefs.checkUpdates}
              onCheckedChange={checked =>
                setStartupPrefs(prev => ({ ...prev, checkUpdates: !!checked }))
              }
              className="mt-0.5"
            />
            <span>
              <span className="font-medium text-foreground">
                {t('settings.startup.checkUpdates')}
              </span>
              <span className="block text-xs text-muted-foreground mt-0.5">
                {t('settings.startup.checkUpdatesDescription')}
              </span>
            </span>
          </label>
        </div>
      </SettingsCard>
    </>
  );
}
