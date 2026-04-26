<template>
  <main v-if="isWide" class="admin-root">
    <RouterView />
  </main>
  <main v-else class="shell">
    <section class="card">
      <header class="hero">
        <p class="eyebrow">tweet-db</p>
        <h1>Account access and session status</h1>
      </header>
      <RouterView />
    </section>
  </main>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { RouterView, useRoute } from 'vue-router'

const route = useRoute()
const isWide = computed(() => Boolean(route.meta.wide))
</script>

<style scoped>
:global(body) {
  margin: 0;
  min-height: 100vh;
  font-family: "IBM Plex Sans", "Helvetica Neue", sans-serif;
  background:
    radial-gradient(circle at top left, rgba(84, 129, 255, 0.2), transparent 26%),
    linear-gradient(145deg, #08101c 0%, #13233a 38%, #eef3f8 38%, #eef3f8 100%);
  color: #10203a;
}

:global(*) {
  box-sizing: border-box;
}

.shell {
  min-height: 100vh;
  display: grid;
  place-items: center;
  padding: 32px 16px;
}

.card {
  width: min(760px, 100%);
  background: rgba(255, 255, 255, 0.96);
  border: 1px solid rgba(16, 32, 58, 0.08);
  border-radius: 24px;
  padding: 32px;
  box-shadow: 0 24px 80px rgba(13, 22, 39, 0.16);
  backdrop-filter: blur(16px);
}

.admin-root {
  min-height: 100vh;
  background: #f6f8fb;
}

.hero {
  margin-bottom: 24px;
}

.hero h1 {
  margin: 0;
  font-size: clamp(2rem, 5vw, 3rem);
  line-height: 0.96;
}

.eyebrow {
  margin: 0 0 10px;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  font-size: 0.75rem;
  color: #4b6589;
}
</style>
