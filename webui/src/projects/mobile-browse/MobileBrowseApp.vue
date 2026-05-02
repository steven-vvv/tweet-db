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
  font-family: "IBM Plex Sans", "Helvetica Neue", Arial, sans-serif;
  background: #ffffff;
  color: #0f1419;
}

:global(*) {
  box-sizing: border-box;
}

:global(a) {
  color: inherit;
}

:global(button),
:global(input),
:global(select) {
  font: inherit;
}
</style>
