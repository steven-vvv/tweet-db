<template>
  <details class="json-panel" :open="open">
    <summary>{{ title }}</summary>
    <pre>{{ serialized }}</pre>
  </details>
</template>

<script setup lang="ts">
import { computed } from 'vue'

import { toJson } from '../../shared/admin-helpers'

const props = withDefaults(
  defineProps<{
    title: string
    value: unknown
    open?: boolean
  }>(),
  {
    open: false,
  },
)

const serialized = computed(() => toJson(props.value))
</script>

<style scoped>
.json-panel {
  border: 1px solid #d9e0ea;
  border-radius: 8px;
  background: #fff;
}

summary {
  cursor: pointer;
  padding: 10px 12px;
  font-weight: 650;
}

pre {
  margin: 0;
  max-height: 520px;
  overflow: auto;
  padding: 12px;
  border-top: 1px solid #edf1f6;
  white-space: pre-wrap;
  word-break: break-word;
  font: 0.82rem/1.55 "IBM Plex Mono", ui-monospace, monospace;
}
</style>
