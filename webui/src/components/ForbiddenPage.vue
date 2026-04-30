<template>
  <section class="stack">
    <p class="eyebrow">403</p>
    <h2>Administrator access required</h2>
    <p class="hint">
      The current session is authenticated but does not hold administrator privileges for the
      internal management console.
    </p>
    <a href="/account" class="primary">Return to account</a>
  </section>
</template>

<style scoped>
.stack {
  display: grid;
  gap: 16px;
}

.eyebrow,
.hint,
h2 {
  margin: 0;
}

.eyebrow {
  font-size: 0.74rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #5e7698;
}

.hint {
  color: #516781;
}

.primary {
  width: fit-content;
  border-radius: 999px;
  padding: 11px 18px;
  background: #10203a;
  color: white;
  text-decoration: none;
}
</style>
