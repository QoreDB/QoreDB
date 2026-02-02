import { getCurrentWindow } from '@tauri-apps/api/window';
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Minus, Square, X, Copy, Search, Bell, Settings } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { TFunction } from 'i18next';
import { isMacOS, getShortcut } from '@/utils/platform';
import { NotificationPanel } from '@/components/Notification/NotificationPanel';
import { useNotificationBadge } from '@/lib/notificationStore';

const appWindow = getCurrentWindow();

interface CustomTitlebarProps {
  onOpenSearch?: () => void;
  onNewConnection?: () => void;
  onNewWindow?: () => void;
  onOpenSettings?: () => void;
  onOpenLogs?: () => void;
  onOpenHistory?: () => void;
  onToggleSidebar?: () => void;
  onRefreshData?: () => void;
  onImportData?: () => void;
  onExportData?: () => void;
  onToggleSandbox?: () => void;
  onOpenSchemaGenerator?: () => void;

  onToggleReadOnly?: (next: boolean) => void;
  readOnly?: boolean;
  settingsOpen?: boolean;
}

export const CustomTitlebar = ({
  onOpenSearch,
  onNewConnection,
  onNewWindow,
  onOpenSettings,
  onOpenLogs,
  onOpenHistory,
  onToggleSidebar,
  onRefreshData,
  onImportData,
  onExportData,
  onToggleSandbox,
  onOpenSchemaGenerator,

  // onToggleReadOnly,
  // readOnly = false,
  settingsOpen = false,
}: CustomTitlebarProps) => {
  const { t } = useTranslation();
  const [isMaximized, setIsMaximized] = useState(false);
  const isMac = isMacOS();

  useEffect(() => {
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

  const [activeMenu, setActiveMenu] = useState<string | null>(null);

  const handleMenuOpenChange = (menu: string, open: boolean) => {
    if (open) {
      setActiveMenu(menu);
    } else if (activeMenu === menu) {
      setActiveMenu(null);
    }
  };

  const handleMenuHover = (menu: string) => {
    if (activeMenu) {
      setActiveMenu(menu);
    }
  };

  const minimize = () => appWindow.minimize();
  const toggleMaximize = () => appWindow.toggleMaximize();
  const close = () => appWindow.close();

  return (
    <div
      className={cn(
        'bg-muted/80 border-b border-border shadow-sm flex items-center select-none shrink-0',
        isMac ? 'h-9' : 'h-10'
      )}
      data-tauri-drag-region
    >
      <div className="flex items-center pl-2 z-20">
        {isMac ? (
          <div className="w-20" />
        ) : (
          <div className="flex items-center ml-1">
            <img src="/logo.png" alt="" className="w-5 h-5" />
          </div>
        )}

        <div className="h-4 w-px bg-border/50 mx-2" />

        <div className="flex items-center gap-0.5">
          <MenuFile
            t={t}
            isOpen={activeMenu === 'file'}
            onOpenChange={open => handleMenuOpenChange('file', open)}
            onMouseEnter={() => handleMenuHover('file')}
            onNewConnection={onNewConnection}
            onNewWindow={onNewWindow}
            onOpenSettings={onOpenSettings}
            onQuit={close}
          />
          <MenuView
            t={t}
            isOpen={activeMenu === 'view'}
            onOpenChange={open => handleMenuOpenChange('view', open)}
            onMouseEnter={() => handleMenuHover('view')}
            onToggleSidebar={onToggleSidebar}
            onOpenLogs={onOpenLogs}
          />
          <MenuData
            t={t}
            isOpen={activeMenu === 'data'}
            onOpenChange={open => handleMenuOpenChange('data', open)}
            onMouseEnter={() => handleMenuHover('data')}
            onRefreshData={onRefreshData}
            onImportData={onImportData}
            onExportData={onExportData}
          />
          <MenuTools
            t={t}
            isOpen={activeMenu === 'tools'}
            onOpenChange={open => handleMenuOpenChange('tools', open)}
            onMouseEnter={() => handleMenuHover('tools')}
            onOpenHistory={onOpenHistory}
            onOpenSchemaGenerator={onOpenSchemaGenerator}
            onToggleSandbox={onToggleSandbox}
          />
        </div>
      </div>

      <div className="flex-1 flex justify-center px-4" data-tauri-drag-region>
        <div
          className={cn(
            'w-full max-w-xl h-7 bg-background/80 hover:bg-background transition-colors rounded-md border border-border/60 hover:border-border flex items-center px-3 gap-2 text-muted-foreground group shadow-sm cursor-pointer'
          )}
          role={onOpenSearch ? 'button' : undefined}
          tabIndex={onOpenSearch ? 0 : -1}
          aria-label={t('titlebar.search.placeholder')}
          onClick={() => onOpenSearch?.()}
          onKeyDown={event => {
            if (!onOpenSearch) return;
            if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault();
              onOpenSearch();
            }
          }}
        >
          <Search className="w-3.5 h-3.5 group-hover:text-foreground transition-colors" />
          <span className="text-xs group-hover:text-foreground transition-colors truncate">
            {t('titlebar.search.placeholder')}
          </span>
          <span className="ml-auto text-[9px] font-mono border border-border px-1.5 py-0.5 rounded bg-muted/50">
            {getShortcut('K', { symbol: true })}
          </span>
        </div>
      </div>

      <div className="flex items-center pr-2 z-20">
        {/* <div className="flex items-center gap-2 px-2" title={t('titlebar.controls.readOnly')}>
          {readOnly ? (
            <Lock className="w-3.5 h-3.5 text-red-500" />
          ) : (
            <LockOpen className="w-3.5 h-3.5 text-muted-foreground" />
          )}
          <Switch
            checked={readOnly}
            onCheckedChange={next => onToggleReadOnly?.(next)}
            disabled={!onToggleReadOnly}
            className="scale-75 origin-right data-[state=checked]:bg-red-500"
          />
        </div> */}

        <div className="h-4 w-px bg-border/50 mx-1" />

        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 transition-transform duration-200 active:scale-90"
            onClick={() => onOpenSettings?.()}
            disabled={!onOpenSettings}
            aria-label={t('settings.title')}
          >
            <div className="relative w-4 h-4">
              <Settings
                className={`w-4 h-4 absolute inset-0 transition-all duration-300 ${
                  settingsOpen
                    ? 'opacity-0 rotate-90 scale-75'
                    : 'opacity-100 rotate-0 scale-100 text-muted-foreground'
                }`}
              />
              <X
                className={`w-4 h-4 absolute inset-0 transition-all duration-300 ${
                  settingsOpen
                    ? 'opacity-100 rotate-0 scale-100 text-foreground'
                    : 'opacity-0 -rotate-90 scale-75'
                }`}
              />
            </div>
          </Button>

          <NotificationBell />
        </div>

        {!isMac && (
          <div className="flex items-center h-10 -mr-2 ml-2 pl-2 border-l border-border/50">
            <WindowButton onClick={minimize}>
              <Minus className="w-4 h-4" />
            </WindowButton>
            <WindowButton onClick={toggleMaximize}>
              {isMaximized ? <Copy className="w-3.5 h-3.5" /> : <Square className="w-3.5 h-3.5" />}
            </WindowButton>
            <WindowButton onClick={close} isClose>
              <X className="w-4 h-4" />
            </WindowButton>
          </div>
        )}
      </div>
    </div>
  );
};

interface TitlebarMenuProps {
  t: TFunction;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onMouseEnter: () => void;
}

interface MenuFileProps extends TitlebarMenuProps {
  onNewConnection?: () => void;
  onNewWindow?: () => void;
  onOpenSettings?: () => void;
  onQuit?: () => void;
}

const MenuFile = ({
  t,
  isOpen,
  onOpenChange,
  onMouseEnter,
  onNewConnection,
  onNewWindow,
  onOpenSettings,
  onQuit,
}: MenuFileProps) => (
  <DropdownMenu open={isOpen} onOpenChange={onOpenChange} modal={false}>
    <DropdownMenuTrigger asChild onMouseEnter={onMouseEnter}>
      <Button
        variant="ghost"
        size="sm"
        className="h-7 px-2 text-xs font-normal text-muted-foreground hover:text-foreground hover:bg-accent/50 data-[state=open]:bg-accent data-[state=open]:text-foreground"
      >
        {t('titlebar.menu.file.label')}
      </Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" className="w-56">
      <DropdownMenuItem onClick={onNewConnection} disabled={!onNewConnection}>
        <span>{t('titlebar.menu.file.newConnection')}</span>
        <DropdownMenuShortcut>{getShortcut('N')}</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuItem onClick={onNewWindow} disabled={!onNewWindow}>
        <span>{t('titlebar.menu.file.newWindow')}</span>
        <DropdownMenuShortcut>{getShortcut('N', { shift: true })}</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem onClick={onOpenSettings} disabled={!onOpenSettings}>
        <span>{t('titlebar.menu.file.settings')}</span>
        <DropdownMenuShortcut>{getShortcut(',')}</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuItem
        onClick={onQuit}
        disabled={!onQuit}
        className="text-red-500 focus:text-red-500 focus:bg-red-500/10"
      >
        <span>{t('titlebar.menu.file.quit')}</span>
        <DropdownMenuShortcut>Alt+F4</DropdownMenuShortcut>
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>
);

interface MenuViewProps extends TitlebarMenuProps {
  onToggleSidebar?: () => void;
  onOpenLogs?: () => void;
  onToggleZenMode?: () => void;
}

const MenuView = ({
  t,
  isOpen,
  onOpenChange,
  onMouseEnter,
  onToggleSidebar,
  onOpenLogs,
  onToggleZenMode,
}: MenuViewProps) => (
  <DropdownMenu open={isOpen} onOpenChange={onOpenChange} modal={false}>
    <DropdownMenuTrigger asChild onMouseEnter={onMouseEnter}>
      <Button
        variant="ghost"
        size="sm"
        className="h-7 px-2 text-xs font-normal text-muted-foreground hover:text-foreground hover:bg-accent/50 data-[state=open]:bg-accent data-[state=open]:text-foreground"
      >
        {t('titlebar.menu.view.label')}
      </Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" className="w-56">
      <DropdownMenuItem onClick={onToggleSidebar} disabled={!onToggleSidebar}>
        <span>{t('titlebar.menu.view.explorer')}</span>
        <DropdownMenuShortcut>{getShortcut('B')}</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuItem onClick={onOpenLogs} disabled={!onOpenLogs}>
        <span>{t('titlebar.menu.view.logs')}</span>
        <DropdownMenuShortcut>{getShortcut('J')}</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem onClick={onToggleZenMode} disabled={!onToggleZenMode}>
        <span>{t('titlebar.menu.view.zenMode')}</span>
        <DropdownMenuShortcut>{getShortcut('K')} Z</DropdownMenuShortcut>
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>
);

interface MenuDataProps extends TitlebarMenuProps {
  onRefreshData?: () => void;
  onImportData?: () => void;
  onExportData?: () => void;
}

const MenuData = ({
  t,
  isOpen,
  onOpenChange,
  onMouseEnter,
  onRefreshData,
  onImportData,
  onExportData,
}: MenuDataProps) => (
  <DropdownMenu open={isOpen} onOpenChange={onOpenChange} modal={false}>
    <DropdownMenuTrigger asChild onMouseEnter={onMouseEnter}>
      <Button
        variant="ghost"
        size="sm"
        className="h-7 px-2 text-xs font-normal text-muted-foreground hover:text-foreground hover:bg-accent/50 data-[state=open]:bg-accent data-[state=open]:text-foreground"
      >
        {t('titlebar.menu.data.label')}
      </Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" className="w-56">
      <DropdownMenuItem onClick={onRefreshData} disabled={!onRefreshData}>
        <span>{t('titlebar.menu.data.refresh')}</span>
        <DropdownMenuShortcut>{getShortcut('R')}</DropdownMenuShortcut>
      </DropdownMenuItem>
      <DropdownMenuItem onClick={onImportData} disabled={!onImportData}>
        <span>{t('titlebar.menu.data.import')}</span>
      </DropdownMenuItem>
      <DropdownMenuItem onClick={onExportData} disabled={!onExportData}>
        <span>{t('titlebar.menu.data.export')}</span>
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem disabled>
        <span>{t('titlebar.menu.data.commit')}</span>
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>
);

interface MenuToolsProps extends TitlebarMenuProps {
  onOpenHistory?: () => void;
  onOpenSchemaGenerator?: () => void;
  onToggleSandbox?: () => void;
}

const MenuTools = ({
  t,
  isOpen,
  onOpenChange,
  onMouseEnter,
  onOpenHistory,
  onOpenSchemaGenerator,
  onToggleSandbox,
}: MenuToolsProps) => (
  <DropdownMenu open={isOpen} onOpenChange={onOpenChange} modal={false}>
    <DropdownMenuTrigger asChild onMouseEnter={onMouseEnter}>
      <Button
        variant="ghost"
        size="sm"
        className="h-7 px-2 text-xs font-normal text-muted-foreground hover:text-foreground hover:bg-accent/50 data-[state=open]:bg-accent data-[state=open]:text-foreground"
      >
        {t('titlebar.menu.tools.label')}
      </Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" className="w-56">
      <DropdownMenuItem onClick={onOpenHistory} disabled={!onOpenHistory}>
        <span>{t('titlebar.menu.tools.history')}</span>
      </DropdownMenuItem>
      <DropdownMenuItem onClick={onOpenSchemaGenerator} disabled={!onOpenSchemaGenerator}>
        <span>{t('titlebar.menu.tools.schemaGenerator')}</span>
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem onClick={onToggleSandbox} disabled={!onToggleSandbox}>
        <span>{t('titlebar.menu.tools.sandbox')}</span>
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>
);

const WindowButton = ({
  onClick,
  children,
  isClose,
}: {
  onClick: () => void;
  children: React.ReactNode;
  isClose?: boolean;
}) => (
  <button
    onClick={onClick}
    className={cn(
      'w-12 h-10 flex items-center justify-center transition-colors hover:bg-muted/80',
      isClose && 'hover:bg-red-600 hover:text-white'
    )}
  >
    {children}
  </button>
);

/**
 * NotificationBell - Bell icon with popover panel and badge
 */
const NotificationBell = () => {
  const { t } = useTranslation();
  const badgeCount = useNotificationBadge();

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 relative"
          aria-label={t('notifications.title')}
        >
          <Bell className="w-4 h-4 text-muted-foreground" />
          {badgeCount > 0 && (
            <span className="absolute -top-0.5 -right-0.5 min-w-[14px] h-[14px] px-1 text-[9px] font-medium bg-red-500 text-white rounded-full flex items-center justify-center">
              {badgeCount > 9 ? '9+' : badgeCount}
            </span>
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-auto p-3">
        <NotificationPanel />
      </PopoverContent>
    </Popover>
  );
};

