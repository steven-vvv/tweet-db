<template>
  <section class="panel">
    <header v-if="title" class="panel-head">
      <h3>{{ title }}</h3>
    </header>
    <pre>{{ serialized }}</pre>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue'

import { toJson } from '../../admin-helpers'

const props = defineProps<{
  title?: string
  value: unknown
}>()

const serialized = computed(() => toJson(props.value))
</script>

<style scoped>
.panel {
  display: grid;
  gap: 12px;
  padding: 18px;
  border: 1px solid #d3ddeb;
  border-radius: 18px;
  background: #fbfcfe;
}

.panel-head h3 {
  margin: 0;
  font-size: 0.95rem;
}

pre {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font: 0.82rem/1.6 "IBM Plex Mono", monospace;
  color: #17304f;
}
</style>
