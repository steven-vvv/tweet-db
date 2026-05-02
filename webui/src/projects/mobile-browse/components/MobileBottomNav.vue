<template>
  <nav class="bottom-nav" aria-label="Mobile browse navigation">
    <RouterLink class="nav-item" to="/mobile/browse">Home</RouterLink>
    <RouterLink class="nav-item" to="/mobile/browse/search">Search</RouterLink>
    <a class="nav-item" href="/account">Account</a>
  </nav>
</template>

<script setup lang="ts">
import { RouterLink } from 'vue-router'
</script>

<style scoped>
.bottom-nav {
  position: fixed;
  right: 0;
  bottom: 0;
  left: 0;
  z-index: 20;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  padding: 6px 8px calc(6px + env(safe-area-inset-bottom));
  border-top: 1px solid rgba(239, 243, 244, 0.94);
  background: rgba(255, 255, 255, 0.96);
  backdrop-filter: blur(14px);
}

.nav-item {
  min-height: 44px;
  display: grid;
  place-items: center;
  border-radius: 999px;
  color: #536471;
  font-size: 0.84rem;
  font-weight: 750;
  text-decoration: none;
}

.nav-item.router-link-active {
  color: #0f1419;
  background: #eff3f4;
}

@media (min-width: 720px) {
  .bottom-nav {
    right: calc((100vw - 620px) / 2);
    left: calc((100vw - 620px) / 2);
    border-right: 1px solid #eff3f4;
    border-left: 1px solid #eff3f4;
  }
}
</style>
