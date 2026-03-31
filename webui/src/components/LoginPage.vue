<template>
  <section class="stack">
    <p class="lead">
      Sign in through the configured SSO provider, then finish local registration if this is the
      first login.
    </p>
    <button type="button" class="primary">Continue to SSO</button>
  </section>
</template>

<style scoped>
.stack {
  display: grid;
  gap: 16px;
}

.lead {
  margin: 0;
  color: #42536e;
}

.primary {
  width: fit-content;
  border: 0;
  border-radius: 999px;
  background: #10203a;
  color: white;
  padding: 12px 20px;
  font: inherit;
}
</style>

