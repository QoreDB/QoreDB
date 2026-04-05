// SPDX-License-Identifier: Apache-2.0

import { relaunch } from '@tauri-apps/plugin-process';
import { check } from '@tauri-apps/plugin-updater';
import {
  ChevronDown,
  Download,
  Loader2,
  Monitor,
  Moon,
  RefreshCw,
  RotateCcw,
  Sun,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useTheme } from '@/hooks/useTheme';
import {
  setUpdateAvailable,
  setUpdateError,
  setUpdateInstalled,
  setUpdateInstalling,
  useUpdateStore,
} from '@/lib/updateStore';
import { APP_VERSION } from '@/lib/version';
import { SettingsCard } from '../SettingsCard';
import { Label } from '@/components/ui/label';

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
  const updateState = useUpdateStore();
  const [checking, setChecking] = useState(false);
  const [upToDate, setUpToDate] = useState(false);

  useEffect(() => {
    setStartupPreferences(startupPrefs);
  }, [startupPrefs]);

  const handleCheckForUpdate = useCallback(async () => {
    setChecking(true);
    setUpToDate(false);
    try {
      const update = await check();
      if (update) {
        setUpdateAvailable(update);
      } else {
        setUpToDate(true);
      }
    } catch {
      // silently fail
    } finally {
      setChecking(false);
    }
  }, []);

  const handleInstallUpdate = useCallback(async () => {
    if (!updateState.update) return;
    try {
      setUpdateInstalling();
      await updateState.update.downloadAndInstall();
      setUpdateInstalled();
    } catch (err) {
      setUpdateError(err instanceof Error ? err.message : String(err));
    }
  }, [updateState.update]);

  const isLanguageModified = !i18n.language.startsWith('en');
  const isThemeModified = theme !== 'auto';
  const isStartupModified =
    startupPrefs.restoreSession !== DEFAULT_STARTUP_PREFS.restoreSession ||
    startupPrefs.checkUpdates !== DEFAULT_STARTUP_PREFS.checkUpdates;

  return (
    <>
      <SettingsCard
        id="about"
        title={t('settings.about.title')}
        description={t('settings.about.description')}
        searchQuery={searchQuery}
      >
        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <span className="text-sm text-muted-foreground">{t('settings.about.version')}</span>
            <span className="text-sm font-mono font-semibold">{APP_VERSION}</span>
          </div>

          <div className="flex items-center gap-2">
            {updateState.status === 'available' ? (
              <Button
                variant="default"
                size="sm"
                className="gap-1.5"
                onClick={handleInstallUpdate}
                disabled={updateState.status !== 'available'}
              >
                <Download size={14} />
                {t('settings.about.installUpdate', { version: updateState.version })}
              </Button>
            ) : updateState.status === 'installing' ? (
              <Button variant="outline" size="sm" className="gap-1.5" disabled>
                <Loader2 size={14} className="animate-spin" />
                {t('settings.about.installing')}
              </Button>
            ) : updateState.status === 'installed' ? (
              <div className="flex items-center gap-2">
                <Button variant="default" size="sm" className="gap-1.5" onClick={() => relaunch()}>
                  <RotateCcw size={14} />
                  {t('settings.about.restart')}
                </Button>
                <span className="text-xs text-success">{t('settings.about.installed')}</span>
              </div>
            ) : (
              <Button
                variant="outline"
                size="sm"
                className="gap-1.5"
                onClick={handleCheckForUpdate}
                disabled={checking}
              >
                {checking ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : (
                  <RefreshCw size={14} />
                )}
                {checking ? t('settings.about.checking') : t('settings.about.checkForUpdate')}
              </Button>
            )}
            {upToDate && updateState.status === 'idle' && (
              <span className="text-xs text-success">{t('settings.about.upToDate')}</span>
            )}
            {updateState.status === 'error' && updateState.error && (
              <span className="text-xs text-error">{updateState.error}</span>
            )}
          </div>
        </div>
      </SettingsCard>

      <SettingsCard
        id="language"
        title={t('settings.language')}
        description={t('settings.languageDescription')}
        isModified={isLanguageModified}
        searchQuery={searchQuery}
      >
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" className="w-52 justify-between">
              {i18n.language.startsWith('fr')
                ? 'Français'
                : i18n.language.startsWith('es')
                  ? 'Español'
                  : i18n.language.startsWith('de')
                    ? 'Deutsch'
                    : i18n.language.startsWith('pt')
                      ? 'Português (Brasil)'
                      : i18n.language.startsWith('zh')
                        ? '简体中文'
                        : i18n.language.startsWith('ja')
                          ? '日本語'
                          : i18n.language.startsWith('ko')
                            ? '한국어'
                            : i18n.language.startsWith('ru')
                              ? 'Русский'
                              : 'English'}
              <ChevronDown className="ml-2 h-3.5 w-3.5 opacity-50" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent className="w-52">
            <DropdownMenuItem onClick={() => i18n.changeLanguage('en')}>English</DropdownMenuItem>
            <DropdownMenuItem onClick={() => i18n.changeLanguage('fr')}>Français</DropdownMenuItem>
            <DropdownMenuItem onClick={() => i18n.changeLanguage('es')}>Español</DropdownMenuItem>
            <DropdownMenuItem onClick={() => i18n.changeLanguage('de')}>Deutsch</DropdownMenuItem>
            <DropdownMenuItem onClick={() => i18n.changeLanguage('pt-BR')}>
              Português (Brasil)
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => i18n.changeLanguage('zh-CN')}>
              简体中文
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => i18n.changeLanguage('ja')}>日本語</DropdownMenuItem>
            <DropdownMenuItem onClick={() => i18n.changeLanguage('ko')}>한국어</DropdownMenuItem>
            <DropdownMenuItem onClick={() => i18n.changeLanguage('ru')}>Русский</DropdownMenuItem>
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
          <Label className="flex items-start gap-2.5 text-sm cursor-pointer">
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
          </Label>

          <Label className="flex items-start gap-2.5 text-sm cursor-pointer">
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
          </Label>
        </div>
      </SettingsCard>
    </>
  );
}
