<script lang="ts">
  import type { Snippet } from 'svelte'
  import X from '@lucide/svelte/icons/x'

  /** ErisDE's settings modal: rises in, sinks away, and closes on Escape or its close button. */
  let { children, onclose }: { children: Snippet; onclose: () => void } = $props()

  const titleId = $props.id()
  let modal: HTMLDialogElement
  let closing = false
  let animation: Animation | undefined
  const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches

  async function close(): Promise<void> {
    if (closing) return
    closing = true
    const { opacity, transform } = getComputedStyle(modal)
    animation?.cancel()
    animation = modal.animate([{ opacity, transform }, { opacity: 0, transform: 'translateY(12px) scale(.985)' }], {
      duration: reducedMotion() ? 0 : 180,
      easing: 'cubic-bezier(.4,0,1,1)',
      fill: 'forwards',
    })
    try {
      await animation.finished
    } catch {
      return
    }
    modal.close()
    onclose()
  }

  function open(dialog: HTMLDialogElement) {
    dialog.showModal()
    animation = dialog.animate(
      [
        { opacity: 0, transform: 'translateY(18px) scale(.975)' },
        { opacity: 1, transform: 'translateY(0) scale(1)' },
      ],
      { duration: reducedMotion() ? 0 : 300, easing: 'cubic-bezier(.16,1,.3,1)' },
    )
    dialog.querySelector<HTMLButtonElement>('.settings-close')?.focus()
    return {
      destroy: () => {
        animation?.cancel()
        dialog.close()
      },
    }
  }
</script>

<dialog
  class="settings-modal"
  aria-labelledby={titleId}
  bind:this={modal}
  use:open
  oncancel={(event) => {
    event.preventDefault()
    void close()
  }}
>
  <header class="settings-modal-header">
    <h1 id={titleId}>Settings</h1>
    <button class="settings-close ui-icon-button" aria-label="Close settings" title="Close settings · Esc" onclick={() => void close()}
      ><X size={18} strokeWidth={1.5} aria-hidden="true" /></button
    >
  </header>
  <div class="settings-list">{@render children()}</div>
</dialog>

<style>
  .settings-modal {
    width: min(640px, calc(100vw - 48px));
    max-width: none;
    max-height: calc(100vh - 48px);
    margin: auto;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 22px;
    background: var(--color-panel-1);
    color: var(--color-text);
    box-shadow: 0 8px 28px #0004;
  }

  .settings-modal[open] {
    display: flex;
    flex-direction: column;
  }

  .settings-modal::backdrop {
    background: transparent;
  }

  .settings-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 20px;
    border-bottom: 1px solid var(--border);
  }

  h1 {
    margin: 0;
    font: 550 14px var(--font-ui);
  }

  .settings-close {
    width: 30px;
    height: 30px;
  }

  .settings-close:focus-visible {
    outline: 1px solid var(--color-accent);
    outline-offset: 2px;
  }

  .settings-list {
    min-height: 0;
    flex: 1 1 auto;
    overflow: auto;
    padding: 20px 26px 8px;
  }
</style>
