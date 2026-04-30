<template>
  <main class="placeholder">
    <h1>User {{ route.params.userId }}</h1>
  </main>
</template>

<script setup lang="ts">
import { useRoute } from 'vue-router'

const route = useRoute()
</script>

<style scoped>
.placeholder {
  min-height: 100vh;
  display: grid;
  place-items: center;
}

h1 {
  margin: 0;
  font-size: 1.5rem;
}
</style>
