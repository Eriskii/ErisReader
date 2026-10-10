<script lang="ts">
  import type { Account, SignInMethod } from '../../shared/reader'
  import type { ReaderStore } from '../annotations/reader-store.svelte'
  import SettingsSection from './SettingsSection.svelte'

  /** The accounts annotations' agents run on, signed in as ErisAE's are: ChatGPT as the Codex CLI signs in, Claude from Claude Code's login. */
  let { store }: { store: ReaderStore } = $props()

  const METHODS: Record<SignInMethod, { label: string; primary?: boolean }> = {
    browser: { label: 'Sign in with ChatGPT', primary: true },
    'device-code': { label: 'Use a device code' },
    'credentials-file': { label: 'Add Claude login file…' },
    keychain: { label: 'Use Claude Code’s login', primary: true },
  }

  const CLAUDE_NOTE =
    'ErisReader refreshes the login itself, after which the Claude Code it came from has to sign in again, and the other way around, so a login kept for ErisReader is best.'

  function status(account: Account): string {
    if (account.status === 'login-pending') {
      return account.login?.userCode ? 'Enter the code below on the sign-in page.' : 'Finish signing in in your browser.'
    }
    if (account.status === 'error') return account.detail ?? 'This login no longer works.'
    if (account.status === 'signed-out') return 'Not signed in.'
    const who = account.email ?? (account.providerId === 'openai' ? 'your ChatGPT account' : 'your Claude account')
    return `Signed in as ${who}${account.plan ? ` (${account.plan})` : ''}.`
  }

  function signIn(method: SignInMethod): void {
    if (method === 'browser' || method === 'device-code') void store.login('openai', method)
    else void store.addClaudeLogin(method)
  }

  /** Methods in the order their buttons read, the main one last. */
  const methods = (account: Account) =>
    [...account.signInMethods].sort((a, b) => Number(METHODS[a].primary ?? false) - Number(METHODS[b].primary ?? false))
</script>

<SettingsSection title="Accounts" description="Annotations use the models of every account signed in here. Sign-ins stay in ErisReader’s data folder, readable only by you.">
  {#each store.accounts ?? [] as account (account.providerId)}
    <div class="account" role="group" aria-label={account.providerLabel}>
      <div class="heading">
        <strong>{account.providerLabel}</strong>
        <p class="status" class:error={account.status === 'error'} role="status">{status(account)}</p>
      </div>
      {#if account.status === 'login-pending' && account.login}
        <p class="sign-in">
          <a href={account.login.url} target="_blank" rel="noreferrer">Open the sign-in page</a>
          {#if account.login.userCode}<code aria-label="Device code">{account.login.userCode}</code>{/if}
        </p>
        <div class="actions"><button type="button" class="ui-button quiet" onclick={() => void store.cancelLogin()}>Cancel</button></div>
      {:else if account.status === 'connected'}
        <div class="actions">
          <button type="button" class="ui-button quiet" onclick={() => void store.logout(account.providerId)}>Sign out</button>
        </div>
      {:else}
        {#if account.providerId === 'anthropic'}<p class="note">{CLAUDE_NOTE}</p>{/if}
        <div class="actions">
          {#if account.status === 'error'}
            <button type="button" class="ui-button quiet" onclick={() => void store.logout(account.providerId)}>Remove</button>
          {/if}
          {#each methods(account) as method (method)}
            <button type="button" class="ui-button" class:primary={METHODS[method].primary} onclick={() => signIn(method)}
              >{METHODS[method].label}</button
            >
          {/each}
        </div>
      {/if}
      {#if store.accountProblems[account.providerId]}<p class="problem" role="alert">{store.accountProblems[account.providerId]}</p>{/if}
    </div>
  {:else}
    <p class="status">Checking…</p>
  {/each}
</SettingsSection>

<style>
  .account {
    display: grid;
    gap: 8px;
    padding: 12px 0;
    border-bottom: 1px solid var(--color-panel-3);
  }

  .heading {
    display: grid;
    gap: 3px;
  }

  strong {
    color: var(--color-subtext);
    font: 450 12px/1.35 var(--font-ui);
  }

  .status,
  .sign-in {
    margin: 0;
    color: var(--color-subtext);
    font: 11px/1.5 var(--font-ui);
  }

  .status.error {
    color: var(--color-red);
  }

  .sign-in {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .sign-in a {
    color: var(--color-accent);
  }

  code {
    padding: 2px 8px;
    border-radius: var(--ui-radius);
    background: var(--color-panel-4);
    color: var(--color-text);
    font: 13px var(--font-mono);
    letter-spacing: 0.08em;
  }

  .note {
    margin: 0;
    color: var(--color-subsubtext);
    font: 10px/1.5 var(--font-mono);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }

  .problem {
    margin: 0;
    color: var(--color-red);
    font: 10px/1.5 var(--font-mono);
  }
</style>
