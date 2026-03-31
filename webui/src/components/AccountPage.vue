<template>
  <section class="stack">
    <p class="lead">Minimal account view for session state and local username.</p>
    <div class="panel">
      <div class="row">
        <span>Status</span>
        <strong>Not wired yet</strong>
      </div>
      <div class="row">
        <span>Username</span>
        <strong>-</strong>
      </div>
    </div>
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

.panel {
  display: grid;
  gap: 12px;
  padding: 18px;
  border: 1px solid #dbe2ea;
  border-radius: 18px;
  background: #f8fafc;
}

.row {
  display: flex;
  justify-content: space-between;
  gap: 24px;
}
</style>
