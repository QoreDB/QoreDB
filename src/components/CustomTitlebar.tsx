import { getCurrentWindow } from '@tauri-apps/api/window';
import { useState, useEffect } from 'react';
import { Minus, Square, X, Copy } from 'lucide-react';

const appWindow = getCurrentWindow();

interface CustomTitlebarProps {
  children?: React.ReactNode;
}

export const CustomTitlebar = ({ children }: CustomTitlebarProps) => {
  const [isMaximized, setIsMaximized] = useState(false);
  const [isMacOS, setIsMacOS] = useState(false);

  useEffect(() => {
    // Détecter la plateforme via navigator
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

    // Écouter les changements de l'état de la fenêtre
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

  const handleDragStart = (e: React.MouseEvent) => {
    if (e.buttons === 1) {
      if (e.detail === 2) {
        toggleMaximize();
      } else {
        appWindow.startDragging();
      }
    }
  };

  return (
    <div className="h-10 bg-background border-b border-border flex items-center justify-between select-none shrink-0">
      {/* macOS : boutons à gauche */}
      {isMacOS && (
        <div className="flex items-center gap-2 px-3 h-full">
          <button
            onClick={close}
            className="w-3 h-3 rounded-full bg-[#ff5f57] hover:bg-[#ff3b30] transition-colors flex items-center justify-center group"
            aria-label="Close"
          >
            <X
              className="w-2 h-2 opacity-0 group-hover:opacity-100 transition-opacity text-[#4d0000]"
              strokeWidth={3}
            />
          </button>
          <button
            onClick={minimize}
            className="w-3 h-3 rounded-full bg-[#febc2e] hover:bg-[#ffb000] transition-colors flex items-center justify-center group"
            aria-label="Minimize"
          >
            <Minus
              className="w-2 h-2 opacity-0 group-hover:opacity-100 transition-opacity text-[#6b4600]"
              strokeWidth={3}
            />
          </button>
          <button
            onClick={toggleMaximize}
            className="w-3 h-3 rounded-full bg-[#28c840] hover:bg-[#1faa34] transition-colors flex items-center justify-center group"
            aria-label={isMaximized ? 'Restore' : 'Maximize'}
          >
            <div className="w-1.5 h-1.5 opacity-0 group-hover:opacity-100 transition-opacity">
              {isMaximized ? (
                <Copy className="w-1.5 h-1.5 text-[#004d0a]" strokeWidth={3} />
              ) : (
                <Square className="w-1.5 h-1.5 text-[#004d0a]" strokeWidth={3} />
              )}
            </div>
          </button>
        </div>
      )}

      {/* Zone draggable - Logo et titre */}
      {!isMacOS && (
        <div className="flex items-center px-3 h-full" onMouseDown={handleDragStart}>
          <span className="text-sm font-semibold">QoreDB</span>
        </div>
      )}

      {/* Zone pour vos onglets */}
      <div className="flex-1 h-full flex items-center px-2 overflow-hidden" onMouseDown={handleDragStart}>
        {children}
      </div>

      {/* macOS : titre à droite pour équilibrer */}
      {isMacOS && (
        <div className="flex items-center px-3 h-full" onMouseDown={handleDragStart}>
          <span className="text-sm font-semibold">QoreDB</span>
        </div>
      )}

      {/* Windows/Linux : boutons à droite */}
      {!isMacOS && (
        <div className="flex h-full">
          <button
            onClick={minimize}
            className="w-12 h-full flex items-center justify-center hover:bg-muted transition-colors"
            aria-label="Minimize"
          >
            <Minus className="w-4 h-4" />
          </button>
          <button
            onClick={toggleMaximize}
            className="w-12 h-full flex items-center justify-center hover:bg-muted transition-colors"
            aria-label={isMaximized ? 'Restore' : 'Maximize'}
          >
            {isMaximized ? <Copy className="w-3.5 h-3.5" /> : <Square className="w-3.5 h-3.5" />}
          </button>
          <button
            onClick={close}
            className="w-12 h-full flex items-center justify-center hover:bg-red-600 hover:text-white transition-colors"
            aria-label="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      )}
    </div>
  );
};
