import { useTranslation } from 'react-i18next';
import { useTheme } from '../../hooks/useTheme';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Settings, Moon, Sun, ChevronDown } from 'lucide-react';

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();

  return (
    <div className="flex flex-col h-full bg-background p-8 overflow-auto">
      <div className="max-w-2xl mx-auto w-full space-y-8">
        
        <div className="flex items-center gap-3 mb-8">
          <div className="p-3 rounded-lg bg-primary/10 text-primary">
            <Settings size={32} />
          </div>
          <div>
            <h1 className="text-3xl font-bold tracking-tight">{t('settings.title')}</h1>
          </div>
        </div>

        <div className="grid gap-6">
          <div className="rounded-lg border border-border bg-card text-card-foreground shadow-sm">
            <div className="flex flex-col space-y-1.5 p-6">
              <h3 className="font-semibold leading-none tracking-tight">{t('settings.language')}</h3>
              <p className="text-sm text-muted-foreground">
                {t('settings.languageDescription')}
              </p>
            </div>
            <div className="p-6 pt-0">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" className="w-[200px] justify-between">
                    {i18n.language.startsWith('fr') ? 'Français' : 'English'}
                    <ChevronDown className="ml-2 h-4 w-4 opacity-50" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent className="w-[200px]">
                  <DropdownMenuItem onClick={() => i18n.changeLanguage('en')}>
                    English
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => i18n.changeLanguage('fr')}>
                    Français
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>

          <div className="rounded-lg border border-border bg-card text-card-foreground shadow-sm">
            <div className="flex flex-col space-y-1.5 p-6">
              <h3 className="font-semibold leading-none tracking-tight">{t('settings.theme')}</h3>
              <p className="text-sm text-muted-foreground">
                {t('settings.themeDescription')}
              </p>
            </div>
            <div className="p-6 pt-0">
               <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" className="w-[200px] justify-between">
                    <div className="flex items-center gap-2">
                       {theme === 'dark' ? <Moon size={16} /> : <Sun size={16} />}
                       {theme === 'dark' ? t('settings.themeDark') : t('settings.themeLight')}
                    </div>
                    <ChevronDown className="ml-2 h-4 w-4 opacity-50" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent className="w-[200px]">
                  <DropdownMenuItem onClick={() => setTheme('light')}>
                    <div className="flex items-center gap-2">
                      <Sun size={16} />
                      {t('settings.themeLight')}
                    </div>
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setTheme('dark')}>
                    <div className="flex items-center gap-2">
                       <Moon size={16} />
                       {t('settings.themeDark')}
                    </div>
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
        </div>

      </div>
    </div>
  );
}
