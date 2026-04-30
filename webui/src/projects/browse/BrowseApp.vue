<template>
  <RouterView />
</template>

<script setup lang="ts">
import { RouterView } from 'vue-router'
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
</style>
