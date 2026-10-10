import type { ReaderApi } from './shared/reader'

declare global {
  interface Window {
    /** The main process's services, from the preload (see src/preload). */
    erisreader: ReaderApi
  }
}

export {}
