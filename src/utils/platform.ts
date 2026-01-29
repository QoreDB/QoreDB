export const isMacOS = (): boolean => {
  if (typeof window !== 'undefined' && window.navigator) {
    return window.navigator.userAgent.toLowerCase().includes('mac');
  }
  return false;
};

export const getModifierKey = (): 'Cmd' | 'Ctrl' => {
  return isMacOS() ? 'Cmd' : 'Ctrl';
};

export const getShortcutSymbol = (): '⌘' | 'Ctrl' => {
  return isMacOS() ? '⌘' : 'Ctrl';
};

/**
 * Returns a standardized shortcut string based on the OS.
 * @param key The key to combine with the modifier (e.g., 'K', 'T', 'Enter')
 * @param options.shift Whether to include Shift
 * @param options.alt Whether to include Alt/Option
 * @param options.symbol Whether to use symbols (⌘) or text (Cmd/Ctrl)
 */
export const getShortcut = (
  key: string,
  options: { shift?: boolean; alt?: boolean; symbol?: boolean } = {}
): string => {
  const isMac = isMacOS();
  const mod = options.symbol ? (isMac ? '⌘' : 'Ctrl') : (isMac ? 'Cmd' : 'Ctrl');
  const separator = options.symbol && isMac ? '' : '+'; // Mac symbols often don't use + (e.g. ⌘K), but Windows uses Ctrl+K
  const parts = [mod];

  if (options.shift) {
    parts.push(options.symbol && isMac ? '⇧' : 'Shift');
  }
  
  if (options.alt) {
     parts.push(options.symbol && isMac ? '⌥' : 'Alt');
  }

  // If using symbols on Mac, we usually just append the key without separator involved for the last part if we want tight packing, 
  // but standard practice varies. Let's keep it simple: separated for text, tight or separated for symbols.
  // For consistency with existing app style which seems to use "Cmd+T" or "⌘K", let's conform to the requested format.
  // The existing app uses "Cmd+T" in text and "⌘K" in badges.
  
  if (options.symbol && isMac) {
      // ⌘K, ⌘⇧L
      return parts.join('') + key.toUpperCase();
  }

  // Ctrl+K, Cmd+T
  parts.push(key);
  return parts.join(separator);
};
