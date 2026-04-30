<template>
  <main class="admin-root">
    <RouterView />
  </main>
</template>

<script setup lang="ts">
import { RouterView } from 'vue-router'
</script>

<style scoped>
:global(body) {
  margin: 0;
  min-height: 100vh;
  font-family: "IBM Plex Sans", "Helvetica Neue", sans-serif;
  background: #f6f8fb;
  color: #172033;
}

:global(*) {
  box-sizing: border-box;
}

.admin-root {
  min-height: 100vh;
  background: #f6f8fb;
}
</style>
