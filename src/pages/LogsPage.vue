<script setup lang="ts">
import LogPanel from "../components/LogPanel.vue";
import StatusPanel from "../components/StatusPanel.vue";
</script>

<template>
  <div class="grid gap-5">
    <section>
      <StatusPanel />
    </section>

    <section>
      <LogPanel mode="page" />
    </section>
  </div>
</template>
