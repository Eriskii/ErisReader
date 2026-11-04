/** A command key's name as this system writes it: `Ctrl Shift S` elsewhere, `⇧⌘S` on macOS. */
export function shortcut(keys: string, platform = navigator.platform): string {
  if (!platform.startsWith('Mac')) return `Ctrl ${keys}`
  return keys.startsWith('Shift ') ? `⇧⌘${keys.slice('Shift '.length)}` : `⌘${keys}`
}
