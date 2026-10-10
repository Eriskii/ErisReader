import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'

/** One item of a Responses API request's input. */
export interface InputItem {
  type?: string
  role?: string
  content?: { type: string; text?: string; image_url?: string }[]
  call_id?: string
  name?: string
  arguments?: string
  /** A tool's result: text, or parts when it carries images. */
  output?: string | { type: string; text?: string; image_url?: string }[]
}

/** A Responses API request as the Codex endpoint receives it. */
export interface CodexCall {
  headers: Record<string, string | string[] | undefined>
  body: {
    model: string
    instructions: string
    input: InputItem[]
    tools: { type: string; name: string; description: string; parameters: unknown }[]
    reasoning?: { effort: string }
  }
}

/** A reply: text streamed in pieces after `until` settles, or tool calls. */
export type Turn = { text: string; until?: Promise<unknown> } | { calls: { name: string; arguments: Record<string, unknown> }[] }

/** The model answers in words, streaming them a few at a time. */
export const says = (text: string, until?: Promise<unknown>): Turn => ({ text, until })

/** The model calls a tool. */
export const calls = (name: string, args: Record<string, unknown>): Turn => ({ calls: [{ name, arguments: args }] })

/** What the user said in a call: the text of each user message, in order. */
export const userTexts = (call: CodexCall): string[] =>
  call.body.input.filter((item) => item.role === 'user').map((item) => item.content?.filter((c) => c.type === 'input_text').map((c) => c.text).join('') ?? '')

/** The images the user sent in a call, as data URLs. */
export const userImages = (call: CodexCall): string[] =>
  call.body.input.flatMap((item) => (item.role === 'user' ? (item.content ?? []).filter((c) => c.type === 'input_image').map((c) => c.image_url!) : []))

/** The result of the tool call a call's input ends with. */
export const lastToolOutput = (call: CodexCall): InputItem['output'] =>
  call.body.input.filter((item) => item.type === 'function_call_output').at(-1)?.output

const unsigned = (claims: unknown) => `e30.${Buffer.from(JSON.stringify(claims)).toString('base64url')}.fixture`

const event = (body: Record<string, unknown>) => `event: ${body.type}\ndata: ${JSON.stringify(body)}\n\n`

/**
 * Stands in for ChatGPT: the sign-in's token exchange at /oauth/token, and the Codex endpoint
 * at /codex (its model list, and Responses answered by `script`).
 */
export async function startCodexServer() {
  const calls: CodexCall[] = []
  let script = (call: CodexCall): Turn => says(`You said: ${userTexts(call).at(-1)}`)
  let nextCall = 1

  const server = createServer(async (request, response) => {
    const chunks: Buffer[] = []
    for await (const chunk of request) chunks.push(chunk as Buffer)
    const text = Buffer.concat(chunks).toString('utf8')
    const url = new URL(request.url ?? '/', 'http://localhost')
    const json = (status: number, body: unknown) => {
      response.writeHead(status, { 'content-type': 'application/json' })
      response.end(JSON.stringify(body))
    }
    if (url.pathname === '/oauth/token') {
      const access = unsigned({
        exp: Math.floor(Date.now() / 1000) + 3600,
        email: 'fixture@example.test',
        'https://api.openai.com/auth': { chatgpt_account_id: 'fixture-account', chatgpt_plan_type: 'plus' },
      })
      return json(200, { access_token: access, refresh_token: 'fixture-refresh', expires_in: 3600 })
    }
    if (url.pathname === '/codex/models') {
      return json(200, {
        models: [
          {
            slug: 'gpt-test',
            display_name: 'GPT Test',
            visibility: 'list',
            context_window: 200000,
            supported_reasoning_levels: [{ effort: 'low' }, { effort: 'medium' }, { effort: 'high' }],
          },
          { slug: 'gpt-small', display_name: 'GPT Small', visibility: 'list', context_window: 100000, supported_reasoning_levels: [] },
        ],
      })
    }
    if (url.pathname === '/codex/responses') {
      const call: CodexCall = { headers: request.headers, body: JSON.parse(text) }
      calls.push(call)
      const turn = script(call)
      response.writeHead(200, { 'content-type': 'text/event-stream' })
      const usage = { usage: { input_tokens: 1200 * calls.length, output_tokens: 40 } }
      if ('calls' in turn) {
        for (const tool of turn.calls) {
          response.write(
            event({
              type: 'response.output_item.done',
              item: { type: 'function_call', call_id: `call-${nextCall++}`, name: tool.name, arguments: JSON.stringify(tool.arguments) },
            }),
          )
        }
        response.end(event({ type: 'response.completed', response: usage }))
        return
      }
      const pieces = turn.text.match(/\S+\s*/g) ?? []
      const half = Math.ceil(pieces.length / 2)
      // The first half streams at once, the rest once the turn may finish.
      for (const delta of pieces.slice(0, half)) response.write(event({ type: 'response.output_text.delta', delta }))
      await turn.until
      if (response.destroyed) return
      for (const delta of pieces.slice(half)) response.write(event({ type: 'response.output_text.delta', delta }))
      response.write(
        event({
          type: 'response.output_item.done',
          item: { type: 'message', role: 'assistant', content: [{ type: 'output_text', text: turn.text }] },
        }),
      )
      response.end(event({ type: 'response.completed', response: usage }))
      return
    }
    json(404, { error: 'not found' })
  })
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const { port } = server.address() as AddressInfo
  const url = `http://127.0.0.1:${port}`

  return {
    /** What the app needs to reach it in place of ChatGPT. */
    env: { ERISREADER_OPENAI_AUTH: url, ERISREADER_CODEX_API: `${url}/codex` },
    calls,
    script(next: (call: CodexCall) => Turn) {
      script = next
    },
    close: () =>
      new Promise<void>((resolve) => {
        server.closeAllConnections()
        server.close(() => resolve())
      }),
  }
}

/** A promise and the function that settles it. */
export function gate() {
  let open!: () => void
  const opened = new Promise<void>((resolve) => (open = resolve))
  return { opened, open }
}
