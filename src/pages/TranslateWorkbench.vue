<script setup lang="ts">
import LogPanel from "../components/LogPanel.vue";
import PreviewSamples from "../components/PreviewSamples.vue";
import ProjectConfigCard from "../components/ProjectConfigCard.vue";
import StatusPanel from "../components/StatusPanel.vue";
</script>

<template>
  <div class="grid gap-5 xl:grid-cols-[minmax(0,1.15fr)_minmax(360px,0.85fr)]">
    <section id="project-config">
      <ProjectConfigCard />
    </section>

    <div class="grid content-start gap-5">
      <section id="status">
        <StatusPanel />
      </section>
      <section id="logs">
        <LogPanel mode="compact" />
      </section>
      <section id="preview">
        <PreviewSamples />
      </section>
    </div>
  </div>
</template>
