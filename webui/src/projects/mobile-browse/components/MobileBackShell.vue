<template>
  <main class="mobile-shell">
    <header class="top-bar">
      <button class="back-button" type="button" @click="goBack">Back</button>
      <h1>{{ title }}</h1>
      <div class="spacer" />
    </header>
    <section class="mobile-content">
      <slot />
    </section>
  </main>
</template>

<script setup lang="ts">
import { useRouter } from 'vue-router'

defineProps<{
  title: string
}>()

const router = useRouter()

function goBack() {
  if (window.history.length > 1) {
    router.back()
    return
  }
  router.push('/mobile/browse')
}
</script>

<style scoped>
.mobile-shell {
  min-height: 100vh;
  background: #ffffff;
}

.top-bar {
  position: fixed;
  top: 0;
  right: 0;
  left: 0;
  z-index: 20;
  height: 49px;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  border-bottom: 1px solid rgba(239, 243, 244, 0.92);
  padding: 0 12px;
  background: rgba(255, 255, 255, 0.94);
  backdrop-filter: blur(14px);
}

h1 {
  margin: 0;
  overflow: hidden;
  color: #0f1419;
  font-size: 1.05rem;
  font-weight: 800;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.back-button {
  min-height: 34px;
  border: 0;
  border-radius: 999px;
  padding: 0 10px;
  background: #eff3f4;
  color: #0f1419;
  font-weight: 750;
  cursor: pointer;
}

.spacer {
  width: 54px;
}

.mobile-content {
  min-height: 100vh;
  padding-top: 49px;
}

@media (min-width: 720px) {
  .mobile-shell {
    max-width: 620px;
    margin: 0 auto;
    border-right: 1px solid #eff3f4;
    border-left: 1px solid #eff3f4;
  }

  .top-bar {
    right: calc((100vw - 620px) / 2);
    left: calc((100vw - 620px) / 2);
    border-right: 1px solid #eff3f4;
    border-left: 1px solid #eff3f4;
  }
}
</style>
