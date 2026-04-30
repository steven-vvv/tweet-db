<template>
  <p class="tweet-text" :class="{ clamp: maxLines > 0 }" :style="styleVars">
    <template v-for="(part, index) in parts" :key="index">
      <a
        v-if="part.href"
        class="entity"
        :href="part.href"
        target="_blank"
        rel="noreferrer"
      >{{ part.text }}</a>
      <RouterLink v-else-if="part.to" class="entity" :to="part.to">{{ part.text }}</RouterLink>
      <span v-else>{{ part.text }}</span>
    </template>
  </p>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { RouterLink } from 'vue-router'

import { asArray, asRecord, optionalText, type TextEntity } from '../browse-helpers'
import type { JsonRecord } from '../../../shared/api'

type TextPart = {
  text: string
  href?: string
  to?: string
}

const props = withDefaults(
  defineProps<{
    text: JsonRecord
    maxLines?: number
  }>(),
  {
    maxLines: 0,
  },
)

const body = computed(() => optionalText(props.text.text ?? props.text.body))
const styleVars = computed(() => ({
  '--max-lines': String(props.maxLines),
}))

const parts = computed<TextPart[]>(() => {
  const source = Array.from(body.value)
  const entities = collectEntities(props.text)
  const output: TextPart[] = []
  let cursor = 0

  for (const entity of entities) {
    const start = entity.range?.start ?? 0
    const end = entity.range?.end ?? start
    if (start < cursor || end <= start || start > source.length) {
      continue
    }
    if (start > cursor) {
      output.push({ text: source.slice(cursor, start).join('') })
    }

    const rawText = source.slice(start, Math.min(end, source.length)).join('')
    const part = entityPart(entity, rawText)
    if (part.text) {
      output.push(part)
    }
    cursor = Math.min(end, source.length)
  }

  if (cursor < source.length) {
    output.push({ text: source.slice(cursor).join('') })
  }
  return output.length > 0 ? output : [{ text: body.value }]
})

function collectEntities(text: JsonRecord): TextEntity[] {
  const entities = asRecord(text.entities)
  return [
    ...asArray<TextEntity>(entities.urls).map((item) => ({ ...item, kind: 'url' })),
    ...asArray<TextEntity>(entities.mentions).map((item) => ({ ...item, kind: 'mention' })),
    ...asArray<TextEntity>(entities.hashtags).map((item) => ({ ...item, kind: 'hashtag' })),
    ...asArray<TextEntity>(entities.symbols).map((item) => ({ ...item, kind: 'symbol' })),
    ...asArray<TextEntity>(entities.media).map((item) => ({ ...item, kind: 'media' })),
  ].sort((a, b) => (a.range?.start ?? 0) - (b.range?.start ?? 0))
}

function entityPart(entity: TextEntity, rawText: string): TextPart {
  const kind = optionalText(entity.kind)
  if (kind === 'url' || kind === 'media') {
    const display = optionalText(entity.displayText) || rawText
    const href = optionalText(entity.expandedUrl) || optionalText(entity.url)
    return href ? { text: display, href } : { text: display }
  }
  if (kind === 'mention') {
    const userId = optionalText(entity.userId)
    return userId ? { text: rawText, to: `/browse/users/${userId}` } : { text: rawText }
  }
  return { text: rawText }
}
</script>

<style scoped>
.tweet-text {
  margin: 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  line-height: 1.45;
  font-size: 0.95rem;
}

.tweet-text.clamp {
  display: -webkit-box;
  overflow: hidden;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: var(--max-lines);
}

.entity {
  color: #2761a8;
  text-decoration: none;
}

.entity:hover {
  text-decoration: underline;
}
</style>
