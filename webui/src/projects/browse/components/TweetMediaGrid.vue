<template>
  <div v-if="visibleMedia.length > 0" class="media-grid" :class="`count-${visibleMedia.length}`">
    <a
      v-for="(item, index) in visibleMedia"
      :key="textValue(item.id, String(index))"
      class="media-tile"
      :class="{ lead: visibleMedia.length === 3 && index === 0 }"
      :href="mediaOpenUrl(item) || '#'"
      target="_blank"
      rel="noreferrer"
      @click="handleClick($event, item)"
    >
      <img
        v-if="mediaPreviewUrl(item) && !failed[textValue(item.id)]"
        :src="mediaPreviewUrl(item)"
        :alt="mediaAlt(item)"
        loading="lazy"
        @error="failed[textValue(item.id)] = true"
      />
      <span v-else class="media-fallback">
        <strong>{{ textValue(item.type, 'media') }}</strong>
        <small>{{ textValue(item.id) }}</small>
      </span>
      <span v-if="mediaBadge(item)" class="tag">{{ mediaBadge(item) }}</span>
      <span v-if="index === 3 && extraCount > 0" class="tag extra">+{{ extraCount }}</span>
    </a>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'

import type { JsonRecord } from '../../../shared/api'
import {
  mediaAlt,
  mediaOpenUrl,
  mediaPreviewUrl,
  mediaWarnings,
  textValue,
} from '../browse-helpers'

const props = defineProps<{
  media: JsonRecord[]
}>()

const failed = ref<Record<string, boolean>>({})
const visibleMedia = computed(() => props.media.slice(0, 4))
const extraCount = computed(() => Math.max(0, props.media.length - 4))

function mediaBadge(item: JsonRecord): string {
  const warnings = mediaWarnings(item)
  if (warnings.length > 0) {
    return warnings.slice(0, 2).join(', ')
  }
  const type = textValue(item.type, '')
  return type === 'photo' ? '' : type.replace('_', ' ')
}

function handleClick(event: MouseEvent, item: JsonRecord) {
  if (!mediaOpenUrl(item)) {
    event.preventDefault()
  }
}
</script>

<style scoped>
.media-grid {
  display: grid;
  gap: 2px;
  overflow: hidden;
  border: 1px solid #d7dde5;
  border-radius: 8px;
  background: #d7dde5;
}

.count-1 {
  grid-template-columns: 1fr;
}

.count-2,
.count-4 {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

.count-3 {
  grid-template-columns: 1.1fr 0.9fr;
  grid-template-rows: repeat(2, minmax(118px, 1fr));
}

.media-tile {
  position: relative;
  min-height: 132px;
  aspect-ratio: 16 / 10;
  display: block;
  overflow: hidden;
  background: #edf1f5;
  color: #253044;
  text-decoration: none;
}

.media-tile.lead {
  grid-row: span 2;
  aspect-ratio: auto;
}

.media-tile img {
  width: 100%;
  height: 100%;
  display: block;
  object-fit: cover;
}

.media-fallback {
  height: 100%;
  min-height: inherit;
  display: grid;
  place-items: center;
  gap: 4px;
  padding: 12px;
  text-align: center;
}

.media-fallback strong {
  text-transform: uppercase;
  font-size: 0.76rem;
  letter-spacing: 0.04em;
}

.media-fallback small {
  max-width: 100%;
  overflow-wrap: anywhere;
  color: #68758a;
  font-family: "IBM Plex Mono", ui-monospace, monospace;
}

.tag {
  position: absolute;
  right: 8px;
  bottom: 8px;
  max-width: calc(100% - 16px);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  border-radius: 999px;
  padding: 3px 7px;
  background: rgba(19, 25, 36, 0.82);
  color: #fff;
  font-size: 0.72rem;
  font-weight: 650;
}

.extra {
  top: 8px;
  bottom: auto;
}
</style>
