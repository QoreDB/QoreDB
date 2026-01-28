import { getCurrentWindow } from '@tauri-apps/api/window';
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Minus, Square, X, Copy, 
  Search, Lock, LockOpen, Settings,
  Bell
} from 'lucide-react';
import { 
  DropdownMenu, 
  DropdownMenuContent, 
  DropdownMenuItem, 
  DropdownMenuSeparator, 
  DropdownMenuShortcut,
  DropdownMenuTrigger 
} from '@/components/ui/dropdown-menu';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { cn } from '@/lib/utils';
import { TFunction } from 'i18next'

const appWindow = getCurrentWindow();

export const CustomTitlebar = () => {
  const { t } = useTranslation();
  const [isMaximized, setIsMaximized] = useState(false);
  const [isMacOS, setIsMacOS] = useState(false);
  const [readOnly, setReadOnly] = useState(false);

  useEffect(() => {
    const detectPlatform = () => {
      const userAgent = window.navigator.userAgent.toLowerCase();
      setIsMacOS(userAgent.includes('mac'));
    };
    detectPlatform();

    const checkMaximized = async () => {
      const maximized = await appWindow.isMaximized();
      setIsMaximized(maximized);
    };

    checkMaximized();

    const unlisten = appWindow.onResized(() => {
      checkMaximized();
    });

    return () => {
      unlisten.then(fn => fn());
    };
  }, []);

  const minimize = () => appWindow.minimize();
  const toggleMaximize = () => appWindow.toggleMaximize();
  const close = () => appWindow.close();

  return (
    <div className="h-10 bg-background border-b border-border flex items-center select-none shrink-0" data-tauri-drag-region>
      {/* GAUCHE : Logo & Menus */}
      <div className="flex items-center px-2 gap-1 z-20"> 
        {isMacOS ? (
           <div className="flex items-center gap-2 px-2 mr-2">
              <button onClick={close} className="w-3 h-3 rounded-full bg-[#ff5f57] hover:bg-[#ff3b30] flex items-center justify-center group"><X className="w-2 h-2 opacity-0 group-hover:opacity-100 text-[#4d0000]" strokeWidth={3} /></button>
              <button onClick={minimize} className="w-3 h-3 rounded-full bg-[#febc2e] hover:bg-[#ffb000] flex items-center justify-center group"><Minus className="w-2 h-2 opacity-0 group-hover:opacity-100 text-[#6b4600]" strokeWidth={3} /></button>
              <button onClick={toggleMaximize} className="w-3 h-3 rounded-full bg-[#28c840] hover:bg-[#1faa34] flex items-center justify-center group">
                  <div className="w-1.5 h-1.5 opacity-0 group-hover:opacity-100">
                      {isMaximized ? <Copy className="w-1.5 h-1.5 text-[#004d0a]" strokeWidth={3} /> : <Square className="w-1.5 h-1.5 text-[#004d0a]" strokeWidth={3} />}
                  </div>
              </button>
           </div>
        ) : (
           <div className="flex items-center gap-2 mr-2 ml-1">
              <img src="/logo.png" alt=""
                className='w-5 h-5'
              />
           </div>
        )}

         <div className="flex items-center">
            <MenuFile t={t} />
            <MenuView t={t} />
            <MenuData t={t} />
            <MenuTools t={t} />
         </div>
      </div>

      {/* CENTRE : OmniBar (Command Palette) */}
      <div className="flex-1 flex justify-center px-4" data-tauri-drag-region>
          <div className="w-full max-w-xl h-7 bg-muted/40 hover:bg-muted/70 transition-colors rounded-md border border-transparent hover:border-border flex items-center px-3 gap-2 text-muted-foreground cursor-text group">
             <Search className="w-3.5 h-3.5 group-hover:text-foreground transition-colors" />
             <span className="text-xs group-hover:text-foreground transition-colors truncate">{t('titlebar.search.placeholder')}</span>
             <span className="ml-auto text-[9px] font-mono border border-border px-1 rounded bg-background/50 opacity-0 group-hover:opacity-100 transition-opacity">{t('titlebar.search.shortcut')}</span>
          </div>
      </div>

      {/* DROITE : Outils & Windows Controls */}
      <div className="flex items-center px-2 gap-2 z-20">
          <div className="flex items-center gap-3 mr-1">
              {/* Read Only Toggle */}
              <div className="flex items-center gap-2" title={t('titlebar.controls.readOnly')}>
                  {readOnly ? <Lock className="w-3.5 h-3.5 text-red-500" /> : <LockOpen className="w-3.5 h-3.5 text-muted-foreground" />}
                  <Switch checked={readOnly} onCheckedChange={setReadOnly} className="scale-75 origin-right data-[state=checked]:bg-red-500" />
              </div>
              
              <div className="h-4 w-px bg-border mx-1" />

              <Button variant="ghost" size="icon" className="h-7 w-7">
                  <Bell className="w-4 h-4 text-muted-foreground" />
              </Button>

              <Button variant="ghost" size="icon" className="h-7 w-7">
                  <Settings className="w-4 h-4 text-muted-foreground" />
              </Button>
          </div>

          {!isMacOS && (
            <div className="flex items-center h-10 -mr-2 pl-2 border-l border-border/50">
               <WindowButton onClick={minimize}><Minus className="w-4 h-4" /></WindowButton>
               <WindowButton onClick={toggleMaximize}>
                  {isMaximized ? <Copy className="w-3.5 h-3.5" /> : <Square className="w-3.5 h-3.5" />}
               </WindowButton>
               <WindowButton onClick={close} isClose><X className="w-4 h-4" /></WindowButton>
            </div>
          )}
      </div>
    </div>
  );
};

/* --- MENUS --- */

const MenuFile = ({ t }: { t: TFunction }) => (
  <DropdownMenu>
    <DropdownMenuTrigger asChild>
      <Button variant="ghost" size="sm" className="h-7 px-2 text-xs font-normal hover:bg-accent/50 data-[state=open]:bg-accent">{t('titlebar.menu.file.label')}</Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" className="w-56">
      <DropdownMenuItem>
        <span>{t('titlebar.menu.file.newConnection')}</span>
        <DropdownMenuShortcut>Ctrl+N</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuItem>
        <span>{t('titlebar.menu.file.newWindow')}</span>
        <DropdownMenuShortcut>Ctrl+Shift+N</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem>
        <span>{t('titlebar.menu.file.settings')}</span>
        <DropdownMenuShortcut>Ctrl+,</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuItem className="text-red-500 focus:text-red-500 focus:bg-red-500/10">
        <span>{t('titlebar.menu.file.quit')}</span>
        <DropdownMenuShortcut>Alt+F4</DropdownMenuShortcut>
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>
);

const MenuView = ({ t }: { t: TFunction }) => (
  <DropdownMenu>
    <DropdownMenuTrigger asChild>
      <Button variant="ghost" size="sm" className="h-7 px-2 text-xs font-normal hover:bg-accent/50 data-[state=open]:bg-accent">{t('titlebar.menu.view.label')}</Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" className="w-56">
      <DropdownMenuItem>
        <span>{t('titlebar.menu.view.explorer')}</span>
        <DropdownMenuShortcut>Ctrl+B</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuItem>
        <span>{t('titlebar.menu.view.logs')}</span>
        <DropdownMenuShortcut>Ctrl+J</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem>
        <span>{t('titlebar.menu.view.zenMode')}</span>
        <DropdownMenuShortcut>Ctrl+K Z</DropdownMenuShortcut>
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>
);

const MenuData = ({ t }: { t: TFunction }) => (
  <DropdownMenu>
    <DropdownMenuTrigger asChild>
      <Button variant="ghost" size="sm" className="h-7 px-2 text-xs font-normal hover:bg-accent/50 data-[state=open]:bg-accent">{t('titlebar.menu.data.label')}</Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" className="w-56">
      <DropdownMenuItem>
        <span>{t('titlebar.menu.data.refresh')}</span>
        <DropdownMenuShortcut>Ctrl+R</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuItem>
        <span>{t('titlebar.menu.data.import')}</span>
      </DropdownMenuItem>
      <DropdownMenuItem>
         <span>{t('titlebar.menu.data.export')}</span>
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem disabled>
        <span>{t('titlebar.menu.data.commit')}</span>
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>
);

const MenuTools = ({ t }: { t: TFunction }) => (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className="h-7 px-2 text-xs font-normal hover:bg-accent/50 data-[state=open]:bg-accent">{t('titlebar.menu.tools.label')}</Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        <DropdownMenuItem>
          <span>{t('titlebar.menu.tools.history')}</span>
        </DropdownMenuItem>
        <DropdownMenuItem>
          <span>{t('titlebar.menu.tools.schemaGenerator')}</span>
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem>
           <span>{t('titlebar.menu.tools.sandbox')}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

const WindowButton = ({ onClick, children, isClose }: { onClick: () => void, children: React.ReactNode, isClose?: boolean }) => (
  <button
    onClick={onClick}
    className={cn(
      "w-12 h-10 flex items-center justify-center transition-colors hover:bg-muted/80",
       isClose && "hover:bg-red-600 hover:text-white"
    )}
  >
    {children}
  </button>
);
