<template>
  <main class="shell">
    <section class="card">
      <header class="hero">
        <p class="eyebrow">tweet-db</p>
        <h1>Capture storage and status control plane</h1>
      </header>
      <RouterView />
    </section>
  </main>
</template>

<style scoped>
:global(body) {
  margin: 0;
  min-height: 100vh;
  font-family: "IBM Plex Sans", "Helvetica Neue", sans-serif;
  background:
    radial-gradient(circle at top left, rgba(84, 129, 255, 0.22), transparent 28%),
    linear-gradient(135deg, #0d1627 0%, #172740 45%, #f2f5f9 45%, #f2f5f9 100%);
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
  width: min(720px, 100%);
  background: rgba(255, 255, 255, 0.95);
  border: 1px solid rgba(16, 32, 58, 0.08);
  border-radius: 24px;
  padding: 32px;
  box-shadow: 0 24px 80px rgba(13, 22, 39, 0.18);
  backdrop-filter: blur(16px);
}

.hero {
  margin-bottom: 24px;
}

.hero h1 {
  margin: 0;
  font-size: clamp(2rem, 5vw, 3rem);
  line-height: 1;
}

.eyebrow {
  margin: 0 0 10px;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  font-size: 0.75rem;
  color: #4b6589;
}
</style>

