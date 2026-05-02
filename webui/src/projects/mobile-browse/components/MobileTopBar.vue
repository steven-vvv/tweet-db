<template>
  <header class="top-bar">
    <h1>{{ title }}</h1>
    <div v-if="$slots.action" class="top-action">
      <slot name="action" />
    </div>
  </header>
</template>

<script setup lang="ts">
defineProps<{
  title: string
}>()
</script>

<style scoped>
.top-bar {
  position: fixed;
  top: 0;
  right: 0;
  left: 0;
  z-index: 20;
  height: 49px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  border-bottom: 1px solid rgba(239, 243, 244, 0.92);
  padding: 0 16px;
  background: rgba(255, 255, 255, 0.94);
  backdrop-filter: blur(14px);
}

h1 {
  min-width: 0;
  margin: 0;
  overflow: hidden;
  color: #0f1419;
  font-size: 1.12rem;
  font-weight: 800;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.top-action {
  flex: 0 0 auto;
}

@media (min-width: 720px) {
  .top-bar {
    right: calc((100vw - 620px) / 2);
    left: calc((100vw - 620px) / 2);
    border-right: 1px solid #eff3f4;
    border-left: 1px solid #eff3f4;
  }
}
</style>
