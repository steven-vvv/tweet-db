<template>
  <RouterView />
  <section
    v-if="showMobilePrompt"
    class="mobile-prompt-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="mobile-prompt-title"
  >
    <div class="mobile-prompt">
      <h2 id="mobile-prompt-title">Open mobile view?</h2>
      <p>This screen size matches the mobile browse layout.</p>
      <div class="mobile-prompt-actions">
        <button class="secondary" type="button" @click="dismissMobilePrompt">Keep desktop</button>
        <button type="button" @click="openMobileBrowse">Open mobile</button>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { RouterView } from 'vue-router'

const mobilePromptStorageKey = 'tweet-db:browse-mobile-prompt-seen'
const showMobilePrompt = ref(false)

onMounted(() => {
  if (hasSeenMobilePrompt()) {
    return
  }

  const viewportMatchesMobile = window.matchMedia('(max-width: 780px), (orientation: portrait)')
  showMobilePrompt.value = viewportMatchesMobile.matches
})

function dismissMobilePrompt() {
  markMobilePromptSeen()
  showMobilePrompt.value = false
}

function openMobileBrowse() {
  markMobilePromptSeen()
  window.location.href = mobileBrowseUrl()
}

function mobileBrowseUrl(): string {
  const path = window.location.pathname.replace(/^\/browse\b/, '/mobile/browse')
  return `${path || '/mobile/browse'}${window.location.search}${window.location.hash}`
}

function hasSeenMobilePrompt(): boolean {
  try {
    return window.localStorage.getItem(mobilePromptStorageKey) === '1'
  } catch {
    return false
  }
}

function markMobilePromptSeen() {
  try {
    window.localStorage.setItem(mobilePromptStorageKey, '1')
  } catch {
    // Storage can be unavailable in restricted browser modes.
  }
}
</script>

<style scoped>
:global(body) {
  margin: 0;
  min-height: 100vh;
  font-family: "IBM Plex Sans", "Helvetica Neue", sans-serif;
  background: #f4f6f8;
  color: #161a22;
}

:global(*) {
  box-sizing: border-box;
}

:global(a) {
  color: inherit;
}

.mobile-prompt-backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: grid;
  place-items: center;
  padding: 18px;
  background: rgba(15, 23, 42, 0.36);
}

.mobile-prompt {
  width: min(100%, 360px);
  display: grid;
  gap: 14px;
  border: 1px solid #dfe5ee;
  border-radius: 8px;
  padding: 18px;
  background: #ffffff;
  box-shadow: 0 20px 60px rgba(15, 23, 42, 0.24);
}

.mobile-prompt h2,
.mobile-prompt p {
  margin: 0;
}

.mobile-prompt h2 {
  color: #111827;
  font-size: 1.08rem;
}

.mobile-prompt p {
  color: #647084;
  line-height: 1.45;
}

.mobile-prompt-actions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.mobile-prompt button {
  min-height: 40px;
  border: 1px solid #111827;
  border-radius: 6px;
  background: #111827;
  color: #ffffff;
  font: inherit;
  font-weight: 750;
  cursor: pointer;
}

.mobile-prompt button.secondary {
  border-color: #cbd5e1;
  background: #ffffff;
  color: #172033;
}
</style>
