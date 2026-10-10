/** When something happened, as the chat list says it: just now, minutes or hours ago, then the day. */
export function ago(at: number, now = Date.now()): string {
  const minutes = Math.floor((now - at) / 60_000)
  if (minutes < 1) return 'now'
  if (minutes < 60) return `${minutes} min`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} h`
  const date = new Date(at)
  const today = new Date(now)
  const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1)
  if (date.getFullYear() === yesterday.getFullYear() && date.getMonth() === yesterday.getMonth() && date.getDate() === yesterday.getDate()) {
    return 'yesterday'
  }
  return date.toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'short',
    ...(date.getFullYear() === today.getFullYear() ? {} : { year: 'numeric' }),
  })
}

/** A token count, short: 950, 12.3k, 1.2m. */
export function tokens(count: number): string {
  if (count < 1_000) return String(count)
  if (count < 1_000_000) return `${Number((count / 1_000).toFixed(count < 10_000 ? 1 : 0))}k`
  return `${Number((count / 1_000_000).toFixed(1))}m`
}
