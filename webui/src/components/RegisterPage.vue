<template>
  <section class="stack">
    <p class="lead">Set a local username after the first SSO login.</p>
    <form class="stack">
      <label class="stack">
        <span>Username</span>
        <input type="text" placeholder="username" />
      </label>
      <button type="submit" class="primary">Complete registration</button>
    </form>
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

input {
  border: 1px solid #ccd4df;
  border-radius: 12px;
  padding: 12px 14px;
  font: inherit;
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

