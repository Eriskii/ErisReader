<script lang="ts">
  import '@fontsource-variable/google-sans-flex/wght.css'
  import '#lib/theme/base.css'
  import '#lib/theme/controls.css'
  import { applyColors } from '#lib/theme/paint.ts'

  let { children } = $props()

  applyColors()
</script>

{@render children()}
