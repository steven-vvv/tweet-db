<template>
  <section class="stack">
    <header class="page-head">
      <div>
        <p class="eyebrow">Stored Resource Detail</p>
        <h2>{{ summary.objectKey ?? route.params.objectId }}</h2>
      </div>
      <button type="button" class="primary" :disabled="signing" @click="openSignedUrl">
        Open signed URL
      </button>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="summary-card">
      <div class="summary-row">
        <span>Bucket</span>
        <strong>{{ summary.bucket ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Provider</span>
        <strong>{{ summary.provider ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Content type</span>
        <strong>{{ summary.contentType ?? '-' }}</strong>
      </div>
      <div class="summary-row">
        <span>Created</span>
        <strong>{{ summary.createdAt ?? '-' }}</strong>
      </div>
    </section>

    <section class="panel">
      <h3>Bound media</h3>
      <RouterLink
        v-for="item in bindings"
        :key="item.media_id"
        :to="`/admin/media/${item.media_id}`"
        class="link-row"
      >
        <strong>{{ item.media_id }}</strong>
        <span>{{ item.identity_kind }}</span>
        <span>{{ item.object_role }}</span>
      </RouterLink>
      <div v-if="bindings.length === 0" class="hint">No managed media bindings were found.</div>
    </section>

    <JsonPanel title="Storage Object Record" :value="detail.record" />
    <JsonPanel title="Related" :value="detail.related" />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

import { asArray, asRecord } from '../../admin-helpers'
import { fetchAdminStorageObject, signAdminStorageObject, type DetailResponse } from '../../api'
import JsonPanel from './JsonPanel.vue'

const route = useRoute()
const detail = ref<DetailResponse>({ summary: {}, record: {}, related: {} })
const error = ref('')
const signing = ref(false)

const summary = computed(() => asRecord(detail.value.summary))
const related = computed(() => asRecord(detail.value.related))
const bindings = computed(() => asArray(related.value.bindings))

onMounted(() => {
  void load()
})

async function load() {
  error.value = ''

  try {
    detail.value = await fetchAdminStorageObject(String(route.params.objectId))
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load storage object detail'
  }
}

async function openSignedUrl() {
  signing.value = true
  error.value = ''

  try {
    const response = await signAdminStorageObject(String(route.params.objectId))
    window.open(response.url, '_blank', 'noopener')
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to request a signed URL'
  } finally {
    signing.value = false
  }
}
</script>

<style scoped>
.stack,
.panel {
  display: grid;
  gap: 18px;
}

.page-head {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: end;
}

.page-head h2,
.panel h3 {
  margin: 0;
}

.eyebrow {
  margin: 0;
  font-size: 0.74rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #5e7698;
}

.error,
.hint {
  margin: 0;
}

.error {
  color: #a12231;
}

.hint {
  color: #516781;
}

.summary-card,
.panel {
  padding: 18px;
  border: 1px solid #d3ddeb;
  border-radius: 18px;
  background: white;
}

.summary-row,
.link-row {
  display: flex;
  justify-content: space-between;
  gap: 18px;
}

.link-row {
  padding: 12px 0;
  color: inherit;
  text-decoration: none;
  border-top: 1px solid #edf2f7;
}

.link-row:first-of-type {
  border-top: 0;
}

.primary {
  width: fit-content;
  border: 0;
  border-radius: 999px;
  padding: 11px 18px;
  background: #10203a;
  color: white;
  font: inherit;
}
</style>
