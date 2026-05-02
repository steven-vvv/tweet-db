<template>
  <main class="mobile-shell">
    <MobileTopBar :title="title">
      <template v-if="$slots.action" #action>
        <slot name="action" />
      </template>
    </MobileTopBar>
    <section class="mobile-content">
      <slot />
    </section>
    <MobileBottomNav />
  </main>
</template>

<script setup lang="ts">
import MobileBottomNav from './MobileBottomNav.vue'
import MobileTopBar from './MobileTopBar.vue'

defineProps<{
  title: string
}>()
</script>

<style scoped>
.mobile-shell {
  min-height: 100vh;
  background: #ffffff;
}

.mobile-content {
  min-height: 100vh;
  padding: 49px 0 calc(68px + env(safe-area-inset-bottom));
}

@media (min-width: 720px) {
  .mobile-shell {
    max-width: 620px;
    margin: 0 auto;
    border-right: 1px solid #eff3f4;
    border-left: 1px solid #eff3f4;
  }
}
</style>
